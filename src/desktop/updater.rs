use std::sync::Arc;

#[derive(Clone)]
pub(super) struct Update {
    pub version: String,
    digest: [u8; 32],
    size: u64,
}

pub(super) enum Status {
    Checking,
    Current,
    Available(Arc<Update>),
    Downloading,
    Ready,
    Error(String),
}

#[cfg(windows)]
mod windows {
    use std::{
        env, fs,
        io::{self, Read},
        os::windows::process::CommandExt,
        path::{Path, PathBuf},
        process::Command,
    };

    use anyhow::{Context, Result, ensure};
    use serde::Deserialize;
    use sha2::{Digest, Sha256};

    use super::Update;

    const REPOSITORY: &str = "Borderliner/sect-forge";
    const ASSET: &str = "sect-forge.exe";
    const DOWNLOAD_LIMIT: u64 = 256 * 1024 * 1024;
    const NO_WINDOW: u32 = 0x08000000;

    #[derive(Deserialize)]
    struct Release {
        tag_name: String,
        assets: Vec<Asset>,
    }

    #[derive(Deserialize)]
    struct Asset {
        name: String,
        size: u64,
        digest: Option<String>,
    }

    fn version(value: &str) -> Option<[u64; 3]> {
        let mut parts = value.split('.');
        let numbers = [
            parts.next()?.parse().ok()?,
            parts.next()?.parse().ok()?,
            parts.next()?.parse().ok()?,
        ];
        (parts.next().is_none()).then_some(numbers)
    }

    fn system32() -> Result<PathBuf> {
        Ok(PathBuf::from(
            env::var_os("SystemRoot").context("Windows system folder is unavailable.")?,
        )
        .join("System32"))
    }

    fn get(url: &str, maximum: u64, destination: Option<&Path>) -> Result<Vec<u8>> {
        let mut curl = Command::new(system32()?.join("curl.exe"));
        curl.creation_flags(NO_WINDOW).args([
            "--disable",
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--max-redirs",
            "5",
            "--proto",
            "=https",
            "--proto-redir",
            "=https",
            "--connect-timeout",
            "8",
            "--max-time",
            "120",
            "--user-agent",
            concat!("SectForge/", env!("CARGO_PKG_VERSION")),
            "--max-filesize",
            &maximum.to_string(),
        ]);
        if let Some(path) = destination {
            curl.arg("--output").arg(path);
        }
        let response = curl.arg(url).output().context("Open the GitHub release")?;
        ensure!(
            response.status.success(),
            "GitHub request failed. {}",
            String::from_utf8_lossy(&response.stderr).trim()
        );
        Ok(response.stdout)
    }

    pub(super) fn check() -> Result<Option<Update>> {
        let url = format!("https://api.github.com/repos/{REPOSITORY}/releases/latest");
        let data = get(&url, 2 * 1024 * 1024, None)?;
        let release: Release = serde_json::from_slice(&data)?;
        let tag = release
            .tag_name
            .strip_prefix('v')
            .context("The GitHub release has an unexpected version.")?;
        let newest = version(tag).context("The GitHub release has an unexpected version.")?;
        let installed =
            version(env!("CARGO_PKG_VERSION")).context("The app has an unexpected version.")?;
        if newest <= installed {
            return Ok(None);
        }
        let asset = release
            .assets
            .into_iter()
            .find(|asset| asset.name == ASSET)
            .context("The latest release has no Windows executable.")?;
        ensure!(
            (1..=DOWNLOAD_LIMIT).contains(&asset.size),
            "The update has an unexpected size."
        );
        let mut digest = [0; 32];
        let checksum = asset
            .digest
            .as_deref()
            .and_then(|value| value.strip_prefix("sha256:"))
            .context("The latest release has no SHA-256 checksum.")?;
        hex::decode_to_slice(checksum, &mut digest).context("Invalid SHA-256 checksum")?;
        Ok(Some(Update {
            version: tag.into(),
            digest,
            size: asset.size,
        }))
    }

    pub(super) fn stage(update: &Update) -> Result<()> {
        let current = env::current_exe()?;
        let directory = current
            .parent()
            .context("The executable folder is unavailable.")?;
        let staging = tempfile::Builder::new()
            .prefix(".sectforge-update-")
            .tempdir_in(directory)
            .context("Open the Sect Forge folder for updates")?;
        let download = staging.path().join(ASSET);
        let url = format!(
            "https://github.com/{REPOSITORY}/releases/download/v{}/{ASSET}",
            update.version
        );
        get(&url, update.size, Some(&download))?;
        let mut file = fs::File::open(&download)?;
        ensure!(
            file.metadata()?.len() == update.size,
            "The update download is incomplete."
        );
        let mut header = [0; 2];
        file.read_exact(&mut header)?;
        ensure!(&header == b"MZ", "The update is not a Windows executable.");
        let mut digest = Sha256::new();
        digest.update(header);
        io::copy(&mut file, &mut digest)?;
        ensure!(
            <[u8; 32]>::from(digest.finalize()) == update.digest,
            "The update failed SHA-256 verification."
        );
        drop(file);
        Command::new(system32()?.join("WindowsPowerShell/v1.0/powershell.exe"))
            .creation_flags(NO_WINDOW)
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                include_str!("apply_update.ps1"),
            ])
            .env("SECTFORGE_UPDATE_OLD", &current)
            .env("SECTFORGE_UPDATE_NEW", &download)
            .env("SECTFORGE_UPDATE_PID", std::process::id().to_string())
            .spawn()
            .context("Start the update installer")?;
        let _ = staging.keep();
        Ok(())
    }
}

#[cfg(windows)]
pub(super) fn check() -> anyhow::Result<Option<Update>> {
    windows::check()
}

#[cfg(windows)]
pub(super) fn stage(update: &Update) -> anyhow::Result<()> {
    windows::stage(update)
}
