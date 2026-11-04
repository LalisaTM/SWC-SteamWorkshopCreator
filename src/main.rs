#![cfg_attr(all(windows, not(feature = "console")), windows_subsystem = "windows")]

mod playback;
mod ui;
use clap::Parser;
use eframe::egui;
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Duration,
};
use steam_workshop_creator::{self as core, *};
use ui::configure_theme;

#[derive(Parser)]
#[command(
    version,
    about = "Steam showcase creator. Ohne Unterbefehl startet die Desktop-App."
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Action>,
}
#[derive(clap::Subcommand)]
enum Action {
    Export {
        input: PathBuf,
        #[arg(short, long, default_value = "exports")]
        output: PathBuf,
        #[arg(long, value_enum, default_value = "workshop5")]
        layout: Layout,
        #[arg(long, value_enum, default_value = "gif")]
        format: Format,
        #[arg(long, default_value_t = 150)]
        width: u32,
        #[arg(long, default_value_t = 600)]
        height: u32,
        #[arg(long, default_value_t = 0)]
        gap: u32,
        #[arg(long, default_value_t = 0.)]
        start: f64,
        #[arg(long, default_value_t = 3.)]
        duration: f64,
        #[arg(long, default_value_t = 20)]
        fps: u32,
        #[arg(long, default_value_t = 128)]
        colors: u32,
        #[arg(long, value_enum, default_value = "sierra")]
        dither: Dither,
        #[arg(long)]
        monochrome: bool,
        #[arg(long)]
        workshop_patch: bool,
    },
}
fn main() -> anyhow::Result<()> {
    if let Some(Action::Export {
        input,
        output,
        layout,
        format,
        width,
        height,
        gap,
        start,
        duration,
        fps,
        colors,
        dither,
        monochrome,
        workshop_patch,
    }) = Cli::parse().command
    {
        let settings = Settings {
            layout,
            format,
            panel_width: width,
            height,
            gap,
            start,
            duration,
            fps,
            colors,
            dither,
            monochrome,
            workshop_patch,
            ..Default::default()
        };
        let result = export(
            &input,
            &output,
            &settings,
            &Arc::new(AtomicBool::new(false)),
            |done, total| eprintln!("Panels: {done}/{total}"),
        )?;
        println!("{}", serde_json::to_string_pretty(&result)?);
        return Ok(());
    }
    let restart = Arc::new(std::sync::Mutex::new(None::<PathBuf>));
    let app_restart = restart.clone();
    eframe::run_native(
        "SteamWorkshopCreator",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([1440., 960.])
                .with_maximized(true)
                .with_icon(app_icon())
                .with_min_inner_size([1180., 740.]),
            ..Default::default()
        },
        Box::new(move |cc| Ok(Box::new(App::new(cc, app_restart.clone())))),
    )
    .map_err(|e| anyhow::anyhow!("{e}"))?;
    if let Some(path) = restart.lock().unwrap().take() {
        std::process::Command::new(path).spawn()?;
    }
    Ok(())
}
enum Event {
    UpdateChecked(Result<Option<core::updater::Update>, String>),
    UpdateInstalled(Result<PathBuf, String>),
    Loaded(Result<Media, String>),
    PoolReady(PathBuf, Result<(Media, Vec<u8>), String>),
    Measured(Result<Vec<ExportFile>, String>),
    Progress(usize, usize),
    Done(Result<Export, String>),
}
struct PoolItem {
    path: PathBuf,
    media: Option<Media>,
    thumbnail: Option<egui::TextureHandle>,
    error: Option<String>,
}
struct App {
    available_update: Option<core::updater::Update>,
    update_status: String,
    checking_update: bool,
    installing_update: bool,
    restart: Arc<std::sync::Mutex<Option<PathBuf>>>,
    pool: Vec<PoolItem>,
    playback: Option<playback::Playback>,
    playing: bool,
    startup: bool,
    preview_reload: Option<std::time::Instant>,
    measured: Option<Vec<ExportFile>>,
    measuring: bool,
    settings: Settings,
    input: Option<PathBuf>,
    output: PathBuf,
    media: Option<Media>,
    texture: Option<egui::TextureHandle>,
    brand_icon: egui::TextureHandle,
    tx: mpsc::Sender<Event>,
    rx: mpsc::Receiver<Event>,
    busy: bool,
    loading: bool,
    cancel: Cancel,
    status: String,
    progress: f32,
    result: Option<Export>,
    preview_dirty: bool,
    preview_settings: Settings,
    status_error: bool,
}
impl App {
    fn new(
        cc: &eframe::CreationContext<'_>,
        restart: Arc<std::sync::Mutex<Option<PathBuf>>>,
    ) -> Self {
        configure_theme(&cc.egui_ctx);
        let icon = app_icon();
        let brand_icon = cc.egui_ctx.load_texture(
            "brand-icon",
            egui::ColorImage::from_rgba_unmultiplied(
                [icon.width as usize, icon.height as usize],
                &icon.rgba,
            ),
            Default::default(),
        );
        let (tx, rx) = mpsc::channel();
        let update_tx = tx.clone();
        std::thread::spawn(move || {
            let _ = update_tx.send(Event::UpdateChecked(
                core::updater::check().map_err(|e| format!("{e:#}")),
            ));
        });
        let settings: Settings = cc
            .storage
            .and_then(|s| s.get_string("settings"))
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        Self {
            available_update: None,
            update_status: "Suche nach Updates …".into(),
            checking_update: true,
            installing_update: false,
            restart,
            pool: Vec::new(),
            playback: None,
            playing: true,
            startup: true,
            preview_reload: None,
            measured: None,
            measuring: false,
            preview_settings: settings.clone(),
            settings,
            input: None,
            output: PathBuf::from("exports"),
            media: None,
            texture: None,
            brand_icon,
            tx,
            rx,
            busy: false,
            loading: false,
            cancel: Arc::new(AtomicBool::new(false)),
            status: "Video, GIF oder Bild auswählen, um zu beginnen.".into(),
            progress: 0.,
            result: None,
            preview_dirty: false,
            status_error: false,
        }
    }
    fn check_updates(&mut self) {
        self.checking_update = true;
        self.update_status = "Suche nach Updates …".into();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Event::UpdateChecked(
                core::updater::check().map_err(|e| format!("{e:#}")),
            ));
        });
    }
    fn install_update(&mut self) {
        let Some(update) = self.available_update.clone() else {
            return;
        };
        self.installing_update = true;
        self.busy = true;
        self.progress = 0.;
        self.update_status = "Update wird heruntergeladen und geprüft …".into();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = core::updater::install(&update, |done, total| {
                let _ = tx.send(Event::Progress(done as usize, total as usize));
            });
            let _ = tx.send(Event::UpdateInstalled(result.map_err(|e| format!("{e:#}"))));
        });
    }
    fn load(&mut self) {
        let Some(input) = self.input.clone() else {
            return;
        };
        self.loading = true;
        self.texture = None;
        self.media = None;
        self.preview_dirty = false;
        self.preview_reload = None;
        self.preview_settings = self.settings.clone();
        self.status_error = false;
        self.measured = None;
        self.playback = Some(playback::Playback::new(
            input.clone(),
            self.settings.clone(),
        ));
        self.playing = true;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Event::Loaded(probe(&input).map_err(|e| format!("{e:#}"))));
        });
    }
    fn import_files(&mut self, paths: Vec<PathBuf>) {
        let mut first = None;
        for path in paths {
            if self.pool.iter().any(|p| p.path == path) {
                continue;
            }
            if first.is_none() {
                first = Some(path.clone());
            }
            self.pool.push(PoolItem {
                path: path.clone(),
                media: None,
                thumbnail: None,
                error: None,
            });
            let tx = self.tx.clone();
            std::thread::spawn(move || {
                let result = (|| -> anyhow::Result<_> {
                    Ok((
                        probe(&path)?,
                        thumbnail(
                            &path,
                            &Settings {
                                height: 100,
                                panel_width: 36,
                                ..Default::default()
                            },
                        )?,
                    ))
                })();
                let _ = tx.send(Event::PoolReady(path, result.map_err(|e| format!("{e:#}"))));
            });
        }
        if self.input.is_none()
            && let Some(path) = first
        {
            self.select_media(path);
        }
    }
    fn select_media(&mut self, path: PathBuf) {
        self.input = Some(path);
        self.settings.start = 0.;
        self.result = None;
        self.load();
    }
    fn measure(&mut self) {
        let Some(input) = self.input.clone() else {
            return;
        };
        self.busy = true;
        self.measuring = true;
        self.progress = 0.;
        self.measured = None;
        self.status_error = false;
        self.status = "Größenanalyse: Panels werden testweise codiert …".into();
        self.cancel = Arc::new(AtomicBool::new(false));
        let (settings, cancel, tx) = (self.settings.clone(), self.cancel.clone(), self.tx.clone());
        std::thread::spawn(move || {
            let result = (|| -> anyhow::Result<_> {
                let tmp = tempfile::tempdir()?;
                let export = export(&input, tmp.path(), &settings, &cancel, |n, t| {
                    let _ = tx.send(Event::Progress(n, t));
                })?;
                Ok(export.files)
            })();
            let _ = tx.send(Event::Measured(result.map_err(|e| format!("{e:#}"))));
        });
    }
    fn generate(&mut self) {
        let Some(input) = self.input.clone() else {
            return;
        };
        self.busy = true;
        self.progress = 0.;
        self.result = None;
        self.cancel = Arc::new(AtomicBool::new(false));
        self.status = "Export läuft …".into();
        self.status_error = false;
        let (settings, output, cancel, tx) = (
            self.settings.clone(),
            self.output.clone(),
            self.cancel.clone(),
            self.tx.clone(),
        );
        std::thread::spawn(move || {
            let result = export(&input, &output, &settings, &cancel, |n, t| {
                let _ = tx.send(Event::Progress(n, t));
            });
            let _ = tx.send(Event::Done(result.map_err(|e| format!("{e:#}"))));
        });
    }
}
impl Drop for App {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}
impl eframe::App for App {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        if let Ok(s) = serde_json::to_string(&self.settings) {
            storage.set_string("settings", s);
        }
    }
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        while let Ok(event) = self.rx.try_recv() {
            match event {
                Event::UpdateChecked(result) => {
                    self.checking_update = false;
                    match result {
                        Ok(Some(update)) => {
                            self.update_status = format!("Version {} verfügbar", update.version);
                            self.available_update = Some(update);
                        }
                        Ok(None) => {
                            self.available_update = None;
                            self.update_status = "Auf dem neuesten Stand".into();
                        }
                        Err(error) => {
                            self.update_status = format!("Updateprüfung nicht möglich: {error}")
                        }
                    }
                }
                Event::UpdateInstalled(result) => {
                    self.installing_update = false;
                    self.busy = false;
                    match result {
                        Ok(path) => {
                            *self.restart.lock().unwrap() = Some(path);
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        Err(error) => {
                            self.update_status = error;
                        }
                    }
                }

                Event::Loaded(result) => {
                    self.loading = false;
                    match result {
                        Ok(media) => {
                            if let Some(duration) = media.duration {
                                let remaining = (duration - self.settings.start).max(0.1);
                                if self.settings.duration > remaining {
                                    self.settings.duration = remaining;
                                    self.preview_reload = Some(std::time::Instant::now());
                                }
                            }
                            self.media = Some(media);
                            self.status = "Vorschau wird abgespielt.".into();
                        }
                        Err(e) => {
                            self.status = e;
                            self.status_error = true;
                        }
                    }
                }
                Event::PoolReady(path, result) => {
                    if let Some(item) = self.pool.iter_mut().find(|p| p.path == path) {
                        match result {
                            Ok((media, bytes)) => {
                                item.media = Some(media);
                                if let Ok(img) = image::load_from_memory(&bytes) {
                                    let img = img.to_rgba8();
                                    item.thumbnail = Some(ctx.load_texture(
                                        path.display().to_string(),
                                        egui::ColorImage::from_rgba_unmultiplied(
                                            [img.width() as usize, img.height() as usize],
                                            img.as_raw(),
                                        ),
                                        Default::default(),
                                    ));
                                }
                            }
                            Err(error) => item.error = Some(error),
                        }
                    }
                }
                Event::Measured(result) => {
                    self.busy = false;
                    self.measuring = false;
                    match result {
                        Ok(files) => {
                            self.measured = Some(files);
                            self.status="Größenanalyse fertig. Werte stammen aus einem vollständigen Testexport.".into();
                        }
                        Err(e) => {
                            self.status = e;
                            self.status_error = true;
                        }
                    }
                }
                Event::Progress(n, t) => self.progress = n as f32 / t as f32,
                Event::Done(result) => {
                    self.busy = false;
                    match result {
                        Ok(result) => {
                            self.status = format!("Fertig: {}", result.directory.display());
                            self.result = Some(result);
                        }
                        Err(e) => {
                            self.status = e;
                            self.status_error = true;
                        }
                    }
                }
            }
        }
        if self.startup {
            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(true));
            self.startup = false;
        }
        if !self.busy && !self.loading {
            let dropped = ctx.input(|i| {
                i.raw
                    .dropped_files
                    .iter()
                    .filter_map(|f| f.path.clone())
                    .collect::<Vec<_>>()
            });
            if !dropped.is_empty() {
                self.import_files(dropped);
            }
        }
        if let Some(playback) = &mut self.playback {
            match playback.tick(self.playing || self.texture.is_none()) {
                Ok(Some(image)) => {
                    if let Some(texture) = &mut self.texture {
                        texture.set(image, Default::default());
                    } else {
                        self.texture = Some(ctx.load_texture("preview", image, Default::default()));
                    }
                }
                Err(error) => {
                    self.status = error;
                    self.status_error = true;
                }
                _ => {}
            }
        }
        self.draw(ctx);
        if !self.busy
            && !self.loading
            && self.input.is_some()
            && self
                .preview_reload
                .is_some_and(|t| t.elapsed() > Duration::from_millis(400))
        {
            self.load();
        }
        if self.busy || self.loading || self.checking_update || self.playback.is_some() {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }
}

fn app_icon() -> egui::IconData {
    let image = image::load_from_memory(include_bytes!("../assets/icon.png"))
        .expect("embedded icon")
        .to_rgba8();
    egui::IconData {
        width: image.width(),
        height: image.height(),
        rgba: image.into_raw(),
    }
}
