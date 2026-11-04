use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Cursor, Read},
    path::{Path, PathBuf},
    sync::OnceLock,
};

include!(concat!(env!("OUT_DIR"), "/ffmpeg_hashes.rs"));
const ARCHIVE: &[u8] = include_bytes!(env!("FFMPEG_BUNDLE_PATH"));
static TOOLS: OnceLock<Result<PathBuf, String>> = OnceLock::new();

pub fn executable(name: &str) -> Result<PathBuf> {
    let directory = TOOLS
        .get_or_init(|| prepare().map_err(|e| format!("{e:#}")))
        .as_ref()
        .map_err(|e| anyhow::anyhow!(e.clone()))?;
    Ok(directory.join(format!("{name}.exe")))
}
fn valid(path: &Path, hash: &str) -> bool {
    let Ok(mut file) = fs::File::open(path) else {
        return false;
    };
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        match file.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => digest.update(&buffer[..n]),
            Err(_) => return false,
        }
    }
    format!("{:x}", digest.finalize()) == hash
}
fn prepare() -> Result<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    unpack(&base.join("SteamWorkshopCreator/tools/ffmpeg-9.0.2-60f467265b1e"))
}
fn unpack(directory: &Path) -> Result<PathBuf> {
    fs::create_dir_all(directory).context(
        "Der integrierte Medienprozessor konnte nicht im lokalen Cache eingerichtet werden",
    )?;
    let files = [
        ("bin/ffmpeg.exe", "ffmpeg.exe", Some(FFMPEG_HASH)),
        ("bin/ffprobe.exe", "ffprobe.exe", Some(FFPROBE_HASH)),
        ("LICENSE", "LICENSE-FFmpeg.txt", None),
        ("README.txt", "README-FFmpeg.txt", None),
    ];
    let mut archive = None;
    for (source, name, hash) in files {
        let destination = directory.join(name);
        if hash
            .map(|h| valid(&destination, h))
            .unwrap_or(destination.is_file())
        {
            continue;
        }
        let zip = archive.get_or_insert(zip::ZipArchive::new(Cursor::new(ARCHIVE))?);
        let mut source = zip.by_name(&format!("ffmpeg-9.0.2-essentials_build/{source}"))?;
        let mut temporary = tempfile::NamedTempFile::new_in(directory)?;
        std::io::copy(&mut source, &mut temporary)?;
        if let Some(hash) = hash {
            ensure!(
                valid(temporary.path(), hash),
                "Integritätsprüfung des integrierten Medienprozessors fehlgeschlagen"
            );
        }
        temporary.persist(&destination).with_context(|| {
            format!(
                "Integrierte Datei konnte nicht bereitgestellt werden: {}",
                destination.display()
            )
        })?;
    }
    Ok(directory.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn extracts_and_repairs_without_path_or_network() {
        let temp = tempfile::tempdir().unwrap();
        let directory = unpack(temp.path()).unwrap();
        assert!(valid(&directory.join("ffmpeg.exe"), FFMPEG_HASH));
        assert!(valid(&directory.join("ffprobe.exe"), FFPROBE_HASH));
        assert!(directory.join("LICENSE-FFmpeg.txt").is_file());
        fs::write(directory.join("ffprobe.exe"), b"corrupted").unwrap();
        unpack(temp.path()).unwrap();
        assert!(valid(&directory.join("ffprobe.exe"), FFPROBE_HASH));
    }
}
