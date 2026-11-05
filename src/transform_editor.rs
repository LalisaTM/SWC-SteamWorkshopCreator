use eframe::egui::{self, Color32, CursorIcon, Pos2, Rect, Stroke, Vec2};
use steam_workshop_creator::{Settings, Transform};

const ACCENT: Color32 = Color32::from_rgb(225, 183, 111);
#[derive(Clone, Copy, Debug)]
enum Handle {
    Move,
    Scale(usize),
    Rotate,
}
#[derive(Clone, Copy)]
struct Drag {
    handle: Handle,
    start: Transform,
    pointer: Pos2,
    corners: [Pos2; 4],
}
pub struct Editor {
    pub selected: bool,
    zoom: f32,
    pan: Vec2,
    panning: bool,
    drag: Option<Drag>,
    snapping: bool,
    guides: [Option<f32>; 2],
}
impl Default for Editor {
    fn default() -> Self {
        Self {
            selected: false,
            zoom: 1.,
            pan: Vec2::ZERO,
            panning: false,
            drag: None,
            snapping: true,
            guides: [None; 2],
        }
    }
}
impl Editor {
    pub fn dragging(&self) -> bool {
        self.drag.is_some()
    }
    pub fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.small_button("−").clicked() {
                self.zoom = (self.zoom / 1.25).max(0.1);
            }
            ui.label(format!("{:.0} %", self.zoom * 100.));
            if ui.small_button("+").clicked() {
                self.zoom = (self.zoom * 1.25).min(8.);
            }
            if ui.small_button("Einpassen").clicked() {
                self.zoom = 1.;
                self.pan = Vec2::ZERO;
            }
            ui.checkbox(&mut self.snapping, "Snapping")
                .on_hover_text("Alt: vorübergehend ausschalten");
        });
    }
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        rect: Rect,
        texture: &egui::TextureHandle,
        source: [f32; 2],
        settings: &mut Settings,
        enabled: bool,
    ) -> bool {
        let Ok(width) = settings.canvas_width() else {
            return false;
        };
        let Ok(panels) = settings.panels() else {
            return false;
        };
        let canvas = [width as f32, settings.height as f32];
        let before = settings.transform;
        let painter = ui.painter_at(rect);
        let hovered = enabled
            && ui.input(|i| i.pointer.hover_pos()).is_some_and(|p| {
                rect.contains(p) && ui.ctx().layer_id_at(p) == Some(ui.layer_id())
            });
        if hovered && self.drag.is_none() {
            let scroll = ui.ctx().input_mut(|i| {
                let y = i.smooth_scroll_delta.y;
                i.smooth_scroll_delta = Vec2::ZERO;
                y
            });
            if scroll != 0.
                && let Some(pointer) = ui.input(|i| i.pointer.hover_pos())
            {
                let zoom = (self.zoom * (scroll * 0.003).exp()).clamp(0.1, 8.);
                self.pan = pointer
                    - rect.center()
                    - (pointer - rect.center() - self.pan) * (zoom / self.zoom);
                self.zoom = zoom;
            }
            if ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Middle)) {
                self.panning = true;
            }
        }
        if self.panning {
            self.pan += ui.input(|i| i.pointer.delta());
            if !ui.input(|i| i.pointer.button_down(egui::PointerButton::Middle)) {
                self.panning = false;
            }
        }
        let base = Transform::default().dimensions(source, canvas);
        let scale = ((rect.width() - 96.) / base[0].max(canvas[0]))
            .min((rect.height() - 96.) / base[1].max(canvas[1]))
            .max(0.0001)
            * self.zoom;
        let area = Rect::from_center_size(
            rect.center() + self.pan,
            Vec2::new(canvas[0], canvas[1]) * scale,
        );
        let origin = area.min;
        let to_screen = |p: [f32; 2]| origin + Vec2::new(p[0], p[1]) * scale;
        let corners = settings.transform.corners(source, canvas).map(to_screen);
        let center = origin
            + Vec2::new(
                canvas[0] / 2. + settings.transform.x,
                canvas[1] / 2. + settings.transform.y,
            ) * scale;
        let top = corners[0].lerp(corners[1], 0.5);
        let rotation = top + (top - center).normalized() * 28.;
        let response = ui.interact(
            rect,
            ui.id().with("transform-canvas"),
            egui::Sense::click_and_drag(),
        );
        let pointer = response
            .interact_pointer_pos()
            .or_else(|| ui.input(|i| i.pointer.hover_pos()));
        let selected = self.selected;
        let current_transform = settings.transform;
        let pick = |p: Pos2| {
            if selected && p.distance(rotation) <= 12. {
                Some(Handle::Rotate)
            } else if selected
                && let Some(index) = corners.iter().position(|c| c.distance(p) <= 11.)
            {
                Some(Handle::Scale(index))
            } else if current_transform.contains(
                [(p.x - origin.x) / scale, (p.y - origin.y) / scale],
                source,
                canvas,
            ) {
                Some(Handle::Move)
            } else {
                None
            }
        };
        let handle = pointer.and_then(pick);
        if enabled {
            if response.hovered()
                && let Some(handle) = handle
            {
                ui.ctx().set_cursor_icon(match handle {
                    Handle::Move => CursorIcon::Grab,
                    Handle::Scale(i) if i % 2 == 0 => CursorIcon::ResizeNwSe,
                    Handle::Scale(_) => CursorIcon::ResizeNeSw,
                    Handle::Rotate => CursorIcon::Crosshair,
                });
            }
            let events = ui.input(|i| i.events.clone());
            for event in events {
                match event {
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        ..
                    } if rect.contains(pos) && ui.ctx().layer_id_at(pos) == Some(ui.layer_id()) => {
                        let handle = pick(pos);
                        self.selected = handle.is_some();
                        self.drag = handle.map(|handle| Drag {
                            handle,
                            start: settings.transform,
                            pointer: pos,
                            corners,
                        });
                    }
                    egui::Event::PointerMoved(pos) => {
                        if let Some(drag) = self.drag {
                            settings.transform = dragged(
                                drag,
                                pos,
                                scale,
                                canvas,
                                origin,
                                ui.input(|i| i.modifiers.shift),
                            );
                            self.snap_drag(drag, pos, scale, source, canvas, &panels, settings, ui);
                        }
                    }
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed: false,
                        ..
                    } => {
                        if let Some(drag) = self.drag {
                            settings.transform = dragged(
                                drag,
                                pos,
                                scale,
                                canvas,
                                origin,
                                ui.input(|i| i.modifiers.shift),
                            );
                            self.snap_drag(drag, pos, scale, source, canvas, &panels, settings, ui);
                        }
                        self.drag = None;
                        self.guides = [None; 2];
                    }
                    _ => {}
                }
            }
            if let Some(drag) = self.drag {
                ui.ctx().set_cursor_icon(match drag.handle {
                    Handle::Move => CursorIcon::Grabbing,
                    Handle::Rotate => CursorIcon::Crosshair,
                    Handle::Scale(i) if i % 2 == 0 => CursorIcon::ResizeNwSe,
                    Handle::Scale(_) => CursorIcon::ResizeNeSw,
                });
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    settings.transform = drag.start;
                    self.drag = None;
                    self.guides = [None; 2];
                }
            } else if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.selected = false;
            }
        } else {
            self.drag = None;
            self.guides = [None; 2];
        }
        painter.rect_filled(area.intersect(rect), 0., Color32::BLACK);
        image(
            &painter.with_clip_rect(area.intersect(rect)),
            texture.id(),
            settings.transform,
            source,
            canvas,
            origin,
            scale,
        );
        painter.rect_stroke(
            area,
            0.,
            Stroke::new(1_f32, Color32::from_gray(90)),
            egui::StrokeKind::Inside,
        );
        for panel in panels.iter().skip(1) {
            let x = area.left() + panel.x as f32 * scale;
            painter.line_segment(
                [egui::pos2(x, area.top()), egui::pos2(x, area.bottom())],
                Stroke::new(3_f32, Color32::from_rgb(15, 18, 23)),
            );
            if settings.gap > 0 {
                painter.rect_filled(
                    Rect::from_min_max(
                        egui::pos2(x - settings.gap as f32 * scale, area.top()),
                        egui::pos2(x, area.bottom()),
                    ),
                    0.,
                    Color32::from_black_alpha(150),
                );
            }
        }
        for (axis, guide) in self.guides.iter().enumerate() {
            if let Some(value) = guide {
                let points = if axis == 0 {
                    let x = origin.x + value * scale;
                    [egui::pos2(x, area.top()), egui::pos2(x, area.bottom())]
                } else {
                    let y = origin.y + value * scale;
                    [egui::pos2(area.left(), y), egui::pos2(area.right(), y)]
                };
                painter.line_segment(points, Stroke::new(1_f32, Color32::from_rgb(85, 210, 226)));
            }
        }
        if self.selected {
            let corners = settings.transform.corners(source, canvas).map(to_screen);
            let center = origin
                + Vec2::new(
                    canvas[0] / 2. + settings.transform.x,
                    canvas[1] / 2. + settings.transform.y,
                ) * scale;
            let top = corners[0].lerp(corners[1], 0.5);
            let rotation = top + (top - center).normalized() * 28.;
            for i in 0..4 {
                painter.line_segment(
                    [corners[i], corners[(i + 1) % 4]],
                    Stroke::new(1.5_f32, ACCENT),
                );
            }
            painter.line_segment([top, rotation], Stroke::new(1_f32, ACCENT));
            painter.circle_filled(rotation, 6., ACCENT);
            painter.circle_stroke(rotation, 6., Stroke::new(1_f32, Color32::BLACK));
            for corner in corners {
                painter.rect_filled(Rect::from_center_size(corner, Vec2::splat(9.)), 1., ACCENT);
            }
            painter.circle_stroke(center, 4., Stroke::new(1_f32, ACCENT));
        }
        before != settings.transform
    }
    #[allow(clippy::too_many_arguments)]
    fn snap_drag(
        &mut self,
        drag: Drag,
        pos: Pos2,
        scale: f32,
        source: [f32; 2],
        canvas: [f32; 2],
        panels: &[steam_workshop_creator::Panel],
        settings: &mut Settings,
        ui: &egui::Ui,
    ) {
        self.guides = [None; 2];
        if self.snapping && matches!(drag.handle, Handle::Move) && !ui.input(|i| i.modifiers.alt) {
            let delta = pos - drag.pointer;
            let axes = if ui.input(|i| i.modifiers.shift) {
                [
                    delta.x.abs() > delta.y.abs(),
                    delta.x.abs() <= delta.y.abs(),
                ]
            } else {
                [true, true]
            };
            self.guides =
                crate::snapping::snap(&mut settings.transform, source, canvas, panels, scale, axes);
        }
    }
}
fn image(
    painter: &egui::Painter,
    texture: egui::TextureId,
    transform: Transform,
    source: [f32; 2],
    canvas: [f32; 2],
    origin: Pos2,
    scale: f32,
) {
    let corners = transform.corners(source, canvas);
    let mut mesh = egui::Mesh::with_texture(texture);
    for (p, uv) in corners
        .into_iter()
        .zip([[0., 0.], [1., 0.], [1., 1.], [0., 1.]])
    {
        mesh.vertices.push(egui::epaint::Vertex {
            pos: origin + Vec2::new(p[0], p[1]) * scale,
            uv: egui::pos2(uv[0], uv[1]),
            color: Color32::WHITE,
        });
    }
    mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
    painter.add(egui::Shape::mesh(mesh));
}
fn dragged(
    drag: Drag,
    pointer: Pos2,
    scale: f32,
    canvas: [f32; 2],
    origin: Pos2,
    shift: bool,
) -> Transform {
    let mut result = drag.start;
    let delta = (pointer - drag.pointer) / scale;
    match drag.handle {
        Handle::Move => {
            let delta = if shift {
                if delta.x.abs() > delta.y.abs() {
                    Vec2::new(delta.x, 0.)
                } else {
                    Vec2::new(0., delta.y)
                }
            } else {
                delta
            };
            result.x += delta.x;
            result.y += delta.y;
        }
        Handle::Scale(index) => {
            let anchor = drag.corners[(index + 2) % 4];
            let diagonal = drag.corners[index] - anchor;
            let target = drag.corners[index] + (pointer - drag.pointer);
            let ratio = ((target - anchor).dot(diagonal) / diagonal.length_sq()).max(0.001);
            result.scale = (drag.start.scale * ratio).clamp(0.05, 4.);
            let center = anchor + diagonal * (result.scale / drag.start.scale) * 0.5;
            let center = (center - origin) / scale;
            result.x = center.x - canvas[0] / 2.;
            result.y = center.y - canvas[1] / 2.;
        }
        Handle::Rotate => {
            let center = origin
                + Vec2::new(canvas[0] / 2. + drag.start.x, canvas[1] / 2. + drag.start.y) * scale;
            let start = drag.pointer - center;
            let end = pointer - center;
            let angle =
                drag.start.rotation + (end.y.atan2(end.x) - start.y.atan2(start.x)).to_degrees();
            result.rotation = Transform::normalize_angle(if shift {
                (angle / 15.).round() * 15.
            } else {
                angle
            });
        }
    }
    result.x = result.x.clamp(-32768., 32768.);
    result.y = result.y.clamp(-32768., 32768.);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pointer_press_move_release_in_one_frame_transforms_media() {
        let ctx = egui::Context::default();
        let texture = ctx.load_texture(
            "pointer-test",
            egui::ColorImage::filled([4, 4], Color32::WHITE),
            Default::default(),
        );
        let mut editor = Editor::default();
        let mut settings = Settings {
            layout: steam_workshop_creator::Layout::Strip4,
            panel_width: 100,
            height: 100,
            ..Default::default()
        };
        let rect = Rect::from_min_size(egui::pos2(100., 100.), Vec2::new(600., 400.));
        let mut run = |events| {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800., 600.))),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        editor.show(ui, rect, &texture, [400., 100.], &mut settings, true);
                    });
                },
            );
        };
        run(vec![]);
        run(vec![
            egui::Event::PointerMoved(egui::pos2(350., 300.)),
            egui::Event::PointerButton {
                pos: egui::pos2(350., 300.),
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            },
            egui::Event::PointerMoved(egui::pos2(400., 325.)),
            egui::Event::PointerButton {
                pos: egui::pos2(400., 325.),
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            },
        ]);
        assert!((settings.transform.x - 50. / 1.26).abs() < 0.01);
        assert!((settings.transform.y - 25. / 1.26).abs() < 0.01);
        assert!(editor.selected);
        assert!(!editor.dragging());
    }
    fn drag(handle: Handle) -> Drag {
        Drag {
            handle,
            start: Transform::default(),
            pointer: egui::pos2(200., 100.),
            corners: [
                egui::pos2(0., 0.),
                egui::pos2(200., 0.),
                egui::pos2(200., 100.),
                egui::pos2(0., 100.),
            ],
        }
    }
    #[test]
    fn corner_scale_keeps_opposite_corner_fixed() {
        let t = dragged(
            drag(Handle::Scale(2)),
            egui::pos2(400., 200.),
            1.,
            [200., 100.],
            Pos2::ZERO,
            false,
        );
        assert_eq!(t.scale, 2.);
        assert_eq!([t.x, t.y], [100., 50.]);
        assert_eq!(t.corners([200., 100.], [200., 100.])[0], [0., 0.]);
    }
    #[test]
    fn movement_uses_output_pixels_and_shift_axis() {
        let t = dragged(
            drag(Handle::Move),
            egui::pos2(220., 105.),
            0.5,
            [200., 100.],
            Pos2::ZERO,
            true,
        );
        assert_eq!([t.x, t.y], [40., 0.]);
    }
}
