mod calc;
mod config;
mod eta_xml;
mod excel;
mod history;
mod importer;
mod models;
mod native_update;

use models::{CalcResult, FormData, HistoryEntry};
use excel::InvoiceSummaryRow;
use excel::HistoryExportRow;
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
fn export_history_registry(rows: Vec<HistoryExportRow>, file_path: String) -> Result<(), String> {
    excel::export_history_registry(&rows, &file_path)
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

#[derive(Clone, serde::Serialize)]
struct PoolImportFailure {
    file: String,
    error: String,
}

#[derive(Clone, serde::Serialize)]
struct PoolImportResult {
    imported: Vec<eta_xml::EtaInvoice>,
    failed: Vec<PoolImportFailure>,
}

/// Best-effort recovery for exports that omit <uuid>/<internalId> in the XML
/// wrapper: browser-extension bundles name each file with the ETA UUID as the
/// final dash-separated token of the file stem.
fn uuid_from_file_name(path: &str) -> Option<String> {
    let stem = std::path::Path::new(path).file_stem()?.to_string_lossy().to_string();
    let token = stem.rsplit('-').next()?.trim().to_string();
    if token.len() >= 10 && token.chars().all(|c| c.is_ascii_alphanumeric()) {
        Some(token)
    } else {
        None
    }
}

#[tauri::command]
fn import_to_pool(app: tauri::AppHandle, state: tauri::State<'_, DbState>, file_paths: Vec<String>) -> Result<PoolImportResult, String> {
    use tauri::Emitter;
    let guard = state.0.lock().map_err(|e| e.to_string())?;
    let conn = guard.as_ref().ok_or("DB not initialized".to_string())?;
    let mut imported = Vec::new();
    let mut failed = Vec::new();
    let total = file_paths.len();
    for (idx, path) in file_paths.iter().enumerate() {
        let base_name = std::path::Path::new(path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.clone());
        let _ = app.emit("pool-import-progress", serde_json::json!({
            "processed": idx,
            "total": total,
            "file": base_name,
            "imported": imported.len(),
            "failed": failed.len(),
        }));
        let xml_content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                failed.push(PoolImportFailure { file: base_name, error: format!("read failed: {}", e) });
                continue;
            }
        };
        let invoice = match eta_xml::parse_eta_xml(&xml_content) {
            Ok(mut inv) => {
                if inv.uuid.is_empty() || inv.invoice_id.is_empty() {
                    if let Some(token) = uuid_from_file_name(path) {
                        if inv.uuid.is_empty() { inv.uuid = token.clone(); }
                        if inv.invoice_id.is_empty() { inv.invoice_id = token; }
                    }
                }
                inv
            }
            Err(e) => {
                failed.push(PoolImportFailure { file: base_name, error: e });
                continue;
            }
        };
        match history::add_to_pool(conn, &invoice, &xml_content, &base_name) {
            Ok(outcome) => match outcome {
                history::PoolAddOutcome::Updated(_) | history::PoolAddOutcome::Inserted(_) => {
                    imported.push(invoice)
                }
            },
            Err(e) => failed.push(PoolImportFailure { file: base_name, error: e }),
        }
    }
    let _ = app.emit("pool-import-progress", serde_json::json!({
        "processed": total,
        "total": total,
        "file": "",
        "imported": imported.len(),
        "failed": failed.len(),
    }));
    Ok(PoolImportResult { imported, failed })
}

#[tauri::command]
fn list_invoice_pool(state: tauri::State<'_, DbState>) -> Result<Vec<history::PoolInvoice>, String> {
    let guard = state.0.lock().map_err(|e| e.to_string())?;
    let conn = guard.as_ref().ok_or("DB not initialized".to_string())?;
    history::list_pool(conn)
}

#[tauri::command]
fn list_invoice_pool_summary(state: tauri::State<'_, DbState>) -> Result<Vec<history::PoolInvoiceSummary>, String> {
    let guard = state.0.lock().map_err(|e| e.to_string())?;
    let conn = guard.as_ref().ok_or("DB not initialized".to_string())?;
    history::list_pool_summary(conn)
}

#[tauri::command]
fn mark_pool_invoice_used(state: tauri::State<'_, DbState>, id: i64, snapshot_id: i64, snapshot_label: String) -> Result<(), String> {
    let guard = state.0.lock().map_err(|e| e.to_string())?;
    let conn = guard.as_ref().ok_or("DB not initialized".to_string())?;
    history::mark_invoice_used(conn, id, snapshot_id, &snapshot_label)
}

#[tauri::command]
fn mark_pool_invoices_used(state: tauri::State<'_, DbState>, ids: Vec<i64>, snapshot_id: i64, snapshot_label: String) -> Result<(), String> {
    let guard = state.0.lock().map_err(|e| e.to_string())?;
    let conn = guard.as_ref().ok_or("DB not initialized".to_string())?;
    history::mark_invoices_used(conn, &ids, snapshot_id, &snapshot_label)
}

#[tauri::command]
fn mark_pool_invoice_available(state: tauri::State<'_, DbState>, id: i64) -> Result<(), String> {
    let guard = state.0.lock().map_err(|e| e.to_string())?;
    let conn = guard.as_ref().ok_or("DB not initialized".to_string())?;
    history::mark_invoice_available(conn, id)
}

#[tauri::command]
fn clean_unlabelled_claims(state: tauri::State<'_, DbState>) -> Result<usize, String> {
    let guard = state.0.lock().map_err(|e| e.to_string())?;
    let conn = guard.as_ref().ok_or("DB not initialized".to_string())?;
    history::clean_unlabelled_claims(conn)
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
fn validate_from_pool(state: tauri::State<'_, DbState>, ids: Vec<i64>, form_json: String) -> Result<Vec<eta_xml::ValidationResult>, String> {
    let guard = state.0.lock().map_err(|e| e.to_string())?;
    let conn = guard.as_ref().ok_or("DB not initialized".to_string())?;
    let pool = history::list_pool(conn)?;
    let mut results = Vec::new();
    for pool_id in &ids {
        let pool_inv = pool.iter().find(|p| p.id == *pool_id)
            .ok_or_else(|| format!("Pool row {} not found", pool_id))?;

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
                doc_status: pool_inv.doc_status.clone(),
                currency: pool_inv.currency.clone(),
                net_amount: pool_inv.net_amount,
                total_vat: pool_inv.total_vat,
                total_wht: pool_inv.total_wht,
                grand_total: pool_inv.grand_total,
                lines,
            }
        };
        let mut result = eta_xml::validate_eta_against_form(&invoice, &form_json)?;
        result.pool_id = Some(pool_inv.id);
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
            // Size the window to fit the monitor's available work area so the
            // app is not clipped on smaller displays (e.g. 1366x768 laptops).
            if let Some(win) = app.get_webview_window("main") {
                if let Ok(Some(monitor)) = win.current_monitor() {
                    let scale = monitor.scale_factor();
                    let area = monitor.work_area();
                    let avail_w = area.size.width as f64 / scale;
                    let avail_h = area.size.height as f64 / scale;
                    let target_w = avail_w.min(1400.0).max(900.0);
                    let target_h = avail_h.min(900.0).max(640.0);
                    let _ = win.set_size(tauri::LogicalSize::new(target_w, target_h));
                    let _ = win.center();
                }
            }

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
            export_history_registry,
            validate_eta_xml,
            import_to_pool,
            list_invoice_pool,
            list_invoice_pool_summary,
            mark_pool_invoice_used,
            mark_pool_invoices_used,
            mark_pool_invoice_available,
            clean_unlabelled_claims,
            delete_pool_invoice,
            request_pool_delete,
            reject_pool_delete,
            sync_pool_from_remote,
            validate_from_pool,
            native_update::check_native_update,
            native_update::install_native_update,
            native_update::update_source,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod export_tests {
    use crate::{calc, excel, models::FormData};

    #[test]
    fn exports_rejected_and_conditional_documents() {
        for decision in ["reject", "conditional"] {
            let mut data = FormData::default();
            data.doc_type = "import".into();
            data.doc_serial = format!("TEST-{decision}");
            data.final_decision = decision.into();
            if decision == "reject" {
                data.reject_reason = "Commercial invoice mismatch".into();
            } else {
                data.conditional_reason = "Pending SAD document".into();
            }
            let computed = calc::recalculate(&data);
            let out = format!("/tmp/opencode/export_test_{decision}.xlsx");
            excel::export_excel(&data, &computed, &out)
                .unwrap_or_else(|e| panic!("export {decision} failed: {e}"));
        }
    }

    #[test]
    fn repro_export_from_real_snapshot() {
        let Ok(path) = std::env::var("VOUCHIFY_REPRO") else {
            eprintln!("skipped: VOUCHIFY_REPRO not set");
            return;
        };
        let raw = std::fs::read_to_string(&path).expect("read snapshot json");
        let data: FormData = serde_json::from_str(&raw).expect("deserialize snapshot");
        println!("deserialize OK, doc_type={:?} decision={:?}", data.doc_type, data.final_decision);
        let computed = calc::recalculate(&data);
        excel::export_excel(&data, &computed, "/tmp/opencode/repro_out.xlsx")
            .expect("export real snapshot");
    }
}

