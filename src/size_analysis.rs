use super::*;
use std::time::Instant;

type Completed = (u64, Result<Vec<ExportFile>, String>);
pub struct Analysis {
    pub files: Option<Vec<ExportFile>>,
    pub error: Option<String>,
    revision: u64,
    due: Option<Instant>,
    active: Option<(u64, Cancel)>,
    tx: mpsc::Sender<Completed>,
    rx: mpsc::Receiver<Completed>,
}
impl Default for Analysis {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            files: None,
            error: None,
            revision: 0,
            due: None,
            active: None,
            tx,
            rx,
        }
    }
}
impl Analysis {
    pub fn invalidate(&mut self) {
        self.revision += 1;
        self.files = None;
        self.error = None;
        self.due = Some(Instant::now() + Duration::from_millis(800));
        if let Some((_, cancel)) = &self.active {
            cancel.store(true, Ordering::Relaxed);
        }
    }
    pub fn cancel(&mut self) {
        self.revision += 1;
        self.due = None;
        if let Some((_, cancel)) = &self.active {
            cancel.store(true, Ordering::Relaxed);
        }
    }
    pub fn pending(&self) -> bool {
        self.due.is_some() || self.active.is_some()
    }
    fn accept(&mut self, (revision, result): Completed) {
        if self.active.as_ref().is_some_and(|(r, _)| *r == revision) {
            self.active = None;
        }
        if revision == self.revision {
            match result {
                Ok(files) => self.files = Some(files),
                Err(error) => self.error = Some(error),
            }
        }
    }
    pub fn tick(&mut self, input: Option<&std::path::Path>, settings: &Settings, enabled: bool) {
        while let Ok(result) = self.rx.try_recv() {
            self.accept(result);
        }
        if !enabled || self.active.is_some() || !self.due.is_some_and(|t| Instant::now() >= t) {
            return;
        }
        let Some(input) = input else {
            self.due = None;
            return;
        };
        if settings.panels().is_err() {
            self.due = None;
            return;
        }
        self.due = None;
        let input = input.to_owned();
        let settings = settings.clone();
        let revision = self.revision;
        let cancel = Arc::new(AtomicBool::new(false));
        self.active = Some((revision, cancel.clone()));
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = (|| -> anyhow::Result<_> {
                let temporary = tempfile::tempdir()?;
                Ok(export(&input, temporary.path(), &settings, &cancel, |_, _| {})?.files)
            })();
            let _ = tx.send((revision, result.map_err(|e| format!("{e:#}"))));
        });
    }
}
impl Drop for Analysis {
    fn drop(&mut self) {
        self.cancel();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn idle_edit_starts_background_size_analysis() {
        let temp = tempfile::tempdir().unwrap();
        let input = temp.path().join("input.png");
        image::RgbImage::from_pixel(630, 32, image::Rgb([20, 50, 90]))
            .save(&input)
            .unwrap();
        let settings = Settings {
            layout: Layout::Featured,
            format: Format::Png,
            height: 32,
            ..Default::default()
        };
        let mut analysis = Analysis::default();
        analysis.invalidate();
        analysis.due = Some(Instant::now());
        analysis.tick(Some(&input), &settings, true);
        assert!(analysis.active.is_some());
        let result = analysis.rx.recv_timeout(Duration::from_secs(30)).unwrap();
        analysis.accept(result);
        assert!(analysis.error.is_none());
        assert!(!analysis.pending());
        let files = analysis.files.as_ref().unwrap();
        assert_eq!(files.len(), 1);
        assert!(files[0].bytes > 0);
    }
    #[test]
    fn stale_results_cannot_replace_new_sizes() {
        let mut a = Analysis::default();
        a.invalidate();
        let old = a.revision;
        let cancel = Arc::new(AtomicBool::new(false));
        a.active = Some((old, cancel.clone()));
        a.invalidate();
        assert!(cancel.load(Ordering::Relaxed));
        a.accept((old, Ok(vec![])));
        assert!(a.files.is_none());
        assert!(a.due.is_some());
        assert!(a.active.is_none());
        a.accept((a.revision, Ok(vec![])));
        assert!(a.files.is_some());
    }
    #[test]
    fn cancellation_invalidates_in_flight_result() {
        let mut a = Analysis::default();
        a.invalidate();
        let old = a.revision;
        a.cancel();
        a.accept((old, Ok(vec![])));
        assert!(a.files.is_none());
        assert!(!a.pending());
    }
}
