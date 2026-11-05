use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ColorAdjustments {
    pub exposure: f32,
    pub highlights: f32,
    pub shadows: f32,
    pub temperature: f32,
    pub tint: f32,
    pub saturation: f32,
}
impl Default for ColorAdjustments {
    fn default() -> Self {
        Self {
            exposure: 0.,
            highlights: 0.,
            shadows: 0.,
            temperature: 0.,
            tint: 0.,
            saturation: 1.,
        }
    }
}
impl ColorAdjustments {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.exposure.is_finite()
                && (-4.0..=4.).contains(&self.exposure)
                && self.saturation.is_finite()
                && (0.0..=2.5).contains(&self.saturation)
                && [self.highlights, self.shadows, self.temperature, self.tint]
                    .into_iter()
                    .all(|x| x.is_finite() && (-1.0..=1.).contains(&x)),
            "Ungültige Farbkorrekturwerte"
        );
        Ok(())
    }
    pub(crate) fn filter(&self) -> String {
        let mut f = String::new();
        if self.exposure != 0. {
            let gain = 2_f32.powf(self.exposure);
            f.push_str(&format!(
                ",lutrgb=r='val*{gain}':g='val*{gain}':b='val*{gain}'"
            ));
        }
        if self.shadows != 0. || self.highlights != 0. {
            f.push_str(&format!(
                ",curves=all='0/0 0.25/{} 0.75/{} 1/1'",
                0.25 + self.shadows * 0.16,
                0.75 + self.highlights * 0.16
            ));
        }
        if self.temperature != 0. || self.tint != 0. {
            f.push_str(&format!(
                ",colorbalance=rm={}:gm={}:bm={}:pl=1",
                self.temperature * 0.3 + self.tint * 0.12,
                -self.tint * 0.24,
                -self.temperature * 0.3 + self.tint * 0.12
            ));
        }
        f
    }
}
