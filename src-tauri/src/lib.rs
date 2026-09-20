mod filesystem;
mod runtime;
mod settings;
mod update;

use tauri::{Manager, State};
use tauri_plugin_single_instance::Builder as SingleInstanceBuilder;

use crate::{
  filesystem::AppPaths,
  runtime::RuntimeSupervisor,
  settings::{load_settings, save_settings, DesktopSettings},
  update::{load_manifest, RuntimeManifest},
};

#[tauri::command]
fn runtime_status(
  app: tauri::AppHandle,
  supervisor: State<'_, RuntimeSupervisor>,
) -> Result<runtime::RuntimeSnapshot, String> {
  supervisor.snapshot(&app).map_err(|e| e.to_string())
}

#[tauri::command]
fn runtime_start(
  app: tauri::AppHandle,
  supervisor: State<'_, RuntimeSupervisor>,
  workspace: String,
  settings: DesktopSettings,
) -> Result<runtime::RuntimeSnapshot, String> {
  supervisor.start(&app, &workspace, &settings).map_err(|e| e.to_string())
}

#[tauri::command]
fn runtime_stop(
  app: tauri::AppHandle,
  supervisor: State<'_, RuntimeSupervisor>,
) -> Result<runtime::RuntimeSnapshot, String> {
  supervisor.stop().map_err(|e| e.to_string())?;
  supervisor.snapshot(&app).map_err(|e| e.to_string())
}

#[tauri::command]
fn runtime_request(
  supervisor: State<'_, RuntimeSupervisor>,
  method: String,
  params: Option<serde_json::Value>,
) -> Result<serde_json::Value, String> {
  supervisor.request(&method, params).map_err(|e| e.to_string())
}

#[tauri::command]
fn runtime_health_check(
  app: tauri::AppHandle,
  supervisor: State<'_, RuntimeSupervisor>,
) -> Result<runtime::HealthCheck, String> {
  supervisor.health_check(&app).map_err(|e| e.to_string())
}

#[tauri::command]
fn settings_get(app: tauri::AppHandle) -> Result<DesktopSettings, String> {
  load_settings(&app).map_err(|e| e.to_string())
}

#[tauri::command]
fn settings_save(app: tauri::AppHandle, settings: DesktopSettings) -> Result<(), String> {
  save_settings(&app, &settings).map_err(|e| e.to_string())
}

#[tauri::command]
fn runtime_manifest() -> Result<RuntimeManifest, String> {
  load_manifest()
}

pub fn run() {
  tauri::Builder::default()
    .plugin(
      SingleInstanceBuilder::new()
        .callback(|app, _argv, _cwd| {
          if let Some(window) = app.get_webview_window("main") {
            let _ = window.unminimize();
            let _ = window.set_focus();
          }
        })
        .build(),
    )
    .plugin(tauri_plugin_dialog::init())
    .manage(RuntimeSupervisor::new())
    .setup(|app| {
      let paths = AppPaths::from_app(app.handle()).expect("resolve Desktop AppData paths");
      paths.ensure_layout().expect("create Desktop AppData layout");
      paths.cleanup_stale_temp().expect("cleanup stale Desktop temp");
      Ok(())
    })
    .on_window_event(|window, event| {
      if let tauri::WindowEvent::CloseRequested { api, .. } = event {
        let state = window.state::<RuntimeSupervisor>();
        if state.is_running() {
          api.prevent_close();
          let _ = state.stop();
        }
      }
    })
    .invoke_handler(tauri::generate_handler![
      runtime_status,
      runtime_start,
      runtime_stop,
      runtime_request,
      runtime_health_check,
      runtime_manifest,
      settings_get,
      settings_save,
    ])
    .run(tauri::generate_context!())
    .expect("error while running DSH Desktop");
}
