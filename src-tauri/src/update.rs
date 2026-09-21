use std::{fs, io::Write, path::PathBuf};

use reqwest::blocking::Client;
use semver::Version;
use serde::{Deserialize, Serialize};

use crate::filesystem::AppPaths;

const RELEASES_URL: &str = "https://api.github.com/repos/yibeixiaobai/dsh-gui-win0/releases?per_page=20";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeManifest {
  pub desktop_runtime_api: u32,
  pub harness_version: String,
  pub runtime_artifact: String,
  pub platform: String,
  pub architecture: String,
  pub protocol: ProtocolManifest,
  pub sha256: Option<String>,
  pub release_channel: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolManifest {
  pub name: String,
  pub server_version: Option<String>,
  pub methods: Vec<String>,
  pub notifications: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
  pub version: String,
  pub tag: String,
  pub release_url: String,
  pub installer_url: String,
  pub checksum_url: Option<String>,
  pub prerelease: bool,
}

#[derive(Debug, Deserialize)]
struct Release {
  tag_name: String,
  html_url: String,
  prerelease: bool,
  assets: Vec<Asset>,
}

#[derive(Debug, Deserialize)]
struct Asset {
  name: String,
  browser_download_url: String,
}

pub fn load_manifest() -> Result<RuntimeManifest, String> {
  let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    .parent().expect("src-tauri parent")
    .join("runtime")
    .join("dsh-runtime.json");

  let bytes = fs::read(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
  serde_json::from_slice(&bytes).map_err(|e| format!("invalid runtime manifest: {e}"))
}

pub fn check_update() -> Result<Option<UpdateInfo>, String> {
  let client = client()?;
  let releases = client.get(RELEASES_URL).send().map_err(|e| e.to_string())?
    .error_for_status().map_err(|e| e.to_string())?
    .json::<Vec<Release>>().map_err(|e| e.to_string())?;

  let current = Version::parse(env!("CARGO_PKG_VERSION")).map_err(|e| e.to_string())?;

  let mut newest: Option<(Version, UpdateInfo)> = None;
  for release in releases {
    let Some(raw) = release.tag_name.strip_prefix("dsh-desktop-v") else { continue };
    let Ok(version) = Version::parse(raw) else { continue };
    if version <= current { continue; }

    let Some(installer) = release.assets.iter().find(|asset| asset.name.ends_with("-windows-x64-setup.exe")) else { continue };
    let checksum = release.assets.iter()
      .find(|asset| asset.name == format!("{}.sha256", installer.name))
      .map(|asset| asset.browser_download_url.clone());

    let info = UpdateInfo {
      version: version.to_string(),
      tag: release.tag_name.clone(),
      release_url: release.html_url.clone(),
      installer_url: installer.browser_download_url.clone(),
      checksum_url: checksum,
      prerelease: release.prerelease,
    };

    if newest.as_ref().map(|(v, _)| version > *v).unwrap_or(true) {
      newest = Some((version, info));
    }
  }

  Ok(newest.map(|(_, info)| info))
}

pub fn download_installer<R: tauri::Runtime>(
  app: &tauri::AppHandle<R>,
  info: &UpdateInfo,
) -> Result<PathBuf, String> {
  let paths = AppPaths::from_app(app).map_err(|e| e.to_string())?;
  let lease = paths.create_temp_lease(&format!("update-{}", info.version)).map_err(|e| e.to_string())?;
  let installer = lease.join(format!("DSH-Desktop-{}-windows-x64-setup.exe", info.version));

  let client = client()?;
  let mut response = client.get(&info.installer_url).send().map_err(|e| e.to_string())?
    .error_for_status().map_err(|e| e.to_string())?;
  let mut file = fs::File::create(&installer).map_err(|e| e.to_string())?;
  response.copy_to(&mut file).map_err(|e| e.to_string())?;
  file.flush().map_err(|e| e.to_string())?;

  if let Some(checksum_url) = &info.checksum_url {
    let expected_text = client.get(checksum_url).send().map_err(|e| e.to_string())?
      .error_for_status().map_err(|e| e.to_string())?
      .text().map_err(|e| e.to_string())?;
    verify_sha256(&installer, &expected_text)?;
  }

  Ok(installer)
}

pub fn launch_installer(path: PathBuf) -> Result<(), String> {
  #[cfg(windows)]
  {
    std::process::Command::new(&path)
      .spawn()
      .map_err(|e| format!("failed to launch installer {}: {e}", path.display()))?;
    return Ok(());
  }

  #[cfg(not(windows))]
  {
    let _ = path;
    Err("the Desktop update installer is currently Windows-only".into())
  }
}

fn verify_sha256(path: &PathBuf, expected_text: &str) -> Result<(), String> {
  use sha2::{Digest, Sha256};

  let expected = expected_text.split_whitespace().next().unwrap_or_default().to_ascii_lowercase();
  if expected.len() != 64 {
    return Err("invalid SHA256 manifest".into());
  }

  let bytes = fs::read(path).map_err(|e| e.to_string())?;
  let actual = format!("{:x}", Sha256::digest(bytes));

  if actual != expected {
    return Err("downloaded installer SHA256 does not match release checksum".into());
  }
  Ok(())
}

fn client() -> Result<Client, String> {
  Client::builder()
    .user_agent("dsh-gui-win0")
    .build()
    .map_err(|e| e.to_string())
}
