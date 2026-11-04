use super::*;
use std::time::Instant;

pub struct Playback {
    rx: mpsc::Receiver<Result<egui::ColorImage, String>>,
    cancel: Cancel,
    next: Instant,
    interval: Duration,
}
impl Playback {
    pub fn new(input: PathBuf, settings: Settings) -> Self {
        let (tx, rx) = mpsc::sync_channel(2);
        let cancel = Arc::new(AtomicBool::new(false));
        let stopped = cancel.clone();
        let interval = Duration::from_secs_f64(1. / settings.fps.min(24) as f64);
        std::thread::spawn(move || {
            let result = core::preview_stream(&input, &settings, &stopped, |bytes, size| {
                let mut message = Ok(egui::ColorImage::from_rgba_unmultiplied(size, &bytes));
                loop {
                    match tx.try_send(message) {
                        Ok(()) => return true,
                        Err(mpsc::TrySendError::Disconnected(_)) => return false,
                        Err(mpsc::TrySendError::Full(value)) => message = value,
                    }
                    if stopped.load(Ordering::Relaxed) {
                        return false;
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
            });
            if let Err(error) = result {
                let _ = tx.try_send(Err(format!("{error:#}")));
            }
        });
        Self {
            rx,
            cancel,
            next: Instant::now(),
            interval,
        }
    }
    pub fn tick(&mut self, playing: bool) -> Result<Option<egui::ColorImage>, String> {
        let now = Instant::now();
        if !playing {
            self.next = now;
            return Ok(None);
        }
        if now < self.next {
            return Ok(None);
        }
        match self.rx.try_recv() {
            Ok(result) => {
                // Keep a fixed clock; adding UI frame latency each time slows the clip.
                self.next = if now.saturating_duration_since(self.next) > self.interval * 3 {
                    now + self.interval
                } else {
                    self.next + self.interval
                };
                result.map(Some)
            }
            _ => Ok(None),
        }
    }
}
impl Drop for Playback {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}
