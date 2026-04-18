use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use std::fs;
use std::io::Read;
use std::os::unix::fs::PermissionsExt;

const API_URL: &str = "https://api.github.com/repos/wei6bin/cc-speedy/releases/latest";

pub async fn run() -> Result<()> {
    let current_version = env!("CARGO_PKG_VERSION");
    println!("Current version: {current_version}");
    println!("Fetching latest release…");

    let client = reqwest::Client::builder()
        .user_agent(format!("cc-speedy/{current_version}"))
        .build()?;

    let release: serde_json::Value = client
        .get(API_URL)
        .send()
        .await
        .context("failed to reach GitHub API")?
        .error_for_status()
        .context("GitHub API returned an error")?
        .json()
        .await
        .context("failed to parse GitHub API response")?;

    let tag = release["tag_name"]
        .as_str()
        .context("missing tag_name in release")?;

    // Release tags are like "v0.2.1.42"; the embedded version is "0.2.1".
    // We can't know which run number this binary came from, so always install
    // when the base versions match but run-number differs, and skip only when
    // the tag is exactly "v{current_version}".
    let latest = tag.trim_start_matches('v');
    if latest == current_version {
        println!("Already up to date (v{current_version}).");
        return Ok(());
    }
    println!("Latest: {tag}  (installed: {current_version})");

    let platform = platform_target();
    let assets = release["assets"]
        .as_array()
        .context("missing assets in release")?;

    let asset = find_asset(assets, &platform)
        .with_context(|| format!("no matching asset for platform '{platform}' in release {tag}"))?;

    let download_url = asset["browser_download_url"]
        .as_str()
        .context("missing browser_download_url")?;
    let asset_name = asset["name"].as_str().unwrap_or("?");
    println!("Downloading {asset_name}…");

    let bytes = client
        .get(download_url)
        .send()
        .await
        .context("download request failed")?
        .error_for_status()
        .context("download returned an error")?
        .bytes()
        .await
        .context("failed to read download bytes")?;

    let binary_bytes = if asset_name.ends_with(".tar.gz") || asset_name.ends_with(".tgz") {
        extract_from_tarball(&bytes)?
    } else {
        bytes.to_vec()
    };

    let current_exe = std::env::current_exe()
        .context("cannot determine current executable path")?;
    let tmp_path = current_exe.with_extension("tmp");
    fs::write(&tmp_path, &binary_bytes).context("failed to write temp binary")?;
    fs::set_permissions(&tmp_path, fs::Permissions::from_mode(0o755))
        .context("failed to set permissions")?;
    fs::rename(&tmp_path, &current_exe).context("failed to replace binary")?;

    println!("Updated to {tag} successfully.");
    Ok(())
}

fn find_asset<'a>(assets: &'a [serde_json::Value], platform: &str) -> Option<&'a serde_json::Value> {
    // Prefer exact platform substring match (e.g. "x86_64-unknown-linux-musl")
    assets.iter().find(|a| {
        a["name"].as_str().map(|n| n.contains(platform)).unwrap_or(false)
    })
    // Fallback: match on arch + os keywords separately
    .or_else(|| {
        let arch = std::env::consts::ARCH;
        let os_key = os_keyword();
        assets.iter().find(|a| {
            a["name"].as_str().map(|n| n.contains(arch) && n.contains(os_key)).unwrap_or(false)
        })
    })
}

// Returns the full Rust target triple for this host, covering common release targets.
fn platform_target() -> String {
    let arch = std::env::consts::ARCH;
    match std::env::consts::OS {
        "linux"  => format!("{arch}-unknown-linux-musl"),
        "macos"  => format!("{arch}-apple-darwin"),
        "windows" => format!("{arch}-pc-windows-msvc"),
        other    => format!("{arch}-{other}"),
    }
}

fn os_keyword() -> &'static str {
    match std::env::consts::OS {
        "macos"   => "darwin",
        "windows" => "windows",
        _         => "linux",
    }
}

// Extract the first executable file from a .tar.gz archive.
fn extract_from_tarball(data: &[u8]) -> Result<Vec<u8>> {
    let gz = GzDecoder::new(data);
    let mut archive = tar::Archive::new(gz);
    for entry in archive.entries().context("failed to read tarball entries")? {
        let mut entry = entry.context("bad tarball entry")?;
        let path = entry.path().context("bad entry path")?;
        let name = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");
        // Pick the first entry that looks like our binary (no extension, or named cc-speedy)
        if !name.contains('.') || name == "cc-speedy" {
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf).context("failed to read binary from tarball")?;
            return Ok(buf);
        }
    }
    anyhow::bail!("no binary found inside tarball")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_target_nonempty() {
        let t = platform_target();
        assert!(t.contains('-'));
    }
}
