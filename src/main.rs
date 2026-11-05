#![cfg_attr(all(windows, not(feature = "console")), windows_subsystem = "windows")]

mod color_editor;
mod history;
mod playback;
mod size_analysis;
mod snapping;
mod transform_editor;
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
        #[arg(long, default_value_t = 0.)]
        offset_x: f32,
        #[arg(long, default_value_t = 0.)]
        offset_y: f32,
        #[arg(long, default_value_t = 1.)]
        scale: f32,
        #[arg(long, default_value_t = 0.)]
        rotation: f32,
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
        offset_x,
        offset_y,
        scale,
        rotation,
        workshop_patch,
    }) = Cli::parse().command
    {
        let settings = Settings {
            transform: Transform {
                x: offset_x,
                y: offset_y,
                scale,
                rotation,
            },
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
    Progress(usize, usize),
    Done(Result<Export, String>),
}
struct PoolItem {
    transform: Transform,
    path: PathBuf,
    media: Option<Media>,
    thumbnail: Option<egui::TextureHandle>,
    error: Option<String>,
}
struct App {
    color_editor: Option<color_editor::ColorEditor>,
    editor: transform_editor::Editor,
    history_action: i8,
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
    analysis: size_analysis::Analysis,
    history: history::History,
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
            color_editor: None,
            editor: Default::default(),
            history_action: 0,
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
            analysis: Default::default(),
            history: Default::default(),
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
            status: String::new(),
            progress: 0.,
            result: None,
            status_error: false,
        }
    }
    fn settings_changed(&mut self, before: &Settings) {
        self.analysis.invalidate();
        self.result = None;
        if before.start != self.settings.start
            || before.duration != self.settings.duration
            || before.fps != self.settings.fps
            || before.format != self.settings.format
            || before.monochrome != self.settings.monochrome
            || before.sepia != self.settings.sepia
            || before.invert != self.settings.invert
            || before.brightness != self.settings.brightness
            || before.contrast != self.settings.contrast
            || before.color != self.settings.color
        {
            self.preview_reload = Some(std::time::Instant::now());
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
        self.preview_reload = None;
        self.status_error = false;
        self.analysis.invalidate();
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
                transform: Transform::default(),
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
        if let Some(current) = &self.input
            && let Some(item) = self.pool.iter_mut().find(|p| &p.path == current)
        {
            item.transform = self.settings.transform;
        }
        self.settings.transform = self
            .pool
            .iter()
            .find(|p| p.path == path)
            .map(|p| p.transform)
            .unwrap_or_default();
        self.editor.selected = false;
        self.history = Default::default();
        self.input = Some(path);
        self.settings.start = 0.;
        self.result = None;
        self.load();
    }
    fn generate(&mut self) {
        self.analysis.cancel();
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
                            self.status.clear();
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
        if !self.busy && !self.loading && self.color_editor.is_none() {
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
        if ctx.input(|i| i.pointer.any_pressed()) {
            self.history.finish();
        }
        let before = self.settings.clone();
        if !self.busy && self.color_editor.is_none() && !ctx.wants_keyboard_input() {
            let (undo, redo) = ctx.input_mut(|i| {
                let redo = i.consume_key(
                    egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                    egui::Key::Z,
                ) || i.consume_key(egui::Modifiers::COMMAND, egui::Key::Y);
                let undo = !redo && i.consume_key(egui::Modifiers::COMMAND, egui::Key::Z);
                (undo, redo)
            });
            if undo {
                self.history.undo(&mut self.settings);
            }
            if redo {
                self.history.redo(&mut self.settings);
            }
            if self.editor.selected && !self.editor.dragging() {
                let step = if ctx.input(|i| i.modifiers.shift) {
                    10.
                } else {
                    1.
                };
                ctx.input_mut(|i| {
                    if i.consume_key(
                        if i.modifiers.shift {
                            egui::Modifiers::SHIFT
                        } else {
                            egui::Modifiers::NONE
                        },
                        egui::Key::ArrowLeft,
                    ) {
                        self.settings.transform.x -= step;
                    }
                    if i.consume_key(
                        if i.modifiers.shift {
                            egui::Modifiers::SHIFT
                        } else {
                            egui::Modifiers::NONE
                        },
                        egui::Key::ArrowRight,
                    ) {
                        self.settings.transform.x += step;
                    }
                    if i.consume_key(
                        if i.modifiers.shift {
                            egui::Modifiers::SHIFT
                        } else {
                            egui::Modifiers::NONE
                        },
                        egui::Key::ArrowUp,
                    ) {
                        self.settings.transform.y -= step;
                    }
                    if i.consume_key(
                        if i.modifiers.shift {
                            egui::Modifiers::SHIFT
                        } else {
                            egui::Modifiers::NONE
                        },
                        egui::Key::ArrowDown,
                    ) {
                        self.settings.transform.y += step;
                    }
                });
            }
            if !undo && !redo && before != self.settings {
                self.history.record(
                    &before,
                    &self.settings,
                    ctx.input(|i| {
                        i.key_down(egui::Key::ArrowLeft)
                            || i.key_down(egui::Key::ArrowRight)
                            || i.key_down(egui::Key::ArrowUp)
                            || i.key_down(egui::Key::ArrowDown)
                    }),
                );
            }
            if before != self.settings {
                self.settings_changed(&before);
            }
        }
        let before_input = self.input.clone();
        let before_ui = self.settings.clone();
        self.draw(ctx);
        if let Some(editor) = &mut self.color_editor {
            let outcome = editor.show(ctx);
            if let Some(settings) = outcome.settings {
                self.settings = settings;
            }
            if outcome.closed {
                self.color_editor = None;
                self.history.finish();
            }
        }
        let active = ctx.input(|i| {
            i.pointer.any_down()
                || i.key_down(egui::Key::ArrowLeft)
                || i.key_down(egui::Key::ArrowRight)
                || i.key_down(egui::Key::ArrowUp)
                || i.key_down(egui::Key::ArrowDown)
        }) || ctx.wants_keyboard_input();
        if self.history_action != 0 {
            if self.history_action < 0 {
                self.history.undo(&mut self.settings);
            } else {
                self.history.redo(&mut self.settings);
            }
            self.history_action = 0;
        } else if before_input == self.input {
            self.history.record(&before_ui, &self.settings, active);
        }
        if before_ui != self.settings {
            self.settings_changed(&before_ui);
        }
        self.analysis.tick(
            self.input.as_deref(),
            &self.settings,
            !self.busy
                && !self.loading
                && !self.editor.dragging()
                && self.color_editor.is_none()
                && !ctx.input(|i| i.pointer.any_down())
                && self.preview_reload.is_none(),
        );
        if !self.busy
            && !self.loading
            && self.input.is_some()
            && self
                .preview_reload
                .is_some_and(|t| t.elapsed() > Duration::from_millis(400))
        {
            self.load();
        }
        if self.busy
            || self.loading
            || self.checking_update
            || self.playback.is_some()
            || self.analysis.pending()
        {
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
