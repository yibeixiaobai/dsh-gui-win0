use std::{fs, io, time::{SystemTime, UNIX_EPOCH}};

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::filesystem::AppPaths;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
  pub id: String,
  pub title: String,
  pub workspace: String,
  pub created_at: u64,
  pub last_used_at: u64,
  pub status: String,
}

fn sessions_file<R: tauri::Runtime>(app: &AppHandle<R>) -> io::Result<std::path::PathBuf> {
  let paths = AppPaths::from_app(app).map_err(io::Error::other)?;
  paths.ensure_layout()?;
  Ok(paths.desktop_home.join("sessions.json"))
}

fn load<R: tauri::Runtime>(app: &AppHandle<R>) -> io::Result<Vec<SessionSummary>> {
  let path = sessions_file(app)?;
  if !path.exists() { return Ok(Vec::new()); }
  let bytes = fs::read(path)?;
  serde_json::from_slice(&bytes).map_err(io::Error::other)
}

fn save<R: tauri::Runtime>(app: &AppHandle<R>, sessions: &[SessionSummary]) -> io::Result<()> {
  let path = sessions_file(app)?;
  let tmp = path.with_extension("json.tmp");
  fs::write(&tmp, serde_json::to_vec_pretty(sessions).map_err(io::Error::other)?)?;
  fs::rename(tmp, path)?;
  Ok(())
}

pub fn list<R: tauri::Runtime>(app: &AppHandle<R>) -> io::Result<Vec<SessionSummary>> {
  let mut items = load(app)?;
  items.sort_by_key(|item| std::cmp::Reverse(item.last_used_at));
  Ok(items)
}

pub fn register<R: tauri::Runtime>(
  app: &AppHandle<R>,
  id: &str,
  workspace: &str,
  title: Option<&str>,
) -> io::Result<SessionSummary> {
  let mut items = load(app)?;
  let now = unix_now();
  if let Some(item) = items.iter_mut().find(|item| item.id == id) {
    item.last_used_at = now;
    if let Some(title) = title.filter(|v| !v.trim().is_empty()) {
      item.title = title.trim().chars().take(80).collect();
    }
    item.workspace = workspace.to_owned();
    item.status = "active".into();
    let result = item.clone();
    save(app, &items)?;
    return Ok(result);
  }

  let result = SessionSummary {
    id: id.to_owned(),
    title: title.filter(|v| !v.trim().is_empty()).unwrap_or("New session").trim().chars().take(80).collect(),
    workspace: workspace.to_owned(),
    created_at: now,
    last_used_at: now,
    status: "active".into(),
  };
  items.push(result.clone());
  save(app, &items)?;
  Ok(result)
}

pub fn set_status<R: tauri::Runtime>(app: &AppHandle<R>, id: &str, status: &str) -> io::Result<()> {
  let mut items = load(app)?;
  if let Some(item) = items.iter_mut().find(|item| item.id == id) {
    item.status = status.to_owned();
    item.last_used_at = unix_now();
    save(app, &items)?;
  }
  Ok(())
}

pub fn forget<R: tauri::Runtime>(app: &AppHandle<R>, id: &str) -> io::Result<()> {
  let mut items = load(app)?;
  items.retain(|item| item.id != id);
  save(app, &items)
}

fn unix_now() -> u64 {
  SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}
