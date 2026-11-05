use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Transform {
    pub x: f32,
    pub y: f32,
    pub scale: f32,
    pub rotation: f32,
}
impl Default for Transform {
    fn default() -> Self {
        Self {
            x: 0.,
            y: 0.,
            scale: 1.,
            rotation: 0.,
        }
    }
}
impl Transform {
    pub fn validate(self) -> Result<()> {
        ensure!(
            [self.x, self.y, self.scale, self.rotation]
                .iter()
                .all(|v| v.is_finite()),
            "Transformation muss endliche Werte enthalten"
        );
        ensure!(
            (0.05..=4.).contains(&self.scale),
            "Skalierung muss zwischen 5 und 400 % liegen"
        );
        ensure!(
            self.rotation.abs() <= 180.,
            "Drehung muss zwischen −180 und 180 Grad liegen"
        );
        ensure!(
            self.x.abs() <= 32768. && self.y.abs() <= 32768.,
            "Position außerhalb des unterstützten Bereichs"
        );
        Ok(())
    }
    pub fn dimensions(self, source: [f32; 2], canvas: [f32; 2]) -> [f32; 2] {
        let fit = (canvas[0] / source[0]).max(canvas[1] / source[1]);
        [
            (source[0] * fit * self.scale).round().max(1.),
            (source[1] * fit * self.scale).round().max(1.),
        ]
    }
    /// Top-left, top-right, bottom-right, bottom-left; output canvas coordinates.
    pub fn corners(self, source: [f32; 2], canvas: [f32; 2]) -> [[f32; 2]; 4] {
        let size = self.dimensions(source, canvas);
        let (sin, cos) = self.rotation.to_radians().sin_cos();
        [[-0.5, -0.5], [0.5, -0.5], [0.5, 0.5], [-0.5, 0.5]].map(|p| {
            let x = p[0] * size[0];
            let y = p[1] * size[1];
            [
                canvas[0] / 2. + self.x + x * cos - y * sin,
                canvas[1] / 2. + self.y + x * sin + y * cos,
            ]
        })
    }
    pub fn contains(self, point: [f32; 2], source: [f32; 2], canvas: [f32; 2]) -> bool {
        let x = point[0] - canvas[0] / 2. - self.x;
        let y = point[1] - canvas[1] / 2. - self.y;
        let (sin, cos) = self.rotation.to_radians().sin_cos();
        let size = self.dimensions(source, canvas);
        (x * cos + y * sin).abs() <= size[0] / 2. && (-x * sin + y * cos).abs() <= size[1] / 2.
    }
    pub fn normalize_angle(degrees: f32) -> f32 {
        (degrees + 180.).rem_euclid(360.) - 180.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rotates_about_media_center() {
        let t = Transform {
            x: 12.,
            y: -5.,
            rotation: 90.,
            ..Default::default()
        };
        let points = t.corners([200., 100.], [200., 100.]);
        assert!((points[0][0] - 162.).abs() < 0.001);
        assert!((points[0][1] + 55.).abs() < 0.001);
        assert!(t.contains([112., 45.], [200., 100.], [200., 100.]));
        assert!(!t.contains([0., 0.], [200., 100.], [200., 100.]));
    }
    #[test]
    fn defaults_load_from_older_settings() {
        let t: Transform = serde_json::from_str("{}").unwrap();
        assert_eq!(t, Transform::default());
        assert!(
            Transform {
                scale: f32::NAN,
                ..t
            }
            .validate()
            .is_err()
        );
    }
}
