use std::{
  io::Write,
  path::{Path, PathBuf},
  process::{Child, ChildStdin, Command, Stdio},
  sync::{mpsc, Arc, Mutex},
  thread,
  time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tauri::{Emitter, Manager};

use crate::{
  filesystem::AppPaths,
  settings::DesktopSettings,
  update::{load_manifest, RuntimeManifest},
};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RuntimeStatus { Stopped, Starting, Running, Stopping, Crashed }

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeSnapshot {
  pub status: RuntimeStatus,
  pub pid: Option<u32>,
  pub runtime_version: String,
  pub harness_home: String,
  pub workspace: Option<String>,
  pub executable: Option<String>,
  pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HealthCheck { pub ok: bool, pub reason: String }

type PendingResult = Result<Value, String>;
type Pending = std::collections::HashMap<u64, mpsc::Sender<PendingResult>>;

struct RuntimeInner {
  status: RuntimeStatus,
  child: Option<Child>,
  stdin: Option<ChildStdin>,
  pid: Option<u32>,
  workspace: Option<PathBuf>,
  executable: Option<PathBuf>,
  last_error: Option<String>,
  pending: Pending,
}

#[derive(Clone)]
pub struct RuntimeSupervisor { inner: Arc<Mutex<RuntimeInner>> }

impl RuntimeSupervisor {
  pub fn new() -> Self {
    Self {
      inner: Arc::new(Mutex::new(RuntimeInner {
        status: RuntimeStatus::Stopped,
        child: None,
        stdin: None,
        pid: None,
        workspace: None,
        executable: None,
        last_error: None,
        pending: Default::default(),
      })),
    }
  }

  pub fn start<R: tauri::Runtime>(
    &self,
    app: &tauri::AppHandle<R>,
    workspace: &str,
    _settings: &DesktopSettings,
  ) -> Result<RuntimeSnapshot, RuntimeError> {
    let workspace_path = validate_workspace(workspace)?;
    let manifest = load_manifest().map_err(RuntimeError::Manifest)?;
    let paths = AppPaths::from_app(app).map_err(RuntimeError::Path)?;
    paths.ensure_layout().map_err(RuntimeError::Io)?;

    let mut inner = self.inner.lock().map_err(|_| RuntimeError::StatePoisoned)?;
    if matches!(inner.status, RuntimeStatus::Running | RuntimeStatus::Starting) {
      return Ok(snapshot_locked(&inner, &paths, &manifest));
    }

    let executable = resolve_runtime_executable(app)?;
    validate_runtime_artifact(&executable, &manifest)?;

    inner.status = RuntimeStatus::Starting;
    inner.workspace = Some(workspace_path.clone());
    inner.executable = Some(executable.clone());
    inner.last_error = None;
    inner.pending.clear();

    let mut command = Command::new(&executable);
    command
      .arg("--profile")
      .arg("sdk")
      .current_dir(&workspace_path)
      .env("DSH_HOME", &paths.harness_home)
      .stdin(Stdio::piped())
      .stdout(Stdio::piped())
      .stderr(Stdio::piped());

    #[cfg(windows)]
    {
      use std::os::windows::process::CommandExt;
      command.creation_flags(0x0800_0000);
    }

    let mut child = command.spawn().map_err(|e| {
      inner.status = RuntimeStatus::Crashed;
      inner.last_error = Some(e.to_string());
      RuntimeError::Spawn(e.to_string())
    })?;

    let pid = child.id();
    let stdin = child.stdin.take().ok_or_else(|| RuntimeError::Spawn("runtime stdin unavailable".into()))?;
    let stdout = child.stdout.take().ok_or_else(|| RuntimeError::Spawn("runtime stdout unavailable".into()))?;
    let stderr = child.stderr.take().ok_or_else(|| RuntimeError::Spawn("runtime stderr unavailable".into()))?;

    inner.pid = Some(pid);
    inner.stdin = Some(stdin);
    inner.child = Some(child);
    inner.status = RuntimeStatus::Running;

    let stdout_app = app.clone();
    let shared = Arc::clone(&self.inner);
    thread::spawn(move || {
      use std::io::{BufRead, BufReader};
      for line in BufReader::new(stdout).lines() {
        match line {
          Ok(line) => {
            let payload = crate::runtime::protocol::parse_json_line(&line)
              .unwrap_or_else(|| json!({ "raw": line }));

            if crate::runtime::protocol::is_response(&payload) {
              if let Some(id) = crate::runtime::protocol::response_id(&payload) {
                let waiter = shared.lock().ok().and_then(|mut guard| guard.pending.remove(&id));
                if let Some(waiter) = waiter {
                  let result = match payload.get("error") {
                    Some(error) => Err(error.to_string()),
                    None => Ok(payload),
                  };
                  let _ = waiter.send(result);
                  continue;
                }
              }
            }

            let _ = stdout_app.emit(
              "runtime:event",
              crate::runtime::protocol::RuntimeEvent {
                kind: crate::runtime::protocol::RuntimeEventKind::JsonRpc,
                payload,
              },
            );
          }
          Err(error) => {
            let _ = stdout_app.emit(
              "runtime:event",
              crate::runtime::protocol::RuntimeEvent {
                kind: crate::runtime::protocol::RuntimeEventKind::Error,
                payload: json!({ "message": error.to_string() }),
              },
            );
            break;
          }
        }
      }
    });

    let stderr_app = app.clone();
    thread::spawn(move || {
      use std::io::{BufRead, BufReader};
      for line in BufReader::new(stderr).lines() {
        match line {
          Ok(line) => {
            let _ = stderr_app.emit(
              "runtime:event",
              crate::runtime::protocol::RuntimeEvent {
                kind: crate::runtime::protocol::RuntimeEventKind::Stderr,
                payload: json!({ "line": line }),
              },
            );
          }
          Err(_) => break,
        }
      }
    });

    let monitor_app = app.clone();
    let shared = Arc::clone(&self.inner);
    thread::spawn(move || loop {
      let exit = {
        let mut guard = match shared.lock() { Ok(guard) => guard, Err(_) => return };
        let Some(child) = guard.child.as_mut() else { return };
        match child.try_wait() {
          Ok(result) => result,
          Err(error) => { guard.last_error = Some(error.to_string()); None }
        }
      };

      match exit {
        Some(status) => {
          let code = status.code();
          if let Ok(mut guard) = shared.lock() {
            guard.child = None;
            guard.stdin = None;
            guard.pid = None;
            let stopping = guard.status == RuntimeStatus::Stopping;
            let message = format!("runtime exited: {:?}", code);
            let pending = std::mem::take(&mut guard.pending);
            if stopping {
              guard.status = RuntimeStatus::Stopped;
              guard.last_error = None;
            } else {
              guard.status = RuntimeStatus::Crashed;
              guard.last_error = Some(message.clone());
            }
            for sender in pending.into_values() {
              let _ = sender.send(Err(message.clone()));
            }
          }

          let _ = monitor_app.emit(
            "runtime:event",
            crate::runtime::protocol::RuntimeEvent {
              kind: crate::runtime::protocol::RuntimeEventKind::Terminated,
              payload: json!({ "code": code }),
            },
          );
          return;
        }
        None => thread::sleep(Duration::from_millis(100)),
      }
    });

    Ok(snapshot_locked(&inner, &paths, &manifest))
  }

  pub fn request(&self, method: &str, params: Option<Value>) -> Result<Value, RuntimeError> {
    if method.is_empty() { return Err(RuntimeError::Protocol("method is empty".into())); }

    let request = crate::runtime::protocol::JsonRpcRequest::new(method, params);
    let line = request.to_line().map_err(|e| RuntimeError::Protocol(e.to_string()))?;
    let (sender, receiver) = mpsc::channel::<PendingResult>();

    let mut stdin = {
      let mut inner = self.inner.lock().map_err(|_| RuntimeError::StatePoisoned)?;
      if inner.status != RuntimeStatus::Running { return Err(RuntimeError::NotRunning); }
      inner.pending.insert(request.id, sender);
      inner.stdin.take().ok_or(RuntimeError::NotRunning)?
    };

    let result = stdin.write_all(line.as_bytes()).and_then(|_| stdin.flush());
    let mut inner = self.inner.lock().map_err(|_| RuntimeError::StatePoisoned)?;
    inner.stdin = Some(stdin);

    if let Err(error) = result {
      inner.pending.remove(&request.id);
      return Err(RuntimeError::Io(error));
    }
    drop(inner);

    match receiver.recv_timeout(Duration::from_secs(60)) {
      Ok(Ok(response)) => Ok(response),
      Ok(Err(error)) => Err(RuntimeError::Remote(error)),
      Err(mpsc::RecvTimeoutError::Timeout) => {
        if let Ok(mut guard) = self.inner.lock() { guard.pending.remove(&request.id); }
        Err(RuntimeError::Timeout(request.method))
      }
      Err(mpsc::RecvTimeoutError::Disconnected) => Err(RuntimeError::Protocol("runtime response channel closed".into())),
    }
  }

  pub fn stop(&self) -> Result<(), RuntimeError> {
    {
      let mut inner = self.inner.lock().map_err(|_| RuntimeError::StatePoisoned)?;
      if inner.status == RuntimeStatus::Stopped { return Ok(()); }
      inner.status = RuntimeStatus::Stopping;

      for sender in std::mem::take(&mut inner.pending).into_values() {
        let _ = sender.send(Err("runtime is stopping".into()));
      }

      if let Some(stdin) = inner.stdin.as_mut() {
        if let Ok(line) = crate::runtime::protocol::JsonRpcRequest::new("shutdown", None).to_line() {
          let _ = stdin.write_all(line.as_bytes());
          let _ = stdin.flush();
        }
      }
      inner.stdin.take();
    }

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
      let done = {
        let mut inner = self.inner.lock().map_err(|_| RuntimeError::StatePoisoned)?;
        match inner.child.as_mut() {
          None => true,
          Some(child) => match child.try_wait() {
            Ok(Some(_)) => true,
            Ok(None) if Instant::now() >= deadline => { let _ = child.kill(); let _ = child.try_wait(); true }
            Ok(None) => false,
            Err(_) => { let _ = child.kill(); let _ = child.try_wait(); true }
          }
        }
      };
      if done { break; }
      thread::sleep(Duration::from_millis(50));
    }

    if let Ok(mut inner) = self.inner.lock() {
      inner.status = RuntimeStatus::Stopped;
      inner.child = None;
      inner.stdin = None;
      inner.pid = None;
    }
    Ok(())
  }

  pub fn snapshot<R: tauri::Runtime>(&self, app: &tauri::AppHandle<R>) -> Result<RuntimeSnapshot, RuntimeError> {
    let manifest = load_manifest().map_err(RuntimeError::Manifest)?;
    let paths = AppPaths::from_app(app).map_err(RuntimeError::Path)?;
    let inner = self.inner.lock().map_err(|_| RuntimeError::StatePoisoned)?;
    Ok(snapshot_locked(&inner, &paths, &manifest))
  }

  pub fn health_check<R: tauri::Runtime>(&self, app: &tauri::AppHandle<R>) -> Result<HealthCheck, RuntimeError> {
    let snapshot = self.snapshot(app)?;
    if snapshot.status != RuntimeStatus::Running {
      return Ok(HealthCheck { ok: false, reason: format!("runtime status is {:?}", snapshot.status) });
    }
    if snapshot.pid.is_none() || snapshot.executable.is_none() {
      return Ok(HealthCheck { ok: false, reason: "runtime process metadata is incomplete".into() });
    }
    Ok(HealthCheck { ok: true, reason: "runtime process is running".into() })
  }

  pub fn is_running(&self) -> bool {
    self.inner.lock().map(|inner| inner.status == RuntimeStatus::Running).unwrap_or(false)
  }
}

fn snapshot_locked(inner: &RuntimeInner, paths: &AppPaths, manifest: &RuntimeManifest) -> RuntimeSnapshot {
  RuntimeSnapshot {
    status: inner.status,
    pid: inner.pid,
    runtime_version: manifest.harness_version.clone(),
    harness_home: paths.harness_home.display().to_string(),
    workspace: inner.workspace.as_ref().map(|p| p.display().to_string()),
    executable: inner.executable.as_ref().map(|p| p.display().to_string()),
    last_error: inner.last_error.clone(),
  }
}

fn validate_workspace(input: &str) -> Result<PathBuf, RuntimeError> {
  let path = PathBuf::from(input);
  if !path.is_absolute() { return Err(RuntimeError::Workspace("workspace must be absolute".into())); }
  if !path.is_dir() { return Err(RuntimeError::Workspace("workspace must be an existing directory".into())); }
  Ok(path)
}

fn resolve_runtime_executable<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Result<PathBuf, RuntimeError> {
  if cfg!(debug_assertions) {
    if let Ok(path) = std::env::var("DSH_RUNTIME_PATH") {
      let path = PathBuf::from(path);
      if path.is_file() { return Ok(path); }
    }
  }

  let resource_dir = app.path().resource_dir().map_err(|e| RuntimeError::Path(e.to_string()))?;
  let candidate = resource_dir.join("runtime").join("bin").join(runtime_name());
  if candidate.is_file() { Ok(candidate) }
  else { Err(RuntimeError::RuntimeMissing(candidate.display().to_string())) }
}

fn validate_runtime_artifact(executable: &Path, manifest: &RuntimeManifest) -> Result<(), RuntimeError> {
  let expected = manifest.runtime_artifact.to_lowercase();
  let actual = executable.file_stem().and_then(|n| n.to_str()).unwrap_or_default().to_lowercase();
  if actual != expected { return Err(RuntimeError::RuntimeMismatch(format!("runtime '{}' does not match pinned artifact '{}'", actual, expected))); }

  if let Some(expected_hash) = manifest.sha256.as_deref() {
    if !expected_hash.is_empty() {
      let bytes = std::fs::read(executable).map_err(RuntimeError::Io)?;
      let digest = Sha256::digest(&bytes);
      let actual_hash = format!("{digest:x}");
      if actual_hash != expected_hash.to_lowercase() {
        return Err(RuntimeError::RuntimeMismatch("runtime SHA256 does not match manifest".into()));
      }
    }
  }
  Ok(())
}

fn runtime_name() -> &'static str {
  if cfg!(windows) { "deepseek-harness-sdk-runtime-win-x64.exe" }
  else if cfg!(target_arch = "aarch64") { "deepseek-harness-sdk-runtime-macos-arm64" }
  else { "deepseek-harness-sdk-runtime-linux-x64" }
}

#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
  #[error("state lock poisoned")] StatePoisoned,
  #[error("runtime is not running")] NotRunning,
  #[error("runtime spawn failed: {0}")] Spawn(String),
  #[error("runtime missing: {0}")] RuntimeMissing(String),
  #[error("runtime mismatch: {0}")] RuntimeMismatch(String),
  #[error("workspace error: {0}")] Workspace(String),
  #[error("path error: {0}")] Path(String),
  #[error("filesystem error: {0}")] Io(std::io::Error),
  #[error("protocol error: {0}")] Protocol(String),
  #[error("remote runtime error: {0}")] Remote(String),
  #[error("request timed out: {0}")] Timeout(String),
  #[error("runtime manifest error: {0}")] Manifest(String),
}
