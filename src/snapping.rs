use steam_workshop_creator::{Panel, Transform};

/// Distances are measured on screen, so snapping feels the same at every zoom.
pub fn snap(
    transform: &mut Transform,
    source: [f32; 2],
    canvas: [f32; 2],
    panels: &[Panel],
    screen_scale: f32,
    axes: [bool; 2],
) -> [Option<f32>; 2] {
    let corners = transform.corners(source, canvas);
    let mut guides = [None; 2];
    for axis in 0..2 {
        if !axes[axis] {
            continue;
        }
        let min = corners
            .iter()
            .map(|p| p[axis])
            .fold(f32::INFINITY, f32::min);
        let max = corners
            .iter()
            .map(|p| p[axis])
            .fold(f32::NEG_INFINITY, f32::max);
        let anchors = [(min + max) / 2., min, max];
        let mut targets = vec![canvas[axis] / 2., 0., canvas[axis]];
        let mut centers = vec![canvas[axis] / 2.];
        if axis == 0 {
            for panel in panels {
                centers.push(panel.x as f32 + panel.width as f32 / 2.);
                targets.extend([
                    panel.x as f32 + panel.width as f32 / 2.,
                    panel.x as f32,
                    (panel.x + panel.width) as f32,
                ]);
            }
        }
        let mut best = 6. / screen_scale.max(0.0001);
        let mut adjustment = 0.;
        // Prefer center-to-center alignment when cover-fit edges are almost
        // coincident with it; otherwise exact centering would be hard to reach.
        for target in centers {
            let delta = target - anchors[0];
            if delta.abs() < best {
                best = delta.abs();
                adjustment = delta;
                guides[axis] = Some(target);
            }
        }
        let centered = guides[axis];
        if centered.is_none() {
            for target in targets {
                for anchor in anchors {
                    let delta = target - anchor;
                    if delta.abs() < best {
                        best = delta.abs();
                        adjustment = delta;
                        guides[axis] = Some(target);
                    }
                }
            }
        }
        let coordinate = if axis == 0 {
            &mut transform.x
        } else {
            &mut transform.y
        };
        *coordinate = if let Some(target) = centered {
            target - canvas[axis] / 2.
        } else {
            *coordinate + adjustment
        };
    }
    guides
}

#[cfg(test)]
mod tests {
    use super::*;
    use steam_workshop_creator::{Layout, Settings};
    #[test]
    fn centers_edges_and_panel_gaps_snap_in_output_coordinates() {
        let settings = Settings {
            layout: Layout::Strip4,
            panel_width: 100,
            gap: 10,
            height: 200,
            ..Default::default()
        };
        let panels = settings.panels().unwrap();
        let canvas = [settings.canvas_width().unwrap() as f32, 200.];
        for (x, target) in [(1., 0.), (3., 0.), (-163., -165.), (-108., -105.)] {
            let mut t = Transform {
                x,
                y: 4.,
                scale: 0.2,
                ..Default::default()
            };
            let guides = snap(&mut t, canvas, canvas, &panels, 1., [true, true]);
            assert!((t.x - target).abs() < 0.01, "{t:?}");
            assert_eq!(t.y, 0.);
            assert!(guides.iter().all(Option::is_some));
        }
    }
    #[test]
    fn screen_threshold_and_axis_lock_are_respected_even_when_rotated() {
        for scale in [0.25, 1., 4.] {
            let mut t = Transform {
                x: 5. / scale,
                y: 20.,
                rotation: 30.,
                scale: 0.25,
            };
            snap(
                &mut t,
                [400., 200.],
                [400., 200.],
                &[],
                scale,
                [true, false],
            );
            assert_eq!(t.x, 0.);
            assert_eq!(t.y, 20.);
            t.x = 7. / scale;
            snap(
                &mut t,
                [400., 200.],
                [400., 200.],
                &[],
                scale,
                [true, false],
            );
            assert_eq!(t.x, 7. / scale);
        }
    }
    #[test]
    fn centering_wins_over_almost_coincident_cover_fit_edges() {
        let mut t = Transform {
            x: 1.6,
            y: 2.,
            ..Default::default()
        };
        let guides = snap(&mut t, [753., 424.], [750., 424.], &[], 1.25, [true, true]);
        assert_eq!([t.x, t.y], [0., 0.]);
        assert_eq!(guides, [Some(375.), Some(212.)]);
    }
}
