use anyhow::{bail, Context, Result};
use serde::Deserialize;
use serde_json::Value;
use std::path::{Path, PathBuf};

const REGISTRY_URL: &str =
    "https://raw.githubusercontent.com/sonyarianto/ngalir/main/docs/registry.json";

#[derive(Debug, Deserialize)]
pub(crate) struct RegistryEntry {
    pub(crate) name: String,
    pub(crate) version: String,
    pub(crate) description: String,
    #[serde(default)]
    pub(crate) use_cases: Vec<String>,
    #[serde(default)]
    pub(crate) _repo: String,
}

async fn fetch_registry() -> Result<Vec<RegistryEntry>> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;
    match client.get(REGISTRY_URL).send().await {
        Ok(resp) if resp.status().is_success() => {
            let entries: Vec<RegistryEntry> = resp.json().await?;
            return Ok(entries);
        }
        _ => {}
    }
    let local = Path::new("docs/registry.json");
    if local.exists() {
        let raw = std::fs::read_to_string(local)?;
        let entries: Vec<RegistryEntry> = serde_json::from_str(&raw)?;
        return Ok(entries);
    }
    bail!("could not fetch registry from {REGISTRY_URL} and docs/registry.json not found locally");
}

pub(crate) async fn cmd_search(keyword: &str) -> Result<()> {
    let kw = keyword.to_lowercase();
    let entries = fetch_registry().await?;
    let matched: Vec<&RegistryEntry> = entries
        .iter()
        .filter(|e| {
            e.name.to_lowercase().contains(&kw)
                || e.description.to_lowercase().contains(&kw)
                || e.use_cases.iter().any(|u| u.to_lowercase().contains(&kw))
        })
        .collect();

    if matched.is_empty() {
        println!("No nodes found matching \"{keyword}\".");
        println!(
            "Registry has {} node(s). Try a broader keyword.",
            entries.len()
        );
        return Ok(());
    }

    println!("{} node(s) matching \"{}\":\n", matched.len(), keyword);
    for e in &matched {
        let short = e.name.strip_prefix("na-").unwrap_or(&e.name);
        let uc = if e.use_cases.is_empty() {
            String::new()
        } else {
            format!(" [{}]", e.use_cases.join(", "))
        };
        println!("  {short:12} v{:<8} — {}{uc}", e.version, e.description);
    }
    Ok(())
}

pub(crate) async fn cmd_install(name: &str) -> Result<()> {
    let node_name = if name.starts_with("na-") {
        name.to_string()
    } else {
        format!("na-{name}")
    };

    let entries = fetch_registry().await?;
    let _entry = entries
        .iter()
        .find(|e| e.name == node_name)
        .ok_or_else(|| anyhow::anyhow!("node '{node_name}' not found in registry"))?;

    let target = detect_target();
    println!("Installing {} for {} ...", node_name, target);

    let install_dir = determine_install_dir()?;
    std::fs::create_dir_all(&install_dir)?;
    let dest = install_dir.join(&node_name);

    let repo = "sonyarianto/ngalir";
    let api_url = format!("https://api.github.com/repos/{repo}/releases/latest");

    let client = reqwest::Client::builder()
        .user_agent("ngalir")
        .timeout(std::time::Duration::from_secs(10))
        .build()?;

    let release: Value = client.get(&api_url).send().await?.json().await?;
    let tag = release["tag_name"]
        .as_str()
        .context("failed to get latest release tag")?;

    let asset_name = format!("ngalir-{tag}-{target}.tar.gz");
    let dl_url = format!("https://github.com/{repo}/releases/download/{tag}/{asset_name}");

    println!("  downloading {asset_name} ...");
    let mut resp = client
        .get(&dl_url)
        .timeout(std::time::Duration::from_secs(300))
        .send()
        .await?;

    if !resp.status().is_success() {
        bail!(
            "download failed: HTTP {} (asset not found: {asset_name})",
            resp.status()
        );
    }

    let tmp = tempfile::NamedTempFile::new()?;
    let tmp_path = tmp.path().to_owned();

    let mut bytes = Vec::new();
    while let Some(chunk) = resp.chunk().await? {
        bytes.extend_from_slice(&chunk);
    }

    let cursor = std::io::Cursor::new(&bytes);
    let decoder = flate2::read::GzDecoder::new(cursor);
    let mut archive = tar::Archive::new(decoder);
    let mut found = false;

    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        if path == std::path::Path::new(&node_name) {
            let mut out = std::fs::File::create(&tmp_path)?;
            std::io::copy(&mut entry, &mut out)?;
            found = true;
            break;
        }
    }

    if !found {
        bail!("binary '{node_name}' not found in release archive");
    }

    std::fs::set_permissions(
        &tmp_path,
        std::os::unix::fs::PermissionsExt::from_mode(0o755),
    )?;
    std::fs::rename(&tmp_path, &dest)?;

    println!("  installed to {}", dest.display());

    if let Some(parent) = dest.parent() {
        if !on_path(parent) {
            println!(
                "  warning: {} is not on PATH. Add it or move the binary.",
                parent.display()
            );
        }
    }
    Ok(())
}

fn detect_target() -> String {
    let arch = std::env::consts::ARCH;
    let os = std::env::consts::OS;
    match (arch, os) {
        ("x86_64", "linux") => "x86_64-unknown-linux-gnu".into(),
        ("aarch64", "linux") => "aarch64-unknown-linux-gnu".into(),
        ("x86_64", "macos") => "x86_64-apple-darwin".into(),
        ("aarch64", "macos") => "aarch64-apple-darwin".into(),
        _ => format!("{arch}-unknown-{os}-gnu"),
    }
}

fn determine_install_dir() -> Result<PathBuf> {
    if let Ok(dir) = std::env::var("NGALIR_INSTALL_DIR") {
        return Ok(PathBuf::from(dir));
    }
    let home = dirs::home_dir().context("cannot determine home directory")?;
    let local_bin = home.join(".local").join("bin");
    if local_bin.exists() {
        return Ok(local_bin);
    }
    Ok(home.join(".local").join("bin"))
}

fn on_path(dir: &Path) -> bool {
    std::env::var("PATH")
        .ok()
        .map(|p| std::env::split_paths(&p).any(|d| d == dir))
        .unwrap_or(false)
}
