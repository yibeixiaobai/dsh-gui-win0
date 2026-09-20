use std::{fs, path::PathBuf};

use serde::{Deserialize, Serialize};

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

pub fn load_manifest() -> Result<RuntimeManifest, String> {
  let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    .parent()
    .expect("src-tauri parent")
    .join("runtime")
    .join("dsh-runtime.json");

  let bytes = fs::read(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
  serde_json::from_slice(&bytes).map_err(|e| format!("invalid runtime manifest: {e}"))
}
