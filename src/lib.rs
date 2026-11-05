use anyhow::{Context, Result, bail, ensure};
#[cfg(windows)]
mod bundled_tools;
pub mod color;
pub mod transform;
pub mod updater;
use clap::ValueEnum;
pub use color::ColorAdjustments;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};
pub use transform::Transform;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
pub enum Layout {
    Workshop5,
    Strip4,
    Artwork,
    Featured,
}
impl Layout {
    pub const ALL: [Self; 4] = [Self::Workshop5, Self::Strip4, Self::Artwork, Self::Featured];
    pub fn label(self) -> &'static str {
        match self {
            Self::Workshop5 => "Workshop · 5 Panels",
            Self::Strip4 => "4 Panels · nebeneinander",
            Self::Artwork => "Artwork · 506 + 100",
            Self::Featured => "Featured Artwork · 630",
        }
    }
    pub fn widths(self, panel_width: u32) -> Vec<u32> {
        match self {
            Self::Workshop5 => vec![panel_width; 5],
            Self::Strip4 => vec![panel_width; 4],
            Self::Artwork => vec![506, 100],
            Self::Featured => vec![630],
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
pub enum Format {
    Gif,
    Png,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
pub enum Dither {
    Sierra,
    Bayer,
    None,
    Floyd,
}
impl Dither {
    fn filter(self) -> &'static str {
        match self {
            Self::Sierra => "sierra2_4a",
            Self::Bayer => "bayer:bayer_scale=3",
            Self::None => "none",
            Self::Floyd => "floyd_steinberg",
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub color: ColorAdjustments,
    pub transform: Transform,
    pub layout: Layout,
    pub format: Format,
    pub panel_width: u32,
    pub height: u32,
    pub gap: u32,
    pub start: f64,
    pub duration: f64,
    pub fps: u32,
    pub colors: u32,
    pub dither: Dither,
    pub monochrome: bool,
    pub workshop_patch: bool,
    pub invert: bool,
    pub sepia: bool,
    pub brightness: f32,
    pub contrast: f32,
    pub auto_compress: bool,
    pub target_mb: f64,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            color: ColorAdjustments::default(),
            transform: Transform::default(),
            layout: Layout::Workshop5,
            format: Format::Gif,
            panel_width: 150,
            height: 600,
            gap: 0,
            start: 0.,
            duration: 3.,
            fps: 20,
            colors: 128,
            dither: Dither::Sierra,
            monochrome: false,
            workshop_patch: false,
            invert: false,
            sepia: false,
            brightness: 0.,
            contrast: 1.,
            auto_compress: false,
            target_mb: 5.,
        }
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct Panel {
    pub x: u32,
    pub width: u32,
    pub height: u32,
}
impl Settings {
    pub fn panels(&self) -> Result<Vec<Panel>> {
        self.transform.validate()?;
        self.color.validate()?;
        ensure!(
            (1..=4096).contains(&self.panel_width) && (1..=8192).contains(&self.height),
            "Breite: 1–4096, Höhe: 1–8192 erforderlich"
        );
        ensure!(self.gap <= 128, "Abstand darf höchstens 128 Pixel betragen");
        ensure!(
            self.start.is_finite()
                && self.start >= 0.
                && self.duration.is_finite()
                && self.duration > 0.
                && self.duration <= 600.,
            "Ungültiger Zeitraum (max. 600 Sekunden)"
        );
        ensure!(
            (1..=50).contains(&self.fps) && (4..=256).contains(&self.colors),
            "FPS: 1–50, Farben: 4–256 erforderlich"
        );
        ensure!(
            self.brightness.is_finite()
                && (-1.0..=1.0).contains(&self.brightness)
                && self.contrast.is_finite()
                && (0.1..=3.).contains(&self.contrast),
            "Ungültige Effektwerte"
        );
        ensure!(
            self.target_mb.is_finite() && (0.01..=100.).contains(&self.target_mb),
            "Zielgröße: 0.01–100 MB"
        );
        let mut x = 0;
        Ok(self
            .layout
            .widths(self.panel_width)
            .into_iter()
            .map(|width| {
                let p = Panel {
                    x,
                    width,
                    height: self.height,
                };
                x += width + self.gap;
                p
            })
            .collect())
    }
    pub fn canvas_width(&self) -> Result<u32> {
        let p = self.panels()?;
        let p = p.last().unwrap();
        Ok(p.x + p.width)
    }
    pub fn effects_filter(&self) -> String {
        let mut f = self.color.filter();
        if self.brightness != 0. || self.contrast != 1. || self.color.saturation != 1. {
            f.push_str(&format!(
                ",eq=brightness={}:contrast={}:saturation={}",
                self.brightness, self.contrast, self.color.saturation
            ));
        }
        if self.sepia {
            f.push_str(",colorchannelmixer=.393:.769:.189:0:.349:.686:.168:0:.272:.534:.131");
        }
        if self.monochrome {
            f.push_str(",hue=s=0");
        }
        if self.invert {
            f.push_str(",negate");
        }
        f
    }
    fn base_filter(&self, media: &Media) -> Result<String> {
        let w = self.canvas_width()?;
        let h = self.height;
        let size = self.transform.dimensions(
            [media.width as f32, media.height as f32],
            [w as f32, h as f32],
        );
        let angle = self.transform.rotation.to_radians();
        let (sin, cos) = angle.sin_cos();
        let rw = (size[0] * cos.abs() + size[1] * sin.abs()).ceil();
        let rh = (size[0] * sin.abs() + size[1] * cos.abs()).ceil();
        let pw = rw.max(w as f32) + 2. * self.transform.x.abs().ceil();
        let ph = rh.max(h as f32) + 2. * self.transform.y.abs().ceil();
        ensure!(
            pw <= 32768. && ph <= 32768. && pw * ph <= 64_000_000.,
            "Transformation benötigt zu viel Bildspeicher. Größe, Skalierung oder Abstand reduzieren."
        );
        Ok(format!(
            "scale={}:{}:flags=lanczos,format=rgba,rotate={angle}:ow={rw}:oh={rh}:c=black,pad={pw}:{ph}:(ow-iw)/2+{}:(oh-ih)/2+{}:color=black,crop={w}:{h}:(iw-{w})/2:(ih-{h})/2,setsar=1",
            size[0], size[1], self.transform.x, self.transform.y
        ))
    }
}
#[derive(Debug, Clone, Deserialize)]
pub struct Media {
    pub width: u32,
    pub height: u32,
    pub duration: Option<f64>,
}
fn command(program: &str) -> Result<Command> {
    #[cfg(windows)]
    let program = bundled_tools::executable(program)?;
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    Ok(cmd)
}
pub fn probe(input: &Path) -> Result<Media> {
    let out = command("ffprobe")?
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height:format=duration",
            "-of",
            "json",
        ])
        .arg(input)
        .output()
        .context("Der Medienprozessor ffprobe konnte nicht gestartet werden")?;
    ensure!(
        out.status.success(),
        "ffprobe: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout)?;
    let s = &v["streams"][0];
    Ok(Media {
        width: s["width"].as_u64().context("Keine Videospur gefunden")? as u32,
        height: s["height"].as_u64().context("Höhe fehlt")? as u32,
        duration: v["format"]["duration"]
            .as_str()
            .and_then(|s| s.parse().ok()),
    })
}
pub fn thumbnail(input: &Path, settings: &Settings) -> Result<Vec<u8>> {
    let media = probe(input)?;
    let out = command("ffmpeg")?
        .args(["-v", "error", "-ss", &settings.start.to_string(), "-i"])
        .arg(input)
        .args([
            "-vf",
            &format!(
                "format=rgba{},{},scale=800:800:force_original_aspect_ratio=decrease",
                settings.effects_filter(),
                settings.base_filter(&media)?
            ),
            "-frames:v",
            "1",
            "-f",
            "image2pipe",
            "-vcodec",
            "png",
            "pipe:1",
        ])
        .output()
        .context("Der Medienprozessor FFmpeg konnte nicht gestartet werden")?;
    ensure!(
        out.status.success(),
        "Vorschau: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    Ok(out.stdout)
}
pub type Cancel = Arc<AtomicBool>;
/// Bounded-memory decoder. The callback controls pacing and can stop playback.
pub fn preview_stream(
    input: &Path,
    settings: &Settings,
    cancel: &Cancel,
    frame: impl FnMut(Vec<u8>, [usize; 2]) -> bool,
) -> Result<()> {
    stream(input, settings, cancel, false, frame)
}
pub fn source_preview_stream(
    input: &Path,
    settings: &Settings,
    cancel: &Cancel,
    frame: impl FnMut(Vec<u8>, [usize; 2]) -> bool,
) -> Result<()> {
    stream(input, settings, cancel, true, frame)
}
fn stream(
    input: &Path,
    settings: &Settings,
    cancel: &Cancel,
    source: bool,
    mut frame: impl FnMut(Vec<u8>, [usize; 2]) -> bool,
) -> Result<()> {
    settings.panels()?;
    let media = probe(input)?;
    let width = if source {
        media.width
    } else {
        settings.canvas_width()?
    } as f64;
    let height = if source {
        media.height
    } else {
        settings.height
    } as f64;
    let scale = (960.0 / width).min(720.0 / height).min(1.0);
    let size = [
        (width * scale).round().max(1.) as usize,
        (height * scale).round().max(1.) as usize,
    ];
    let duration = media
        .duration
        .map(|d| settings.duration.min(d - settings.start))
        .unwrap_or(settings.duration);
    ensure!(duration > 0., "Start liegt hinter dem Medienende");
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Ok(());
        }
        let mut log = tempfile::tempfile()?;
        let content = if source {
            format!("setsar=1,format=rgba{}", settings.effects_filter())
        } else {
            format!(
                "format=rgba{},{}",
                settings.effects_filter(),
                settings.base_filter(&media)?
            )
        };
        let filter = format!(
            "{content},scale={}:{},fps={}",
            size[0],
            size[1],
            settings.fps.min(24)
        );
        let child = command("ffmpeg")?
            .args([
                "-v",
                "error",
                "-nostdin",
                "-ss",
                &settings.start.to_string(),
                "-i",
            ])
            .arg(input)
            .args([
                "-t",
                &duration.to_string(),
                "-vf",
                &filter,
                "-an",
                "-f",
                "rawvideo",
                "-pix_fmt",
                "rgba",
                "pipe:1",
            ])
            .stdout(Stdio::piped())
            .stderr(log.try_clone()?)
            .spawn()?;
        struct Guard(std::process::Child);
        impl Drop for Guard {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        let mut child = Guard(child);
        let mut stdout = child.0.stdout.take().unwrap();
        let mut count = 0;
        loop {
            let mut bytes = vec![0; size[0] * size[1] * 4];
            if stdout.read_exact(&mut bytes).is_err() {
                break;
            }
            count += 1;
            if cancel.load(Ordering::Relaxed) || !frame(bytes, size) {
                return Ok(());
            }
            if settings.format == Format::Png {
                return Ok(());
            }
        }
        let status = child.0.wait()?;
        if !status.success() || count == 0 {
            log.rewind()?;
            let mut message = String::new();
            log.read_to_string(&mut message)?;
            bail!("Vorschau konnte nicht decodiert werden: {message}");
        }
        if media.duration.is_none() {
            return Ok(());
        }
    }
}
fn run_ffmpeg(args: &[std::ffi::OsString], cancel: &Cancel) -> Result<()> {
    ensure!(!cancel.load(Ordering::Relaxed), "Export abgebrochen");
    let mut log = tempfile::tempfile()?;
    let mut child = command("ffmpeg")?
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(log.try_clone()?)
        .spawn()
        .context("Der Medienprozessor FFmpeg konnte nicht gestartet werden")?;
    loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            bail!("Export abgebrochen");
        }
        if let Some(status) = child.try_wait()? {
            if !status.success() {
                log.seek(SeekFrom::Start(0))?;
                let mut msg = String::new();
                log.read_to_string(&mut msg)?;
                bail!("FFmpeg: {msg}");
            }
            return Ok(());
        }
        thread::sleep(Duration::from_millis(60));
    }
}
#[derive(Debug, Serialize)]
pub struct Export {
    pub directory: PathBuf,
    pub files: Vec<ExportFile>,
}
#[derive(Debug, Serialize)]
pub struct ExportFile {
    pub name: String,
    pub bytes: u64,
    pub over_budget: bool,
    pub colors: Option<u32>,
}
pub fn export(
    input: &Path,
    output: &Path,
    settings: &Settings,
    cancel: &Cancel,
    progress: impl Fn(usize, usize),
) -> Result<Export> {
    let panels = settings.panels()?;
    let media = probe(input)?;
    if let Some(duration) = media.duration {
        ensure!(
            settings.start < duration,
            "Start liegt hinter dem Medienende"
        );
        ensure!(
            settings.format == Format::Png || settings.start + settings.duration <= duration + 0.1,
            "Clip reicht über das Medienende hinaus"
        );
    }
    fs::create_dir_all(output).context("Ausgabeordner konnte nicht erstellt werden")?;
    let staging = tempfile::Builder::new()
        .prefix("showcase-")
        .tempdir_in(output)?;
    let mut files = Vec::new();
    for (i, p) in panels.iter().enumerate() {
        progress(i, panels.len());
        let ext = if settings.format == Format::Gif {
            "gif"
        } else {
            "png"
        };
        let name = format!("panel_{:02}.{ext}", i + 1);
        let path = staging.path().join(&name);
        let crop = format!(
            "format=rgba{},{},crop={}:{}:{}:0",
            settings.effects_filter(),
            settings.base_filter(&media)?,
            p.width,
            p.height,
            p.x
        );
        let mut palette_colors = settings.colors;
        loop {
            let filter = if settings.format == Format::Gif {
                format!(
                    "fps={},{} ,split[a][b];[a]palettegen=max_colors={}:reserve_transparent=0[pal];[b][pal]paletteuse=dither={}",
                    settings.fps,
                    crop,
                    palette_colors,
                    settings.dither.filter()
                )
            } else {
                crop.clone()
            };
            let mut args: Vec<std::ffi::OsString> = [
                "-hide_banner",
                "-loglevel",
                "error",
                "-nostdin",
                "-y",
                "-ss",
                &settings.start.to_string(),
                "-i",
            ]
            .iter()
            .map(Into::into)
            .collect();
            args.push(input.as_os_str().into());
            args.extend([
                "-t".into(),
                settings.duration.to_string().into(),
                "-filter_complex".into(),
                filter.into(),
                "-an".into(),
            ]);
            if settings.format == Format::Png {
                args.extend(["-frames:v".into(), "1".into()]);
            } else {
                args.extend(["-loop".into(), "0".into()]);
            }
            args.push(path.as_os_str().into());
            run_ffmpeg(&args, cancel)?;
            if settings.format != Format::Gif
                || !settings.auto_compress
                || fs::metadata(&path)?.len() as f64 <= settings.target_mb * 1_000_000.
            {
                break;
            }
            ensure!(
                palette_colors > 4,
                "Panel {} passt auch mit 4 Farben nicht unter {:.2} MB. FPS, Höhe oder Dauer reduzieren.",
                i + 1,
                settings.target_mb
            );
            palette_colors = (palette_colors / 2).max(4);
        }

        if settings.format == Format::Gif && settings.workshop_patch {
            let mut file = fs::OpenOptions::new().read(true).write(true).open(&path)?;
            file.seek(SeekFrom::End(-1))?;
            let mut last = [0];
            file.read_exact(&mut last)?;
            ensure!(last[0] == 0x3b, "GIF-Abschluss fehlt");
            file.seek(SeekFrom::End(-1))?;
            file.write_all(&[0x21])?;
        }
        let bytes = fs::metadata(&path)?.len();
        files.push(ExportFile {
            name,
            bytes,
            over_budget: bytes > 5_000_000,
            colors: if settings.format == Format::Gif {
                Some(palette_colors)
            } else {
                None
            },
        });
    }
    ensure!(!cancel.load(Ordering::Relaxed), "Export abgebrochen");
    fs::write(
        staging.path().join("manifest.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"settings":settings,"panels":panels,"files":files}),
        )?,
    )?;
    fs::write(staging.path().join("UPLOAD.txt"), upload_notes(settings))?;
    let directory = staging.keep();
    progress(panels.len(), panels.len());
    Ok(Export { directory, files })
}
pub fn upload_notes(settings: &Settings) -> String {
    format!(
        "SteamWorkshopCreator\nLayout: {}\nDateien von links nach rechts verwenden.\n\n{}\n\nVier Panels sind ein frei konfigurierbares Exportlayout; keine garantierte Steam-Anordnung.\nArtwork-Guide: https://steamcommunity.com/sharedfiles/filedetails/?id=748624905\nWorkshop-Referenz: https://github.com/VOiD-Ayush/steam-showcase-maker\nDie Upload-Tricks sind Community-Verfahren und können sich ändern.\nDateien über dem konfigurierten Richtwert von 5 MB kürzen oder mit weniger FPS/Farben neu exportieren.\nGIF-Byte-Patch nur bei Bedarf aktivieren: ersetzt den GIF-Trailer durch 0x21 und kann normale GIF-Decoder stören.\n",
        settings.layout.label(),
        match settings.layout {
            Layout::Artwork | Layout::Featured =>
                "Für lange Artwork-/Screenshot-Showcases den verlinkten Guide verwenden. Export allein ändert Steams Anzeige nicht.",
            _ =>
                "Die Panels manuell in Steam hochladen. Kein automatischer Upload und keine Steam-Anmeldedaten erforderlich.",
        }
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn artwork_gap_preserves_geometry() {
        let s = Settings {
            layout: Layout::Artwork,
            gap: 8,
            ..Default::default()
        };
        let p = s.panels().unwrap();
        assert_eq!(s.canvas_width().unwrap(), 614);
        assert_eq!(p[1].x, 514);
        assert_eq!(p[1].width, 100);
    }
    #[test]
    fn four_panels_cover_strip() {
        let s = Settings {
            layout: Layout::Strip4,
            ..Default::default()
        };
        assert_eq!(s.panels().unwrap().len(), 4);
        assert_eq!(s.canvas_width().unwrap(), 600);
    }
    #[test]
    fn rejects_invalid_ranges() {
        for bad in [f64::NAN, -1., 0., 601.] {
            assert!(
                Settings {
                    duration: bad,
                    ..Default::default()
                }
                .panels()
                .is_err()
            );
        }
    }
}
