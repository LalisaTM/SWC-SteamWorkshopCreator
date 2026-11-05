use std::sync::{Arc, atomic::AtomicBool};
use steam_workshop_creator::*;

#[test]
fn color_preview_and_export_use_the_same_pipeline() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("color.png");
    image::RgbImage::from_pixel(630, 100, image::Rgb([70, 90, 110]))
        .save(&source)
        .unwrap();
    let mut settings = Settings {
        layout: Layout::Featured,
        format: Format::Png,
        height: 100,
        color: ColorAdjustments {
            exposure: 0.6,
            highlights: -0.2,
            shadows: 0.3,
            temperature: 0.4,
            tint: 0.2,
            saturation: 0.7,
        },
        brightness: 0.03,
        contrast: 1.1,
        ..Default::default()
    };
    let cancel = Arc::new(AtomicBool::new(false));
    let result = export(&source, temp.path(), &settings, &cancel, |_, _| {}).unwrap();
    let output = image::open(result.directory.join("panel_01.png"))
        .unwrap()
        .to_rgb8();
    let preview = image::load_from_memory(&thumbnail(&source, &settings).unwrap())
        .unwrap()
        .to_rgb8();
    let expected = output.get_pixel(315, 50).0;
    let error = preview
        .pixels()
        .flat_map(|p| p.0.into_iter().zip(expected).map(|(a, b)| a.abs_diff(b)))
        .max()
        .unwrap();
    assert!(
        error <= 2,
        "preview/export maximum channel difference: {error}; {:?} / {:?}",
        output.get_pixel(315, 50).0,
        preview.get_pixel(315, 50).0
    );
    assert_ne!(output.get_pixel(315, 50).0, [70, 90, 110]);
    let mut source_frame = None;
    source_preview_stream(&source, &settings, &cancel, |bytes, size| {
        source_frame = Some((bytes, size));
        false
    })
    .unwrap();
    let (bytes, size) = source_frame.unwrap();
    assert_eq!(size, [630, 100]);
    let pixel = (50 * 630 + 315) * 4;
    assert_eq!(&bytes[pixel..pixel + 3], &output.get_pixel(315, 50).0);
    // All slider extremes must remain valid FFmpeg expressions.
    for sign in [-1., 1.] {
        settings.color = ColorAdjustments {
            exposure: sign * 4.,
            highlights: sign,
            shadows: sign,
            temperature: sign,
            tint: sign,
            saturation: if sign < 0. { 0. } else { 2.5 },
        };
        assert!(!thumbnail(&source, &settings).unwrap().is_empty());
    }
}

#[test]
fn old_settings_are_neutral_and_nonfinite_corrections_are_rejected() {
    let mut settings: Settings = serde_json::from_str("{}").unwrap();
    assert_eq!(settings.effects_filter(), "");
    for value in [f32::NAN, f32::INFINITY, 5.] {
        settings.color.exposure = value;
        assert!(settings.panels().is_err());
    }
}
