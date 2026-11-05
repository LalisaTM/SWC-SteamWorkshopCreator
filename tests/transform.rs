use std::sync::{Arc, atomic::AtomicBool};
use steam_workshop_creator::*;

#[test]
fn transform_export_matches_canvas_coordinates() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("quadrants.png");
    let mut image = image::RgbImage::new(630, 200);
    for (x, y, pixel) in image.enumerate_pixels_mut() {
        *pixel = image::Rgb(match (x < 315, y < 100) {
            (true, true) => [255, 0, 0],
            (false, true) => [0, 255, 0],
            (false, false) => [255, 255, 0],
            (true, false) => [0, 0, 255],
        });
    }
    image.save(&source).unwrap();
    let settings = Settings {
        layout: Layout::Featured,
        format: Format::Png,
        height: 200,
        transform: Transform {
            x: 80.,
            y: -20.,
            scale: 0.5,
            rotation: 90.,
        },
        ..Default::default()
    };
    let cancel = Arc::new(AtomicBool::new(false));
    let result = export(&source, temp.path(), &settings, &cancel, |_, _| {}).unwrap();
    let output = image::open(result.directory.join("panel_01.png"))
        .unwrap()
        .to_rgb8();
    assert_eq!(output.dimensions(), (630, 200));
    for (point, color) in [
        ((370, 40), [0, 0, 255]),
        ((420, 40), [255, 0, 0]),
        ((370, 120), [255, 255, 0]),
        ((420, 120), [0, 255, 0]),
        ((0, 0), [0, 0, 0]),
    ] {
        let got = output.get_pixel(point.0, point.1).0;
        assert!(
            got.into_iter().zip(color).all(|(a, b)| a.abs_diff(b) <= 2),
            "{point:?}: got {got:?}, expected {color:?}"
        );
    }
    // Inverting the media must not invert the black composition background.
    let inverted = Settings {
        invert: true,
        ..settings.clone()
    };
    let result = export(&source, temp.path(), &inverted, &cancel, |_, _| {}).unwrap();
    let output = image::open(result.directory.join("panel_01.png"))
        .unwrap()
        .to_rgb8();
    assert_eq!(output.get_pixel(0, 0).0, [0, 0, 0]);
    assert_eq!(output.get_pixel(420, 40).0, [0, 255, 255]);
    let outside = Settings {
        transform: Transform {
            x: 1000.,
            ..Default::default()
        },
        ..settings.clone()
    };
    let result = export(&source, temp.path(), &outside, &cancel, |_, _| {}).unwrap();
    let output = image::open(result.directory.join("panel_01.png"))
        .unwrap()
        .to_rgb8();
    assert!(output.pixels().all(|p| p.0 == [0, 0, 0]));

    // The processed preview is the same image pipeline at this small resolution.
    let mut frame = None;
    preview_stream(&source, &settings, &cancel, |bytes, size| {
        frame = Some((bytes, size));
        false
    })
    .unwrap();
    let (bytes, size) = frame.unwrap();
    assert_eq!(size, [630, 200]);
    assert_eq!(
        &bytes[(40 * 630 + 420) * 4..(40 * 630 + 420) * 4 + 3],
        &[255, 0, 0]
    );
    // The interactive preview must retain the full source instead of an old crop.
    let mut full = None;
    source_preview_stream(&source, &settings, &cancel, |bytes, size| {
        full = Some((bytes, size));
        false
    })
    .unwrap();
    let (bytes, size) = full.unwrap();
    assert_eq!(size, [630, 200]);
    assert_eq!(
        &bytes[(20 * 630 + 20) * 4..(20 * 630 + 20) * 4 + 3],
        &[255, 0, 0]
    );
}

#[test]
fn transformed_gif_and_video_still_animate() {
    if std::process::Command::new("ffmpeg")
        .arg("-version")
        .output()
        .is_err()
    {
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let video = temp.path().join("input.mp4");
    assert!(
        std::process::Command::new("ffmpeg")
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=160x100:rate=10:duration=0.8",
                "-y"
            ])
            .arg(&video)
            .status()
            .unwrap()
            .success()
    );
    let settings = Settings {
        layout: Layout::Strip4,
        panel_width: 40,
        height: 100,
        duration: 0.6,
        fps: 10,
        transform: Transform {
            x: 7.,
            y: -3.,
            scale: 0.7,
            rotation: 25.,
        },
        ..Default::default()
    };
    let cancel = Arc::new(AtomicBool::new(false));
    let result = export(&video, temp.path(), &settings, &cancel, |_, _| {}).unwrap();
    assert_eq!(result.files.len(), 4);
    for file in result.files {
        let gif = result.directory.join(file.name);
        let media = probe(&gif).unwrap();
        assert_eq!((media.width, media.height), (40, 100));
        assert!(media.duration.unwrap() >= 0.5);
        let mut frames = Vec::new();
        source_preview_stream(&gif, &settings, &cancel, |bytes, _| {
            frames.push(bytes);
            frames.len() < 3
        })
        .unwrap();
        assert_eq!(frames.len(), 3);
        assert_ne!(frames[0], frames[2]);
    }
}
