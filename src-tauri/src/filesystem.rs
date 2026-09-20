use std::{
  fs,
  io::{self, Write},
  path::PathBuf,
  time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

const HARNESS_DIR: &str = "harness";
const DESKTOP_DIR: &str = "desktop";
const TEMP_DIR: &str = "temp";
const LOG_DIR: &str = "logs";
const SETTINGS_FILE: &str = "settings.json";
const STALE_AFTER: Duration = Duration::from_secs(24 * 60 * 60);
const MAX_LOG_BYTES: u64 = 10 * 1024 * 1024;
const MAX_LOG_FILES: u32 = 5;

#[derive(Clone, Debug)]
pub struct AppPaths {
  pub root: PathBuf,
  pub harness_home: PathBuf,
  pub desktop_home: PathBuf,
  pub temp_home: PathBuf,
  pub logs_home: PathBuf,
  pub settings_file: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
struct Lease {
  owner: String,
  created_at: u64,
  last_heartbeat_at: u64,
}

impl AppPaths {
  pub fn from_app<R: tauri::Runtime>(app: &AppHandle<R>) -> Result<Self, String> {
    let root = app.path().app_data_dir().map_err(|e| e.to_string())?.join("dsh-gui-win0");
    Ok(Self {
      harness_home: root.join(HARNESS_DIR),
      desktop_home: root.join(DESKTOP_DIR),
      temp_home: root.join(TEMP_DIR),
      logs_home: root.join(LOG_DIR),
      settings_file: root.join(DESKTOP_DIR).join(SETTINGS_FILE),
      root,
    })
  }

  pub fn ensure_layout(&self) -> io::Result<()> {
    fs::create_dir_all(&self.harness_home)?;
    fs::create_dir_all(&self.desktop_home)?;
    fs::create_dir_all(&self.temp_home)?;
    fs::create_dir_all(&self.logs_home)?;
    Ok(())
  }

  pub fn create_temp_lease(&self, purpose: &str) -> io::Result<PathBuf> {
    let now = unix_now();
    let dir = self.temp_home.join(format!("{}-{}", now, slug(purpose)));
    fs::create_dir_all(&dir)?;
    let lease = Lease { owner: "dsh-gui-win0".into(), created_at: now, last_heartbeat_at: now };
    fs::write(dir.join("lease.json"), serde_json::to_vec_pretty(&lease).expect("lease is serializable"))?;
    Ok(dir)
  }

  pub fn cleanup_stale_temp(&self) -> io::Result<()> {
    if !self.temp_home.exists() { return Ok(()); }
    let cutoff = unix_now().saturating_sub(STALE_AFTER.as_secs());
    for entry in fs::read_dir(&self.temp_home)? {
      let path = entry?.path();
      if !path.is_dir() { continue; }
      let Ok(bytes) = fs::read(path.join("lease.json")) else { continue };
      let Ok(lease) = serde_json::from_slice::<Lease>(&bytes) else { continue };
      if lease.owner != "dsh-gui-win0" || lease.last_heartbeat_at > cutoff { continue; }
      let _ = fs::remove_dir_all(path);
    }
    Ok(())
  }

  pub fn append_runtime_log(&self, line: &str) -> io::Result<()> {
    self.ensure_layout()?;
    let path = self.logs_home.join("runtime.log");
    let bytes = line.as_bytes();
    let current = fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
    if current.saturating_add(bytes.len() as u64 + 1) > MAX_LOG_BYTES {
      self.rotate_logs()?;
    }
    let mut file = fs::OpenOptions::new().create(true).append(true).open(path)?;
    file.write_all(bytes)?;
    file.write_all(b"\n")?;
    Ok(())
  }

  fn rotate_logs(&self) -> io::Result<()> {
    for index in (1..MAX_LOG_FILES).rev() {
      let from = self.logs_home.join(format!("runtime.log.{}", index));
      let to = self.logs_home.join(format!("runtime.log.{}", index + 1));
      if from.exists() {
        let _ = fs::remove_file(&to);
        fs::rename(from, to)?;
      }
    }
    let current = self.logs_home.join("runtime.log");
    if current.exists() {
      let first = self.logs_home.join("runtime.log.1");
      let _ = fs::remove_file(&first);
      fs::rename(current, first)?;
    }
    Ok(())
  }
}

fn unix_now() -> u64 {
  SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

fn slug(input: &str) -> String {
  input.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect()
}
