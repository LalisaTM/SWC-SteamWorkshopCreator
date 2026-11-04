use anyhow::{Context, Result, ensure};
use reqwest::blocking::Client;
use semver::Version;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

pub const REPOSITORY: &str = "https://github.com/LalisaTM/SWC-SteamWorkshopCreator";
pub const ASSET: &str = "SWC-SteamWorkshopCreator-windows-x64.exe";
const API: &str = "https://api.github.com/repos/LalisaTM/SWC-SteamWorkshopCreator/releases/latest";
const MAX_DOWNLOAD: u64 = 300_000_000;

#[derive(Clone, Debug)]
pub struct Update {
    pub version: Version,
    pub url: String,
    pub checksum_url: String,
    pub size: u64,
}
#[derive(Deserialize)]
struct Release {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<Asset>,
}
#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    size: u64,
}

fn client() -> Result<Client> {
    Ok(Client::builder()
        .user_agent(concat!(
            "SWC-SteamWorkshopCreator/",
            env!("CARGO_PKG_VERSION")
        ))
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(300))
        .https_only(true)
        .build()?)
}
fn trusted_asset(url: &str, tag: &str, name: &str) -> bool {
    url == format!("{REPOSITORY}/releases/download/{tag}/{name}")
}
fn select_update(release: Release, current: &str) -> Result<Option<Update>> {
    if release.draft || release.prerelease {
        return Ok(None);
    }
    let version = Version::parse(release.tag_name.trim_start_matches('v'))?;
    if !version.pre.is_empty() || version <= Version::parse(current)? {
        return Ok(None);
    }
    let binary = release
        .assets
        .iter()
        .find(|a| a.name == ASSET)
        .context("Release enthält keine Windows-x64-EXE")?;
    let checksum_name = format!("{ASSET}.sha256");
    let checksum = release
        .assets
        .iter()
        .find(|a| a.name == checksum_name)
        .context("Release enthält keine SHA-256-Prüfsumme")?;
    ensure!(
        trusted_asset(&binary.browser_download_url, &release.tag_name, ASSET)
            && trusted_asset(
                &checksum.browser_download_url,
                &release.tag_name,
                &checksum_name
            ),
        "Update-Download gehört nicht zum erwarteten GitHub-Repository"
    );
    ensure!(
        (1024..=MAX_DOWNLOAD).contains(&binary.size),
        "Ungültige Größe des Updatepakets"
    );
    Ok(Some(Update {
        version,
        url: binary.browser_download_url.clone(),
        checksum_url: checksum.browser_download_url.clone(),
        size: binary.size,
    }))
}
pub fn check() -> Result<Option<Update>> {
    let response = client()?
        .get(API)
        .header("Accept", "application/vnd.github+json")
        .timeout(Duration::from_secs(20))
        .send()?;
    // A new repository may not have published its first release yet.
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    select_update(
        response.error_for_status()?.json()?,
        env!("CARGO_PKG_VERSION"),
    )
}
fn checksum(text: &str) -> Result<String> {
    let hash = text.split_whitespace().next().context("Leere Prüfsumme")?;
    ensure!(
        hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()),
        "Ungültige SHA-256-Prüfsumme"
    );
    Ok(hash.to_ascii_lowercase())
}
fn verify(path: &Path, expected: &str) -> Result<()> {
    let mut file = fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    ensure!(
        format!("{:x}", hash.finalize()) == expected,
        "Update-Prüfsumme stimmt nicht überein. Die bisherige Version bleibt erhalten."
    );
    file.rewind()?;
    let mut magic = [0; 2];
    file.read_exact(&mut magic)?;
    ensure!(&magic == b"MZ", "Update ist keine Windows-Anwendung");
    Ok(())
}
use std::io::Seek;

/// Downloads to a temporary file, verifies it, and then replaces the current EXE.
/// The caller closes the UI cleanly before restarting the returned original path.
pub fn install(update: &Update, progress: impl Fn(u64, u64)) -> Result<PathBuf> {
    ensure!(
        cfg!(all(windows, target_arch = "x86_64")),
        "Automatische Updates sind für Windows x64 verfügbar"
    );
    let http = client()?;
    let mut checksum_response = http.get(&update.checksum_url).send()?.error_for_status()?;
    let mut text = String::new();
    (&mut checksum_response)
        .take(4097)
        .read_to_string(&mut text)?;
    ensure!(text.len() <= 4096, "Prüfsummendatei ist zu groß");
    let expected = checksum(&text)?;
    let mut response = http.get(&update.url).send()?.error_for_status()?;
    let mut temporary = tempfile::NamedTempFile::new()?;
    let mut downloaded = 0u64;
    let mut buffer = [0; 64 * 1024];
    loop {
        let n = response.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        downloaded += n as u64;
        ensure!(
            downloaded <= update.size && downloaded <= MAX_DOWNLOAD,
            "Update überschreitet die erwartete Dateigröße"
        );
        temporary.write_all(&buffer[..n])?;
        progress(downloaded, update.size);
    }
    ensure!(
        downloaded == update.size,
        "Update-Download ist unvollständig"
    );
    temporary.flush()?;
    verify(temporary.path(), &expected)?;
    replace_file(temporary.path())
}

fn replace_file(new_binary: &Path) -> Result<PathBuf> {
    let original = std::env::current_exe()?;
    let parent = original.parent().context("Programmordner fehlt")?;
    // Verify write access and keep a recoverable copy before touching the running EXE.
    let backup = tempfile::NamedTempFile::new_in(parent)
        .context("Die App benötigt Schreibrechte im eigenen Ordner")?;
    fs::copy(&original, backup.path())
        .context("Die bisherige Version konnte nicht gesichert werden")?;
    if let Err(error) = self_replace::self_replace(new_binary) {
        if !original.is_file() {
            backup
                .persist(&original)
                .context("Die bisherige EXE konnte nicht wiederhergestellt werden")?;
        }
        return Err(error).context(
            "Update konnte nicht installiert werden. Die bisherige Version wurde wiederhergestellt",
        );
    }
    Ok(original)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn release(tag: &str) -> Release {
        Release {
            tag_name: tag.into(),
            draft: false,
            prerelease: false,
            assets: vec![
                Asset {
                    name: ASSET.into(),
                    browser_download_url: format!("{REPOSITORY}/releases/download/{tag}/{ASSET}"),
                    size: 2048,
                },
                Asset {
                    name: format!("{ASSET}.sha256"),
                    browser_download_url: format!(
                        "{REPOSITORY}/releases/download/{tag}/{ASSET}.sha256"
                    ),
                    size: 100,
                },
            ],
        }
    }
    #[test]
    fn versions_and_assets_are_checked() {
        assert!(select_update(release("v0.2.0"), "0.1.0").unwrap().is_some());
        assert!(select_update(release("v0.1.0"), "0.1.0").unwrap().is_none());
        assert!(select_update(release("v0.0.9"), "0.1.0").unwrap().is_none());
        assert!(
            select_update(release("v0.2.0-beta.1"), "0.1.0")
                .unwrap()
                .is_none()
        );
        let mut bad = release("v0.2.0");
        bad.assets[0].browser_download_url = "https://example.com/app.exe".into();
        assert!(select_update(bad, "0.1.0").is_err());
        let mut bad = release("v0.2.0");
        bad.assets.pop();
        assert!(select_update(bad, "0.1.0").is_err());
    }
    #[test]
    fn checksum_rejects_corruption_before_replacement() {
        let mut temp = tempfile::NamedTempFile::new().unwrap();
        temp.write_all(b"MZtest").unwrap();
        temp.flush().unwrap();
        let hash = format!("{:x}", Sha256::digest(b"MZtest"));
        assert!(verify(temp.path(), &checksum(&format!("{hash}  {ASSET}")).unwrap()).is_ok());
        assert!(verify(temp.path(), &"0".repeat(64)).is_err());
        assert!(checksum("bad").is_err());
    }

    #[cfg(windows)]
    #[test]
    fn windows_replaces_running_copy() {
        const VARIABLE: &str = "SWC_TEST_REPLACEMENT_FILE";
        if let Some(source) = std::env::var_os(VARIABLE) {
            replace_file(&PathBuf::from(source)).unwrap();
            return;
        }
        let temp = tempfile::tempdir().unwrap();
        let original = temp.path().join("running.exe");
        let replacement = temp.path().join("replacement.exe");
        let this_test = std::env::current_exe().unwrap();
        fs::copy(&this_test, &original).unwrap();
        fs::copy(&this_test, &replacement).unwrap();
        fs::OpenOptions::new()
            .append(true)
            .open(&replacement)
            .unwrap()
            .write_all(b"SWC-update-smoke-test")
            .unwrap();
        let mut process = std::process::Command::new(&original);
        use std::os::windows::process::CommandExt;
        let status = process
            .creation_flags(0x08000000)
            .args(["--exact", "updater::tests::windows_replaces_running_copy"])
            .env(VARIABLE, &replacement)
            .status()
            .unwrap();
        assert!(status.success());
        assert_eq!(
            fs::metadata(&original).unwrap().len(),
            fs::metadata(&replacement).unwrap().len()
        );
        assert!(
            fs::read(&original)
                .unwrap()
                .ends_with(b"SWC-update-smoke-test")
        );
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "Downloads the current public release; run manually after publishing"]
    fn github_release_updates_isolated_copy() {
        let release: Release = client()
            .unwrap()
            .get(API)
            .send()
            .unwrap()
            .error_for_status()
            .unwrap()
            .json()
            .unwrap();
        let update = select_update(release, "0.0.0")
            .unwrap()
            .expect("A public release must exist");
        if std::env::var_os("SWC_TEST_GITHUB_UPDATE").is_some() {
            install(&update, |_, _| {}).unwrap();
            return;
        }
        let temp = tempfile::tempdir().unwrap();
        let original = temp.path().join("isolated-swc.exe");
        fs::copy(std::env::current_exe().unwrap(), &original).unwrap();
        let mut command = std::process::Command::new(&original);
        use std::os::windows::process::CommandExt;
        let status = command
            .creation_flags(0x08000000)
            .args([
                "--exact",
                "updater::tests::github_release_updates_isolated_copy",
                "--ignored",
            ])
            .env("SWC_TEST_GITHUB_UPDATE", "1")
            .status()
            .unwrap();
        assert!(status.success());
        let hash = client()
            .unwrap()
            .get(update.checksum_url)
            .send()
            .unwrap()
            .error_for_status()
            .unwrap()
            .text()
            .unwrap();
        verify(&original, &checksum(&hash).unwrap()).unwrap();
    }
}
