//! Daily Entries: parse an accounting "Daily Entries" PDF report into individual
//! journal documents, store them locally, and flag invoices that were paid before.
//!
//! The report is an Oracle Reports "Daily Entries" export. A single document can
//! span several pages: only the first page of a document repeats the
//! `Doc. No ... Doc. Date ... Journal ...` header; continuation pages start
//! directly with the column header and account lines. We therefore stream the
//! whole report line by line and start a new entry only when a new `Doc. No`
//! header appears.

use regex::Regex;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::OnceLock;

// ── Regexes (compiled once) ──────────────────────────────────────────────────
fn re_header() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(r"Doc\.\s+No\s+(\S+)\s+(\d+)\s+Doc\.\s+Date\s+(\S+)\s+Journal\s+(\S+)\s+(.*?)\s+With\s+Currency")
            .unwrap()
    })
}
fn re_acct() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"^ {0,20}(\d{3,})\s+").unwrap())
}
fn re_gap() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"\s{2,}").unwrap())
}
fn re_branch() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"^\s*Branch\s+(.+?)\s*$").unwrap())
}
fn re_local_amt() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"Local\s+Amt\.\s*(-?[\d,]+\.\d{2})").unwrap())
}
fn re_doc_source() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"Doc\.\s+Source\s+(\S+)").unwrap())
}
fn re_total() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"Doc\.\s+Total\s+Value\s+Currency\s+(-?[\d,]+\.\d{2})").unwrap())
}
fn re_contain() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"Contain\s+\S+\s+(.+?)\s+Curr\.\s*Rate").unwrap())
}
fn re_currency_tail() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"(?i)\s*((?:Local|Foreign)\s*Curr(?:ency)?|[\$€£])\s*$").unwrap())
}

fn is_rate_value(s: &str) -> bool {
    let first = s.chars().next().unwrap_or(' ');
    (first.is_ascii_digit() || first == '-') && s.parse::<f64>().is_ok()
}

fn is_pure_currency(s: &str) -> bool {
    matches!(s, "$" | "€" | "£")
        || (s.len() == 3 && s.chars().all(|c| c.is_ascii_uppercase()))
        || s.to_lowercase().contains("curr")
}

/// Split the columns before the value into an account name + currency + local
/// value. Tolerates: a long account name touching the currency column with a
/// single space (they arrive merged), symbol currencies (e.g. `$`), and a
/// foreign-currency rate instead of the default `1.0`.
fn split_name_currency(before: &[String]) -> (String, String, String) {
    let mut parts: Vec<String> = before.to_vec();
    let mut local_value = String::new();
    if let Some(last) = parts.last() {
        if is_rate_value(last) {
            local_value = parts.pop().unwrap();
        }
    }
    let mut currency = String::new();
    if let Some(last) = parts.last().cloned() {
        if is_pure_currency(&last) {
            currency = parts.pop().unwrap();
        } else if let Some(cap) = re_currency_tail().captures(&last) {
            currency = cap[1].trim().to_string();
            let head = last[..cap.get(0).unwrap().start()].trim().to_string();
            parts.pop();
            if !head.is_empty() {
                parts.push(head);
            }
        }
    }
    (parts.join(" ").trim().to_string(), currency, local_value)
}

fn re_amount_exact() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"^-?[\d,]+\.\d{2}$").unwrap())
}
/// Matches an invoice marker: `inv.`, `inv`, `inv no.`, `inv#`, `invoice no`,
/// `invoice#`, optionally followed by `:`.
fn re_invoice_marker() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"(?i)\binv(?:oice)?\.?[ \t]*(?:no\.?|#)?[ \t]*:?[ \t]*").unwrap())
}
/// The invoice token(s) immediately after a marker: `1864` or `2/2021` or
/// `2-2026`, plus any `+ 1865 + 1815` continuations.
fn re_invoice_run() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(r"^[ \t]*([0-9][0-9A-Za-z]*(?:[-/][0-9A-Za-z]+)*(?:[ \t]*\+[ \t]*[0-9][0-9A-Za-z]*(?:[-/][0-9A-Za-z]+)*)*)")
            .unwrap()
    })
}

/// Normalize an invoice token so equivalent forms collapse: `2-2026` and
/// `2/2026` are the same invoice.
fn normalize_invoice(t: &str) -> String {
    let t = t.trim();
    if let Some((a, b)) = t.split_once('-') {
        let year = b.len() == 4 && (b.starts_with("19") || b.starts_with("20"));
        if year && !a.is_empty() && a.chars().all(|c| c.is_ascii_digit()) && b.chars().all(|c| c.is_ascii_digit()) {
            return format!("{}/{}", a, b);
        }
    }
    t.to_uppercase()
}
fn re_from_doc() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"(?i)\b([a-z]{2,4}\d{0,3}-\d{6,})\b").unwrap())
}

// ── Parsed (in-memory) shapes ────────────────────────────────────────────────
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DailyEntryLine {
    pub account_no: String,
    pub account: String,
    pub currency: String,
    pub local_value: String,
    pub debit: String,
    pub credit: String,
    pub narration: String,
    pub from_document: String,
}

#[derive(Debug, Clone, Default)]
struct ParsedEntry {
    doc_no: String,
    serial: String,
    doc_date: String,
    journal_no: String,
    journal: String,
    branch: String,
    doc_source: String,
    contain_currency: String,
    local_amt: String,
    total_value: String,
    source_page: i64,
    pages: i64,
    cur_page: i64,
    lines: Vec<DailyEntryLine>,
}

/// Split a text line into "columns" separated by two or more spaces, keeping the
/// byte offset of each column so numeric values can be placed in the correct
/// Debit/Credit column by comparing against the header's column position.
fn split_columns(s: &str) -> Vec<(usize, String)> {
    let mut out: Vec<(usize, String)> = Vec::new();
    let push = |seg: &str, off: usize, out: &mut Vec<(usize, String)>| {
        let trimmed = seg.trim();
        if !trimmed.is_empty() {
            let lead = seg.len() - seg.trim_start().len();
            out.push((off + lead, trimmed.to_string()));
        }
    };
    let mut last = 0usize;
    for m in re_gap().find_iter(s) {
        push(&s[last..m.start()], last, &mut out);
        last = m.end();
    }
    push(&s[last..], last, &mut out);
    out
}

fn is_meta_line(t: &str) -> bool {
    const PREFIXES: &[&str] = &[
        "System", "Company", "Year", "Page", "Branch", "Contain", "Journal", "Doc.",
        "Constraint", "Line ", "Line", "Cost", "Account No", "Fin.", "From Date",
        "To Date", "From Doc", "To Doc", "Percent", "Remarks",
    ];
    PREFIXES.iter().any(|p| t.starts_with(p))
}

fn is_amount(t: &str) -> bool {
    re_amount_exact().is_match(t)
}

/// Pick the value column: among the amount-looking columns (excluding the
/// `1.0` default rate), choose the one closest to the Debit or Credit column.
/// This avoids mistaking an exchange rate (e.g. `$ 54.66`) that happens to have
/// two decimals for the amount.
fn pick_amount(
    cols: &[(usize, String)],
    base: usize,
    debit_col: i32,
    credit_col: i32,
) -> Option<(usize, String, i32)> {
    let mut best: Option<(usize, String, i32)> = None;
    let mut best_dist = i32::MAX;
    for (i, (off, txt)) in cols.iter().enumerate() {
        if i == 0 || txt == "1.0" || !is_amount(txt) {
            continue;
        }
        let start = (base + off) as i32;
        let dist = if debit_col >= 0 && credit_col >= 0 {
            (start - debit_col).abs().min((start - credit_col).abs())
        } else {
            0
        };
        if dist < best_dist {
            best_dist = dist;
            best = Some((i, txt.clone(), start));
        }
    }
    best
}

/// Parse the extracted report text into entries. `text` is the output of
/// `pdftotext -layout` and uses `\x0c` (form feed) between pages.
fn parse_daily_entries(text: &str) -> Vec<ParsedEntry> {
    let mut entries: Vec<ParsedEntry> = Vec::new();
    let mut cur: Option<ParsedEntry> = None;
    let mut branch = String::new();
    let mut page_no: i64 = 0;
    // Debit/Credit column positions (character offsets) for the current page.
    // The report auto-sizes the account columns per page, so these must be
    // refreshed on every page's column header.
    let mut mid: f64 = 0.0;
    let mut debit_col: i32 = -1;
    let mut credit_col: i32 = -1;

    for page in text.split('\x0c') {
        page_no += 1;
        for line in page.lines() {
            // Column header: refresh the Debit/Credit midpoint for this page.
            if line.contains("Debit")
                && line.contains("Credit")
                && line.contains("Narration")
                && line.contains("Account")
            {
                if let (Some(d), Some(c)) = (line.find("Debit"), line.find("Credit")) {
                    debit_col = d as i32;
                    credit_col = c as i32;
                    mid = (d as f64 + c as f64) / 2.0;
                }
                continue;
            }

            // New document header.
            if let Some(cap) = re_header().captures(line) {
                if let Some(mut e) = cur.take() {
                    if !e.lines.is_empty() {
                        e.pages = e.pages.max(1);
                        apply_currency_pairing(&mut e.lines);
                        entries.push(e);
                    }
                }
                cur = Some(ParsedEntry {
                    doc_no: cap[1].trim().to_string(),
                    serial: cap[2].trim().to_string(),
                    doc_date: cap[3].trim().to_string(),
                    journal_no: cap[4].trim().to_string(),
                    journal: cap[5].trim().to_string(),
                    branch: branch.clone(),
                    source_page: page_no,
                    cur_page: page_no,
                    pages: 1,
                    ..Default::default()
                });
                continue;
            }

            // Branch appears once per document (just above the header on that page).
            if let Some(cap) = re_branch().captures(line) {
                branch = cap[1].split_whitespace().collect::<Vec<_>>().join(" ");
                if let Some(e) = cur.as_mut() {
                    e.branch = branch.clone();
                }
                continue;
            }

            // Per-document metadata lines.
            if let Some(e) = cur.as_mut() {
                if let Some(cap) = re_local_amt().captures(line) {
                    e.local_amt = cap[1].trim().to_string();
                    if page_no != e.cur_page {
                        e.pages += 1;
                        e.cur_page = page_no;
                    }
                    continue;
                }
                if let Some(cap) = re_doc_source().captures(line) {
                    e.doc_source = cap[1].trim().to_string();
                    continue;
                }
                if let Some(cap) = re_total().captures(line) {
                    e.total_value = cap[1].trim().to_string();
                    continue;
                }
                if let Some(cap) = re_contain().captures(line) {
                    e.contain_currency = cap[1].split_whitespace().collect::<Vec<_>>().join(" ");
                    continue;
                }
                // Account line?
                if let Some(m) = re_acct().find(line) {
                    let acct = m.as_str().trim().to_string();
                    let rest = &line[m.end()..];
                    let cols = split_columns(rest);
                    if page_no != e.cur_page {
                        e.pages += 1;
                        e.cur_page = page_no;
                    }
                    let amount_col = pick_amount(&cols, m.end(), debit_col, credit_col);
                    // Everything before the value column is the account name,
                    // possibly merged with the currency and the local value.
                    let before: Vec<String> = match amount_col {
                        Some((i, _, _)) => cols[..i].iter().map(|c| c.1.clone()).collect(),
                        None => cols.iter().map(|c| c.1.clone()).collect(),
                    };
                    let (account_name, currency, local_value) = split_name_currency(&before);
                    let mut line_rec = DailyEntryLine {
                        account_no: acct,
                        account: account_name,
                        currency,
                        local_value,
                        ..Default::default()
                    };
                    if let Some((i, amt, pos)) = amount_col {
                        if pos as f64 >= mid {
                            line_rec.credit = amt;
                        } else {
                            line_rec.debit = amt;
                        }
                        line_rec.narration = cols[i + 1..]
                            .iter()
                            .map(|c| c.1.clone())
                            .collect::<Vec<_>>()
                            .join(" ");
                    } else {
                        // No value on this row: treat the remainder as narration.
                        line_rec.narration = before.join(" ");
                    }
                    line_rec.from_document = re_from_doc()
                        .find(&line_rec.narration)
                        .map(|m| m.as_str().to_string())
                        .unwrap_or_default();
                    e.lines.push(line_rec);
                    continue;
                }
            }

            // Wrapped account line: when the account name is long, Oracle prints
            // the account number on its own line and the name + amount on the
            // next line (no account number). Complete the previous, incomplete
            // line from this one.
            let trimmed = line.trim();
            let first_ch = trimmed.chars().next().unwrap_or(' ');
            if !trimmed.is_empty() && !is_meta_line(trimmed) && !first_ch.is_ascii_digit() {
                if let Some(e) = cur.as_mut() {
                    let incomplete = e.lines.last().map(|l| l.debit.is_empty() && l.credit.is_empty()).unwrap_or(false);
                    if incomplete {
                        let cols = split_columns(line);
                        let found = pick_amount(&cols, 0, debit_col, credit_col);
                        if let Some((i, amt, start)) = found {
                            let before: Vec<String> = cols[..i].iter().map(|c| c.1.clone()).collect();
                            let (name, currency, local_value) = split_name_currency(&before);
                            let narration = cols[i + 1..].iter().map(|c| c.1.clone()).collect::<Vec<_>>().join(" ");
                            if let Some(last) = e.lines.last_mut() {
                                if last.account.is_empty() { last.account = name; }
                                if last.currency.is_empty() { last.currency = currency; }
                                if last.local_value.is_empty() { last.local_value = local_value; }
                                if start as f64 >= mid { last.credit = amt; } else { last.debit = amt; }
                                last.narration = narration;
                                last.from_document = re_from_doc().find(&last.narration).map(|m| m.as_str().to_string()).unwrap_or_default();
                            }
                            continue;
                        }
                    }
                }
            }

            // Narration continuation line (indented into the narration column).
            if trimmed.is_empty() || is_meta_line(trimmed) {
                continue;
            }
            let lead = line.len() - line.trim_start().len();
            if lead >= 40 {
                if let Some(e) = cur.as_mut() {
                    if let Some(last) = e.lines.last_mut() {
                        if !last.narration.is_empty() {
                            last.narration.push(' ');
                        }
                        last.narration.push_str(trimmed);
                        if last.from_document.is_empty() {
                            if let Some(m) = re_from_doc().find(trimmed) {
                                last.from_document = m.as_str().to_string();
                            }
                        }
                    }
                }
            }
        }
    }
    if let Some(mut e) = cur.take() {
        if !e.lines.is_empty() {
            e.pages = e.pages.max(1);
            apply_currency_pairing(&mut e.lines);
            entries.push(e);
        }
    }
    entries
}

fn join_refs(items: &[String]) -> String {
    items.join("|")
}

/// Invoice references mentioned anywhere in narration lines. Supports the
/// `inv.N/YYYY` form as well as `INV NO :1864`, `INV NO.2-2026`, `INV#7712` and
/// `... + 1865 + 1815` continuations.
fn invoice_refs_from_lines(lines: &[DailyEntryLine]) -> Vec<String> {
    let mut seen = Vec::new();
    for l in lines {
        for m in re_invoice_marker().find_iter(&l.narration) {
            let tail = &l.narration[m.end()..];
            let Some(cap) = re_invoice_run().captures(tail) else { continue };
            for tok in cap[1].split('+') {
                let r = normalize_invoice(tok);
                if r.is_empty() {
                    continue;
                }
                if !seen.contains(&r) {
                    seen.push(r);
                }
            }
        }
    }
    seen
}

fn from_documents_from_lines(lines: &[DailyEntryLine]) -> Vec<String> {
    let mut seen = Vec::new();
    for l in lines {
        for cap in re_from_doc().captures_iter(&l.narration) {
            let r = cap[1].to_uppercase();
            if !seen.contains(&r) {
                seen.push(r);
            }
        }
    }
    seen
}

fn invoice_refs_of(e: &ParsedEntry) -> Vec<String> {
    invoice_refs_from_lines(&e.lines)
}

fn from_documents_of(e: &ParsedEntry) -> Vec<String> {
    from_documents_from_lines(&e.lines)
}

// ── Storage ──────────────────────────────────────────────────────────────────
pub fn init_schema(conn: &Connection) -> Result<(), String> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS daily_entries (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            entry_key TEXT NOT NULL UNIQUE,
            source_file TEXT DEFAULT '',
            source_page INTEGER DEFAULT 0,
            pages INTEGER DEFAULT 1,
            doc_no TEXT DEFAULT '',
            serial TEXT DEFAULT '',
            doc_date TEXT DEFAULT '',
            journal_no TEXT DEFAULT '',
            journal TEXT DEFAULT '',
            branch TEXT DEFAULT '',
            doc_source TEXT DEFAULT '',
            contain_currency TEXT DEFAULT '',
            local_amt TEXT DEFAULT '',
            total_value TEXT DEFAULT '',
            invoice_refs TEXT DEFAULT '',
            from_documents TEXT DEFAULT '',
            lines_json TEXT DEFAULT '[]',
            report_date TEXT DEFAULT '',
            debit_total REAL DEFAULT 0,
            credit_total REAL DEFAULT 0,
            balanced INTEGER DEFAULT 1,
            line_count INTEGER DEFAULT 0,
            date_key TEXT DEFAULT '',
            s_doc TEXT DEFAULT '',
            s_account_no TEXT DEFAULT '',
            s_account TEXT DEFAULT '',
            s_value TEXT DEFAULT '',
            s_narration TEXT DEFAULT '',
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        )",
        [],
    )
    .map_err(|e| e.to_string())?;
    conn.execute("CREATE TABLE IF NOT EXISTS daily_meta (k TEXT PRIMARY KEY, v TEXT)", [])
        .map_err(|e| e.to_string())?;
    conn.execute("CREATE INDEX IF NOT EXISTS idx_daily_source ON daily_entries(source_file)", [])
        .map_err(|e| e.to_string())?;
    conn.execute("CREATE INDEX IF NOT EXISTS idx_daily_doc ON daily_entries(doc_no)", [])
        .map_err(|e| e.to_string())?;
    // Column list (used by several migrations below).
    let cols: Vec<String> = conn
        .prepare("PRAGMA table_info(daily_entries)")
        .map_err(|e| e.to_string())?
        .query_map([], |r| r.get::<_, String>(1))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    // Migration (v0.3.152): add report_date and rebuild entry_key on document
    // identity (Doc. No + serial + Doc. Date) so the same voucher from
    // overlapping reports isn't counted twice.
    if !cols.iter().any(|c| c == "report_date") {
        conn.execute("ALTER TABLE daily_entries ADD COLUMN report_date TEXT DEFAULT ''", [])
            .map_err(|e| e.to_string())?;
        conn.execute(
            "DELETE FROM daily_entries WHERE id NOT IN (
                 SELECT MAX(id) FROM daily_entries GROUP BY doc_no, serial, doc_date)",
            [],
        )
        .map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE daily_entries SET entry_key = doc_no || '|' || serial || '|' || doc_date",
            [],
        )
        .map_err(|e| e.to_string())?;
    }

    // Migration (v0.3.154): persist computed fields + a search index so listing
    // no longer JSON-parses every row. Backfill once for existing rows.
    let derived_cols = [
        ("debit_total", "REAL DEFAULT 0"),
        ("credit_total", "REAL DEFAULT 0"),
        ("balanced", "INTEGER DEFAULT 1"),
        ("line_count", "INTEGER DEFAULT 0"),
        ("date_key", "TEXT DEFAULT ''"),
        ("s_doc", "TEXT DEFAULT ''"),
        ("s_account_no", "TEXT DEFAULT ''"),
        ("s_account", "TEXT DEFAULT ''"),
        ("s_value", "TEXT DEFAULT ''"),
        ("s_narration", "TEXT DEFAULT ''"),
    ];
    let mut added = false;
    for (name, decl) in derived_cols {
        if !cols.iter().any(|c| c == name) {
            conn.execute(&format!("ALTER TABLE daily_entries ADD COLUMN {} {}", name, decl), [])
                .map_err(|e| e.to_string())?;
            added = true;
        }
    }
    if added {
        // Defer the (one-time) backfill to the first Daily Entries query so app
        // startup isn't blocked; ensure_backfilled() runs it off the main thread.
        conn.execute("INSERT OR REPLACE INTO daily_meta (k, v) VALUES ('backfill_pending', '1')", [])
            .map_err(|e| e.to_string())?;
    }
    conn.execute("CREATE INDEX IF NOT EXISTS idx_daily_datekey ON daily_entries(date_key)", [])
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Run the deferred derived-column backfill once (guarded by a marker row).
fn ensure_backfilled(conn: &Connection) -> Result<(), String> {
    let pending: Option<String> = conn
        .query_row("SELECT v FROM daily_meta WHERE k = 'backfill_pending'", [], |r| r.get(0))
        .optional()
        .map_err(|e| e.to_string())?;
    if pending.as_deref() == Some("1") {
        backfill_derived(conn)?;
        conn.execute("DELETE FROM daily_meta WHERE k = 'backfill_pending'", [])
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Compute the stored (derived) columns for one entry and return them.
struct Derived {
    debit_total: f64,
    credit_total: f64,
    balanced: i64,
    line_count: i64,
    date_key: String,
    s_doc: String,
    s_account_no: String,
    s_account: String,
    s_value: String,
    s_narration: String,
}

fn compute_derived(source_file: &str, doc_no: &str, serial: &str, doc_date: &str, lines: &[DailyEntryLine]) -> Derived {
    let debit_total: f64 = lines.iter().map(|l| local_amount(&l.debit, &l.local_value)).sum();
    let credit_total: f64 = lines.iter().map(|l| local_amount(&l.credit, &l.local_value)).sum();
    let mut s_account_no = String::new();
    let mut s_account = String::new();
    let mut s_value = String::new();
    let mut s_narration = String::new();
    for l in lines {
        s_account_no.push_str(&l.account_no);
        s_account_no.push(' ');
        s_account.push_str(&l.account);
        s_account.push(' ');
        // Both the formatted ("154,396.00") and un-comma'd ("154396.00") forms.
        s_value.push_str(&l.debit);
        s_value.push(' ');
        s_value.push_str(&l.credit);
        s_value.push(' ');
        s_value.push_str(&l.debit.replace(',', ""));
        s_value.push(' ');
        s_value.push_str(&l.credit.replace(',', ""));
        s_value.push(' ');
        s_narration.push_str(&l.narration);
        s_narration.push(' ');
        s_narration.push_str(&l.from_document);
        s_narration.push(' ');
    }
    for r in invoice_refs_from_lines(lines) {
        s_narration.push_str(&r);
        s_narration.push(' ');
    }
    for r in from_documents_from_lines(lines) {
        s_narration.push_str(&r);
        s_narration.push(' ');
    }
    Derived {
        debit_total,
        credit_total,
        balanced: if is_balanced(debit_total, credit_total) { 1 } else { 0 },
        line_count: lines.len() as i64,
        date_key: date_sort_key(doc_date),
        s_doc: format!("{} {} {} {}", doc_no, serial, source_file, doc_date).to_lowercase(),
        s_account_no: s_account_no.to_lowercase(),
        s_account: s_account.to_lowercase(),
        s_value: s_value.to_lowercase(),
        s_narration: s_narration.to_lowercase(),
    }
}

fn derived_params(d: &Derived) -> [rusqlite::types::Value; 10] {
    use rusqlite::types::Value;
    [
        Value::Real(d.debit_total),
        Value::Real(d.credit_total),
        Value::Integer(d.balanced),
        Value::Integer(d.line_count),
        Value::Text(d.date_key.clone()),
        Value::Text(d.s_doc.clone()),
        Value::Text(d.s_account_no.clone()),
        Value::Text(d.s_account.clone()),
        Value::Text(d.s_value.clone()),
        Value::Text(d.s_narration.clone()),
    ]
}

/// One-time backfill of the derived/index columns by parsing every stored entry.
fn backfill_derived(conn: &Connection) -> Result<(), String> {
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    let mut rows: Vec<(i64, String, String, String, String, String)> = Vec::new();
    {
        let mut stmt = tx
            .prepare("SELECT id, source_file, doc_no, serial, doc_date, lines_json FROM daily_entries")
            .map_err(|e| e.to_string())?;
        let iter = stmt
            .query_map([], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?))
            })
            .map_err(|e| e.to_string())?;
        for r in iter {
            rows.push(r.map_err(|e| e.to_string())?);
        }
    }
    for (id, source_file, doc_no, serial, doc_date, lines_json) in rows {
        let mut lines: Vec<DailyEntryLine> = serde_json::from_str(&lines_json).unwrap_or_default();
        // Ensure older rows carry the currency-pairing correction and store it.
        let changed = apply_currency_pairing(&mut lines);
        let d = compute_derived(&source_file, &doc_no, &serial, &doc_date, &lines);
        if changed {
            let lj = serde_json::to_string(&lines).unwrap_or(lines_json);
            tx.execute(
                "UPDATE daily_entries SET debit_total=?1, credit_total=?2, balanced=?3, line_count=?4,
                     date_key=?5, s_doc=?6, s_account_no=?7, s_account=?8, s_value=?9, s_narration=?10,
                     lines_json=?11 WHERE id=?12",
                rusqlite::params_from_iter(derived_params(&d).into_iter().chain([
                    rusqlite::types::Value::Text(lj),
                    rusqlite::types::Value::Integer(id),
                ])),
            )
            .map_err(|e| e.to_string())?;
        } else {
            tx.execute(
                "UPDATE daily_entries SET debit_total=?1, credit_total=?2, balanced=?3, line_count=?4,
                     date_key=?5, s_doc=?6, s_account_no=?7, s_account=?8, s_value=?9, s_narration=?10
                 WHERE id=?11",
                rusqlite::params_from_iter(derived_params(&d).into_iter().chain([
                    rusqlite::types::Value::Integer(id),
                ])),
            )
            .map_err(|e| e.to_string())?;
        }
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(())
}

/// A wire row for Supabase sync (lines kept as a JSON string).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DailyEntrySyncRow {
    pub entry_key: String,
    #[serde(default)]
    pub source_file: String,
    #[serde(default)]
    pub source_page: i64,
    #[serde(default)]
    pub pages: i64,
    #[serde(default)]
    pub doc_no: String,
    #[serde(default)]
    pub serial: String,
    #[serde(default)]
    pub doc_date: String,
    #[serde(default)]
    pub journal_no: String,
    #[serde(default)]
    pub journal: String,
    #[serde(default)]
    pub branch: String,
    #[serde(default)]
    pub doc_source: String,
    #[serde(default)]
    pub contain_currency: String,
    #[serde(default)]
    pub local_amt: String,
    #[serde(default)]
    pub total_value: String,
    #[serde(default)]
    pub invoice_refs: String,
    #[serde(default)]
    pub from_documents: String,
    #[serde(default)]
    pub lines_json: String,
    #[serde(default)]
    pub report_date: String,
    #[serde(default)]
    pub updated_at: String,
}

/// Identity of a voucher: the same document can appear in more than one report
/// (overlapping date ranges), so dedupe on Doc. No + serial + Doc. Date rather
/// than on the source file.
fn entry_key_for(e: &ParsedEntry) -> String {
    format!("{}|{}|{}", e.doc_no, e.serial, e.doc_date)
}

/// Report generation date from the page header (`Date : DD-MM-YYYY`). Used so
/// the newest report replaces an older one for the same document.
fn parse_report_date(text: &str) -> String {
    static R: OnceLock<Regex> = OnceLock::new();
    let re = R.get_or_init(|| Regex::new(r"Date\s*:\s*(\d{2}-\d{2}-\d{4})").unwrap());
    re.captures(text)
        .map(|c| c[1].to_string())
        .unwrap_or_default()
}

/// True when `new_report` is at least as new as `old_report` (an empty old date
/// counts as older, and equal dates mean a re-upload of the same report).
fn report_at_least_as_new(new_report: &str, old_report: &str) -> bool {
    if old_report.trim().is_empty() {
        return true;
    }
    date_sort_key(new_report) >= date_sort_key(old_report)
}

fn insert_entry(conn: &Connection, source_file: &str, report_date: &str, e: &ParsedEntry) -> Result<(), String> {
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let key = entry_key_for(e);
    let lines_json = serde_json::to_string(&e.lines).unwrap_or_else(|_| "[]".into());
    let refs = join_refs(&invoice_refs_of(e));
    let docs = join_refs(&from_documents_of(e));
    let d = compute_derived(source_file, &e.doc_no, &e.serial, &e.doc_date, &e.lines);
    let mut args: Vec<rusqlite::types::Value> = vec![
        rusqlite::types::Value::Text(key),
        rusqlite::types::Value::Text(source_file.to_string()),
        rusqlite::types::Value::Integer(e.source_page),
        rusqlite::types::Value::Integer(e.pages),
        rusqlite::types::Value::Text(e.doc_no.clone()),
        rusqlite::types::Value::Text(e.serial.clone()),
        rusqlite::types::Value::Text(e.doc_date.clone()),
        rusqlite::types::Value::Text(e.journal_no.clone()),
        rusqlite::types::Value::Text(e.journal.clone()),
        rusqlite::types::Value::Text(e.branch.clone()),
        rusqlite::types::Value::Text(e.doc_source.clone()),
        rusqlite::types::Value::Text(e.contain_currency.clone()),
        rusqlite::types::Value::Text(e.local_amt.clone()),
        rusqlite::types::Value::Text(e.total_value.clone()),
        rusqlite::types::Value::Text(refs),
        rusqlite::types::Value::Text(docs),
        rusqlite::types::Value::Text(lines_json),
        rusqlite::types::Value::Text(report_date.to_string()),
    ];
    args.extend(derived_params(&d));
    args.push(rusqlite::types::Value::Text(now.clone()));
    args.push(rusqlite::types::Value::Text(now));
    conn.execute(
        "INSERT OR REPLACE INTO daily_entries
            (entry_key, source_file, source_page, pages, doc_no, serial, doc_date, journal_no, journal,
             branch, doc_source, contain_currency, local_amt, total_value, invoice_refs, from_documents,
             lines_json, report_date,
             debit_total, credit_total, balanced, line_count, date_key, s_doc, s_account_no, s_account, s_value, s_narration,
             created_at, updated_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,
                 ?19,?20,?21,?22,?23,?24,?25,?26,?27,?28,
                 COALESCE((SELECT created_at FROM daily_entries WHERE entry_key=?1), ?29), ?30)",
        rusqlite::params_from_iter(args),
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
pub struct DailyEntriesImportResult {
    pub source_file: String,
    pub entries: usize,
    pub lines: usize,
    pub pages: i64,
    pub multipage: usize,
    pub invoice_refs: usize,
    pub duplicates: usize,
    pub skipped: usize,
    pub report_date: String,
}

pub fn import_file(conn: &Connection, file_path: &str) -> Result<DailyEntriesImportResult, String> {
    let base = std::path::Path::new(file_path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| file_path.to_string());
    let text = crate::importer::extract_layout_text(file_path)?;
    let report_date = parse_report_date(&text);
    let parsed = parse_daily_entries(&text);
    if parsed.is_empty() {
        return Err("No daily-entry documents were found in this PDF. Is it a Daily Entries report?".into());
    }

    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    // Re-uploading this same file: drop the documents currently attributed to it.
    tx.execute("DELETE FROM daily_entries WHERE source_file = ?1", params![base])
        .map_err(|e| e.to_string())?;
    let mut lines = 0usize;
    let mut refs = 0usize;
    let mut inserted = 0usize;
    let mut skipped = 0usize;
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for e in &parsed {
        let key = entry_key_for(e);
        // Same document twice within one report: last one wins.
        if !seen.insert(key.clone()) {
            // fall through to replace
        }
        // If another (newer) report already has this document, keep it.
        let existing: Option<String> = tx
            .query_row(
                "SELECT report_date FROM daily_entries WHERE entry_key = ?1",
                params![key],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        if let Some(old) = existing {
            if !report_at_least_as_new(&report_date, &old) {
                skipped += 1;
                continue;
            }
        }
        insert_entry(&tx, &base, &report_date, e)?;
        inserted += 1;
        lines += e.lines.len();
        refs += invoice_refs_of(e).len();
    }
    tx.commit().map_err(|e| e.to_string())?;

    let pages: i64 = parsed.iter().map(|e| e.pages).sum();

    Ok(DailyEntriesImportResult {
        source_file: base,
        entries: inserted,
        lines,
        pages,
        multipage: parsed.iter().filter(|e| e.pages > 1).count(),
        invoice_refs: refs,
        duplicates: 0,
        skipped,
        report_date,
    })
}

/// Delete every entry that came from a given source file (used when re-importing
/// an updated report).
pub fn delete_source(conn: &Connection, source_file: &str) -> Result<(), String> {
    conn.execute("DELETE FROM daily_entries WHERE source_file = ?1", params![source_file])
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn clear_all(conn: &Connection) -> Result<(), String> {
    conn.execute("DELETE FROM daily_entries", []).map_err(|e| e.to_string())?;
    Ok(())
}

// ── Records returned to the UI ───────────────────────────────────────────────
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PaidBeforeRef {
    pub reference: String,
    pub doc_no: String,
    pub doc_date: String,
    pub source_file: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DailyEntryRecord {
    pub id: i64,
    pub entry_key: String,
    pub source_file: String,
    pub source_page: i64,
    pub pages: i64,
    pub doc_no: String,
    pub serial: String,
    pub doc_date: String,
    pub journal_no: String,
    pub journal: String,
    pub branch: String,
    pub doc_source: String,
    pub contain_currency: String,
    pub local_amt: String,
    pub total_value: String,
    pub invoice_refs: Vec<String>,
    pub from_documents: Vec<String>,
    pub lines: Vec<DailyEntryLine>,
    /// Sum of this document's Debit / Credit columns (they should be equal).
    pub debit_total: f64,
    pub credit_total: f64,
    pub balanced: bool,
    pub paid_before: bool,
    pub paid_before_refs: Vec<String>,
    pub paid_before_in: Vec<PaidBeforeRef>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct DailyEntriesStats {
    pub total: usize,
    pub debit: f64,
    pub credit: f64,
    pub pages: i64,
    pub multipage: usize,
    pub paid_before: usize,
    pub files: usize,
    pub lines: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct DailyEntriesList {
    pub entries: Vec<DailyEntryRecord>,
    pub total: usize,
    pub stats: DailyEntriesStats,
}

/// Load every entry, compute invoice duplicates, and return them ordered by date.
/// Load every entry with computed fields (used by tests; the UI list now reads
/// the persisted/derived columns directly).
#[allow(dead_code)]
pub fn load_all(conn: &Connection) -> Result<Vec<DailyEntryRecord>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, entry_key, source_file, source_page, pages, doc_no, serial, doc_date,
                    journal_no, journal, branch, doc_source, contain_currency, local_amt,
                    total_value, invoice_refs, from_documents, lines_json
             FROM daily_entries
             ORDER BY doc_date ASC, source_file ASC, doc_no ASC, serial ASC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            let lines_json: String = r.get(17)?;
            let mut lines: Vec<DailyEntryLine> = serde_json::from_str(&lines_json).unwrap_or_default();
            // Correct contra lines labelled Local Curr that actually carry a
            // foreign amount (applies to already-imported entries too).
            apply_currency_pairing(&mut lines);
            // Recompute refs from the lines so an upgraded detector applies to
            // already-imported entries without needing a re-upload.
            let invoice_refs = invoice_refs_from_lines(&lines);
            let from_documents = from_documents_from_lines(&lines);
            let debit_total: f64 = lines.iter().map(|l| local_amount(&l.debit, &l.local_value)).sum();
            let credit_total: f64 = lines.iter().map(|l| local_amount(&l.credit, &l.local_value)).sum();
            let balanced = is_balanced(debit_total, credit_total);
            Ok(DailyEntryRecord {
                id: r.get(0)?,
                entry_key: r.get(1)?,
                source_file: r.get(2)?,
                source_page: r.get(3)?,
                pages: r.get(4)?,
                doc_no: r.get(5)?,
                serial: r.get(6)?,
                doc_date: r.get(7)?,
                journal_no: r.get(8)?,
                journal: r.get(9)?,
                branch: r.get(10)?,
                doc_source: r.get(11)?,
                contain_currency: r.get(12)?,
                local_amt: r.get(13)?,
                total_value: r.get(14)?,
                invoice_refs,
                from_documents,
                lines,
                debit_total,
                credit_total,
                balanced,
                ..Default::default()
            })
        })
        .map_err(|e| e.to_string())?;
    let mut entries: Vec<DailyEntryRecord> = rows.filter_map(|r| r.ok()).collect();
    entries.sort_by(|a, b| {
        date_sort_key(&a.doc_date)
            .cmp(&date_sort_key(&b.doc_date))
            .then(a.source_file.cmp(&b.source_file))
            .then(a.doc_no.cmp(&b.doc_no))
            .then(a.serial.cmp(&b.serial))
    });

    // Duplicate detection: the same invoice reference paid in more than one entry.
    let mut first_seen: HashMap<String, (String, String, String, String)> = HashMap::new();
    for e in entries.iter_mut() {
        let mut dup_refs: Vec<String> = Vec::new();
        let mut dup_in: Vec<PaidBeforeRef> = Vec::new();
        for r in &e.invoice_refs {
            if let Some((key, doc, date, src)) = first_seen.get(r) {
                if key != &e.entry_key {
                    dup_refs.push(r.clone());
                    dup_in.push(PaidBeforeRef {
                        reference: r.clone(),
                        doc_no: doc.clone(),
                        doc_date: date.clone(),
                        source_file: src.clone(),
                    });
                }
            }
        }
        if !dup_refs.is_empty() {
            e.paid_before = true;
            e.paid_before_refs = dup_refs;
            e.paid_before_in = dup_in;
        }
        for r in &e.invoice_refs {
            first_seen.entry(r.clone()).or_insert_with(|| {
                (e.entry_key.clone(), e.doc_no.clone(), e.doc_date.clone(), e.source_file.clone())
            });
        }
    }
    Ok(entries)
}

fn num(s: &str) -> f64 {
    s.replace(',', "").trim().parse::<f64>().unwrap_or(0.0)
}

/// A line's amount converted to local currency. The report's Debit/Credit
/// columns are in the voucher's transaction currency (e.g. `$`) and the
/// "Local Value" column is the exchange rate, so `amount * rate` gives the
/// local amount — and only then do Debit and Credit totals match (they equal
/// the report's own "Doc. Total Value ... Local Value").
fn local_amount(amount: &str, rate: &str) -> f64 {
    let a = num(amount);
    if a == 0.0 {
        return 0.0;
    }
    let r = rate.trim().parse::<f64>().unwrap_or(1.0);
    let r = if r.is_finite() && r != 0.0 { r } else { 1.0 };
    // The report rounds each line's local amount to 2 decimals, so do the same
    // (otherwise summing gives a small drift vs the report's own total).
    ((a * r) * 100.0).round() / 100.0
}

/// Some contra lines (e.g. the `1216 Import Debitors` side of a foreign-currency
/// payment) are labelled `Local Curr` with rate 1.0 even though their amount is
/// in the foreign currency. When such a line's amount matches a foreign-currency
/// line in the same voucher, use that line's rate and currency so the voucher
/// reconciles. Verified to fix exactly the affected vouchers and change nothing
/// else across all the sample reports.
fn apply_currency_pairing(lines: &mut [DailyEntryLine]) -> bool {
    let amount_of = |l: &DailyEntryLine| -> f64 {
        let d = num(&l.debit);
        if d != 0.0 { d } else { num(&l.credit) }
    };
    let refs: Vec<(f64, String, String, bool)> = lines
        .iter()
        .filter_map(|l| {
            let r = l.local_value.trim().parse::<f64>().ok()?;
            if r == 1.0 {
                return None;
            }
            let a = amount_of(l);
            if a == 0.0 {
                return None;
            }
            Some((a, l.local_value.trim().to_string(), l.currency.clone(), num(&l.debit) != 0.0))
        })
        .collect();
    if refs.is_empty() {
        return false;
    }
    let mut changed = false;
    for l in lines.iter_mut() {
        let r = l.local_value.trim().parse::<f64>().unwrap_or(1.0);
        if r != 1.0 {
            continue;
        }
        let a = amount_of(l);
        if a == 0.0 {
            continue;
        }
        let is_debit = num(&l.debit) != 0.0;
        let pick = refs
            .iter()
            .find(|(ra, _, _, rd)| *rd != is_debit && (ra - a).abs() < 0.005)
            .or_else(|| refs.iter().find(|(ra, _, _, _)| (ra - a).abs() < 0.005));
        if let Some((_, rate, currency, _)) = pick {
            l.local_value = rate.clone();
            if !currency.is_empty() {
                l.currency = currency.clone();
            }
            changed = true;
        }
    }
    changed
}

/// Debit and Credit balance within currency-rounding tolerance: the displayed
/// exchange rate has 6 decimals, so `amount * rate` can differ from the report's
/// local amount by a tiny fraction. Genuine imbalances are far larger.
fn is_balanced(debit: f64, credit: f64) -> bool {
    let tol = 1.0_f64.max(debit.abs().max(credit.abs()) * 2e-6);
    (debit - credit).abs() <= tol
}

/// `DD-MM-YYYY` → `YYYYMMDD` so entries order chronologically (a plain string
/// sort of the report dates would order by day-of-month first).
fn date_sort_key(d: &str) -> String {
    let parts: Vec<&str> = d.split('-').collect();
    if parts.len() == 3 && parts[0].len() == 2 && parts[1].len() == 2 && parts[2].len() == 4 {
        format!("{}{}{}", parts[2], parts[1], parts[0])
    } else {
        d.to_string()
    }
}

/// Split a `|`-joined column back into items (invoice refs / from-documents).
fn split_refs(s: &str) -> Vec<String> {
    s.split('|').filter(|p| !p.is_empty()).map(|p| p.to_string()).collect()
}

fn stats_from_db(conn: &Connection) -> Result<DailyEntriesStats, String> {
    conn.query_row(
        "SELECT COUNT(*), COALESCE(SUM(debit_total),0), COALESCE(SUM(credit_total),0),
                COALESCE(SUM(pages),0), COALESCE(SUM(CASE WHEN pages>1 THEN 1 ELSE 0 END),0),
                COALESCE(SUM(line_count),0), COUNT(DISTINCT source_file)
         FROM daily_entries",
        [],
        |r| {
            Ok(DailyEntriesStats {
                total: r.get::<_, i64>(0)? as usize,
                debit: r.get(1)?,
                credit: r.get(2)?,
                pages: r.get(3)?,
                multipage: r.get::<_, i64>(4)? as usize,
                paid_before: 0,
                lines: r.get::<_, i64>(5)? as usize,
                files: r.get::<_, i64>(6)? as usize,
            })
        },
    )
    .map_err(|e| e.to_string())
}

pub fn list(
    conn: &Connection,
    search: &str,
    search_in: &str,
    date_from: &str,
    date_to: &str,
    offset: usize,
    limit: usize,
) -> Result<DailyEntriesList, String> {
    ensure_backfilled(conn)?;
    let stats = stats_from_db(conn)?;
    let q = search.trim().to_lowercase();
    let mut clauses: Vec<String> = Vec::new();
    let mut args: Vec<rusqlite::types::Value> = Vec::new();
    if !q.is_empty() {
        let like = format!("%{}%", q);
        let cols: Vec<&str> = match search_in {
            "doc" => vec!["s_doc"],
            "account_no" => vec!["s_account_no"],
            "account" => vec!["s_account"],
            "value" => vec!["s_value"],
            "narration" => vec!["s_narration"],
            _ => vec!["s_doc", "s_account_no", "s_account", "s_value", "s_narration"],
        };
        let ors: Vec<String> = cols.iter().map(|c| format!("{} LIKE ?", c)).collect();
        clauses.push(format!("({})", ors.join(" OR ")));
        for _ in &cols {
            args.push(rusqlite::types::Value::Text(like.clone()));
        }
    }
    if !date_from.is_empty() {
        clauses.push("date_key >= ?".into());
        args.push(rusqlite::types::Value::Text(date_from.replace('-', "")));
    }
    if !date_to.is_empty() {
        clauses.push("date_key <= ?".into());
        args.push(rusqlite::types::Value::Text(date_to.replace('-', "")));
    }
    let where_sql = if clauses.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", clauses.join(" AND "))
    };
    let total: i64 = conn
        .query_row(
            &format!("SELECT COUNT(*) FROM daily_entries {}", where_sql),
            rusqlite::params_from_iter(args.iter()),
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;

    let mut page_args = args;
    page_args.push(rusqlite::types::Value::Integer(if limit == 0 { i64::MAX } else { limit as i64 }));
    page_args.push(rusqlite::types::Value::Integer(offset as i64));
    let sql = format!(
        "SELECT id, entry_key, source_file, source_page, pages, doc_no, serial, doc_date,
                journal_no, journal, branch, doc_source, contain_currency, local_amt,
                total_value, invoice_refs, from_documents, lines_json,
                debit_total, credit_total, balanced
         FROM daily_entries {} ORDER BY date_key ASC, source_file ASC, doc_no ASC, serial ASC LIMIT ? OFFSET ?",
        where_sql
    );
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(page_args.iter()), |r| {
            let lines_json: String = r.get(17)?;
            let lines: Vec<DailyEntryLine> = serde_json::from_str(&lines_json).unwrap_or_default();
            let refs: String = r.get(15).unwrap_or_default();
            let docs: String = r.get(16).unwrap_or_default();
            Ok(DailyEntryRecord {
                id: r.get(0)?,
                entry_key: r.get(1)?,
                source_file: r.get(2)?,
                source_page: r.get(3)?,
                pages: r.get(4)?,
                doc_no: r.get(5)?,
                serial: r.get(6)?,
                doc_date: r.get(7)?,
                journal_no: r.get(8)?,
                journal: r.get(9)?,
                branch: r.get(10)?,
                doc_source: r.get(11)?,
                contain_currency: r.get(12)?,
                local_amt: r.get(13)?,
                total_value: r.get(14)?,
                invoice_refs: split_refs(&refs),
                from_documents: split_refs(&docs),
                lines,
                debit_total: r.get(18)?,
                credit_total: r.get(19)?,
                balanced: r.get::<_, i64>(20)? != 0,
                ..Default::default()
            })
        })
        .map_err(|e| e.to_string())?;
    let entries: Vec<DailyEntryRecord> = rows.filter_map(|r| r.ok()).collect();
    Ok(DailyEntriesList { entries, total: total as usize, stats })
}

/// Persist an edited set of lines for one entry (used when the user corrects a
/// rate or an amount).
pub fn update_lines(conn: &Connection, entry_key: &str, lines_json: &str) -> Result<(), String> {
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    // Recompute the stored (derived) columns from the edited lines. Pairing is
    // NOT re-applied here so the user's edit is authoritative.
    let (source_file, doc_no, serial, doc_date): (String, String, String, String) = conn
        .query_row(
            "SELECT source_file, doc_no, serial, doc_date FROM daily_entries WHERE entry_key = ?1",
            params![entry_key],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Entry not found".to_string())?;
    let lines: Vec<DailyEntryLine> = serde_json::from_str(lines_json).unwrap_or_default();
    let d = compute_derived(&source_file, &doc_no, &serial, &doc_date, &lines);
    let mut args: Vec<rusqlite::types::Value> = vec![
        rusqlite::types::Value::Text(lines_json.to_string()),
    ];
    args.extend(derived_params(&d));
    args.push(rusqlite::types::Value::Text(now));
    args.push(rusqlite::types::Value::Text(entry_key.to_string()));
    conn.execute(
        "UPDATE daily_entries SET lines_json=?1, debit_total=?2, credit_total=?3, balanced=?4,
             line_count=?5, date_key=?6, s_doc=?7, s_account_no=?8, s_account=?9, s_value=?10,
             s_narration=?11, updated_at=?12 WHERE entry_key=?13",
        rusqlite::params_from_iter(args),
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
pub struct DailyEntryFileInfo {
    pub source_file: String,
    pub documents: usize,
    pub pages: i64,
    pub debit: f64,
    pub credit: f64,
    pub balanced: bool,
}

/// Distinct list of the files that have been imported, with per-file totals.
pub fn list_files(conn: &Connection) -> Result<Vec<DailyEntryFileInfo>, String> {
    ensure_backfilled(conn)?;
    let mut stmt = conn
        .prepare(
            "SELECT source_file, COUNT(*), COALESCE(SUM(pages),0),
                    COALESCE(SUM(debit_total),0), COALESCE(SUM(credit_total),0)
             FROM daily_entries GROUP BY source_file ORDER BY source_file",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            let debit: f64 = r.get(3)?;
            let credit: f64 = r.get(4)?;
            Ok(DailyEntryFileInfo {
                source_file: r.get(0)?,
                documents: r.get::<_, i64>(1)? as usize,
                pages: r.get(2)?,
                debit,
                credit,
                balanced: is_balanced(debit, credit),
            })
        })
        .map_err(|e| e.to_string())?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn for_sync(conn: &Connection) -> Result<Vec<DailyEntrySyncRow>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT entry_key, source_file, source_page, pages, doc_no, serial, doc_date,
                    journal_no, journal, branch, doc_source, contain_currency, local_amt,
                    total_value, invoice_refs, from_documents, lines_json, report_date, updated_at
             FROM daily_entries",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            let lines_json: String = r.get(16)?;
            let lines: Vec<DailyEntryLine> = serde_json::from_str(&lines_json).unwrap_or_default();
            Ok(DailyEntrySyncRow {
                entry_key: r.get(0)?,
                source_file: r.get(1)?,
                source_page: r.get(2)?,
                pages: r.get(3)?,
                doc_no: r.get(4)?,
                serial: r.get(5)?,
                doc_date: r.get(6)?,
                journal_no: r.get(7)?,
                journal: r.get(8)?,
                branch: r.get(9)?,
                doc_source: r.get(10)?,
                contain_currency: r.get(11)?,
                local_amt: r.get(12)?,
                total_value: r.get(13)?,
                invoice_refs: join_refs(&invoice_refs_from_lines(&lines)),
                from_documents: join_refs(&from_documents_from_lines(&lines)),
                lines_json,
                report_date: r.get(17)?,
                updated_at: r.get(18)?,
            })
        })
        .map_err(|e| e.to_string())?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn sync_from_remote(conn: &Connection, rows: &[DailyEntrySyncRow]) -> Result<(), String> {
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    for r in rows {
        // Recompute the key from the document identity so rows pushed by an
        // older app version (keyed by source file) collapse onto one entry.
        let key = format!("{}|{}|{}", r.doc_no, r.serial, r.doc_date);
        // Recompute the stored/derived columns (a row pulled from an older
        // app version may not carry them). Pairing is applied so remote rows
        // stored before that fix still reconcile.
        let mut lines: Vec<DailyEntryLine> = serde_json::from_str(&r.lines_json).unwrap_or_default();
        let pairing_changed = apply_currency_pairing(&mut lines);
        let d = compute_derived(&r.source_file, &r.doc_no, &r.serial, &r.doc_date, &lines);
        let lines_json = if pairing_changed {
            serde_json::to_string(&lines).unwrap_or_else(|_| r.lines_json.clone())
        } else {
            r.lines_json.clone()
        };
        let mut args: Vec<rusqlite::types::Value> = vec![
            rusqlite::types::Value::Text(key),
            rusqlite::types::Value::Text(r.source_file.clone()),
            rusqlite::types::Value::Integer(r.source_page),
            rusqlite::types::Value::Integer(r.pages),
            rusqlite::types::Value::Text(r.doc_no.clone()),
            rusqlite::types::Value::Text(r.serial.clone()),
            rusqlite::types::Value::Text(r.doc_date.clone()),
            rusqlite::types::Value::Text(r.journal_no.clone()),
            rusqlite::types::Value::Text(r.journal.clone()),
            rusqlite::types::Value::Text(r.branch.clone()),
            rusqlite::types::Value::Text(r.doc_source.clone()),
            rusqlite::types::Value::Text(r.contain_currency.clone()),
            rusqlite::types::Value::Text(r.local_amt.clone()),
            rusqlite::types::Value::Text(r.total_value.clone()),
            rusqlite::types::Value::Text(r.invoice_refs.clone()),
            rusqlite::types::Value::Text(r.from_documents.clone()),
            rusqlite::types::Value::Text(lines_json),
            rusqlite::types::Value::Text(r.report_date.clone()),
        ];
        args.extend(derived_params(&d));
        args.push(rusqlite::types::Value::Text(now.clone()));
        tx.execute(
            "INSERT OR REPLACE INTO daily_entries
                (entry_key, source_file, source_page, pages, doc_no, serial, doc_date, journal_no,
                 journal, branch, doc_source, contain_currency, local_amt, total_value,
                 invoice_refs, from_documents, lines_json, report_date,
                 debit_total, credit_total, balanced, line_count, date_key, s_doc, s_account_no, s_account, s_value, s_narration,
                 created_at, updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,
                     ?19,?20,?21,?22,?23,?24,?25,?26,?27,?28,
                     COALESCE((SELECT created_at FROM daily_entries WHERE entry_key=?1), ?29), ?30)",
            rusqlite::params_from_iter(args),
        )
        .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Two documents: 2001 continues onto a second page, 2002 fits on one page.
    const SAMPLE: &str = "\
System     FINANCIAL ACCOUNTS                                  Date : 30-06-2026
Company    China State Eng              Daily Entries           Time : 02:56 PM
Year       2021                                                  Page : 98 : 2

Branch Co3                  New El.Alamen

 Doc. No 2001               1        Doc. Date  10-01-2021   Journal  1   GENERAL LEDGER   With Currency
Contain Cu1   Local Currency          Curr. Rate  1   Local Amt. 308,792.00   Doc. Source   Account
Journal De

   Account No                        Account                Currency     Local Value        Debit               Credit                  Narration                   From Document
     220504           With-holding tax                      Local Curr       1.0                               4,210.80   adt00-20210102001 inv.2/2021 3%wht geo
                                                                                                                          group egypt
     220302           GEO Group Egypt                       Local Curr       1.0          154,396.00                      adt00-20210102001 inv.2/2021
     1209111     Preliminary Soil investigation          Local Curr          1.0          151,834.43                      adt00-20210102001 inv.2/2021
\x0c
 System    FINANCIAL ACCOUNTS                                  Date : 30-06-2026
 Company   China State Eng              Daily Entries            Time : 02:56 PM
 Year      2021                                                   Page : 98 : 3

  Account No                        Account              Currency    Local Value     Debit            Credit             Narration                 From Document
   1226001   Debitors Contract social insurance AL-m  Local Curr    1.0        2,561.57                     adt00-20210102001 inv.2/2021
 Doc. Total Value     Currency        156,957.57        156,957.57     Local Value    156,957.57        156,957.57
 Constraint EdiTor   104              Ahmed Mostafa
\x0c
 System    FINANCIAL ACCOUNTS                                  Date : 30-06-2026
 Company   China State Eng              Daily Entries            Time : 02:56 PM
 Year      2021                                                   Page : 98 : 4

Branch Co3                  New El.Alamen
 Doc. No 2002               1        Doc. Date  10-01-2021   Journal  1   GENERAL LEDGER   With Currency
Contain Cu1   Local Currency          Curr. Rate  1   Local Amt. 1,030.76   Doc. Source   Account

   Account No                        Account              Currency    Local Value     Debit            Credit             Narration                 From Document
    12032052          AL-Alamien Petty Cash               Local Curr       1.0                               1,030.76   adt00-20210101003
    313029           Fuel expense & Gas                  Local Curr       1.0          1,030.76                        adt00-20210101003
 Doc. Total Value     Currency        1,030.76        1,030.76     Local Value    1,030.76        1,030.76
";

    #[test]
    fn parses_entries_and_merges_multipage_docs() {
        let entries = parse_daily_entries(SAMPLE);
        assert_eq!(entries.len(), 2, "expected 2 documents");
        let e2001 = &entries[0];
        assert_eq!(e2001.doc_no, "2001");
        assert_eq!(e2001.serial, "1");
        assert_eq!(e2001.doc_date, "10-01-2021");
        assert_eq!(e2001.branch, "Co3 New El.Alamen");
        assert_eq!(e2001.pages, 2, "2001 spans two pages");
        assert_eq!(e2001.lines.len(), 4);
        // Debit vs Credit columns resolved by position.
        assert_eq!(e2001.lines[0].credit, "4,210.80");
        assert_eq!(e2001.lines[0].debit, "");
        assert_eq!(e2001.lines[1].debit, "154,396.00");
        assert_eq!(e2001.lines[1].credit, "");
        // Wrapped narration continuation is appended.
        assert!(e2001.lines[0].narration.contains("group egypt"));

        let e2002 = &entries[1];
        assert_eq!(e2002.doc_no, "2002");
        assert_eq!(e2002.pages, 1);
        assert_eq!(e2002.lines[1].debit, "1,030.76");
        assert_eq!(e2002.lines[0].credit, "1,030.76");
    }

    #[test]
    fn classifies_debit_and_credit_columns() {
        let entries = parse_daily_entries(SAMPLE);
        let e2001 = &entries[0];
        let d: f64 = e2001.lines.iter().map(|l| num(&l.debit)).sum();
        let c: f64 = e2001.lines.iter().map(|l| num(&l.credit)).sum();
        assert!((d - 308_792.00).abs() < 0.01, "debit column sum: {d}");
        assert!((c - 4_210.80).abs() < 0.01, "credit column sum: {c}");
        let e2002 = &entries[1];
        assert_eq!(num(&e2002.lines[0].credit), 1_030.76);
        assert_eq!(num(&e2002.lines[1].debit), 1_030.76);
    }

    #[test]
    fn real_report_balances() {
        let Ok(path) = std::env::var("VOUCHIFY_ENTRIES_PDF") else {
            eprintln!("skipped: VOUCHIFY_ENTRIES_PDF not set");
            return;
        };
        let text = crate::importer::extract_layout_text(&path).expect("extract layout text");
        let entries = parse_daily_entries(&text);
        assert!(!entries.is_empty(), "no entries parsed");
        let mut checked = 0;
        let mut bad: Vec<String> = Vec::new();
        let mut sum_d = 0.0f64;
        let mut sum_c = 0.0f64;
        let mut sum_pages = 0i64;
        let mut multipage = 0usize;
        for e in &entries {
            assert!(!e.lines.is_empty(), "entry {} has no lines", e.doc_no);
            let d: f64 = e.lines.iter().map(|l| local_amount(&l.debit, &l.local_value)).sum();
            let c: f64 = e.lines.iter().map(|l| local_amount(&l.credit, &l.local_value)).sum();
            sum_d += d;
            sum_c += c;
            sum_pages += e.pages;
            if e.pages > 1 {
                multipage += 1;
            }
            if !is_balanced(d, c) {
                bad.push(format!("{} {}: {:.2} vs {:.2}", e.doc_no, e.doc_date, d, c));
            }
            checked += 1;
        }
        if !bad.is_empty() {
            println!("unbalanced {} of {} (source anomalies): {:?}", bad.len(), checked, &bad[..bad.len().min(8)]);
        }
        // A handful of source vouchers are genuinely unbalanced (e.g. missing an
        // FX adjustment line); fail only if this looks like a parser regression.
        assert!(bad.len() * 200 <= checked, "too many unbalanced entries: {}", bad.len());
        println!(
            "TOTALS docs={} pages={} multipage={} debit={:.2} credit={:.2}",
            entries.len(), sum_pages, multipage, sum_d, sum_c
        );
    }

    #[test]
    fn report_date_precedence() {
        assert!(report_at_least_as_new("02-01-2026", "01-01-2026"));
        assert!(!report_at_least_as_new("01-01-2026", "02-01-2026"));
        assert!(report_at_least_as_new("01-01-2026", ""));
        assert!(report_at_least_as_new("01-01-2026", "01-01-2026"));
    }

    #[test]
    fn dedupes_same_document_across_files() {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        let entries = parse_daily_entries(SAMPLE);
        for e in &entries {
            insert_entry(&conn, "a.pdf", "01-01-2026", e).unwrap();
        }
        for e in &entries {
            insert_entry(&conn, "b.pdf", "02-01-2026", e).unwrap();
        }
        let all = load_all(&conn).unwrap();
        assert_eq!(all.len(), 2, "the same documents must not be duplicated");
        assert!(all.iter().all(|e| e.source_file == "b.pdf"), "newest report should win");
    }

    #[test]
    fn migrate_and_list_real_db() {
        let Ok(path) = std::env::var("VOUCHIFY_ENTRIES_DB") else {
            eprintln!("skipped: VOUCHIFY_ENTRIES_DB not set");
            return;
        };
        let conn = Connection::open(&path).unwrap();
        let t0 = std::time::Instant::now();
        init_schema(&conn).unwrap();
        let migrate = t0.elapsed();
        let t1 = std::time::Instant::now();
        let files = list_files(&conn).unwrap();
        let files_t = t1.elapsed();
        let t2 = std::time::Instant::now();
        let stats = stats_from_db(&conn).unwrap();
        let stats_t = t2.elapsed();
        let t3 = std::time::Instant::now();
        let page = list(&conn, "", "all", "", "", 0, 150).unwrap();
        let list_t = t3.elapsed();
        let t4 = std::time::Instant::now();
        let searched = list(&conn, "220504", "all", "", "", 0, 150).unwrap();
        let search_t = t4.elapsed();
        // Profile the backfill pieces (parse vs compute).
        let mut t_parse = std::time::Duration::ZERO;
        let mut t_comp = std::time::Duration::ZERO;
        {
            let mut stmt = conn.prepare("SELECT source_file, doc_no, serial, doc_date, lines_json FROM daily_entries").unwrap();
            let iter = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?, r.get::<_, String>(4)?))).unwrap();
            for row in iter {
                let (sf, dn, se, dd, lj) = row.unwrap();
                let a = std::time::Instant::now();
                let mut lines: Vec<DailyEntryLine> = serde_json::from_str(&lj).unwrap_or_default();
                t_parse += a.elapsed();
                let b = std::time::Instant::now();
                apply_currency_pairing(&mut lines);
                let _ = compute_derived(&sf, &dn, &se, &dd, &lines);
                t_comp += b.elapsed();
            }
        }
        println!(
            "migrate={:?} files({})={:?} stats({} docs, {:.2} Dr)={:?} list({}/{})={:?} search(220504 -> {})={:?} | parse={:?} compute={:?}",
            migrate, files.len(), files_t, stats.total, stats.debit, stats_t,
            page.entries.len(), page.total, list_t, searched.total, search_t, t_parse, t_comp
        );
    }

    #[test]
    fn parses_invoice_reference_forms() {
        let mut e = ParsedEntry::default();
        for narr in [
            "adt00-20210102001 inv.2/2021 3%wht",
            "ADT0020260302070 - LECTRO - INV NO :1864 + 1865 + 1815 + 1816",
            "ADT0020260201023 - INV NO.2-2026 - Guide Group",
            "INV#7712 KPI",
            "INVOICE NO 590 - Drison",
            "INVOICE NOT RECEIVED - ADT0020260302070",
        ] {
            e.lines.push(DailyEntryLine { narration: narr.to_string(), ..Default::default() });
        }
        let refs = invoice_refs_of(&e);
        for expected in ["2/2021", "1864", "1865", "1815", "1816", "2/2026", "7712", "590"] {
            assert!(refs.contains(&expected.to_string()), "missing {expected}; got {refs:?}");
        }
        // "INVOICE NOT RECEIVED" must not yield a bogus reference.
        assert_eq!(refs.len(), 8, "unexpected refs: {refs:?}");
    }

    #[test]
    fn detects_duplicate_invoice() {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        // The 2002 document also references invoice 2/2021, which 2001 already paid.
        let text = SAMPLE.replace("adt00-20210101003", "adt00-20210101003 inv.2/2021");
        let entries = parse_daily_entries(&text);
        for e in &entries {
            insert_entry(&conn, "file.pdf", "30-06-2026", e).unwrap();
        }
        let all = load_all(&conn).unwrap();
        let paid: Vec<&DailyEntryRecord> = all.iter().filter(|e| e.paid_before).collect();
        assert_eq!(paid.len(), 1, "one later entry should be flagged");
        assert_eq!(paid[0].doc_no, "2002");
        assert_eq!(paid[0].paid_before_in[0].doc_no, "2001");
    }
}
