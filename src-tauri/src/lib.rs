mod calc;
mod config;
mod eta_xml;
mod excel;
mod history;
mod importer;
mod models;

use models::{CalcResult, FormData, HistoryEntry};
use excel::InvoiceSummaryRow;
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
fn update_history(
    state: tauri::State<'_, DbState>,
    id: i64,
    label: String,
    notes: String,
    data_json: String,
) -> Result<(), String> {
    let guard = state.0.lock().map_err(|e| e.to_string())?;
    let conn = guard.as_ref().ok_or("DB not initialized".to_string())?;
    history::update_snapshot(conn, id, &label, &notes, &data_json)
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

#[tauri::command]
fn export_invoice_summary(invoices: Vec<InvoiceSummaryRow>, date_from: String, date_to: String, file_path: String) -> Result<(), String> {
    excel::export_invoice_summary(&invoices, &date_from, &date_to, &file_path)
}

#[tauri::command]
fn export_validation_report(results: Vec<eta_xml::ValidationResult>, file_path: String) -> Result<(), String> {
    excel::export_validation_report(&results, &file_path)
}

#[tauri::command]
fn validate_eta_xml(file_paths: Vec<String>, form_json: String) -> Result<Vec<eta_xml::ValidationResult>, String> {
    let mut results = Vec::new();
    for path in &file_paths {
        let xml_content = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read XML file {}: {}", path, e))?;
        let invoice = eta_xml::parse_eta_xml(&xml_content)?;
        let result = eta_xml::validate_eta_against_form(&invoice, &form_json)?;
        results.push(result);
    }
    Ok(results)
}

#[tauri::command]
fn import_to_pool(state: tauri::State<'_, DbState>, file_paths: Vec<String>) -> Result<Vec<eta_xml::EtaInvoice>, String> {
    let guard = state.0.lock().map_err(|e| e.to_string())?;
    let conn = guard.as_ref().ok_or("DB not initialized".to_string())?;
    let mut imported = Vec::new();
    for path in &file_paths {
        let xml_content = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read XML file {}: {}", path, e))?;
        let invoice = eta_xml::parse_eta_xml(&xml_content)?;
        let file_name = std::path::Path::new(path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        history::add_to_pool(conn, &invoice, &xml_content, &file_name)?;
        imported.push(invoice);
    }
    Ok(imported)
}

#[tauri::command]
fn list_invoice_pool(state: tauri::State<'_, DbState>) -> Result<Vec<history::PoolInvoice>, String> {
    let guard = state.0.lock().map_err(|e| e.to_string())?;
    let conn = guard.as_ref().ok_or("DB not initialized".to_string())?;
    history::list_pool(conn)
}

#[tauri::command]
fn mark_pool_invoice_used(state: tauri::State<'_, DbState>, invoice_id: String, snapshot_id: i64, snapshot_label: String) -> Result<(), String> {
    let guard = state.0.lock().map_err(|e| e.to_string())?;
    let conn = guard.as_ref().ok_or("DB not initialized".to_string())?;
    history::mark_invoice_used(conn, &invoice_id, snapshot_id, &snapshot_label)
}

#[tauri::command]
fn mark_pool_invoices_used(state: tauri::State<'_, DbState>, invoice_ids: Vec<String>, snapshot_id: i64, snapshot_label: String) -> Result<(), String> {
    let guard = state.0.lock().map_err(|e| e.to_string())?;
    let conn = guard.as_ref().ok_or("DB not initialized".to_string())?;
    history::mark_invoices_used(conn, &invoice_ids, snapshot_id, &snapshot_label)
}

#[tauri::command]
fn mark_pool_invoice_available(state: tauri::State<'_, DbState>, invoice_id: String) -> Result<(), String> {
    let guard = state.0.lock().map_err(|e| e.to_string())?;
    let conn = guard.as_ref().ok_or("DB not initialized".to_string())?;
    history::mark_invoice_available(conn, &invoice_id)
}

#[tauri::command]
fn delete_pool_invoice(state: tauri::State<'_, DbState>, id: i64) -> Result<(), String> {
    let guard = state.0.lock().map_err(|e| e.to_string())?;
    let conn = guard.as_ref().ok_or("DB not initialized".to_string())?;
    history::delete_from_pool(conn, id)
}

#[tauri::command]
fn request_pool_delete(state: tauri::State<'_, DbState>, id: i64, requested_by: String) -> Result<(), String> {
    let guard = state.0.lock().map_err(|e| e.to_string())?;
    let conn = guard.as_ref().ok_or("DB not initialized".to_string())?;
    history::request_pool_delete(conn, id, &requested_by)
}

#[tauri::command]
fn reject_pool_delete(state: tauri::State<'_, DbState>, id: i64) -> Result<(), String> {
    let guard = state.0.lock().map_err(|e| e.to_string())?;
    let conn = guard.as_ref().ok_or("DB not initialized".to_string())?;
    history::reject_pool_delete(conn, id)
}

#[tauri::command]
fn sync_pool_from_remote(state: tauri::State<'_, DbState>, invoices: Vec<history::PoolInvoice>) -> Result<(), String> {
    let guard = state.0.lock().map_err(|e| e.to_string())?;
    let conn = guard.as_ref().ok_or("DB not initialized".to_string())?;
    for inv in &invoices {
        history::sync_pool_from_remote(conn, inv)?;
    }
    Ok(())
}

#[tauri::command]
fn validate_from_pool(state: tauri::State<'_, DbState>, invoice_ids: Vec<String>, form_json: String) -> Result<Vec<eta_xml::ValidationResult>, String> {
    let guard = state.0.lock().map_err(|e| e.to_string())?;
    let conn = guard.as_ref().ok_or("DB not initialized".to_string())?;
    let pool = history::list_pool(conn)?;
    let mut results = Vec::new();
    for inv_id in &invoice_ids {
        let pool_inv = pool.iter().find(|p| &p.invoice_id == inv_id)
            .ok_or_else(|| format!("Invoice {} not found in pool", inv_id))?;

        // Re-parse the original XML when available so results are always fresh,
        // regardless of when the invoice was imported.
        let invoice = if !pool_inv.raw_xml.is_empty() {
            eta_xml::parse_eta_xml(&pool_inv.raw_xml)?
        } else {
            let lines: Vec<eta_xml::EtaInvoiceLine> = serde_json::from_str(&pool_inv.lines_json)
                .unwrap_or_default();
            eta_xml::EtaInvoice {
                invoice_id: pool_inv.invoice_id.clone(),
                uuid: pool_inv.uuid.clone(),
                issue_date: pool_inv.issue_date.clone(),
                invoice_type_code: String::new(),
                seller_tax_id: pool_inv.seller_tax_id.clone(),
                seller_name: pool_inv.seller_name.clone(),
                buyer_tax_id: pool_inv.buyer_tax_id.clone(),
                buyer_name: pool_inv.buyer_name.clone(),
                currency: pool_inv.currency.clone(),
                net_amount: pool_inv.net_amount,
                total_vat: pool_inv.total_vat,
                total_wht: pool_inv.total_wht,
                grand_total: pool_inv.grand_total,
                lines,
            }
        };
        let result = eta_xml::validate_eta_against_form(&invoice, &form_json)?;
        results.push(result);
    }
    Ok(results)
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
    #[cfg(target_os = "linux")]
    {
        std::env::set_var("GDK_BACKEND", "x11");
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(DbState(Mutex::new(None)))
        .setup(|app| {
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
            update_history,
            load_history,
            delete_history,
            check_serial_exists,
            export_excel,
            export_invoice_summary,
            export_validation_report,
            validate_eta_xml,
            import_to_pool,
            list_invoice_pool,
            mark_pool_invoice_used,
            mark_pool_invoices_used,
            mark_pool_invoice_available,
            delete_pool_invoice,
            request_pool_delete,
            reject_pool_delete,
            sync_pool_from_remote,
            validate_from_pool,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
