use crate::models::HistoryEntry;
use rusqlite::{params, Connection};
use std::path::Path;

pub fn init_db(db_path: &Path) -> Result<Connection, String> {
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let conn = Connection::open(db_path).map_err(|e| e.to_string())?;
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
    Ok(conn)
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

pub fn list_snapshots(conn: &Connection, search: &str) -> Result<Vec<HistoryEntry>, String> {
    let mut result = Vec::new();

    if search.is_empty() {
        let mut stmt = conn
            .prepare("SELECT id, label, notes, created_at FROM snapshots ORDER BY created_at DESC")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok(HistoryEntry {
                    id: row.get(0)?,
                    label: row.get(1)?,
                    notes: row.get(2)?,
                    created_at: row.get(3)?,
                })
            })
            .map_err(|e| e.to_string())?;
        for row in rows {
            result.push(row.map_err(|e| e.to_string())?);
        }
    } else {
        let pattern = format!("%{}%", search);
        let mut stmt = conn
            .prepare("SELECT id, label, notes, created_at FROM snapshots WHERE label LIKE ?1 OR notes LIKE ?1 ORDER BY created_at DESC")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![pattern], |row| {
                Ok(HistoryEntry {
                    id: row.get(0)?,
                    label: row.get(1)?,
                    notes: row.get(2)?,
                    created_at: row.get(3)?,
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
