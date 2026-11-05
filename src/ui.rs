use super::*;
use egui::{Color32 as Color, RichText, Stroke, Vec2};

const BG: Color = Color::from_rgb(19, 22, 27);
const SURFACE: Color = Color::from_rgb(27, 31, 37);
const LINE: Color = Color::from_rgb(49, 55, 63);
const MUTED: Color = Color::from_rgb(155, 164, 177);
const TEXT: Color = Color::from_rgb(235, 237, 240);
const ACCENT: Color = Color::from_rgb(225, 183, 111);

pub fn configure_theme(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.visuals = egui::Visuals::dark();
    style.visuals.panel_fill = BG;
    style.visuals.window_fill = SURFACE;
    style.visuals.extreme_bg_color = Color::from_rgb(14, 17, 21);
    style.visuals.faint_bg_color = SURFACE;
    style.visuals.override_text_color = Some(TEXT);
    style.visuals.selection.bg_fill = Color::from_rgb(85, 69, 46);
    style.visuals.selection.stroke = Stroke::new(1.5_f32, ACCENT);
    for widget in [
        &mut style.visuals.widgets.inactive,
        &mut style.visuals.widgets.active,
        &mut style.visuals.widgets.hovered,
    ] {
        widget.corner_radius = 6.into();
        widget.bg_stroke = Stroke::new(1_f32, LINE);
        widget.fg_stroke = Stroke::new(1_f32, TEXT);
    }
    style.visuals.widgets.inactive.bg_fill = Color::from_rgb(38, 43, 51);
    style.visuals.widgets.hovered.bg_fill = Color::from_rgb(53, 58, 66);
    style.visuals.widgets.active.bg_fill = Color::from_rgb(71, 61, 45);
    style.spacing.item_spacing = Vec2::new(10., 10.);
    style.spacing.button_padding = Vec2::new(14., 9.);
    style.spacing.interact_size = Vec2::new(40., 32.);
    style.spacing.slider_width = 165.;
    style
        .text_styles
        .insert(egui::TextStyle::Body, egui::FontId::proportional(15.));
    style
        .text_styles
        .insert(egui::TextStyle::Button, egui::FontId::proportional(14.));
    style
        .text_styles
        .insert(egui::TextStyle::Small, egui::FontId::proportional(12.));
    style
        .text_styles
        .insert(egui::TextStyle::Heading, egui::FontId::proportional(24.));
    ctx.set_style(style);
}

fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(8.);
    ui.horizontal(|ui| {
        ui.label(RichText::new(title).size(17.).strong());
    });
    ui.add_space(4.);
}

fn layout_name(layout: Layout) -> &'static str {
    match layout {
        Layout::Workshop5 => "Workshop",
        Layout::Strip4 => "Vier Panels",
        Layout::Artwork => "Artwork",
        Layout::Featured => "Featured",
    }
}

fn layout_tile(ui: &mut egui::Ui, layout: Layout, selected: bool, width: f32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 94.), egui::Sense::click());
    let painter = ui.painter();
    let fill = if selected {
        Color::from_rgb(53, 46, 36)
    } else if response.hovered() {
        Color::from_rgb(40, 45, 53)
    } else {
        SURFACE
    };
    let color = if selected { ACCENT } else { MUTED };
    painter.rect(
        rect,
        8.,
        fill,
        Stroke::new(
            if selected || response.has_focus() {
                1.5_f32
            } else {
                1_f32
            },
            if selected || response.has_focus() {
                ACCENT
            } else {
                LINE
            },
        ),
        egui::StrokeKind::Inside,
    );
    let widths = layout.widths(150);
    let total = widths.iter().sum::<u32>() as f32;
    let mut x = rect.left() + 14.;
    let width = rect.width() - 28. - (widths.len() - 1) as f32 * 4.;
    for w in widths {
        let w = width * w as f32 / total;
        painter.rect_filled(
            egui::Rect::from_min_size(egui::pos2(x, rect.top() + 14.), Vec2::new(w, 30.)),
            2.,
            color.gamma_multiply(0.65),
        );
        x += w + 4.;
    }
    painter.text(
        rect.left_bottom() + Vec2::new(14., -23.),
        egui::Align2::LEFT_CENTER,
        layout_name(layout),
        egui::FontId::proportional(14.),
        if selected { TEXT } else { MUTED },
    );
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::SelectableLabel,
            true,
            selected,
            layout_name(layout),
        )
    });
    response
}

impl App {
    fn choose_input(&mut self) {
        if let Some(paths) = rfd::FileDialog::new()
            .add_filter(
                "Medien",
                &[
                    "mp4", "mov", "mkv", "avi", "webm", "gif", "png", "jpg", "jpeg", "webp",
                ],
            )
            .pick_files()
        {
            self.import_files(paths);
        }
    }

    pub(super) fn draw(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("header")
            .frame(
                egui::Frame::new()
                    .fill(BG)
                    .inner_margin(egui::Margin::symmetric(28, 20)),
            )
            .show(ctx, |ui| {
                if self.color_editor.is_some() {
                    ui.disable();
                }
                ui.horizontal(|ui| {
                    let (rect, _) = ui.allocate_exact_size(Vec2::splat(36.), egui::Sense::hover());
                    ui.painter().image(
                        self.brand_icon.id(),
                        rect,
                        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
                        Color::WHITE,
                    );
                    ui.add_space(8.);
                    ui.vertical(|ui| {
                        ui.label(RichText::new("Workshop Creator").size(23.).strong());
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if self.available_update.is_some() {
                            if ui
                                .add_enabled(
                                    !self.busy && !self.loading,
                                    egui::Button::new("Update installieren & neu starten"),
                                )
                                .clicked()
                            {
                                self.install_update();
                            }
                        } else if ui
                            .add_enabled(
                                !self.checking_update && !self.busy,
                                egui::Button::new("Nach Updates suchen"),
                            )
                            .clicked()
                        {
                            self.check_updates();
                        }
                        ui.label(
                            RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                                .size(12.)
                                .color(MUTED),
                        )
                        .on_hover_text(&self.update_status);
                        if self.available_update.is_some() {
                            ui.label(RichText::new(&self.update_status).size(12.).color(ACCENT));
                        }
                    });
                });
            });

        egui::TopBottomPanel::bottom("export_bar")
            .frame(
                egui::Frame::new()
                    .fill(SURFACE)
                    .inner_margin(egui::Margin::symmetric(28, 18)),
            )
            .show(ctx, |ui| {
                if self.color_editor.is_some() {
                    ui.disable();
                }
                ui.horizontal(|ui| {
                    ui.allocate_ui_with_layout(
                        Vec2::new((ui.available_width() - 440.).max(180.), 58.),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.label(RichText::new("Ausgabeordner").size(12.).color(MUTED));
                            let path = self.output.display().to_string();
                            if ui
                                .add_enabled(
                                    !self.busy,
                                    egui::Button::new(RichText::new(&path).size(14.)).truncate(),
                                )
                                .on_hover_text(&path)
                                .clicked()
                                && let Some(path) = rfd::FileDialog::new().pick_folder()
                            {
                                self.output = path;
                            }
                        },
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if self.busy {
                            if ui
                                .add_enabled(
                                    !self.installing_update,
                                    egui::Button::new("Abbrechen"),
                                )
                                .clicked()
                            {
                                self.cancel.store(true, Ordering::Relaxed);
                            }
                            ui.add(
                                egui::ProgressBar::new(self.progress)
                                    .desired_width(200.)
                                    .fill(ACCENT)
                                    .show_percentage(),
                            );
                        } else {
                            let valid = self.settings.panels().is_ok();
                            let count =
                                self.settings.layout.widths(self.settings.panel_width).len();
                            let button = egui::Button::new(
                                RichText::new(format!("{count} Panels exportieren  →"))
                                    .color(BG)
                                    .strong(),
                            )
                            .fill(ACCENT)
                            .min_size(Vec2::new(205., 44.));
                            if ui
                                .add_enabled(self.input.is_some() && !self.loading && valid, button)
                                .clicked()
                            {
                                self.generate();
                            }
                            ui.label(
                                RichText::new(if self.settings.format == Format::Gif {
                                    "GIF"
                                } else {
                                    "PNG"
                                })
                                .color(MUTED)
                                .monospace(),
                            );
                        }
                    });
                });
            });

        egui::SidePanel::left("media_pool")
            .default_width(240.)
            .width_range(190.0..=320.0)
            .frame(egui::Frame::new().fill(SURFACE).inner_margin(16))
            .show(ctx, |ui| {
                if self.color_editor.is_some() {
                    ui.disable();
                }
                ui.heading("Medienpool");
                ui.label(
                    RichText::new(format!("{} Medien", self.pool.len()))
                        .size(12.)
                        .color(MUTED),
                );
                if ui
                    .add_enabled(
                        !self.busy && !self.loading,
                        egui::Button::new("+ Medien importieren"),
                    )
                    .clicked()
                {
                    self.choose_input();
                }
                ui.separator();
                let mut selected = None;
                egui::ScrollArea::vertical()
                    .id_salt("pool_scroll")
                    .show(ui, |ui| {
                        for item in &self.pool {
                            let active = self.input.as_ref() == Some(&item.path);
                            egui::Frame::new()
                                .fill(if active {
                                    Color::from_rgb(53, 46, 36)
                                } else {
                                    BG
                                })
                                .corner_radius(6)
                                .inner_margin(10)
                                .show(ui, |ui| {
                                    if let Some(texture) = &item.thumbnail
                                        && ui
                                            .add(
                                                egui::Image::new(texture)
                                                    .fit_to_exact_size(Vec2::new(
                                                        ui.available_width(),
                                                        90.,
                                                    ))
                                                    .sense(egui::Sense::click()),
                                            )
                                            .clicked()
                                        && !self.busy
                                        && !self.loading
                                    {
                                        selected = Some(item.path.clone());
                                    }
                                    let title =
                                        item.path.file_name().unwrap_or_default().to_string_lossy();
                                    if ui
                                        .add_enabled(
                                            !self.busy && !self.loading,
                                            egui::Button::selectable(active, title).truncate(),
                                        )
                                        .on_hover_text(item.path.display().to_string())
                                        .clicked()
                                    {
                                        selected = Some(item.path.clone());
                                    }
                                    if let Some(media) = &item.media {
                                        ui.label(
                                            RichText::new(format!(
                                                "{} × {} · {}",
                                                media.width,
                                                media.height,
                                                media
                                                    .duration
                                                    .map(|d| format!("{d:.1} s"))
                                                    .unwrap_or("Bild".into())
                                            ))
                                            .size(11.)
                                            .color(MUTED),
                                        );
                                    }
                                    if let Some(error) = &item.error {
                                        ui.colored_label(Color::LIGHT_RED, error);
                                    }
                                });
                        }
                    });
                if let Some(path) = selected {
                    self.select_media(path);
                }
            });
        egui::SidePanel::right("inspector")
            .exact_width(350.)
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(BG)
                    .inner_margin(egui::Margin::symmetric(24, 12)),
            )
            .show(ctx, |ui| {
                if self.color_editor.is_some() {
                    ui.disable();
                }
                egui::ScrollArea::vertical()
                    .id_salt("settings_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.spacing_mut().slider_width = 160.;
                        ui.spacing_mut().interact_size.x = 70.;
                        ui.add_enabled_ui(!self.busy && !self.loading, |ui| {
                            section(ui, "Layout");
                            // Both columns receive a fixed width so the hit areas stay equal.
                            let tile_width = (ui.available_width() - 10.) / 2.;
                            for row in [
                                [Layout::Workshop5, Layout::Strip4],
                                [Layout::Artwork, Layout::Featured],
                            ] {
                                ui.horizontal(|ui| {
                                    for layout in row {
                                        if layout_tile(
                                            ui,
                                            layout,
                                            self.settings.layout == layout,
                                            tile_width,
                                        )
                                        .clicked()
                                        {
                                            self.settings.layout = layout;
                                        }
                                    }
                                });
                            }
                            ui.add_space(4.);
                            egui::Grid::new("dimensions")
                                .num_columns(2)
                                .min_col_width(110.)
                                .spacing([8., 10.])
                                .show(ui, |ui| {
                                    if matches!(
                                        self.settings.layout,
                                        Layout::Workshop5 | Layout::Strip4
                                    ) {
                                        ui.label(RichText::new("Panelbreite").color(MUTED));
                                        ui.add(
                                            egui::DragValue::new(&mut self.settings.panel_width)
                                                .range(1..=4096)
                                                .suffix(" px"),
                                        );
                                        ui.end_row();
                                    }
                                    ui.label(RichText::new("Höhe").color(MUTED));
                                    ui.add(
                                        egui::DragValue::new(&mut self.settings.height)
                                            .range(1..=8192)
                                            .suffix(" px"),
                                    );
                                    ui.end_row();
                                });
                            ui.add_space(8.);
                            ui.separator();
                            section(ui, "Transformieren");
                            let transform_before = self.settings.transform;
                            egui::Grid::new("transform_fields")
                                .min_col_width(110.)
                                .spacing([8., 8.])
                                .show(ui, |ui| {
                                    ui.label("Position X");
                                    ui.add(
                                        egui::DragValue::new(&mut self.settings.transform.x)
                                            .range(-32768.0..=32768.)
                                            .speed(1.)
                                            .suffix(" px"),
                                    );
                                    ui.end_row();
                                    ui.label("Position Y");
                                    ui.add(
                                        egui::DragValue::new(&mut self.settings.transform.y)
                                            .range(-32768.0..=32768.)
                                            .speed(1.)
                                            .suffix(" px"),
                                    );
                                    ui.end_row();
                                    ui.label("Skalierung");
                                    let mut percent = self.settings.transform.scale * 100.;
                                    if ui
                                        .add(
                                            egui::DragValue::new(&mut percent)
                                                .range(5.0..=400.)
                                                .speed(0.5)
                                                .suffix(" %"),
                                        )
                                        .changed()
                                    {
                                        self.settings.transform.scale = percent / 100.;
                                    }
                                    ui.end_row();
                                    ui.label("Drehung");
                                    ui.add(
                                        egui::DragValue::new(&mut self.settings.transform.rotation)
                                            .range(-180.0..=180.)
                                            .speed(0.5)
                                            .suffix("°"),
                                    );
                                    ui.end_row();
                                });
                            if ui.button("Transformation zurücksetzen").clicked() {
                                self.settings.transform = Transform::default();
                            }
                            if transform_before != self.settings.transform {
                                self.editor.selected = true;
                            }
                            ui.add_space(8.);
                            ui.separator();
                            section(ui, "Format & Ausschnitt");
                            ui.horizontal_wrapped(|ui| {
                                ui.selectable_value(
                                    &mut self.settings.format,
                                    Format::Gif,
                                    "Animiertes GIF",
                                );
                                ui.selectable_value(
                                    &mut self.settings.format,
                                    Format::Png,
                                    "Standbild · PNG",
                                );
                            });
                            ui.add_space(4.);
                            egui::Grid::new("timing")
                                .min_col_width(110.)
                                .spacing([8., 10.])
                                .show(ui, |ui| {
                                    ui.label(RichText::new("Startzeit").color(MUTED));
                                    ui.add(
                                        egui::DragValue::new(&mut self.settings.start)
                                            .range(0.0..=86400.)
                                            .speed(0.1)
                                            .suffix(" s"),
                                    );
                                    ui.end_row();
                                    if self.settings.format == Format::Gif {
                                        ui.label(RichText::new("Dauer").color(MUTED));
                                        ui.add(
                                            egui::DragValue::new(&mut self.settings.duration)
                                                .range(0.1..=600.)
                                                .speed(0.1)
                                                .suffix(" s"),
                                        );
                                        ui.end_row();
                                    }
                                });
                            if self.settings.format == Format::Gif {
                                ui.add_space(8.);
                                ui.separator();
                                section(ui, "Qualität");
                                ui.label(
                                    RichText::new("Bilder pro Sekunde").size(12.).color(MUTED),
                                );
                                ui.add(
                                    egui::Slider::new(&mut self.settings.fps, 1..=50)
                                        .suffix(" fps"),
                                );
                                ui.label(
                                    RichText::new("Farben in der Palette")
                                        .size(12.)
                                        .color(MUTED),
                                );
                                ui.add(egui::Slider::new(&mut self.settings.colors, 4..=256));
                            }
                            ui.add_space(8.);
                            ui.separator();
                            section(ui, "Effekte");
                            if ui
                                .add_enabled(
                                    self.input.is_some(),
                                    egui::Button::new("Farbkorrektur …"),
                                )
                                .clicked()
                                && let Some(input) = &self.input
                            {
                                self.history.finish();
                                self.color_editor = Some(color_editor::ColorEditor::new(
                                    input.clone(),
                                    self.settings.clone(),
                                ));
                            }
                            if self.settings.format == Format::Gif {
                                ui.separator();
                                section(ui, "Kompression");
                                ui.horizontal_wrapped(|ui| {
                                    if ui.button("Klein").clicked() {
                                        self.settings.fps = 10;
                                        self.settings.colors = 32;
                                        self.settings.dither = Dither::Bayer;
                                    }
                                    if ui.button("Balance").clicked() {
                                        self.settings.fps = 15;
                                        self.settings.colors = 128;
                                        self.settings.dither = Dither::Sierra;
                                    }
                                    if ui.button("Qualität").clicked() {
                                        self.settings.fps = 24;
                                        self.settings.colors = 256;
                                        self.settings.dither = Dither::Sierra;
                                    }
                                });
                                ui.checkbox(
                                    &mut self.settings.auto_compress,
                                    "Automatisch auf Zielgröße komprimieren",
                                );
                                if self.settings.auto_compress {
                                    ui.add(
                                        egui::DragValue::new(&mut self.settings.target_mb)
                                            .range(0.01..=100.)
                                            .speed(0.1)
                                            .suffix(" MB pro Panel"),
                                    );
                                }
                            }
                            ui.add_space(4.);
                            ui.collapsing("Erweiterte Einstellungen", |ui| {
                                ui.label(
                                    RichText::new("Ausgelassene Pixel zwischen Panels")
                                        .size(12.)
                                        .color(MUTED),
                                );
                                ui.add(
                                    egui::DragValue::new(&mut self.settings.gap)
                                        .range(0..=128)
                                        .suffix(" px"),
                                );
                                if self.settings.format == Format::Gif {
                                    ui.label("Dithering");
                                    egui::ComboBox::from_id_salt("dither")
                                        .selected_text(format!("{:?}", self.settings.dither))
                                        .show_ui(ui, |ui| {
                                            for d in [
                                                Dither::Sierra,
                                                Dither::Bayer,
                                                Dither::None,
                                                Dither::Floyd,
                                            ] {
                                                ui.selectable_value(
                                                    &mut self.settings.dither,
                                                    d,
                                                    format!("{d:?}"),
                                                );
                                            }
                                        });
                                    ui.checkbox(
                                        &mut self.settings.workshop_patch,
                                        "Workshop-Byte-Patch",
                                    );
                                }
                            });
                            if let Err(error) = self.settings.panels() {
                                ui.colored_label(Color::LIGHT_RED, error.to_string());
                            }
                            ui.add_space(12.);
                            ui.collapsing("Hilfe zum Steam-Upload", |ui| {
                                ui.label(
                                    RichText::new(core::upload_notes(&self.settings))
                                        .size(12.)
                                        .color(MUTED),
                                );
                            });
                            ui.add_space(16.);
                        });
                    });
            });
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(Color::from_rgb(15, 18, 23))
                    .inner_margin(24),
            )
            .show(ctx, |ui| {
                if self.color_editor.is_some() {
                    ui.disable();
                }
                ui.horizontal_wrapped(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if self.input.is_some()
                            && ui
                                .add_enabled(
                                    !self.busy && !self.loading,
                                    egui::Button::new("Datei wechseln"),
                                )
                                .clicked()
                        {
                            self.choose_input();
                        }
                    });
                });
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .add_enabled(
                            !self.busy && self.history.can_undo(),
                            egui::Button::new("Rückgängig"),
                        )
                        .on_hover_text("Strg+Z")
                        .clicked()
                    {
                        self.history_action = -1;
                    }
                    if ui
                        .add_enabled(
                            !self.busy && self.history.can_redo(),
                            egui::Button::new("Wiederholen"),
                        )
                        .on_hover_text("Strg+Y / Strg+Shift+Z")
                        .clicked()
                    {
                        self.history_action = 1;
                    }
                    self.editor.toolbar(ui);
                });
                ui.add_space(6.);
                let available = ui.available_rect_before_wrap();
                let (viewport, footer) = preview_regions(available);
                let height = viewport.height();
                let (rect, _response) = ui.allocate_exact_size(
                    Vec2::new(ui.available_width(), height),
                    egui::Sense::hover(),
                );
                let painter = ui.painter_at(rect);
                painter.rect_filled(rect, 12., BG);
                // A restrained dot grid gives the empty canvas a visible boundary.
                for x in (12..rect.width() as usize).step_by(24) {
                    for y in (12..rect.height() as usize).step_by(24) {
                        painter.circle_filled(
                            rect.min + Vec2::new(x as f32, y as f32),
                            0.7,
                            LINE.gamma_multiply(0.6),
                        );
                    }
                }
                if let (Some(texture), Some(media)) = (&self.texture, &self.media) {
                    self.editor.show(
                        ui,
                        rect,
                        texture,
                        [media.width as f32, media.height as f32],
                        &mut self.settings,
                        !self.busy && !self.loading && self.color_editor.is_none(),
                    );
                } else {
                    ui.scope_builder(
                        egui::UiBuilder::new().max_rect(egui::Rect::from_center_size(
                            rect.center(),
                            Vec2::new(rect.width().min(380.), 220.),
                        )),
                        |ui| {
                            ui.vertical_centered(|ui| {
                                ui.add_space(35.);
                                if self.loading {
                                    ui.spinner();
                                    ui.label("Vorschau wird vorbereitet …");
                                } else if ui
                                    .add_enabled(
                                        !self.busy,
                                        egui::Button::new(
                                            RichText::new("Datei auswählen  +").color(BG).strong(),
                                        )
                                        .fill(ACCENT)
                                        .min_size(Vec2::new(190., 44.)),
                                    )
                                    .clicked()
                                {
                                    self.choose_input();
                                }
                            });
                        },
                    );
                }
                ui.scope_builder(egui::UiBuilder::new().max_rect(footer), |ui| {
                    ui.set_clip_rect(footer.intersect(ui.clip_rect()));
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(
                                self.playback.is_some(),
                                egui::Button::new(if self.playing { "Pause" } else { "Abspielen" }),
                            )
                            .clicked()
                        {
                            self.playing = !self.playing;
                        }
                        if let Some(path) = &self.input {
                            let label_width = (ui.available_width() - 100.).max(0.);
                            ui.add_sized(
                                [label_width, ui.spacing().interact_size.y],
                                egui::Label::new(
                                    RichText::new(
                                        path.file_name().unwrap_or_default().to_string_lossy(),
                                    )
                                    .strong(),
                                )
                                .truncate(),
                            )
                            .on_hover_text(path.display().to_string());
                        }

                        if let Some(media) = &self.media {
                            ui.label(
                                RichText::new(format!("{} × {}", media.width, media.height))
                                    .size(12.)
                                    .color(MUTED)
                                    .monospace(),
                            );
                        }
                    });
                    egui::ScrollArea::vertical()
                        .id_salt("results")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            if self.status_error {
                                ui.colored_label(Color::LIGHT_RED, &self.status);
                            }
                            if let Some(files) = self
                                .analysis
                                .files
                                .as_ref()
                                .or(self.result.as_ref().map(|r| &r.files))
                            {
                                ui.horizontal_wrapped(|ui| {
                                    for file in files {
                                        ui.label(
                                            RichText::new(format!(
                                                "{} · {:.3} MB{}",
                                                file.name,
                                                file.bytes as f64 / 1_000_000.,
                                                if file.over_budget {
                                                    " · über 5 MB"
                                                } else {
                                                    ""
                                                }
                                            ))
                                            .size(12.)
                                            .monospace()
                                            .color(
                                                if file.over_budget {
                                                    Color::LIGHT_RED
                                                } else {
                                                    ACCENT
                                                },
                                            ),
                                        );
                                    }
                                });
                            } else if self.analysis.pending() && self.input.is_some() {
                                ui.horizontal(|ui| {
                                    ui.spinner();
                                    ui.label(RichText::new("Dateigrößen …").size(12.).color(MUTED));
                                });
                            } else if let Some(error) = &self.analysis.error {
                                ui.label(
                                    RichText::new("Größe nicht verfügbar")
                                        .size(12.)
                                        .color(MUTED),
                                )
                                .on_hover_text(error);
                            }
                        });
                });
            });
    }
}

fn preview_regions(available: egui::Rect) -> (egui::Rect, egui::Rect) {
    let footer_height = 112_f32.min(available.height() * 0.5);
    let split = available.bottom() - footer_height;
    (
        egui::Rect::from_min_max(
            available.min,
            egui::pos2(available.right(), (split - 10.).max(available.top())),
        ),
        egui::Rect::from_min_max(egui::pos2(available.left(), split), available.max),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preview_and_status_stay_above_export_bar_at_small_and_large_sizes() {
        for height in [180., 400., 740., 1080.] {
            let available =
                egui::Rect::from_min_size(egui::pos2(264., 180.), Vec2::new(500., height));
            let (preview, status) = preview_regions(available);
            assert!(available.contains_rect(preview));
            assert!(available.contains_rect(status));
            assert!(preview.bottom() < status.top());
            assert!(status.height() >= 90.);
        }
    }
}
