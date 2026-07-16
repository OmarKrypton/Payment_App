mod calc;
mod config;
mod excel;
mod history;
mod importer;
mod models;

use models::{CalcResult, FormData, HistoryEntry};
use rusqlite::Connection;
use std::sync::Mutex;
use tauri::{Emitter, Listener, Manager};
use serde::{Deserialize, Serialize};

struct DbState(Mutex<Option<Connection>>);

#[tauri::command]
fn ping() -> bool { true }

#[tauri::command]
fn recalculate(data: FormData) -> CalcResult {
    calc::recalculate(&data)
}

#[tauri::command]
fn load_config() -> FormData {
    config::load_config()
}

#[tauri::command]
fn save_config(data: FormData) -> Result<(), String> {
    config::save_config(&data)
}

#[tauri::command]
fn compute_retention(val_1b: String, ret_rate: String) -> f64 {
    let base: f64 = val_1b.parse().unwrap_or(0.0);
    let rate: f64 = ret_rate.trim_end_matches('%').parse().unwrap_or(0.0);
    (base * rate / 100.0 * 100.0).round() / 100.0
}

#[tauri::command]
fn list_history(state: tauri::State<'_, DbState>, search: String) -> Result<Vec<HistoryEntry>, String> {
    let guard = state.0.lock().map_err(|e| e.to_string())?;
    let conn = guard.as_ref().ok_or("DB not initialized".to_string())?;
    history::list_snapshots(conn, &search)
}

#[tauri::command]
fn save_history(
    state: tauri::State<'_, DbState>,
    label: String,
    notes: String,
    data_json: String,
) -> Result<i64, String> {
    let guard = state.0.lock().map_err(|e| e.to_string())?;
    let conn = guard.as_ref().ok_or("DB not initialized".to_string())?;
    history::save_snapshot(conn, &label, &notes, &data_json)
}

#[tauri::command]
fn load_history(state: tauri::State<'_, DbState>, id: i64) -> Result<String, String> {
    let guard = state.0.lock().map_err(|e| e.to_string())?;
    let conn = guard.as_ref().ok_or("DB not initialized".to_string())?;
    history::load_snapshot(conn, id)
}

#[tauri::command]
fn check_serial_exists(state: tauri::State<'_, DbState>, serial: String) -> Result<bool, String> {
    let guard = state.0.lock().map_err(|e| e.to_string())?;
    let conn = guard.as_ref().ok_or("DB not initialized".to_string())?;
    history::check_serial_exists(conn, &serial)
}

#[tauri::command]
fn delete_history(state: tauri::State<'_, DbState>, id: i64) -> Result<(), String> {
    let guard = state.0.lock().map_err(|e| e.to_string())?;
    let conn = guard.as_ref().ok_or("DB not initialized".to_string())?;
    history::delete_snapshot(conn, id)
}

#[tauri::command]
fn export_excel(data: FormData, computed: CalcResult, file_path: String) -> Result<(), String> {
    excel::export_excel(&data, &computed, &file_path)
}

#[derive(Clone, Serialize, Deserialize)]
struct StartImportPayload {
    #[serde(rename = "filePath")]
    file_path: String,
}

#[derive(Clone, Serialize)]
struct ImportProgress {
    status: String,
    message: String,
}

fn start_import_in_background(app: tauri::AppHandle, file_path: String) {
    std::thread::spawn(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let app_clone = app.clone();
            let progress_cb: importer::ProgressFn = Box::new(move |stage, message| {
                let _ = app_clone.emit("import-progress", ImportProgress {
                    status: stage.to_string(),
                    message: message.to_string(),
                });
            });
            importer::import_pdf_with_progress(&file_path, &Some(progress_cb))
        }));
        match result {
            Ok(Ok(data)) => {
                let _ = app.emit("import-complete", data);
            }
            Ok(Err(e)) => {
                let _ = app.emit("import-error", ImportProgress {
                    status: "error".into(),
                    message: e,
                });
            }
            Err(panic_err) => {
                let msg = if let Some(s) = panic_err.downcast_ref::<&str>() {
                    s.to_string()
                } else if let Some(s) = panic_err.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "unknown panic during import".into()
                };
                let _ = app.emit("import-error", ImportProgress {
                    status: "error".into(),
                    message: msg,
                });
            }
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(DbState(Mutex::new(None)))
        .setup(|app| {
            #[cfg(target_os = "linux")]
            std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
            let app_dir = app.path().app_data_dir().expect("no app data dir");
            std::fs::create_dir_all(&app_dir).expect("failed to create app data dir");
            let db_path = app_dir.join("history.db");
            let conn = history::init_db(&db_path).expect("failed to init db");
            let state = app.state::<DbState>();
            *state.0.lock().unwrap() = Some(conn);

            // Listen for import requests from frontend
            let app_handle = app.handle().clone();
            app.listen("start-import", move |event| {
                let payload: StartImportPayload = match serde_json::from_str(event.payload()) {
                    Ok(p) => p,
                    Err(e) => {
                        eprintln!("start-import: invalid payload: {} (raw: {})", e, event.payload());
                        return;
                    }
                };
                start_import_in_background(app_handle.clone(), payload.file_path);
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            ping,
            recalculate,
            load_config,
            save_config,
            compute_retention,
            list_history,
            save_history,
            load_history,
            delete_history,
            check_serial_exists,
            export_excel,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
