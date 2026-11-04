fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    println!("cargo:rerun-if-changed=assets/icon.rc");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        prepare_bundle();
        embed_resource::compile("assets/icon.rc", embed_resource::NONE)
            .manifest_optional()
            .unwrap();
    }
}

fn prepare_bundle() {
    use sha2::{Digest, Sha256};
    use std::{
        fs,
        io::{Read, Write},
        path::PathBuf,
        process::Command,
    };
    assert_eq!(
        std::env::var("CARGO_CFG_TARGET_ARCH").unwrap(),
        "x86_64",
        "The bundled FFmpeg build requires Windows x64"
    );
    println!("cargo:rerun-if-changed=scripts/prepare-ffmpeg.ps1");
    let archive = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap())
        .join("vendor/ffmpeg/ffmpeg-9.0.2-essentials_build.zip");
    if !archive.is_file() {
        let status = Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-File",
                "scripts/prepare-ffmpeg.ps1",
            ])
            .status()
            .expect("Could not prepare bundled FFmpeg");
        assert!(
            status.success(),
            "Bundled FFmpeg download failed; run scripts/prepare-ffmpeg.ps1 before rebuilding"
        );
    }
    println!("cargo:rerun-if-changed={}", archive.display());
    let bytes = fs::read(&archive).expect("FFmpeg archive missing");
    assert_eq!(
        format!("{:x}", Sha256::digest(&bytes)),
        "60f467265b1e312373dbcd92200c2618a74850f98d3d078e94296bb3fa2047ba",
        "FFmpeg archive checksum mismatch"
    );
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(&bytes)).unwrap();
    let mut hashes = String::new();
    for (name, constant) in [
        ("ffmpeg.exe", "FFMPEG_HASH"),
        ("ffprobe.exe", "FFPROBE_HASH"),
    ] {
        let mut file = zip
            .by_name(&format!("ffmpeg-9.0.2-essentials_build/bin/{name}"))
            .expect("FFmpeg archive layout changed");
        let mut content = Vec::new();
        file.read_to_end(&mut content).unwrap();
        hashes.push_str(&format!(
            "pub const {constant}: &str = {:?};\n",
            format!("{:x}", Sha256::digest(&content))
        ));
    }
    let out = PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    fs::write(out.join("ffmpeg_hashes.rs"), hashes).unwrap();
    // Keep the original executables, but omit ffplay and unused documentation.
    let bundle_path = out.join("media-tools.zip");
    let mut bundle = zip::ZipWriter::new(fs::File::create(&bundle_path).unwrap());
    for name in ["bin/ffmpeg.exe", "bin/ffprobe.exe", "LICENSE", "README.txt"] {
        let path = format!("ffmpeg-9.0.2-essentials_build/{name}");
        let mut source = zip.by_name(&path).unwrap();
        bundle
            .start_file(
                path,
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Deflated),
            )
            .unwrap();
        std::io::copy(&mut source, &mut bundle).unwrap();
    }
    bundle.finish().unwrap().flush().unwrap();
    println!(
        "cargo:rustc-env=FFMPEG_BUNDLE_PATH={}",
        bundle_path.display()
    );
}
