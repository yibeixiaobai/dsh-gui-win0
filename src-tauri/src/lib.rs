mod filesystem;
mod runtime;
mod session;
mod settings;
mod update;

use tauri::{Manager, State};
use tauri_plugin_single_instance::Builder as SingleInstanceBuilder;

use crate::{
  runtime::RuntimeSupervisor,
  session::SessionSummary,
  settings::{load_settings, save_settings, DesktopSettings},
  update::{check_update, download_installer, launch_installer, load_manifest, RuntimeManifest, UpdateInfo},
};

#[tauri::command]
fn runtime_status(app: tauri::AppHandle, supervisor: State<'_, RuntimeSupervisor>) -> Result<runtime::RuntimeSnapshot, String> {
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
fn runtime_stop(app: tauri::AppHandle, supervisor: State<'_, RuntimeSupervisor>) -> Result<runtime::RuntimeSnapshot, String> {
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

#[tauri::command]
fn sessions_list(app: tauri::AppHandle) -> Result<Vec<SessionSummary>, String> {
  session::list(&app).map_err(|e| e.to_string())
}

#[tauri::command]
fn session_register(app: tauri::AppHandle, id: String, workspace: String, title: Option<String>) -> Result<SessionSummary, String> {
  session::register(&app, &id, &workspace, title.as_deref()).map_err(|e| e.to_string())
}

#[tauri::command]
fn session_set_status(app: tauri::AppHandle, id: String, status: String) -> Result<(), String> {
  session::set_status(&app, &id, &status).map_err(|e| e.to_string())
}

#[tauri::command]
fn session_forget(app: tauri::AppHandle, id: String) -> Result<(), String> {
  session::forget(&app, &id).map_err(|e| e.to_string())
}

#[tauri::command]
fn update_check() -> Result<Option<UpdateInfo>, String> {
  check_update()
}

#[tauri::command]
fn update_install(app: tauri::AppHandle, info: UpdateInfo) -> Result<(), String> {
  let installer = download_installer(&app, &info)?;
  launch_installer(installer)?;
  let exit_app = app.clone();
  std::thread::spawn(move || {
    std::thread::sleep(std::time::Duration::from_millis(800));
    exit_app.exit(0);
  });
  Ok(())
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
      let paths = crate::filesystem::AppPaths::from_app(app.handle())
        .map_err(tauri::Error::Anyhow)?;
      paths.ensure_layout().map_err(tauri::Error::Io)?;
      paths.cleanup_stale_temp().map_err(tauri::Error::Io)?;
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
      sessions_list,
      session_register,
      session_set_status,
      session_forget,
      update_check,
      update_install
    ])
    .run(tauri::generate_context!())
    .expect("error while running DSH Desktop");
}
