use eframe::egui::{self, Color32, Rect, Stroke, Vec2};
use std::{
    path::PathBuf,
    sync::mpsc,
    time::{Duration, Instant},
};
use steam_workshop_creator::{Settings, thumbnail};

pub struct Outcome {
    pub closed: bool,
    pub settings: Option<Settings>,
}
struct Preview {
    texture: egui::TextureHandle,
    histogram: [[u32; 256]; 3],
}
struct Completion {
    revision: u64,
    edited: Result<Vec<u8>, String>,
    original: Option<Result<Vec<u8>, String>>,
}
pub struct ColorEditor {
    input: PathBuf,
    draft: Settings,
    revision: u64,
    due: Option<Instant>,
    active: bool,
    edited: Option<Preview>,
    original: Option<Preview>,
    compare: bool,
    error: Option<String>,
    tx: mpsc::Sender<Completion>,
    rx: mpsc::Receiver<Completion>,
}
impl ColorEditor {
    pub fn new(input: PathBuf, settings: Settings) -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            input,
            draft: settings,
            revision: 0,
            due: Some(Instant::now()),
            active: false,
            edited: None,
            original: None,
            compare: false,
            error: None,
            tx,
            rx,
        }
    }
    fn tick(&mut self, ctx: &egui::Context) {
        while let Ok(completion) = self.rx.try_recv() {
            self.active = false;
            if let Some(original) = completion.original {
                match original.and_then(|bytes| preview(ctx, "color-original", &bytes)) {
                    Ok(image) => self.original = Some(image),
                    Err(error) => self.error = Some(error),
                }
            }
            if completion.revision == self.revision {
                match completion
                    .edited
                    .and_then(|bytes| preview(ctx, "color-edited", &bytes))
                {
                    Ok(image) => {
                        self.edited = Some(image);
                        self.error = None;
                    }
                    Err(error) => self.error = Some(error),
                }
            }
        }
        if !self.active && self.due.is_some_and(|due| due <= Instant::now()) {
            self.due = None;
            self.active = true;
            let input = self.input.clone();
            let draft = self.draft.clone();
            let original = self.original.is_none();
            let revision = self.revision;
            let tx = self.tx.clone();
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                let original = original.then(|| {
                    let mut neutral = draft.clone();
                    reset(&mut neutral);
                    thumbnail(&input, &neutral).map_err(|e| format!("{e:#}"))
                });
                let edited = thumbnail(&input, &draft).map_err(|e| format!("{e:#}"));
                let _ = tx.send(Completion {
                    revision,
                    original,
                    edited,
                });
                ctx.request_repaint();
            });
        }
        if self.active || self.due.is_some() {
            ctx.request_repaint_after(Duration::from_millis(32));
        }
    }
    pub fn show(&mut self, ctx: &egui::Context) -> Outcome {
        self.tick(ctx);
        ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of("color-correction"),
            egui::ViewportBuilder::default()
                .with_title("Farbkorrektur · Workshop Creator")
                .with_inner_size([1120., 780.])
                .with_min_inner_size([850., 580.]),
            |ctx, class| {
                let mut outcome = Outcome {
                    closed: false,
                    settings: None,
                };
                let before = self.draft.clone();
                if class == egui::ViewportClass::Embedded {
                    egui::Modal::new(egui::Id::new("color-modal")).show(ctx, |ui| {
                        ui.set_min_size(Vec2::new(800., 540.));
                        ui.heading("Farbkorrektur");
                        self.contents(ui, &mut outcome);
                    });
                } else {
                    egui::CentralPanel::default()
                        .frame(
                            egui::Frame::new()
                                .inner_margin(20)
                                .fill(Color32::from_rgb(19, 22, 27)),
                        )
                        .show(ctx, |ui| self.contents(ui, &mut outcome));
                }
                if before != self.draft {
                    self.revision += 1;
                    self.due = Some(Instant::now() + Duration::from_millis(220));
                    self.error = None;
                }
                if ctx.input(|i| i.viewport().close_requested())
                    || ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
                {
                    outcome.closed = true;
                    outcome.settings = None;
                }
                outcome
            },
        )
    }
    fn contents(&mut self, ui: &mut egui::Ui, outcome: &mut Outcome) {
        let rect = ui.available_rect_before_wrap();
        let content_bottom = rect.bottom() - 62.;
        let controls_left = rect.right() - 285.;
        let preview_rect =
            Rect::from_min_max(rect.min, egui::pos2(controls_left - 20., content_bottom));
        ui.scope_builder(egui::UiBuilder::new().max_rect(preview_rect), |ui| {
            ui.set_clip_rect(preview_rect.intersect(ui.clip_rect()));
            ui.painter()
                .rect_filled(preview_rect, 4., Color32::from_rgb(10, 12, 16));
            let image = if self.compare {
                self.original.as_ref()
            } else {
                self.edited.as_ref()
            };
            if let Some(image) = image {
                let size = image.texture.size_vec2();
                let available = preview_rect.shrink(16.).size();
                let factor = (available.x / size.x).min(available.y / size.y);
                let target = Rect::from_center_size(preview_rect.center(), size * factor);
                ui.painter().image(
                    image.texture.id(),
                    target,
                    Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
                    Color32::WHITE,
                );
            }
            if self.active || self.due.is_some() {
                ui.put(
                    Rect::from_min_size(preview_rect.min + Vec2::splat(12.), Vec2::splat(24.)),
                    egui::Spinner::new(),
                );
            }
            if let Some(error) = &self.error {
                ui.put(
                    Rect::from_min_size(
                        preview_rect.left_bottom() + Vec2::new(12., -60.),
                        Vec2::new(preview_rect.width() - 24., 48.),
                    ),
                    egui::Label::new(egui::RichText::new(error).color(Color32::LIGHT_RED)).wrap(),
                );
            }
        });
        let controls = Rect::from_min_max(
            egui::pos2(controls_left, rect.top()),
            egui::pos2(rect.right(), content_bottom),
        );
        ui.scope_builder(egui::UiBuilder::new().max_rect(controls), |ui| {
            ui.set_clip_rect(controls.intersect(ui.clip_rect()));
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .id_salt("color-controls")
                .show(ui, |ui| {
                    ui.spacing_mut().slider_width = 175.;
                    let image = if self.compare {
                        self.original.as_ref()
                    } else {
                        self.edited.as_ref()
                    };
                    histogram(ui, image);
                    ui.separator();
                    ui.strong("Weißabgleich");
                    slider(
                        ui,
                        "Temperatur",
                        &mut self.draft.color.temperature,
                        -1.0..=1.,
                    );
                    slider(ui, "Tönung", &mut self.draft.color.tint, -1.0..=1.);
                    ui.separator();
                    ui.strong("Licht");
                    slider(
                        ui,
                        "Belichtung (EV)",
                        &mut self.draft.color.exposure,
                        -4.0..=4.,
                    );
                    slider(ui, "Kontrast", &mut self.draft.contrast, 0.1..=3.);
                    slider(ui, "Lichter", &mut self.draft.color.highlights, -1.0..=1.);
                    slider(ui, "Tiefen", &mut self.draft.color.shadows, -1.0..=1.);
                    slider(ui, "Helligkeit", &mut self.draft.brightness, -1.0..=1.);
                    ui.separator();
                    ui.strong("Farbe");
                    slider(ui, "Sättigung", &mut self.draft.color.saturation, 0.0..=2.5);
                    ui.checkbox(&mut self.draft.monochrome, "Schwarzweiß");
                    ui.checkbox(&mut self.draft.sepia, "Sepia");
                    ui.checkbox(&mut self.draft.invert, "Farben invertieren");
                    ui.add_space(16.);
                });
        });
        let footer = Rect::from_min_max(egui::pos2(rect.left(), content_bottom + 18.), rect.max);
        ui.scope_builder(egui::UiBuilder::new().max_rect(footer), |ui| {
            ui.horizontal(|ui| {
                ui.toggle_value(&mut self.compare, "Original");
                if ui.button("Zurücksetzen").clicked() {
                    reset(&mut self.draft);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add_enabled(
                            self.draft.panels().is_ok(),
                            egui::Button::new("Übernehmen").fill(Color32::from_rgb(85, 69, 46)),
                        )
                        .clicked()
                    {
                        outcome.settings = Some(self.draft.clone());
                        outcome.closed = true;
                    }
                    if ui.button("Abbrechen").clicked() {
                        outcome.closed = true;
                    }
                });
            });
        });
    }
}
fn slider(ui: &mut egui::Ui, label: &str, value: &mut f32, range: std::ops::RangeInclusive<f32>) {
    ui.label(label);
    ui.add(egui::Slider::new(value, range).fixed_decimals(2));
}
fn reset(settings: &mut Settings) {
    settings.color = Default::default();
    settings.brightness = 0.;
    settings.contrast = 1.;
    settings.monochrome = false;
    settings.sepia = false;
    settings.invert = false;
}
fn preview(ctx: &egui::Context, name: &str, bytes: &[u8]) -> Result<Preview, String> {
    let image = image::load_from_memory(bytes)
        .map_err(|e| e.to_string())?
        .to_rgba8();
    let mut histogram = [[0; 256]; 3];
    for p in image.pixels() {
        for channel in 0..3 {
            histogram[channel][p.0[channel] as usize] += 1;
        }
    }
    Ok(Preview {
        texture: ctx.load_texture(
            name,
            egui::ColorImage::from_rgba_unmultiplied(
                [image.width() as usize, image.height() as usize],
                image.as_raw(),
            ),
            Default::default(),
        ),
        histogram,
    })
}
fn histogram(ui: &mut egui::Ui, preview: Option<&Preview>) {
    ui.label("Histogramm");
    let (rect, _) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 72.), egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, 3., Color32::from_rgb(10, 12, 16));
    if let Some(preview) = preview {
        let max = preview
            .histogram
            .iter()
            .flatten()
            .copied()
            .max()
            .unwrap_or(1)
            .max(1) as f32;
        for (values, color) in preview.histogram.iter().zip([
            Color32::from_rgb(220, 102, 108),
            Color32::from_rgb(110, 190, 128),
            Color32::from_rgb(108, 153, 230),
        ]) {
            let points = values
                .iter()
                .enumerate()
                .map(|(x, count)| {
                    egui::pos2(
                        rect.left() + x as f32 / 255. * rect.width(),
                        rect.bottom() - *count as f32 / max * rect.height(),
                    )
                })
                .collect();
            ui.painter()
                .add(egui::Shape::line(points, Stroke::new(1_f32, color)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn editing_and_reset_are_isolated_until_applied() {
        let settings = Settings {
            brightness: 0.2,
            transform: steam_workshop_creator::Transform {
                x: 42.,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut editor = ColorEditor::new(PathBuf::from("sample.png"), settings.clone());
        editor.draft.color.exposure = 2.;
        assert_eq!(settings.color.exposure, 0.);
        reset(&mut editor.draft);
        assert_eq!(editor.draft.transform, settings.transform);
        assert_eq!(editor.draft.effects_filter(), "");
    }
}
