use crate::models::HistoryEntry;
use crate::eta_xml::EtaInvoice;
use rusqlite::{params, Connection};
use std::path::Path;

pub fn init_db(db_path: &Path) -> Result<Connection, String> {
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let conn = Connection::open(db_path).map_err(|e| e.to_string())?;
    init_schema(&conn)?;
    Ok(conn)
}

/// Create/upgrade the schema on an open connection (also used by tests with
/// in-memory databases).
pub fn init_schema(conn: &Connection) -> Result<(), String> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS snapshots (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            label TEXT NOT NULL,
            notes TEXT DEFAULT '',
            created_at TEXT NOT NULL,
            data TEXT NOT NULL
        )",
        [],
    )
    .map_err(|e| e.to_string())?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS eta_invoices (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            invoice_id TEXT NOT NULL,
            uuid TEXT DEFAULT '',
            seller_tax_id TEXT DEFAULT '',
            seller_name TEXT DEFAULT '',
            buyer_tax_id TEXT DEFAULT '',
            buyer_name TEXT DEFAULT '',
            issue_date TEXT DEFAULT '',
            currency TEXT DEFAULT '',
            net_amount REAL DEFAULT 0,
            total_vat REAL DEFAULT 0,
            total_wht REAL DEFAULT 0,
            grand_total REAL DEFAULT 0,
            lines_json TEXT DEFAULT '[]',
            raw_xml TEXT DEFAULT '',
            file_name TEXT DEFAULT '',
            doc_status TEXT DEFAULT 'Valid',
            status TEXT DEFAULT 'available',
            used_by_snapshot_id INTEGER,
            used_by_label TEXT DEFAULT '',
            delete_requested_at TEXT DEFAULT NULL,
            delete_requested_by TEXT DEFAULT '',
            created_at TEXT NOT NULL
        )",
        [],
    )
    .map_err(|e| e.to_string())?;
    conn.execute(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_eta_invoice_id ON eta_invoices(invoice_id)",
        [],
    )
    .map_err(|e| e.to_string())?;
    // Migration: add raw_xml column to existing tables
    let cols: Vec<String> = conn
        .prepare("PRAGMA table_info(eta_invoices)")
        .map_err(|e| e.to_string())?
        .query_map([], |r| r.get::<_, String>(1))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    if !cols.iter().any(|c| c == "raw_xml") {
        conn.execute("ALTER TABLE eta_invoices ADD COLUMN raw_xml TEXT DEFAULT ''", [])
            .map_err(|e| e.to_string())?;
    }
    if !cols.iter().any(|c| c == "file_name") {
        conn.execute("ALTER TABLE eta_invoices ADD COLUMN file_name TEXT DEFAULT ''", [])
            .map_err(|e| e.to_string())?;
    }
    if !cols.iter().any(|c| c == "delete_requested_at") {
        conn.execute("ALTER TABLE eta_invoices ADD COLUMN delete_requested_at TEXT DEFAULT NULL", [])
            .map_err(|e| e.to_string())?;
    }
    if !cols.iter().any(|c| c == "delete_requested_by") {
        conn.execute("ALTER TABLE eta_invoices ADD COLUMN delete_requested_by TEXT DEFAULT ''", [])
            .map_err(|e| e.to_string())?;
    }
    if !cols.iter().any(|c| c == "doc_status") {
        conn.execute("ALTER TABLE eta_invoices ADD COLUMN doc_status TEXT DEFAULT 'Valid'", [])
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn save_snapshot(conn: &Connection, label: &str, notes: &str, data_json: &str) -> Result<i64, String> {
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    conn.execute(
        "INSERT INTO snapshots (label, notes, created_at, data) VALUES (?1, ?2, ?3, ?4)",
        params![label, notes, now, data_json],
    )
    .map_err(|e| e.to_string())?;
    Ok(conn.last_insert_rowid())
}

pub fn update_snapshot(conn: &Connection, id: i64, label: &str, notes: &str, data_json: &str) -> Result<(), String> {
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    conn.execute(
        "UPDATE snapshots SET label = ?1, notes = ?2, data = ?3, created_at = ?4 WHERE id = ?5",
        params![label, notes, data_json, now, id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn list_snapshots(conn: &Connection, search: &str) -> Result<Vec<HistoryEntry>, String> {
    let mut result = Vec::new();

    if search.is_empty() {
        let mut stmt = conn
            .prepare("SELECT id, label, notes, created_at, data FROM snapshots ORDER BY created_at DESC")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok(HistoryEntry {
                    id: row.get(0)?,
                    label: row.get(1)?,
                    notes: row.get(2)?,
                    created_at: row.get(3)?,
                    data_json: row.get(4)?,
                })
            })
            .map_err(|e| e.to_string())?;
        for row in rows {
            result.push(row.map_err(|e| e.to_string())?);
        }
    } else {
        let pattern = format!("%{}%", search);
        let mut stmt = conn
            .prepare("SELECT id, label, notes, created_at, data FROM snapshots WHERE label LIKE ?1 OR notes LIKE ?1 ORDER BY created_at DESC")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![pattern], |row| {
                Ok(HistoryEntry {
                    id: row.get(0)?,
                    label: row.get(1)?,
                    notes: row.get(2)?,
                    created_at: row.get(3)?,
                    data_json: row.get(4)?,
                })
            })
            .map_err(|e| e.to_string())?;
        for row in rows {
            result.push(row.map_err(|e| e.to_string())?);
        }
    }

    Ok(result)
}

pub fn load_snapshot(conn: &Connection, id: i64) -> Result<String, String> {
    conn.query_row(
        "SELECT data FROM snapshots WHERE id = ?1",
        params![id],
        |row| row.get::<_, String>(0),
    )
    .map_err(|e| e.to_string())
}

pub fn delete_snapshot(conn: &Connection, id: i64) -> Result<(), String> {
    conn.execute("DELETE FROM snapshots WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn check_serial_exists(conn: &Connection, serial: &str) -> Result<bool, String> {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM snapshots WHERE label = ?1",
            params![serial],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    Ok(count > 0)
}

// ── ETA Invoice Pool ──

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PoolInvoice {
    #[serde(default)]
    pub id: i64,
    pub invoice_id: String,
    pub uuid: String,
    pub seller_tax_id: String,
    pub seller_name: String,
    pub buyer_tax_id: String,
    pub buyer_name: String,
    pub issue_date: String,
    pub currency: String,
    pub net_amount: f64,
    pub total_vat: f64,
    pub total_wht: f64,
    pub grand_total: f64,
    pub     lines_json: String,
    #[serde(default)]
    pub raw_xml: String,
    #[serde(default)]
    pub file_name: String,
    /// Document state from ETA: Valid / Rejected / Cancelled. Rejected or
    /// cancelled invoices stay visible but can never be claimed or validated.
    #[serde(default = "default_doc_status", deserialize_with = "deserialize_doc_status")]
    pub doc_status: String,
    pub status: String,
    pub used_by_snapshot_id: Option<i64>,
    pub used_by_label: String,
    #[serde(default)]
    pub delete_requested_at: Option<String>,
    #[serde(default)]
    pub delete_requested_by: String,
    pub created_at: String,
}

// Absent/empty doc_status must stay EMPTY here so sync_pool_from_remote can
// distinguish "remote has no opinion" (keep local value) from a real "Valid".
// list_pool normalizes empty to "Valid" on read.
fn default_doc_status() -> String { String::new() }

fn deserialize_doc_status<'de, D>(d: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let opt: Option<String> = serde::Deserialize::deserialize(d)?;
    Ok(opt.unwrap_or_default())
}
/// Outcome of adding an invoice to the pool.
pub enum PoolAddOutcome {
    /// New row inserted; payload is the rowid.
    Inserted(_i64),
    /// Existing row refreshed (same invoice, newer data or genuine status change).
    Updated(_i64),
    /// A different submission of the same internalID was skipped because a
    /// Valid version is already stored and the incoming one is Rejected/Cancelled.
    /// Suppliers resubmit corrected invoices under the same internalID; the
    /// rejected attempt must never mask the accepted one.
    Superseded,
}

pub fn add_to_pool(conn: &Connection, invoice: &EtaInvoice, raw_xml: &str, file_name: &str) -> Result<PoolAddOutcome, String> {
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let lines_json = serde_json::to_string(&invoice.lines).unwrap_or_else(|_| "[]".to_string());
    let incoming_status = if invoice.doc_status.is_empty() { "Valid".to_string() } else { invoice.doc_status.clone() };

    // Duplicate internalID handling: ETA allows resubmissions under the same
    // internalID (each with its own UUID). Decide whether this submission may
    // replace what is stored:
    //   - same UUID            -> genuine refresh (e.g. accepted then cancelled later)
    //   - invalid -> valid     -> upgrade to the accepted version
    //   - valid -> invalid     -> skip (separate rejected attempt)
    //   - same class           -> refresh with the newest data
    if let Ok((existing_uuid, existing_status)) = conn.query_row(
        "SELECT uuid, doc_status FROM eta_invoices WHERE invoice_id = ?1",
        params![invoice.invoice_id],
        |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
    ) {
        let existing_bad = existing_status != "Valid";
        let incoming_bad = incoming_status != "Valid";
        if existing_uuid != invoice.uuid && !existing_bad && incoming_bad {
            return Ok(PoolAddOutcome::Superseded);
        }
        conn.execute(
            "UPDATE eta_invoices SET uuid=?2, seller_tax_id=?3, seller_name=?4, buyer_tax_id=?5, buyer_name=?6,
             issue_date=?7, currency=?8, net_amount=?9, total_vat=?10, total_wht=?11, grand_total=?12,
             lines_json=?13, raw_xml=?14, file_name=?15, doc_status=?16 WHERE invoice_id=?1",
            params![
                invoice.invoice_id, invoice.uuid, invoice.seller_tax_id, invoice.seller_name,
                invoice.buyer_tax_id, invoice.buyer_name, invoice.issue_date, invoice.currency,
                invoice.net_amount, invoice.total_vat, invoice.total_wht, invoice.grand_total,
                lines_json, raw_xml, file_name, incoming_status
            ],
        ).map_err(|e| e.to_string())?;
        let id: i64 = conn.query_row("SELECT id FROM eta_invoices WHERE invoice_id = ?1", params![invoice.invoice_id], |r| r.get(0)).map_err(|e| e.to_string())?;
        return Ok(PoolAddOutcome::Updated(id));
    }

    conn.execute(
        "INSERT INTO eta_invoices (invoice_id, uuid, seller_tax_id, seller_name, buyer_tax_id, buyer_name, issue_date, currency, net_amount, total_vat, total_wht, grand_total, lines_json, raw_xml, file_name, doc_status, status, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, 'available', ?17)",
        params![
            invoice.invoice_id, invoice.uuid, invoice.seller_tax_id, invoice.seller_name,
            invoice.buyer_tax_id, invoice.buyer_name, invoice.issue_date, invoice.currency,
            invoice.net_amount, invoice.total_vat, invoice.total_wht, invoice.grand_total,
            lines_json, raw_xml, file_name, incoming_status, now
        ],
    ).map_err(|e| e.to_string())?;
    Ok(PoolAddOutcome::Inserted(conn.last_insert_rowid()))
}

pub fn list_pool(conn: &Connection) -> Result<Vec<PoolInvoice>, String> {
    let mut result = Vec::new();
    let mut stmt = conn
        .prepare("SELECT id, invoice_id, uuid, seller_tax_id, seller_name, buyer_tax_id, buyer_name, issue_date, currency, net_amount, total_vat, total_wht, grand_total, lines_json, raw_xml, file_name, doc_status, status, used_by_snapshot_id, used_by_label, delete_requested_at, delete_requested_by, created_at FROM eta_invoices ORDER BY created_at DESC")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok(PoolInvoice {
                id: row.get(0)?,
                invoice_id: row.get(1)?,
                uuid: row.get(2)?,
                seller_tax_id: row.get(3)?,
                seller_name: row.get(4)?,
                buyer_tax_id: row.get(5)?,
                buyer_name: row.get(6)?,
                issue_date: row.get(7)?,
                currency: row.get(8)?,
                net_amount: row.get(9)?,
                total_vat: row.get(10)?,
                total_wht: row.get(11)?,
                grand_total: row.get(12)?,
                lines_json: row.get(13)?,
                raw_xml: row.get(14)?,
                file_name: row.get(15)?,
                doc_status: {
                    let s: String = row.get(16)?;
                    if s.is_empty() { "Valid".to_string() } else { s }
                },
                status: row.get(17)?,
                used_by_snapshot_id: row.get(18)?,
                used_by_label: row.get(19)?,
                delete_requested_at: row.get(20)?,
                delete_requested_by: row.get(21)?,
                created_at: row.get(22)?,
            })
        })
        .map_err(|e| e.to_string())?;
    for row in rows {
        result.push(row.map_err(|e| e.to_string())?);
    }
    Ok(result)
}

pub fn mark_invoice_used(conn: &Connection, invoice_id: &str, snapshot_id: i64, snapshot_label: &str) -> Result<(), String> {
    conn.execute(
        "UPDATE eta_invoices SET status = 'used', used_by_snapshot_id = ?1, used_by_label = ?2 WHERE invoice_id = ?3",
        params![snapshot_id, snapshot_label, invoice_id],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn mark_invoices_used(conn: &Connection, invoice_ids: &[String], snapshot_id: i64, snapshot_label: &str) -> Result<(), String> {
    for invoice_id in invoice_ids {
        conn.execute(
            "UPDATE eta_invoices SET status = 'used', used_by_snapshot_id = ?1, used_by_label = ?2 WHERE invoice_id = ?3",
            params![snapshot_id, snapshot_label, invoice_id],
        ).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn mark_invoice_available(conn: &Connection, invoice_id: &str) -> Result<(), String> {
    conn.execute(
        "UPDATE eta_invoices SET status = 'available', used_by_snapshot_id = NULL, used_by_label = '' WHERE invoice_id = ?1",
        params![invoice_id],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

/// Downgrades any claim that has no serial label back to available. A claim
/// without a serial is meaningless (the serial is what links the invoice to a
/// document), so such rows must not linger as "used".
pub fn clean_unlabelled_claims(conn: &Connection) -> Result<usize, String> {
    conn.execute(
        "UPDATE eta_invoices SET status = 'available', used_by_snapshot_id = NULL, used_by_label = ''
         WHERE status = 'used' AND (used_by_label IS NULL OR used_by_label = '')",
        [],
    ).map_err(|e| e.to_string())
}

pub fn delete_from_pool(conn: &Connection, id: i64) -> Result<(), String> {
    conn.execute("DELETE FROM eta_invoices WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn request_pool_delete(conn: &Connection, id: i64, requested_by: &str) -> Result<(), String> {
    let now = chrono::Local::now().to_rfc3339();
    conn.execute(
        "UPDATE eta_invoices SET delete_requested_at = ?1, delete_requested_by = ?2 WHERE id = ?3 AND delete_requested_at IS NULL",
        params![now, requested_by, id],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn reject_pool_delete(conn: &Connection, id: i64) -> Result<(), String> {
    conn.execute(
        "UPDATE eta_invoices SET delete_requested_at = NULL, delete_requested_by = '' WHERE id = ?1",
        params![id],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

// Upsert an invoice pulled from the shared Supabase pool into local SQLite so
// that local validation/attach operations work even for invoices imported by
// other users. Keeps raw_xml so validate_from_pool can re-parse fresh.
pub fn sync_pool_from_remote(conn: &Connection, inv: &PoolInvoice) -> Result<(), String> {
    conn.execute(
        "INSERT INTO eta_invoices (invoice_id, uuid, seller_tax_id, seller_name, buyer_tax_id, buyer_name, issue_date, currency, net_amount, total_vat, total_wht, grand_total, lines_json, raw_xml, file_name, doc_status, status, used_by_label, delete_requested_at, delete_requested_by, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21)
         ON CONFLICT(invoice_id) DO UPDATE SET
            uuid = excluded.uuid,
            seller_tax_id = excluded.seller_tax_id,
            seller_name = excluded.seller_name,
            buyer_tax_id = excluded.buyer_tax_id,
            buyer_name = excluded.buyer_name,
            issue_date = excluded.issue_date,
            currency = excluded.currency,
            net_amount = excluded.net_amount,
            total_vat = excluded.total_vat,
            total_wht = excluded.total_wht,
            grand_total = excluded.grand_total,
            lines_json = excluded.lines_json,
            raw_xml = excluded.raw_xml,
            file_name = excluded.file_name,
            doc_status = CASE WHEN excluded.doc_status IS NULL OR excluded.doc_status = '' THEN eta_invoices.doc_status ELSE excluded.doc_status END,
            status = excluded.status,
            used_by_label = excluded.used_by_label,
            delete_requested_at = excluded.delete_requested_at,
            delete_requested_by = excluded.delete_requested_by",
        params![
            inv.invoice_id, inv.uuid, inv.seller_tax_id, inv.seller_name,
            inv.buyer_tax_id, inv.buyer_name, inv.issue_date, inv.currency,
            inv.net_amount, inv.total_vat, inv.total_wht, inv.grand_total,
            inv.lines_json, inv.raw_xml, inv.file_name, inv.doc_status,
            inv.status, inv.used_by_label, inv.delete_requested_at, inv.delete_requested_by,
            inv.created_at,
        ],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod pool_supersede_tests {
    use super::*;
    use crate::eta_xml::EtaInvoice;

    fn sample(id: &str, uuid: &str, doc_status: &str) -> EtaInvoice {
        EtaInvoice {
            invoice_id: id.into(),
            uuid: uuid.into(),
            issue_date: "2026-08-01T00:00:00Z".into(),
            invoice_type_code: "I".into(),
            seller_tax_id: "100000000".into(),
            seller_name: "Test Seller".into(),
            buyer_tax_id: "200000000".into(),
            buyer_name: "Test Buyer".into(),
            currency: "EGP".into(),
            net_amount: 100.0,
            total_vat: 14.0,
            total_wht: 0.0,
            grand_total: 114.0,
            lines: vec![],
            doc_status: doc_status.into(),
        }
    }

    fn row(conn: &Connection, id: &str) -> (String, String, String) {
        conn.query_row(
            "SELECT uuid, doc_status, status FROM eta_invoices WHERE invoice_id = ?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        ).unwrap()
    }

    #[test]
    fn duplicate_internalid_resolution_rules() {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();

        // 1. Valid version first
        let o = add_to_pool(&conn, &sample("X", "UUID-A", "Valid"), "<x/>", "a.xml").unwrap();
        assert!(matches!(o, PoolAddOutcome::Inserted(_)));

        // 2. Different submission (other UUID), Rejected -> must NOT mask the valid one
        let o = add_to_pool(&conn, &sample("X", "UUID-B", "Rejected"), "<x/>", "b.xml").unwrap();
        assert!(matches!(o, PoolAddOutcome::Superseded));
        assert_eq!(row(&conn, "X"), ("UUID-A".into(), "Valid".into(), "available".into()));

        // 3. Same submission (same UUID) later becomes Rejected (genuine state change)
        let o = add_to_pool(&conn, &sample("X", "UUID-A", "Rejected"), "<x/>", "a.xml").unwrap();
        assert!(matches!(o, PoolAddOutcome::Updated(_)));
        assert_eq!(row(&conn, "X").1, "Rejected");

        // 4. Now a corrected resubmission (new UUID, Valid) upgrades the row
        let o = add_to_pool(&conn, &sample("X", "UUID-C", "Valid"), "<x/>", "c.xml").unwrap();
        assert!(matches!(o, PoolAddOutcome::Updated(_)));
        assert_eq!(row(&conn, "X"), ("UUID-C".into(), "Valid".into(), "available".into()));

        // 5. Claim survives an upgrade (status untouched by add_to_pool)
        mark_invoice_used(&conn, "X", 1, "SER-1").unwrap();
        add_to_pool(&conn, &sample("X", "UUID-D", "Rejected"), "<x/>", "d.xml").unwrap();
        assert_eq!(row(&conn, "X"), ("UUID-C".into(), "Valid".into(), "used".into()));
    }
}
