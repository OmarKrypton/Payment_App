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
use rusqlite::{params, Connection};
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
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        )",
        [],
    )
    .map_err(|e| e.to_string())?;
    conn.execute("CREATE INDEX IF NOT EXISTS idx_daily_source ON daily_entries(source_file)", [])
        .map_err(|e| e.to_string())?;
    conn.execute("CREATE INDEX IF NOT EXISTS idx_daily_doc ON daily_entries(doc_no)", [])
        .map_err(|e| e.to_string())?;
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
    pub updated_at: String,
}

fn entry_key_for(source_file: &str, e: &ParsedEntry) -> String {
    format!("{}|{}|{}|{}", source_file, e.doc_no, e.serial, e.doc_date)
}

fn insert_entry(conn: &Connection, source_file: &str, e: &ParsedEntry) -> Result<(), String> {
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let key = entry_key_for(source_file, e);
    let lines_json = serde_json::to_string(&e.lines).unwrap_or_else(|_| "[]".into());
    let refs = join_refs(&invoice_refs_of(e));
    let docs = join_refs(&from_documents_of(e));
    conn.execute(
        "INSERT OR REPLACE INTO daily_entries
            (entry_key, source_file, source_page, pages, doc_no, serial, doc_date, journal_no, journal,
             branch, doc_source, contain_currency, local_amt, total_value, invoice_refs, from_documents,
             lines_json, created_at, updated_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,
                 COALESCE((SELECT created_at FROM daily_entries WHERE entry_key=?1), ?18), ?18)",
        params![
            key, source_file, e.source_page, e.pages, e.doc_no, e.serial, e.doc_date,
            e.journal_no, e.journal, e.branch, e.doc_source, e.contain_currency,
            e.local_amt, e.total_value, refs, docs, lines_json, now
        ],
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
}

pub fn import_file(conn: &Connection, file_path: &str) -> Result<DailyEntriesImportResult, String> {
    let base = std::path::Path::new(file_path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| file_path.to_string());
    let text = crate::importer::extract_layout_text(file_path)?;
    let parsed = parse_daily_entries(&text);
    if parsed.is_empty() {
        return Err("No daily-entry documents were found in this PDF. Is it a Daily Entries report?".into());
    }

    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    tx.execute("DELETE FROM daily_entries WHERE source_file = ?1", params![base])
        .map_err(|e| e.to_string())?;
    let mut lines = 0usize;
    let mut refs = 0usize;
    for e in &parsed {
        insert_entry(&tx, &base, e)?;
        lines += e.lines.len();
        refs += invoice_refs_of(e).len();
    }
    tx.commit().map_err(|e| e.to_string())?;

    let all = load_all(conn)?;
    let duplicates = all
        .iter()
        .filter(|e| e.source_file == base && e.paid_before)
        .count();
    let pages: i64 = parsed.iter().map(|e| e.pages).sum();

    Ok(DailyEntriesImportResult {
        source_file: base,
        entries: parsed.len(),
        lines,
        pages,
        multipage: parsed.iter().filter(|e| e.pages > 1).count(),
        invoice_refs: refs,
        duplicates,
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
            let lines: Vec<DailyEntryLine> = serde_json::from_str(&lines_json).unwrap_or_default();
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

/// Convert a report `DD-MM-YYYY` date to `YYYY-MM-DD` for range comparisons.
fn ymd(d: &str) -> Option<String> {
    let parts: Vec<&str> = d.split('-').collect();
    if parts.len() == 3 && parts[0].len() == 2 && parts[1].len() == 2 && parts[2].len() == 4 {
        Some(format!("{}-{}-{}", parts[2], parts[1], parts[0]))
    } else {
        None
    }
}

fn matches_date_range(doc_date: &str, from: &str, to: &str) -> bool {
    if from.is_empty() && to.is_empty() {
        return true;
    }
    match ymd(doc_date) {
        Some(d) => (from.is_empty() || d.as_str() >= from) && (to.is_empty() || d.as_str() <= to),
        // Unparseable dates are kept rather than silently dropped.
        None => true,
    }
}

fn stats_of(entries: &[DailyEntryRecord]) -> DailyEntriesStats {
    let mut st = DailyEntriesStats { total: entries.len(), ..Default::default() };
    let mut files: Vec<&str> = Vec::new();
    for e in entries {
        st.pages += e.pages;
        if e.pages > 1 {
            st.multipage += 1;
        }
        if e.paid_before {
            st.paid_before += 1;
        }
        st.lines += e.lines.len();
        if !files.contains(&e.source_file.as_str()) {
            files.push(&e.source_file);
        }
        for l in &e.lines {
            st.debit += local_amount(&l.debit, &l.local_value);
            st.credit += local_amount(&l.credit, &l.local_value);
        }
    }
    st.files = files.len();
    st
}

fn matches_search(e: &DailyEntryRecord, q: &str, search_in: &str) -> bool {
    if q.is_empty() {
        return true;
    }
    let ql = q.to_lowercase();
    let qdigits: String = ql.chars().filter(|c| c.is_ascii_digit()).collect();
    let doc_hit = || {
        e.doc_no.to_lowercase().contains(&ql)
            || e.serial.to_lowercase().contains(&ql)
            || e.source_file.to_lowercase().contains(&ql)
            || e.doc_date.contains(&ql)
    };
    let account_no_hit = || e.lines.iter().any(|l| l.account_no.to_lowercase().contains(&ql));
    let account_hit = || e.lines.iter().any(|l| l.account.to_lowercase().contains(&ql));
    let value_hit = || {
        e.lines.iter().any(|l| {
            l.debit.to_lowercase().contains(&ql)
                || l.credit.to_lowercase().contains(&ql)
                || (!qdigits.is_empty()
                    && (l.debit.replace(',', "").contains(&qdigits)
                        || l.credit.replace(',', "").contains(&qdigits)))
        })
    };
    let narration_hit = || {
        e.lines.iter().any(|l| l.narration.to_lowercase().contains(&ql))
            || e.from_documents.iter().any(|d| d.to_lowercase().contains(&ql))
            || e.invoice_refs.iter().any(|d| d.to_lowercase().contains(&ql))
    };
    match search_in {
        "doc" => doc_hit(),
        "account_no" => account_no_hit(),
        "account" => account_hit(),
        "value" => value_hit(),
        "narration" => narration_hit(),
        _ => doc_hit() || account_no_hit() || account_hit() || value_hit() || narration_hit(),
    }
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
    let all = load_all(conn)?;
    let stats = stats_of(&all);
    let filtered: Vec<DailyEntryRecord> = all
        .into_iter()
        .filter(|e| matches_search(e, search.trim(), search_in))
        .filter(|e| matches_date_range(&e.doc_date, date_from, date_to))
        .collect();
    let total = filtered.len();
    let entries: Vec<DailyEntryRecord> = if limit == 0 {
        filtered
    } else {
        filtered.into_iter().skip(offset).take(limit).collect()
    };
    Ok(DailyEntriesList { entries, total, stats })
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
    let all = load_all(conn)?;
    let mut map: std::collections::BTreeMap<String, DailyEntryFileInfo> = std::collections::BTreeMap::new();
    for e in &all {
        let f = map.entry(e.source_file.clone()).or_insert_with(|| DailyEntryFileInfo {
            source_file: e.source_file.clone(),
            documents: 0,
            pages: 0,
            debit: 0.0,
            credit: 0.0,
            balanced: true,
        });
        f.documents += 1;
        f.pages += e.pages;
        f.debit += e.debit_total;
        f.credit += e.credit_total;
    }
    for f in map.values_mut() {
        f.balanced = is_balanced(f.debit, f.credit);
    }
    Ok(map.into_values().collect())
}

pub fn for_sync(conn: &Connection) -> Result<Vec<DailyEntrySyncRow>, String> {    let mut stmt = conn
        .prepare(
            "SELECT entry_key, source_file, source_page, pages, doc_no, serial, doc_date,
                    journal_no, journal, branch, doc_source, contain_currency, local_amt,
                    total_value, invoice_refs, from_documents, lines_json, updated_at
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
                updated_at: r.get(17)?,
            })
        })
        .map_err(|e| e.to_string())?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn sync_from_remote(conn: &Connection, rows: &[DailyEntrySyncRow]) -> Result<(), String> {
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    for r in rows {
        tx.execute(
            "INSERT OR REPLACE INTO daily_entries
                (entry_key, source_file, source_page, pages, doc_no, serial, doc_date, journal_no,
                 journal, branch, doc_source, contain_currency, local_amt, total_value,
                 invoice_refs, from_documents, lines_json, created_at, updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,
                     COALESCE((SELECT created_at FROM daily_entries WHERE entry_key=?1), ?18), ?18)",
            params![
                r.entry_key, r.source_file, r.source_page, r.pages, r.doc_no, r.serial, r.doc_date,
                r.journal_no, r.journal, r.branch, r.doc_source, r.contain_currency, r.local_amt,
                r.total_value, r.invoice_refs, r.from_documents, r.lines_json, now
            ],
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
            insert_entry(&conn, "file.pdf", e).unwrap();
        }
        let all = load_all(&conn).unwrap();
        let paid: Vec<&DailyEntryRecord> = all.iter().filter(|e| e.paid_before).collect();
        assert_eq!(paid.len(), 1, "one later entry should be flagged");
        assert_eq!(paid[0].doc_no, "2002");
        assert_eq!(paid[0].paid_before_in[0].doc_no, "2001");
    }
}
