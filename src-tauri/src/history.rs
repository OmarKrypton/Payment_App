use crate::models::HistoryEntry;
use crate::eta_xml::EtaInvoice;
use rusqlite::{params, Connection};
use std::path::Path;

pub fn init_db(db_path: &Path) -> Result<Connection, String> {
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let conn = Connection::open(db_path).map_err(|e| e.to_string())?;
    // A previous instance may still be closing its connection. Wait briefly
    // instead of failing the whole startup with SQLITE_BUSY.
    conn.busy_timeout(std::time::Duration::from_secs(15))
        .map_err(|e| e.to_string())?;
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
            exchange_rate REAL DEFAULT 0,
            doc_kind TEXT DEFAULT '',
            references_json TEXT DEFAULT '[]',
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
    // Identity of a pool invoice is the ETA document UUID.  The same invoice can
    // be resubmitted under different attempt UUIDs, so a rejected/cancelled
    // attempt never masks, overwrites, or collides with a valid one.  Drop any
    // legacy single-column or composite unique indexes and mirror that with a
    // unique index on uuid (skipping empty uuids, which are just unmatched).
    for idx in ["idx_eta_invoice_id", "idx_eta_invoice_seller"] {
        let exists: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM pragma_index_list('eta_invoices') WHERE name = ?1",
                params![idx],
                |r| r.get(0),
            )
            .unwrap_or(false);
        if exists {
            conn.execute(&format!("DROP INDEX {}", idx), [])
                .map_err(|e| e.to_string())?;
        }
    }
    conn.execute(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_eta_invoice_uuid ON eta_invoices(uuid) WHERE uuid <> ''",
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
    // ETA rate the invoice figures are expressed at (1.0 for EGP). Local,
    // derived: filled on import / pull / repair from the raw XML, so no shared
    // schema change is required.
    if !cols.iter().any(|c| c == "exchange_rate") {
        conn.execute("ALTER TABLE eta_invoices ADD COLUMN exchange_rate REAL DEFAULT 0", [])
            .map_err(|e| e.to_string())?;
    }
    // Document kind (I invoice / C credit note / D debit note) and the ETA
    // `references` UUIDs a credit note points at. Both are derived locally from
    // `raw_xml` (like exchange_rate), so the shared Supabase schema is unchanged.
    if !cols.iter().any(|c| c == "doc_kind") {
        conn.execute("ALTER TABLE eta_invoices ADD COLUMN doc_kind TEXT DEFAULT ''", [])
            .map_err(|e| e.to_string())?;
    }
    if !cols.iter().any(|c| c == "references_json") {
        conn.execute("ALTER TABLE eta_invoices ADD COLUMN references_json TEXT DEFAULT '[]'", [])
            .map_err(|e| e.to_string())?;
    }
    // These repairs used to run one statement per row in autocommit mode. On
    // Windows (especially with antivirus scanning the database) that can take
    // long enough for the window to be marked "Not Responding". One transaction
    // commits all backfills together and is dramatically faster.
    conn.execute_batch("BEGIN").map_err(|e| e.to_string())?;
    let repair = (|| -> Result<(), String> {
        // Guarantee no pool row is left with an empty / legacy uuid, and collapse
        // any rows that are the same document as another (they used to multiply
        // when a file without an embedded uuid was re-imported).
        normalize_synthetic_uuids(conn)?;
        // Backfill line items / VAT for pool rows an older build could not parse.
        repair_pool_lines(conn)?;
        // Backfill document kind + credit-note references for older rows.
        repair_pool_meta(conn)?;
        crate::entries::init_schema(conn)?;
        Ok(())
    })();
    match repair {
        Ok(()) => conn.execute_batch("COMMIT").map_err(|e| e.to_string())?,
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK");
            return Err(e);
        }
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
    // created_at records when the snapshot was first saved. Full edits update
    // the payload and label only, matching the cloud update path; otherwise a
    // local edit jumps to the top of History while the same edit on another
    // device keeps its original date.
    conn.execute(
        "UPDATE snapshots SET label = ?1, notes = ?2, data = ?3 WHERE id = ?4",
        params![label, notes, data_json, id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Update only the snapshot payload. Used by metadata-only changes such as the
/// Paid/Unpaid toggle: the document keeps its original position and date in the
/// History Browser instead of jumping to the top as a full edit would.
pub fn update_snapshot_data(conn: &Connection, id: i64, data_json: &str) -> Result<(), String> {
    conn.execute(
        "UPDATE snapshots SET data = ?1 WHERE id = ?2",
        params![data_json, id],
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
    /// ETA rate the figures are expressed at (1.0 for EGP). Local/derived — not
    /// part of the shared Supabase schema.
    #[serde(default)]
    pub exchange_rate: f64,
    /// I = invoice, C = credit note, D = debit note. Local/derived.
    #[serde(default)]
    pub doc_kind: String,
    /// JSON array of ETA UUIDs a credit note references (empty otherwise).
    #[serde(default)]
    pub references_json: String,
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
#[allow(dead_code)]
/// Outcome of adding an invoice to the pool.
pub enum PoolAddOutcome {
    /// New row inserted; payload is the rowid.
    Inserted(i64),
    /// Existing row refreshed (same uuid, newer data or genuine status change).
    Updated(i64),
}

/// A UUID is "synthetic" when it did not come from the ETA document itself:
/// empty, a legacy backfill (`legacy-<rowid>`), or a deterministic fallback we
/// generated (`GEN:…`). A real ETA UUID always wins over a synthetic one.
pub fn is_synthetic_pool_uuid(uuid: &str) -> bool {
    let u = uuid.trim();
    u.is_empty() || u.starts_with("legacy-") || u.starts_with("GEN:")
}

fn fnv1a_hex(s: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{:016x}", h)
}

/// Deterministic, never-empty UUID for an invoice whose ETA UUID is missing.
/// Built from stable identity fields so the same document derives the same
/// value on every device — and so it can be reproduced in SQL as
/// `'GEN:' || invoice_id || ':' || seller_tax_id`.
pub fn derived_pool_uuid(invoice_id: &str, seller_tax_id: &str, file_name: &str, raw_xml: &str) -> String {
    let iid = invoice_id.trim();
    if !iid.is_empty() {
        return format!("GEN:{}:{}", iid, seller_tax_id.trim());
    }
    let fname = file_name.trim();
    if !fname.is_empty() {
        return format!("GEN:file:{}", fname);
    }
    format!("GEN:raw:{}", fnv1a_hex(raw_xml))
}

/// Never returns an empty uuid: keeps a real ETA uuid, otherwise derives one.
pub fn resolve_pool_uuid(uuid: &str, invoice_id: &str, seller_tax_id: &str, file_name: &str, raw_xml: &str) -> String {
    let u = uuid.trim();
    if u.is_empty() || u.starts_with("legacy-") {
        derived_pool_uuid(invoice_id, seller_tax_id, file_name, raw_xml)
    } else {
        u.to_string()
    }
}

/// Decide which existing row an incoming invoice should update, so re-imports
/// and cross-device syncs converge instead of duplicating:
///   * exact uuid match -> that row;
///   * incoming carries a real uuid while the same (invoice_id, seller_tax_id)
///     already has only a synthetic row -> upgrade that row (write the real id);
///   * incoming carries a synthetic uuid while the document already has a real
///     uuid -> update the real row and keep its uuid.
/// Genuine resubmissions (different real uuids) match nothing and get their own
/// row, as before. Returns (row id, uuid to write).
fn reconcile_pool_row(
    conn: &Connection,
    invoice_id: &str,
    seller_tax_id: &str,
    incoming_uuid: &str,
) -> Result<Option<(i64, String)>, String> {
    if let Ok(id) = conn.query_row(
        "SELECT id FROM eta_invoices WHERE uuid = ?1",
        params![incoming_uuid],
        |r| r.get::<_, i64>(0),
    ) {
        return Ok(Some((id, incoming_uuid.to_string())));
    }
    if invoice_id.trim().is_empty() {
        return Ok(None);
    }
    let mut stmt = conn
        .prepare("SELECT id, uuid FROM eta_invoices WHERE invoice_id = ?1 AND seller_tax_id = ?2")
        .map_err(|e| e.to_string())?;
    let candidates: Vec<(i64, String)> = stmt
        .query_map(params![invoice_id, seller_tax_id], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    if is_synthetic_pool_uuid(incoming_uuid) {
        if let Some((id, existing)) = candidates.iter().find(|(_, u)| !is_synthetic_pool_uuid(u)) {
            return Ok(Some((*id, existing.clone())));
        }
    } else if let Some((id, _)) = candidates.iter().find(|(_, u)| is_synthetic_pool_uuid(u)) {
        return Ok(Some((*id, incoming_uuid.to_string())));
    }
    Ok(None)
}

/// Carry a claim (used status / delete request) from a row about to be removed
/// onto the row that survives, so de-duplication never loses a claim.
fn merge_claim_into(conn: &Connection, from_id: i64, to_id: i64) -> Result<(), String> {
    let (status, snap, label, del_at, del_by): (String, Option<i64>, String, Option<String>, String) = conn
        .query_row(
            "SELECT status, used_by_snapshot_id, used_by_label, delete_requested_at, delete_requested_by FROM eta_invoices WHERE id = ?1",
            params![from_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .map_err(|e| e.to_string())?;
    if status == "used" {
        let to_used: bool = conn
            .query_row("SELECT status = 'used' FROM eta_invoices WHERE id = ?1", params![to_id], |r| r.get(0))
            .unwrap_or(false);
        if !to_used {
            conn.execute(
                "UPDATE eta_invoices SET status = 'used', used_by_snapshot_id = ?2, used_by_label = ?3 WHERE id = ?1",
                params![to_id, snap, label],
            )
            .map_err(|e| e.to_string())?;
        }
    }
    if del_at.is_some() {
        conn.execute(
            "UPDATE eta_invoices SET delete_requested_at = ?2, delete_requested_by = ?3 WHERE id = ?1 AND delete_requested_at IS NULL",
            params![to_id, del_at, del_by],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// One-time repair for databases written before UUIDs were guaranteed non-empty:
/// give every synthetic row a deterministic uuid, drop rows that are the same
/// document as a real-uuid row (or as another synthetic row), and keep claims.
/// Returns how many rows were changed or removed.
pub fn normalize_synthetic_uuids(conn: &Connection) -> Result<usize, String> {
    let rows: Vec<(i64, String, String, String, String)> = {
        let mut stmt = conn
            .prepare("SELECT id, invoice_id, seller_tax_id, file_name, raw_xml FROM eta_invoices WHERE uuid IS NULL OR uuid = '' OR uuid LIKE 'legacy-%'")
            .map_err(|e| e.to_string())?;
        let collected = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        collected
    };
    let mut changed = 0usize;
    for (id, invoice_id, seller_tax_id, file_name, raw_xml) in rows {
        if !invoice_id.trim().is_empty() {
            let real: Option<i64> = conn
                .query_row(
                    "SELECT id FROM eta_invoices WHERE id <> ?1 AND invoice_id = ?2 AND seller_tax_id = ?3 \
                     AND uuid <> '' AND uuid NOT LIKE 'legacy-%' AND uuid NOT LIKE 'GEN:%' \
                     ORDER BY (status = 'used') DESC, id ASC LIMIT 1",
                    params![id, invoice_id, seller_tax_id],
                    |r| r.get(0),
                )
                .ok();
            if let Some(keep) = real {
                merge_claim_into(conn, id, keep)?;
                conn.execute("DELETE FROM eta_invoices WHERE id = ?1", params![id])
                    .map_err(|e| e.to_string())?;
                changed += 1;
                continue;
            }
        }
        let derived = derived_pool_uuid(&invoice_id, &seller_tax_id, &file_name, &raw_xml);
        let clash: Option<i64> = conn
            .query_row(
                "SELECT id FROM eta_invoices WHERE id <> ?1 AND uuid = ?2",
                params![id, derived],
                |r| r.get(0),
            )
            .ok();
        if let Some(keep) = clash {
            merge_claim_into(conn, id, keep)?;
            conn.execute("DELETE FROM eta_invoices WHERE id = ?1", params![id])
                .map_err(|e| e.to_string())?;
        } else {
            conn.execute("UPDATE eta_invoices SET uuid = ?2 WHERE id = ?1", params![id, derived])
                .map_err(|e| e.to_string())?;
        }
        changed += 1;
    }
    Ok(changed)
}

pub fn add_to_pool(conn: &Connection, invoice: &EtaInvoice, raw_xml: &str, file_name: &str) -> Result<PoolAddOutcome, String> {
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let lines_json = serde_json::to_string(&invoice.lines).unwrap_or_else(|_| "[]".to_string());
    let incoming_status = if invoice.doc_status.is_empty() { "Valid".to_string() } else { invoice.doc_status.clone() };
    let doc_kind = crate::eta_xml::normalize_doc_kind(&invoice.invoice_type_code);
    let references_json = serde_json::to_string(&invoice.references).unwrap_or_else(|_| "[]".to_string());

    // Identity is the ETA document UUID, and it is NEVER empty: when the file
    // carries no uuid we derive a deterministic one (GEN:…) so a re-import
    // updates the same row instead of piling up duplicates.
    let resolved = resolve_pool_uuid(&invoice.uuid, &invoice.invoice_id, &invoice.seller_tax_id, file_name, raw_xml);

    // Reconcile so a real ETA uuid always wins over a synthetic one and a
    // synthetic uuid never adds a second row for a document that already has
    // its real uuid. Genuine resubmissions (different real uuids) still get
    // their own row.
    if let Some((id, final_uuid)) = reconcile_pool_row(conn, &invoice.invoice_id, &invoice.seller_tax_id, &resolved)? {
        conn.execute(
            "UPDATE eta_invoices SET invoice_id=?2, uuid=?3, seller_tax_id=?4, seller_name=?5, buyer_tax_id=?6, buyer_name=?7,
             issue_date=?8, currency=?9, net_amount=?10, total_vat=?11, total_wht=?12, grand_total=?13,
             lines_json=?14, raw_xml=?15, file_name=?16, doc_status=?17, exchange_rate=?18,
             doc_kind=?19, references_json=?20 WHERE id=?1",
            params![
                id, invoice.invoice_id, final_uuid, invoice.seller_tax_id, invoice.seller_name, invoice.buyer_tax_id,
                invoice.buyer_name, invoice.issue_date, invoice.currency, invoice.net_amount,
                invoice.total_vat, invoice.total_wht, invoice.grand_total, lines_json,
                raw_xml, file_name, incoming_status, invoice.exchange_rate, doc_kind, references_json
            ],
        ).map_err(|e| e.to_string())?;
        return Ok(PoolAddOutcome::Updated(id));
    }

    conn.execute(
        "INSERT INTO eta_invoices (invoice_id, uuid, seller_tax_id, seller_name, buyer_tax_id, buyer_name, issue_date, currency, net_amount, total_vat, total_wht, grand_total, lines_json, raw_xml, file_name, doc_status, status, created_at, exchange_rate, doc_kind, references_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, 'available', ?17, ?18, ?19, ?20)",
        params![
            invoice.invoice_id, resolved, invoice.seller_tax_id, invoice.seller_name,
            invoice.buyer_tax_id, invoice.buyer_name, invoice.issue_date, invoice.currency,
            invoice.net_amount, invoice.total_vat, invoice.total_wht, invoice.grand_total,
            lines_json, raw_xml, file_name, incoming_status, now, invoice.exchange_rate,
            doc_kind, references_json
        ],
    ).map_err(|e| e.to_string())?;
    Ok(PoolAddOutcome::Inserted(conn.last_insert_rowid()))
}

pub fn list_pool(conn: &Connection) -> Result<Vec<PoolInvoice>, String> {
    let mut result = Vec::new();
    let mut stmt = conn
        .prepare("SELECT id, invoice_id, uuid, seller_tax_id, seller_name, buyer_tax_id, buyer_name, issue_date, currency, net_amount, total_vat, total_wht, grand_total, lines_json, raw_xml, file_name, doc_status, status, used_by_snapshot_id, used_by_label, delete_requested_at, delete_requested_by, created_at, exchange_rate, doc_kind, references_json FROM eta_invoices ORDER BY created_at DESC")
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
                exchange_rate: row.get(23).unwrap_or(0.0),
                doc_kind: row.get(24).unwrap_or_default(),
                references_json: row.get(25).unwrap_or_else(|_| "[]".to_string()),
            })
        })
        .map_err(|e| e.to_string())?;
    for row in rows {
        result.push(row.map_err(|e| e.to_string())?);
    }
    Ok(result)
}

/// Fetch full pool rows only for the given ETA uuids. Used so the sync can pull
/// the (large) `raw_xml`/`lines_json` payload for just the rows it is about to
/// push, instead of shipping the whole pool into the UI.
pub fn list_pool_by_uuids(conn: &Connection, uuids: &[String]) -> Result<Vec<PoolInvoice>, String> {
    if uuids.is_empty() {
        return Ok(Vec::new());
    }
    let mut result = Vec::new();
    for chunk in uuids.chunks(500) {
        let placeholders = vec!["?"; chunk.len()].join(",");
        let sql = format!(
            "SELECT id, invoice_id, uuid, seller_tax_id, seller_name, buyer_tax_id, buyer_name, issue_date, currency, net_amount, total_vat, total_wht, grand_total, lines_json, raw_xml, file_name, doc_status, status, used_by_snapshot_id, used_by_label, delete_requested_at, delete_requested_by, created_at, exchange_rate, doc_kind, references_json FROM eta_invoices WHERE uuid IN ({})",
            placeholders
        );
        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(chunk.iter()), |row| {
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
                    exchange_rate: row.get(23).unwrap_or(0.0),
                    doc_kind: row.get(24).unwrap_or_default(),
                    references_json: row.get(25).unwrap_or_else(|_| "[]".to_string()),
                })
            })
            .map_err(|e| e.to_string())?;
        for row in rows {
            result.push(row.map_err(|e| e.to_string())?);
        }
    }
    Ok(result)
}

#[derive(serde::Serialize)]
pub struct PoolRatedLine {
    pub description: String,
    pub vat_rate: f64,
}

/// Compact pool row for the Suppliers tab: only what it needs to show a
/// supplier's invoices and derive per-item VAT rates — no `raw_xml`/`lines_json`
/// payload beyond the description + VAT rate of each rated line.
#[derive(serde::Serialize)]
pub struct PoolSupplierRow {
    pub invoice_id: String,
    pub seller_tax_id: String,
    pub seller_name: String,
    pub doc_status: String,
    pub net_amount: f64,
    pub total_vat: f64,
    pub rated_lines: Vec<PoolRatedLine>,
}

pub fn list_pool_supplier_rows(conn: &Connection) -> Result<Vec<PoolSupplierRow>, String> {
    let mut stmt = conn
        .prepare("SELECT invoice_id, seller_tax_id, seller_name, doc_status, net_amount, total_vat, lines_json, doc_kind FROM eta_invoices WHERE UPPER(COALESCE(doc_kind, '')) <> 'C'")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            let lines_json: String = r.get(6)?;
            let rated_lines: Vec<PoolRatedLine> = serde_json::from_str::<Vec<serde_json::Value>>(&lines_json)
                .unwrap_or_default()
                .into_iter()
                .filter_map(|l| {
                    let vr = l.get("vat_rate").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let va = l.get("vat_amount").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let lt = l.get("line_total").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    // Prefer the declared rate. When a line carries a tax amount
                    // but no rate (some stamp/fee taxes omit it) derive the
                    // effective rate from that line's *own* figures. Never use
                    // the invoice total — it sums every added tax, so a rate
                    // derived from it is meaningless (e.g. 6.5M% on a 0.01
                    // "fees" line).
                    let mut rate = vr;
                    if rate <= 0.0 && va > 0.0 && lt > 0.0 {
                        let r = (va / lt * 100.0 * 100.0).round() / 100.0;
                        if r > 0.0 && r <= 100.0 {
                            rate = r;
                        }
                    }
                    if rate <= 0.0 {
                        return None;
                    }
                    Some(PoolRatedLine {
                        description: l.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        vat_rate: rate,
                    })
                })
                .collect();
            Ok(PoolSupplierRow {
                invoice_id: r.get(0)?,
                seller_tax_id: r.get(1)?,
                seller_name: r.get(2)?,
                doc_status: {
                    let s: String = r.get(3)?;
                    if s.is_empty() { "Valid".to_string() } else { s }
                },
                net_amount: r.get(4)?,
                total_vat: r.get(5)?,
                rated_lines,
            })
        })
        .map_err(|e| e.to_string())?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Re-parse pool rows whose line items were never captured. Older builds did not
/// understand the ETA portal's escaped-XML export, so those rows stored an empty
/// `lines_json` (and no VAT). Re-running the parser on the kept `raw_xml` fills
/// them in. Cheap and idempotent: only rows that still have no lines and whose
/// raw XML actually contains invoice lines are touched.
pub fn repair_pool_lines(conn: &Connection) -> Result<usize, String> {
    let raws: Vec<(i64, String)> = {
        let mut stmt = conn
            .prepare(
                "SELECT id, raw_xml FROM eta_invoices
                 WHERE raw_xml LIKE '%invoiceLine%'
                   AND (
                     lines_json IS NULL OR lines_json = '' OR lines_json = '[]'
                     OR (currency NOT IN ('EGP', 'egp', '')
                         AND (exchange_rate IS NULL OR exchange_rate = 0))
                   )",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
            .map_err(|e| e.to_string())?;
        rows.filter_map(|r| r.ok()).collect()
    };
    let mut fixed = 0usize;
    for (id, raw) in raws {
        let Ok(inv) = crate::eta_xml::parse_eta_xml(&raw) else { continue };
        if inv.lines.is_empty() {
            continue;
        }
        let lines_json = serde_json::to_string(&inv.lines).unwrap_or_else(|_| "[]".into());
        // A foreign-currency export parses into the *sold* currency, so adopt its
        // converted amounts (an earlier build that could not derive the rate left
        // the EGP totals under a foreign-currency label); otherwise keep the
        // wrapper's EGP totals.
        let fx = inv.currency != "EGP" && !inv.currency.is_empty();
        conn.execute(
            "UPDATE eta_invoices SET lines_json=?2, total_vat=?3, total_wht=?4,
                 net_amount = CASE WHEN net_amount = 0 OR ?8 THEN ?5 ELSE net_amount END,
                 grand_total = CASE WHEN grand_total = 0 OR ?8 THEN ?6 ELSE grand_total END,
                 currency = CASE WHEN ?7 = '' OR ?7 IS NULL THEN currency ELSE ?7 END,
                 exchange_rate = ?9
             WHERE id=?1",
            params![id, lines_json, inv.total_vat, inv.total_wht, inv.net_amount, inv.grand_total, inv.currency, fx, inv.exchange_rate],
        )
        .map_err(|e| e.to_string())?;
        fixed += 1;
    }
    Ok(fixed)
}

/// Backfill the document kind (`I`/`C`/`D`) and credit-note references for rows
/// written before those columns existed. The raw XML is enough; no full parse of
/// the invoice lines is needed. Idempotent: only rows with an empty `doc_kind`
/// are touched, and a row that cannot be parsed defaults to an ordinary invoice
/// so it is not retried on every launch.
pub fn repair_pool_meta(conn: &Connection) -> Result<usize, String> {
    let raws: Vec<(i64, String)> = {
        let mut stmt = conn
            .prepare("SELECT id, raw_xml FROM eta_invoices WHERE doc_kind IS NULL OR doc_kind = ''")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1).unwrap_or_default())))
            .map_err(|e| e.to_string())?;
        rows.filter_map(|r| r.ok()).collect()
    };
    let mut fixed = 0usize;
    for (id, raw) in raws {
        let (kind, refs) = if raw.trim().is_empty() {
            ("I".to_string(), Vec::new())
        } else {
            crate::eta_xml::parse_eta_meta(&raw)
        };
        let references_json = serde_json::to_string(&refs).unwrap_or_else(|_| "[]".into());
        conn.execute(
            "UPDATE eta_invoices SET doc_kind=?2, references_json=?3 WHERE id=?1",
            params![id, kind, references_json],
        )
        .map_err(|e| e.to_string())?;
        fixed += 1;
    }
    Ok(fixed)
}

/// Supplier-analysis pool row: full financials plus per-line JSON (used by the
/// Suppliers tab to derive per-item VAT rates from the real invoices), but no
/// raw_xml. The lightweight summary omits lines entirely.
#[derive(serde::Serialize)]
pub struct PoolInvoiceDetail {    pub id: i64,
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
    pub lines_json: String,
    pub file_name: String,
    pub doc_status: String,
    pub status: String,
    pub used_by_label: String,
    pub delete_requested_at: Option<String>,
    pub delete_requested_by: Option<String>,
    pub created_at: String,
}

pub fn list_pool_detail(conn: &Connection) -> Result<Vec<PoolInvoiceDetail>, String> {
    let mut result = Vec::new();
    let mut stmt = conn
        .prepare("SELECT id, invoice_id, uuid, seller_tax_id, seller_name, buyer_tax_id, buyer_name, issue_date, currency, net_amount, total_vat, total_wht, grand_total, lines_json, file_name, doc_status, status, used_by_label, delete_requested_at, delete_requested_by, created_at FROM eta_invoices ORDER BY created_at DESC")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok(PoolInvoiceDetail {
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
                file_name: row.get(14)?,
                doc_status: {
                    let s: String = row.get(15)?;
                    if s.is_empty() { "Valid".to_string() } else { s }
                },
                status: row.get(16)?,
                used_by_label: row.get(17)?,
                delete_requested_at: row.get(18)?,
                delete_requested_by: row.get(19)?,
                created_at: row.get(20)?,
            })
        })
        .map_err(|e| e.to_string())?;
    for row in rows {
        result.push(row.map_err(|e| e.to_string())?);
    }
    Ok(result)
}

/// Lightweight pool row for the list UI — excludes raw_xml and lines_json
/// which are large and not needed for display.
#[derive(serde::Serialize)]
pub struct PoolInvoiceSummary {
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
    pub exchange_rate: f64,
    /// I = invoice, C = credit note, D = debit note.
    pub doc_kind: String,
    /// JSON array of ETA UUIDs a credit note references (empty otherwise).
    pub references_json: String,
    /// True when a valid credit note references this invoice and fully offsets
    /// it. Such an invoice can no longer be claimed or attached.
    pub cancelled: bool,
    /// Invoice id of the credit note that cancelled this invoice, if any.
    pub cancelled_by: String,
    /// How the credit note was linked: "reference" (ETA `references`) or
    /// "amount" (unambiguous same seller + currency + total).
    pub cancelled_via: String,
    pub file_name: String,
    pub doc_status: String,
    pub status: String,
    pub used_by_label: String,
    pub delete_requested_at: Option<String>,
    pub delete_requested_by: Option<String>,
    pub created_at: String,
}

/// A credit note that fully offsets the invoice(s) it references.
#[derive(Debug, Clone)]
pub struct CreditCancellation {
    pub credit_note_id: String,
    /// "reference" or "amount".
    pub via: String,
}

#[derive(Clone)]
struct PoolMeta {
    uuid: String,
    invoice_id: String,
    seller_tax_id: String,
    grand_total: f64,
    currency: String,
    doc_kind: String,
    references_json: String,
    doc_status: String,
    delete_requested_at: Option<String>,
    issue_date: String,
}

fn date_only(s: &str) -> &str {
    let t = s.trim();
    if t.len() >= 10 && t.as_bytes()[4] == b'-' && t.as_bytes()[7] == b'-' {
        t.get(..10).unwrap_or("")
    } else {
        ""
    }
}

fn currency_key(s: &str) -> String {
    let t = s.trim();
    if t.is_empty() { "EGP".to_string() } else { t.to_uppercase() }
}

fn same_currency(a: &str, b: &str) -> bool {
    currency_key(a) == currency_key(b)
}

/// Only ordinary invoices (or legacy rows whose kind is unknown) can be
/// cancelled by a credit note; another credit/debit note is never a target.
fn is_invoice_kind(kind: &str) -> bool {
    let t = kind.trim();
    t.is_empty() || t.eq_ignore_ascii_case("I")
}

/// Map invoice UUID -> credit note for invoices fully cancelled by a credit
/// note. ETA `references` are authoritative. A credit note that carries no
/// reference at all falls back to an unambiguous amount match, because suppliers
/// frequently bill the same amount more than once; a credit note whose reference
/// is present but not in the pool never falls back (its reference already tells
/// us it belongs to another invoice).
pub fn credit_cancellations(conn: &Connection) -> Result<std::collections::HashMap<String, CreditCancellation>, String> {
    let mut stmt = conn
        .prepare("SELECT uuid, invoice_id, seller_tax_id, grand_total, currency, doc_kind, references_json, doc_status, delete_requested_at, issue_date FROM eta_invoices")
        .map_err(|e| e.to_string())?;
    let rows: Vec<PoolMeta> = stmt
        .query_map([], |r| {
            Ok(PoolMeta {
                uuid: r.get(0)?,
                invoice_id: r.get(1)?,
                seller_tax_id: r.get(2)?,
                grand_total: r.get(3)?,
                currency: r.get(4)?,
                doc_kind: r.get(5).unwrap_or_default(),
                references_json: r.get(6).unwrap_or_else(|_| "[]".to_string()),
                doc_status: r.get(7).unwrap_or_default(),
                delete_requested_at: r.get(8)?,
                issue_date: r.get(9).unwrap_or_default(),
            })
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    let by_uuid: std::collections::HashMap<String, usize> = rows
        .iter()
        .enumerate()
        .map(|(i, r)| (r.uuid.trim().to_uppercase(), i))
        .collect();

    let credit_ok = |cn: &PoolMeta| {
        cn.delete_requested_at.is_none()
            && (cn.doc_status.is_empty() || cn.doc_status.eq_ignore_ascii_case("Valid"))
    };
    let target_ok = |cn: &PoolMeta, inv: &PoolMeta| {
        cn.seller_tax_id.trim() != ""
            && cn.seller_tax_id.trim() == inv.seller_tax_id.trim()
            && same_currency(&cn.currency, &inv.currency)
    };

    let mut out = std::collections::HashMap::new();
    // Single-reference credit notes can also offset an invoice cumulatively
    // (two partial credits summing to the invoice total).
    let mut single_ref: std::collections::HashMap<String, Vec<usize>> = std::collections::HashMap::new();
    for (i, cn) in rows.iter().enumerate() {
        if !cn.doc_kind.eq_ignore_ascii_case("C") || !credit_ok(cn) {
            continue;
        }
        let refs: Vec<String> = serde_json::from_str(&cn.references_json).unwrap_or_default();
        let refs: Vec<String> = refs
            .into_iter()
            .map(|s| s.trim().to_uppercase())
            .filter(|s| !s.is_empty())
            .collect();
        if refs.len() == 1 {
            single_ref.entry(refs[0].clone()).or_default().push(i);
        } else if refs.len() > 1 {
            let targets: Vec<usize> = refs
                .iter()
                .filter_map(|u| by_uuid.get(u).copied())
                .filter(|&j| j != i && target_ok(cn, &rows[j]) && is_invoice_kind(&rows[j].doc_kind))
                .collect();
            if targets.is_empty() {
                continue;
            }
            let sum: f64 = targets.iter().map(|&j| rows[j].grand_total).sum();
            if cn.grand_total + 0.02 >= sum {
                for j in targets {
                    out.entry(rows[j].uuid.trim().to_uppercase())
                        .or_insert(CreditCancellation { credit_note_id: cn.invoice_id.clone(), via: "reference".to_string() });
                }
            }
        }
    }
    for (uuid, cns) in single_ref {
        let Some(&inv_i) = by_uuid.get(&uuid) else { continue };
        let inv = &rows[inv_i];
        if !is_invoice_kind(&inv.doc_kind) {
            continue;
        }
        let valid: Vec<&PoolMeta> = cns.iter().map(|&i| &rows[i]).filter(|cn| target_ok(cn, inv)).collect();
        if valid.is_empty() {
            continue;
        }
        let credited: f64 = valid.iter().map(|cn| cn.grand_total).sum();
        if credited + 0.02 >= inv.grand_total {
            out.entry(inv.uuid.trim().to_uppercase())
                .or_insert(CreditCancellation { credit_note_id: valid[0].invoice_id.clone(), via: "reference".to_string() });
        }
    }

    // Fallback for credit notes that carry no ETA `references` at all. Only an
    // unambiguous amount link is accepted: same seller, currency and total, an
    // invoice issued no later than the credit note (same-day allowed), exactly
    // one candidate invoice, and exactly one such credit note. A credit note
    // that *does* carry a reference is never amount-matched, because that
    // reference tells us it belongs to a different invoice even if that invoice
    // is no longer in the pool.
    let cn_key = |cn: &PoolMeta| {
        (
            cn.seller_tax_id.trim().to_uppercase(),
            currency_key(&cn.currency),
            (cn.grand_total * 100.0).round() as i64,
        )
    };
    let mut no_ref_counts: std::collections::HashMap<(String, String, i64), usize> =
        std::collections::HashMap::new();
    for cn in rows.iter() {
        if !cn.doc_kind.eq_ignore_ascii_case("C") || !credit_ok(cn) {
            continue;
        }
        let refs: Vec<String> = serde_json::from_str(&cn.references_json).unwrap_or_default();
        if refs.iter().any(|r| !r.trim().is_empty()) {
            continue;
        }
        *no_ref_counts.entry(cn_key(cn)).or_default() += 1;
    }
    for (i, cn) in rows.iter().enumerate() {
        if !cn.doc_kind.eq_ignore_ascii_case("C") || !credit_ok(cn) {
            continue;
        }
        let refs: Vec<String> = serde_json::from_str(&cn.references_json).unwrap_or_default();
        if refs.iter().any(|r| !r.trim().is_empty()) {
            continue;
        }
        if no_ref_counts.get(&cn_key(cn)).copied().unwrap_or(0) != 1 {
            continue;
        }
        let cn_date = date_only(&cn.issue_date);
        let candidates: Vec<usize> = rows
            .iter()
            .enumerate()
            .filter(|(j, inv)| {
                if *j == i || !is_invoice_kind(&inv.doc_kind) || !target_ok(cn, inv) {
                    return false;
                }
                if !inv.doc_status.is_empty() && !inv.doc_status.eq_ignore_ascii_case("Valid") {
                    return false;
                }
                if (inv.grand_total - cn.grand_total).abs() > 0.02 {
                    return false;
                }
                let inv_date = date_only(&inv.issue_date);
                if !cn_date.is_empty() && !inv_date.is_empty() && inv_date > cn_date {
                    return false;
                }
                true
            })
            .map(|(j, _)| j)
            .collect();
        if candidates.len() == 1 {
            let j = candidates[0];
            let key = rows[j].uuid.trim().to_uppercase();
            if !out.contains_key(&key) {
                out.insert(key, CreditCancellation { credit_note_id: cn.invoice_id.clone(), via: "amount".to_string() });
            }
        }
    }
    Ok(out)
}

pub fn list_pool_summary(conn: &Connection) -> Result<Vec<PoolInvoiceSummary>, String> {
    let mut result = Vec::new();
    let mut stmt = conn
        .prepare("SELECT id, invoice_id, uuid, seller_tax_id, seller_name, buyer_tax_id, buyer_name, issue_date, currency, net_amount, total_vat, total_wht, grand_total, file_name, doc_status, status, used_by_label, delete_requested_at, delete_requested_by, created_at, exchange_rate, doc_kind, references_json FROM eta_invoices ORDER BY created_at DESC")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok(PoolInvoiceSummary {
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
                file_name: row.get(13)?,
                doc_status: {
                    let s: String = row.get(14)?;
                    if s.is_empty() { "Valid".to_string() } else { s }
                },
                status: row.get(15)?,
                used_by_label: row.get(16)?,
                delete_requested_at: row.get(17)?,
                delete_requested_by: row.get(18)?,
                created_at: row.get(19)?,
                exchange_rate: row.get(20).unwrap_or(0.0),
                doc_kind: row.get(21).unwrap_or_default(),
                references_json: row.get(22).unwrap_or_else(|_| "[]".to_string()),
                cancelled: false,
                cancelled_by: String::new(),
                cancelled_via: String::new(),
            })
        })
        .map_err(|e| e.to_string())?;
    for row in rows {
        result.push(row.map_err(|e| e.to_string())?);
    }
    // Mark invoices fully offset by a credit note that references them.
    let cancellations = credit_cancellations(conn)?;
    for r in result.iter_mut() {
        if let Some(cn) = cancellations.get(&r.uuid.trim().to_uppercase()) {
            r.cancelled = true;
            r.cancelled_by = cn.credit_note_id.clone();
            r.cancelled_via = cn.via.clone();
        }
    }
    Ok(result)
}

pub fn mark_invoice_used(conn: &Connection, id: i64, snapshot_id: i64, snapshot_label: &str) -> Result<(), String> {
    conn.execute(
        "UPDATE eta_invoices SET status = 'used', used_by_snapshot_id = ?1, used_by_label = ?2 WHERE id = ?3",
        params![snapshot_id, snapshot_label, id],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn mark_invoices_used(conn: &Connection, ids: &[i64], snapshot_id: i64, snapshot_label: &str) -> Result<(), String> {
    for id in ids {
        conn.execute(
            "UPDATE eta_invoices SET status = 'used', used_by_snapshot_id = ?1, used_by_label = ?2 WHERE id = ?3",
            params![snapshot_id, snapshot_label, id],
        ).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn mark_invoice_available(conn: &Connection, id: i64) -> Result<(), String> {
    conn.execute(
        "UPDATE eta_invoices SET status = 'available', used_by_snapshot_id = NULL, used_by_label = '' WHERE id = ?1",
        params![id],
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
// Identity is the ETA document uuid: a different-uuid resubmission inserts its
// own row (mirroring add_to_pool), while the same uuid refreshes that row.
pub fn sync_pool_from_remote(conn: &Connection, inv: &PoolInvoice) -> Result<(), String> {
    // Never store an empty uuid, and reconcile against local rows so a real ETA
    // uuid wins over a synthetic one and a synthetic remote row does not create
    // a second copy of a document we already hold under its real uuid.
    let resolved = resolve_pool_uuid(&inv.uuid, &inv.invoice_id, &inv.seller_tax_id, &inv.file_name, &inv.raw_xml);
    // Re-derive the amounts from the raw XML when present: the shared table has
    // no exchange-rate column, and an older importing device may have stored EGP
    // totals under a foreign-currency label. Parsing keeps every device
    // consistent with the current parser (only changed rows are pulled).
    let parsed = if inv.raw_xml.is_empty() { None } else { crate::eta_xml::parse_eta_xml(&inv.raw_xml).ok() };
    let (currency, net_amount, total_vat, total_wht, grand_total, exchange_rate, lines_json) = match &parsed {
        Some(p) if !p.lines.is_empty() => (
            p.currency.clone(),
            p.net_amount,
            p.total_vat,
            p.total_wht,
            p.grand_total,
            p.exchange_rate,
            serde_json::to_string(&p.lines).unwrap_or_else(|_| inv.lines_json.clone()),
        ),
        _ => (
            inv.currency.clone(),
            inv.net_amount,
            inv.total_vat,
            inv.total_wht,
            inv.grand_total,
            inv.exchange_rate,
            inv.lines_json.clone(),
        ),
    };
    // Document kind / credit-note references are not stored in the shared table;
    // derive them from the raw XML on every pull so each device reaches the same
    // view (and old imports are upgraded as soon as their row is refreshed).
    let (doc_kind, references_json) = if !inv.raw_xml.is_empty() {
        let (kind, refs) = crate::eta_xml::parse_eta_meta(&inv.raw_xml);
        (kind, serde_json::to_string(&refs).unwrap_or_else(|_| "[]".to_string()))
    } else if !inv.doc_kind.is_empty() {
        (inv.doc_kind.clone(), inv.references_json.clone())
    } else {
        (String::new(), String::new())
    };
    if let Some((id, final_uuid)) = reconcile_pool_row(conn, &inv.invoice_id, &inv.seller_tax_id, &resolved)? {
        conn.execute(
            "UPDATE eta_invoices SET invoice_id=?2, uuid=?3, seller_tax_id=?4, seller_name=?5, buyer_tax_id=?6, buyer_name=?7,
             issue_date=?8, currency=?9, net_amount=?10, total_vat=?11, total_wht=?12, grand_total=?13,
             lines_json = CASE WHEN ?14 = '' OR ?14 = '[]' THEN lines_json ELSE ?14 END,
             raw_xml = CASE WHEN ?15 = '' THEN raw_xml ELSE ?15 END, file_name=?16,
             doc_status = CASE WHEN ?17 IS NULL OR ?17 = '' THEN doc_status ELSE ?17 END,
             status=?18, used_by_label=?19, delete_requested_at=?20, delete_requested_by=?21,
             exchange_rate=?22,
             doc_kind = CASE WHEN ?23 = '' THEN doc_kind ELSE ?23 END,
             references_json = CASE WHEN ?24 = '' THEN references_json ELSE ?24 END
             WHERE id=?1",
            params![
                id, inv.invoice_id, final_uuid, inv.seller_tax_id, inv.seller_name, inv.buyer_tax_id, inv.buyer_name,
                inv.issue_date, currency, net_amount, total_vat, total_wht, grand_total,
                lines_json, inv.raw_xml, inv.file_name, inv.doc_status, inv.status, inv.used_by_label,
                inv.delete_requested_at, inv.delete_requested_by, exchange_rate, doc_kind, references_json
            ],
        ).map_err(|e| e.to_string())?;
        return Ok(());
    }
    let insert_kind = if doc_kind.is_empty() { "I".to_string() } else { doc_kind };
    let insert_refs = if references_json.is_empty() { "[]".to_string() } else { references_json };
    conn.execute(
        "INSERT INTO eta_invoices (invoice_id, uuid, seller_tax_id, seller_name, buyer_tax_id, buyer_name, issue_date, currency, net_amount, total_vat, total_wht, grand_total, lines_json, raw_xml, file_name, doc_status, status, used_by_label, delete_requested_at, delete_requested_by, created_at, exchange_rate, doc_kind, references_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24)",
        params![
            inv.invoice_id, resolved, inv.seller_tax_id, inv.seller_name,
            inv.buyer_tax_id, inv.buyer_name, inv.issue_date, currency,
            net_amount, total_vat, total_wht, grand_total,
            lines_json, inv.raw_xml, inv.file_name, inv.doc_status,
            inv.status, inv.used_by_label, inv.delete_requested_at, inv.delete_requested_by,
            inv.created_at, exchange_rate, insert_kind, insert_refs,
        ],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod pool_supersede_tests {
    use super::*;
    use crate::eta_xml::EtaInvoice;

    fn sample(id: &str, uuid: &str, doc_status: &str) -> EtaInvoice {
        sample_seller(id, uuid, doc_status, "100000000")
    }

    fn sample_seller(id: &str, uuid: &str, doc_status: &str, seller: &str) -> EtaInvoice {
        EtaInvoice {
            invoice_id: id.into(),
            uuid: uuid.into(),
            issue_date: "2026-08-01T00:00:00Z".into(),
            invoice_type_code: "I".into(),
            seller_tax_id: seller.into(),
            seller_name: "Test Seller".into(),
            buyer_tax_id: "200000000".into(),
            buyer_name: "Test Buyer".into(),
            currency: "EGP".into(),
            net_amount: 100.0,
            total_vat: 14.0,
            total_wht: 0.0,
            grand_total: 114.0,
            exchange_rate: 1.0,
            lines: vec![],
            references: vec![],
            doc_status: doc_status.into(),
        }
    }

    fn row(conn: &Connection, uuid: &str) -> (String, String, String, String) {
        conn.query_row(
            "SELECT invoice_id, doc_status, status, used_by_label FROM eta_invoices WHERE uuid = ?1",
            params![uuid],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        ).unwrap()
    }

    #[test]
    fn paid_metadata_update_keeps_created_at() {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        conn.execute(
            "INSERT INTO snapshots (label, notes, created_at, data) VALUES ('DOC-1', '', '2026-01-02 03:04:05', '{\"paid\":false}')",
            [],
        ).unwrap();
        let id: i64 = conn.query_row("SELECT id FROM snapshots WHERE label='DOC-1'", [], |r| r.get(0)).unwrap();
        update_snapshot_data(&conn, id, "{\"paid\":true,\"paid_by\":\"omar\"}").unwrap();
        let (created, data): (String, String) = conn.query_row(
            "SELECT created_at, data FROM snapshots WHERE id = ?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        ).unwrap();
        assert_eq!(created, "2026-01-02 03:04:05", "status toggle must not reorder history");
        assert!(data.contains("\"paid\":true"));
    }

    #[test]
    fn full_edit_keeps_created_at() {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        conn.execute(
            "INSERT INTO snapshots (label, notes, created_at, data) VALUES ('DOC-1', '', '2026-01-02 03:04:05', '{\"v\":1}')",
            [],
        ).unwrap();
        let id: i64 = conn.query_row("SELECT id FROM snapshots WHERE label='DOC-1'", [], |r| r.get(0)).unwrap();
        update_snapshot(&conn, id, "DOC-1-EDIT", "", "{\"v\":2}").unwrap();
        let (label, created, data): (String, String, String) = conn.query_row(
            "SELECT label, created_at, data FROM snapshots WHERE id = ?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        ).unwrap();
        assert_eq!(label, "DOC-1-EDIT");
        assert_eq!(created, "2026-01-02 03:04:05", "a full edit must not change the creation date");
        assert!(data.contains("\"v\":2"));
    }

    #[test]
    fn repair_real_pool_db() {
        // Env-gated: point VOUCHIFY_POOL_DB at a copy of history.db to verify the
        // one-time line/VAT backfill over real ETA portal exports.
        let Ok(path) = std::env::var("VOUCHIFY_POOL_DB") else { return };
        let conn = Connection::open(&path).unwrap();
        init_schema(&conn).unwrap();
        let total: i64 = conn.query_row("SELECT COUNT(*) FROM eta_invoices", [], |r| r.get(0)).unwrap();
        let with_lines: i64 = conn.query_row(
            "SELECT COUNT(*) FROM eta_invoices WHERE lines_json IS NOT NULL AND lines_json <> '[]' AND lines_json <> ''",
            [], |r| r.get(0)).unwrap();
        let vat: f64 = conn.query_row("SELECT COALESCE(SUM(total_vat),0) FROM eta_invoices", [], |r| r.get(0)).unwrap();
        let still_empty: i64 = conn.query_row(
            "SELECT COUNT(*) FROM eta_invoices WHERE (lines_json IS NULL OR lines_json='[]' OR lines_json='') AND raw_xml LIKE '%invoiceLine%'",
            [], |r| r.get(0)).unwrap();
        let credit_notes: i64 = conn.query_row(
            "SELECT COUNT(*) FROM eta_invoices WHERE doc_kind='C'", [], |r| r.get(0)).unwrap();
        let cancellations = credit_cancellations(&conn).unwrap();
        eprintln!("repair: total={} with_lines={} still_empty_with_lines={} total_vat_sum={:.2} credit_notes={} cancellations={}",
            total, with_lines, still_empty, vat, credit_notes, cancellations.len());
        assert_eq!(still_empty, 0, "every parseable row must have line items after repair");
    }

    fn row_count_where(conn: &Connection, id: &str) -> i64 {
        conn.query_row(
            "SELECT COUNT(*) FROM eta_invoices WHERE invoice_id = ?1",
            params![id],
            |r| r.get(0),
        ).unwrap()
    }

    #[test]
    fn uuid_identity_resolution_rules() {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();

        // 1. Valid version first
        let o = add_to_pool(&conn, &sample("X", "UUID-A", "Valid"), "<x/>", "a.xml").unwrap();
        assert!(matches!(o, PoolAddOutcome::Inserted(_)));

        // 2. Same internalID+seller, DIFFERENT submission uuid, Rejected -> kept
        //    as its own row (never masked by / never masks the valid one)
        let o = add_to_pool(&conn, &sample("X", "UUID-B", "Rejected"), "<x/>", "b.xml").unwrap();
        assert!(matches!(o, PoolAddOutcome::Inserted(_)));
        assert_eq!(row_count_where(&conn, "X"), 2);
        assert_eq!(row(&conn, "UUID-A").1, "Valid");
        assert_eq!(row(&conn, "UUID-B").1, "Rejected");

        // 3. Same submission (same uuid) later becomes Rejected (genuine state change)
        let o = add_to_pool(&conn, &sample("X", "UUID-A", "Rejected"), "<x/>", "a.xml").unwrap();
        assert!(matches!(o, PoolAddOutcome::Updated(_)));
        assert_eq!(row(&conn, "UUID-A").1, "Rejected");

        // 4. Reversed too: the same document becoming Valid again refreshes it
        let o = add_to_pool(&conn, &sample("X", "UUID-A", "Valid"), "<x/>", "a.xml").unwrap();
        assert!(matches!(o, PoolAddOutcome::Updated(_)));
        assert_eq!(row(&conn, "UUID-A").1, "Valid");

        // 5. A corrected resubmission (new uuid, Valid) adds its own row
        let o = add_to_pool(&conn, &sample("X", "UUID-C", "Valid"), "<x/>", "c.xml").unwrap();
        assert!(matches!(o, PoolAddOutcome::Inserted(_)));
        assert_eq!(row_count_where(&conn, "X"), 3);

        // 6. Claim survives a refresh (status + serial untouched by add_to_pool)
        mark_invoice_used(&conn, 1, 1, "SER-1").unwrap();
        add_to_pool(&conn, &sample("X", "UUID-A", "Rejected"), "<x/>", "a.xml").unwrap();
        assert_eq!(row(&conn, "UUID-A").1, "Rejected");
        assert_eq!(row(&conn, "UUID-A").2, "used");
        assert_eq!(row(&conn, "UUID-A").3, "SER-1");

        // 7. The refresh must ALSO update the seller identity (a re-upload that
        //    now carries the seller tax id fixes a previously-empty value).
        let o = add_to_pool(&conn, &sample_seller("X", "UUID-A", "Valid", "645923168"), "<x/>", "a.xml").unwrap();
        assert!(matches!(o, PoolAddOutcome::Updated(_)));
        let seller: String = conn.query_row(
            "SELECT seller_tax_id FROM eta_invoices WHERE uuid = ?1",
            params!["UUID-A"],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(seller, "645923168");
        assert_eq!(row(&conn, "UUID-A").2, "used", "claim survives the seller refresh too");
        assert_eq!(row(&conn, "UUID-A").3, "SER-1");
    }

    #[test]
    fn same_internalid_different_sellers_do_not_collide() {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();

        // Seller A has invoice "1199" uuid UUID-A1
        let o = add_to_pool(&conn, &sample_seller("1199", "UUID-A1", "Valid", "645923168"), "<x/>", "a.xml").unwrap();
        assert!(matches!(o, PoolAddOutcome::Inserted(_)));

        // Seller B has invoice "1199" uuid UUID-B1 — must NOT collide
        let o = add_to_pool(&conn, &sample_seller("1199", "UUID-B1", "Valid", "735503508"), "<x/>", "b.xml").unwrap();
        assert!(matches!(o, PoolAddOutcome::Inserted(_)));
        assert_eq!(row_count_where(&conn, "1199"), 2);

        // Both rows coexist
        assert_eq!(row(&conn, "UUID-A1").0, "1199");
        assert_eq!(row(&conn, "UUID-B1").0, "1199");

        // Same seller, same invoice, NEW uuid -> its own row too
        let o = add_to_pool(&conn, &sample_seller("1199", "UUID-A2", "Valid", "645923168"), "<x/>", "a2.xml").unwrap();
        assert!(matches!(o, PoolAddOutcome::Inserted(_)));
        assert_eq!(row_count_where(&conn, "1199"), 3);

        // A rejected revision of the SAME uuid UUID-A1 replaces only that row
        let o = add_to_pool(&conn, &sample_seller("1199", "UUID-A1", "Rejected", "645923168"), "<x/>", "a1.xml").unwrap();
        assert!(matches!(o, PoolAddOutcome::Updated(_)));
        assert_eq!(row(&conn, "UUID-A1").1, "Rejected");

        // Seller B is unaffected
        assert_eq!(row(&conn, "UUID-B1").1, "Valid");
    }

    #[test]
    fn empty_uuid_is_derived_and_reimport_updates() {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();

        let o = add_to_pool(&conn, &sample("555", "", "Valid"), "<x/>", "555.xml").unwrap();
        assert!(matches!(o, PoolAddOutcome::Inserted(_)));
        let uuid: String = conn
            .query_row("SELECT uuid FROM eta_invoices WHERE invoice_id='555'", [], |r| r.get(0))
            .unwrap();
        assert!(!uuid.is_empty(), "uuid must never be empty");

        // Re-import the same document, as if the browser named it "555 (1).xml".
        let o = add_to_pool(&conn, &sample("555", "", "Valid"), "<x/>", "555 (1).xml").unwrap();
        assert!(matches!(o, PoolAddOutcome::Updated(_)), "re-import must update, not duplicate");
        assert_eq!(row_count_where(&conn, "555"), 1);
    }

    #[test]
    fn real_uuid_upgrades_a_synthetic_row() {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();

        add_to_pool(&conn, &sample("777", "", "Valid"), "<x/>", "777.xml").unwrap();
        // A later import of the same document carries the real ETA uuid: it must
        // upgrade the synthetic row in place, not add a second row.
        let o = add_to_pool(&conn, &sample("777", "REALUUID777AAAAAAAAAAAAAAAA", "Valid"), "<x/>", "777.xml").unwrap();
        assert!(matches!(o, PoolAddOutcome::Updated(_)));
        assert_eq!(row_count_where(&conn, "777"), 1);
        assert_eq!(row(&conn, "REALUUID777AAAAAAAAAAAAAAAA").0, "777");
    }

    #[test]
    fn credit_note_cancels_only_the_referenced_invoice() {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();

        let mut inv = sample_seller("INV-1", "UUID-INV-1", "Valid", "111");
        inv.grand_total = 1000.0;
        add_to_pool(&conn, &inv, "<x/>", "inv.xml").unwrap();

        // Same seller, same amount, different invoice: must NOT be cancelled by
        // amount matching alone.
        let mut other = sample_seller("INV-2", "UUID-INV-2", "Valid", "111");
        other.grand_total = 1000.0;
        add_to_pool(&conn, &other, "<x/>", "inv2.xml").unwrap();

        let mut cn = sample_seller("CN-1", "UUID-CN-1", "Valid", "111");
        cn.invoice_type_code = "C".to_string();
        cn.grand_total = 1000.0;
        cn.references = vec!["UUID-INV-1".to_string()];
        add_to_pool(&conn, &cn, "<x/>", "cn.xml").unwrap();

        let cancels = credit_cancellations(&conn).unwrap();
        assert!(cancels.contains_key("UUID-INV-1"));
        assert!(!cancels.contains_key("UUID-INV-2"));

        let summary = list_pool_summary(&conn).unwrap();
        let inv1 = summary.iter().find(|r| r.uuid == "UUID-INV-1").unwrap();
        assert!(inv1.cancelled);
        assert_eq!(inv1.cancelled_by, "CN-1");
        let inv2 = summary.iter().find(|r| r.uuid == "UUID-INV-2").unwrap();
        assert!(!inv2.cancelled);
        let cn_row = summary.iter().find(|r| r.uuid == "UUID-CN-1").unwrap();
        assert_eq!(cn_row.doc_kind, "C");
    }

    #[test]
    fn partial_credit_note_does_not_cancel() {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();

        let mut inv = sample_seller("INV-1", "UUID-INV-1", "Valid", "111");
        inv.grand_total = 1000.0;
        add_to_pool(&conn, &inv, "<x/>", "inv.xml").unwrap();

        let mut cn = sample_seller("CN-1", "UUID-CN-1", "Valid", "111");
        cn.invoice_type_code = "C".to_string();
        cn.grand_total = 400.0;
        cn.references = vec!["UUID-INV-1".to_string()];
        add_to_pool(&conn, &cn, "<x/>", "cn.xml").unwrap();

        assert!(credit_cancellations(&conn).unwrap().is_empty());
    }

    #[test]
    fn no_reference_credit_note_matches_uniquely_by_amount() {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();

        let mut inv = sample_seller("INV-1", "UUID-INV-1", "Valid", "111");
        inv.grand_total = 1000.0;
        add_to_pool(&conn, &inv, "<x/>", "inv.xml").unwrap();

        let mut cn = sample_seller("CN-1", "UUID-CN-1", "Valid", "111");
        cn.invoice_type_code = "C".to_string();
        cn.grand_total = 1000.0;
        add_to_pool(&conn, &cn, "<x/>", "cn.xml").unwrap();

        let cancels = credit_cancellations(&conn).unwrap();
        let c = cancels.get("UUID-INV-1").expect("unambiguous amount match must cancel");
        assert_eq!(c.via, "amount");
        assert_eq!(c.credit_note_id, "CN-1");
    }

    #[test]
    fn no_reference_credit_note_skips_ambiguous_amounts() {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();

        for id in ["INV-1", "INV-2"] {
            let mut inv = sample_seller(id, &format!("UUID-{id}"), "Valid", "111");
            inv.grand_total = 1000.0;
            add_to_pool(&conn, &inv, "<x/>", &format!("{id}.xml")).unwrap();
        }

        let mut cn = sample_seller("CN-1", "UUID-CN-1", "Valid", "111");
        cn.invoice_type_code = "C".to_string();
        cn.grand_total = 1000.0;
        add_to_pool(&conn, &cn, "<x/>", "cn.xml").unwrap();

        assert!(credit_cancellations(&conn).unwrap().is_empty());
    }

    #[test]
    fn mismatched_reference_never_falls_back_to_amount() {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();

        let mut inv = sample_seller("INV-1", "UUID-INV-1", "Valid", "111");
        inv.grand_total = 1000.0;
        add_to_pool(&conn, &inv, "<x/>", "inv.xml").unwrap();

        let mut cn = sample_seller("CN-1", "UUID-CN-1", "Valid", "111");
        cn.invoice_type_code = "C".to_string();
        cn.grand_total = 1000.0;
        cn.references = vec!["SOMEOTHERINVOICEUUID0000000".to_string()];
        add_to_pool(&conn, &cn, "<x/>", "cn.xml").unwrap();

        // The reference points to an invoice that is not in the pool. Matching by
        // amount here could cancel an unrelated invoice of the same total.
        assert!(credit_cancellations(&conn).unwrap().is_empty());
    }

    #[test]
    fn normalize_backfills_and_collapses_synthetic_duplicates() {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();

        // Simulate a pre-fix DB with two copies of the same document and no
        // uuid, one of them already claimed.
        conn.execute(
            "INSERT INTO eta_invoices (invoice_id, uuid, seller_tax_id, seller_name, buyer_tax_id, buyer_name, issue_date, currency, net_amount, total_vat, total_wht, grand_total, lines_json, raw_xml, file_name, doc_status, status, created_at) \
             VALUES ('999','','111','S','2','B','d','EGP',1,0,0,1,'[]','','a.xml','Valid','used','t')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO eta_invoices (invoice_id, uuid, seller_tax_id, seller_name, buyer_tax_id, buyer_name, issue_date, currency, net_amount, total_vat, total_wht, grand_total, lines_json, raw_xml, file_name, doc_status, status, created_at) \
             VALUES ('999','','111','S','2','B','d','EGP',1,0,0,1,'[]','','999 (1).xml','Valid','available','t')",
            [],
        ).unwrap();

        let changed = normalize_synthetic_uuids(&conn).unwrap();
        assert!(changed >= 1);
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM eta_invoices WHERE invoice_id='999'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1, "duplicates must collapse to one row");
        let (uuid, status): (String, String) = conn
            .query_row("SELECT uuid, status FROM eta_invoices WHERE invoice_id='999'", [], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap();
        assert!(!uuid.is_empty());
        assert_eq!(status, "used", "claim must survive the merge");
    }
}
