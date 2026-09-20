use std::{fs, io, path::PathBuf};

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::filesystem::AppPaths;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopSettings {
  pub provider: String,
  pub model: String,
  #[serde(default)]
  pub reasoning_effort: Option<String>,
  pub workspace: Option<String>,
}

impl Default for DesktopSettings {
  fn default() -> Self {
    Self {
      provider: "deepseek-official".into(),
      model: String::new(),
      reasoning_effort: None,
      workspace: None,
    }
  }
}

pub fn load_settings<R: tauri::Runtime>(app: &AppHandle<R>) -> io::Result<DesktopSettings> {
  let paths = AppPaths::from_app(app).map_err(io::Error::other)?;
  if !paths.settings_file.exists() {
    return Ok(DesktopSettings::default());
  }
  let bytes = fs::read(paths.settings_file)?;
  serde_json::from_slice(&bytes).map_err(io::Error::other)
}

pub fn save_settings<R: tauri::Runtime>(
  app: &AppHandle<R>,
  settings: &DesktopSettings,
) -> io::Result<()> {
  let paths = AppPaths::from_app(app).map_err(io::Error::other)?;
  paths.ensure_layout()?;

  let bytes = serde_json::to_vec_pretty(settings).map_err(io::Error::other)?;
  let tmp = PathBuf::from(format!("{}.tmp", paths.settings_file.display()));
  fs::write(&tmp, bytes)?;
  fs::rename(tmp, paths.settings_file)?;
  Ok(())
}
