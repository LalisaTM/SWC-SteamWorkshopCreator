use std::{
    process::Command,
    sync::{Arc, atomic::AtomicBool},
};
use steam_workshop_creator::*;

#[cfg(windows)]
#[test]
fn bundled_app_exports_with_empty_path_and_fresh_cache() {
    let temp = tempfile::tempdir().unwrap();
    let input = temp.path().join("input.png");
    image::RgbImage::from_pixel(630, 64, image::Rgb([20, 80, 140]))
        .save(&input)
        .unwrap();
    let output = temp.path().join("output");
    let status = Command::new(env!("CARGO_BIN_EXE_steam-workshop-creator"))
        .env("PATH", "")
        .env("LOCALAPPDATA", temp.path().join("fresh-cache"))
        .current_dir(temp.path())
        .arg("export")
        .arg(&input)
        .args([
            "--layout", "featured", "--format", "png", "--height", "64", "--output",
        ])
        .arg(&output)
        .status()
        .unwrap();
    assert!(status.success());
    let directory = std::fs::read_dir(output)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    assert!(directory.join("panel_01.png").is_file());
    assert_eq!(
        image::image_dimensions(directory.join("panel_01.png")).unwrap(),
        (630, 64)
    );
    let cache = temp
        .path()
        .join("fresh-cache/SteamWorkshopCreator/tools/ffmpeg-9.0.2-60f467265b1e");
    assert!(cache.join("ffmpeg.exe").is_file());
    assert!(cache.join("ffprobe.exe").is_file());
}

// Run against real FFmpeg, including decoding the generated GIF again.
#[test]
fn real_media_pipeline() {
    if Command::new("ffmpeg").arg("-version").output().is_err() {
        eprintln!("FFmpeg unavailable; integration test skipped");
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let input = temp.path().join("source video.mp4");
    assert!(
        Command::new("ffmpeg")
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=320x240:rate=10:duration=1",
                "-y"
            ])
            .arg(&input)
            .status()
            .unwrap()
            .success()
    );
    let cancel = Arc::new(AtomicBool::new(false));
    let settings = Settings {
        layout: Layout::Featured,
        height: 120,
        duration: 0.8,
        fps: 10,
        monochrome: true,
        ..Default::default()
    };
    let mut frames = Vec::new();
    preview_stream(&input, &settings, &cancel, |bytes, size| {
        assert_eq!(bytes.len(), size[0] * size[1] * 4);
        assert!(
            bytes
                .as_chunks::<4>()
                .0
                .iter()
                .all(|p| p[0].abs_diff(p[1]) <= 1 && p[1].abs_diff(p[2]) <= 1)
        );
        frames.push(bytes);
        frames.len() < 3
    })
    .unwrap();
    assert_eq!(frames.len(), 3);
    assert_ne!(frames[0], frames[2]);
    let plain = thumbnail(
        &input,
        &Settings {
            height: 64,
            ..Default::default()
        },
    )
    .unwrap();
    for effect in [
        Settings {
            sepia: true,
            height: 64,
            ..Default::default()
        },
        Settings {
            invert: true,
            height: 64,
            ..Default::default()
        },
        Settings {
            brightness: 0.2,
            contrast: 1.5,
            height: 64,
            ..Default::default()
        },
    ] {
        let effected = thumbnail(&input, &effect).unwrap();
        assert_ne!(plain, effected);
    }
    let settings = Settings {
        layout: Layout::Featured,
        height: 200,
        duration: 0.8,
        fps: 10,
        colors: 4,
        ..Default::default()
    };
    let small = export(&input, temp.path(), &settings, &cancel, |_, _| {}).unwrap();
    let target = (small.files[0].bytes as f64 / 1_000_000. + 0.001).max(0.01);
    let settings = Settings {
        colors: 256,
        auto_compress: true,
        target_mb: target,
        ..settings
    };
    let compressed = export(&input, temp.path(), &settings, &cancel, |_, _| {}).unwrap();
    assert!(compressed.files[0].bytes as f64 <= target * 1_000_000.);
    assert!(compressed.files[0].colors.unwrap() < 256);
    let repeated = export(&input, temp.path(), &settings, &cancel, |_, _| {}).unwrap();
    assert_eq!(compressed.files[0].bytes, repeated.files[0].bytes);
    let failed = Settings {
        height: 1000,
        target_mb: 0.01,
        ..settings
    };
    let count = std::fs::read_dir(temp.path()).unwrap().count();
    assert!(export(&input, temp.path(), &failed, &cancel, |_, _| {}).is_err());
    assert_eq!(count, std::fs::read_dir(temp.path()).unwrap().count());
    for layout in Layout::ALL {
        let settings = Settings {
            layout,
            height: 64,
            panel_width: 32,
            duration: 0.8,
            fps: 10,
            gap: 8,
            ..Default::default()
        };
        assert!(!thumbnail(&input, &settings).unwrap().is_empty());
        let result = export(&input, temp.path(), &settings, &cancel, |_, _| {}).unwrap();
        let panels = settings.panels().unwrap();
        assert_eq!(result.files.len(), panels.len());
        for (file, panel) in result.files.iter().zip(panels) {
            let path = result.directory.join(&file.name);
            let media = probe(&path).unwrap();
            assert_eq!((media.width, media.height), (panel.width, panel.height));
            assert!(media.duration.unwrap() > 0.5);
            assert!(!file.over_budget);
        }
    }
    let image = temp.path().join("still.png");
    image::RgbImage::from_pixel(614, 64, image::Rgb([23, 80, 140]))
        .save(&image)
        .unwrap();
    let settings = Settings {
        layout: Layout::Artwork,
        format: Format::Png,
        height: 64,
        gap: 8,
        ..Default::default()
    };
    let result = export(&image, temp.path(), &settings, &cancel, |_, _| {}).unwrap();
    for file in &result.files {
        let image = image::open(result.directory.join(&file.name))
            .unwrap()
            .to_rgb8();
        assert_eq!(image.get_pixel(0, 0).0, [23, 80, 140]);
    }
    let settings = Settings {
        layout: Layout::Featured,
        height: 32,
        duration: 0.5,
        workshop_patch: true,
        ..Default::default()
    };
    let result = export(&input, temp.path(), &settings, &cancel, |_, _| {}).unwrap();
    assert_eq!(
        std::fs::read(result.directory.join(&result.files[0].name))
            .unwrap()
            .last(),
        Some(&0x21)
    );
    let cancelled = Arc::new(AtomicBool::new(true));
    let before = std::fs::read_dir(temp.path()).unwrap().count();
    assert!(export(&input, temp.path(), &settings, &cancelled, |_, _| {}).is_err());
    assert_eq!(before, std::fs::read_dir(temp.path()).unwrap().count());
}
