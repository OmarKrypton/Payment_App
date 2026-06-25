use std::process::Command;
use regex::Regex;
use std::collections::HashMap;
use std::path::Path;

use crate::models::{FormData, InvoiceData, OcrFieldInfo};

pub type ProgressFn = Box<dyn Fn(&str, &str) + Send>;

/// Remove spaces between CJK characters introduced by tesseract output,
/// and clean up obvious OCR spacing artifacts.
fn normalize_ocr(text: &str) -> String {
    let s: String = text.chars().filter(|c| *c != '\r').collect();
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == ' ' && i + 1 < chars.len() {
            let prev = if i > 0 { chars[i - 1] } else { ' ' };
            let next = chars[i + 1];
            let is_cjk = |ch: char| -> bool {
                let cp = ch as u32;
                (cp >= 0x4E00 && cp <= 0x9FFF) || (cp >= 0x3400 && cp <= 0x4DBF)
            };
            if is_cjk(prev) && is_cjk(next) { i += 1; continue; }
            if is_cjk(prev) && next.is_ascii_punctuation() { i += 1; continue; }
            if prev.is_ascii_punctuation() && is_cjk(next) { i += 1; continue; }
            if next == ':' || next == '：' { i += 1; continue; }
            if prev == ':' || prev == '：' { i += 1; continue; }
        }
        out.push(c);
        i += 1;
    }
    out
}

fn extract_val(text: &str, patterns: &[&str]) -> String {
    for pat in patterns {
        if let Some(caps) = Regex::new(pat).unwrap().captures(text) {
            let raw = caps.get(1).unwrap().as_str().trim();
            let cleaned: String = raw.chars().filter(|c| c.is_ascii_digit() || *c == '.' || *c == '-' || *c == ',').collect();
            let numeric: String = cleaned.chars().filter(|c| *c != ',').collect();
            if !numeric.is_empty() && numeric != "." && numeric != "-" {
                return numeric;
            }
        }
    }
    String::new()
}

fn extract_rate(text: &str, patterns: &[&str]) -> String {
    for pat in patterns {
        if let Some(caps) = Regex::new(pat).unwrap().captures(text) {
            let raw = caps.get(1).unwrap().as_str().trim();
            if raw.contains('%') { return raw.to_string(); }
            let numeric: String = raw.chars().filter(|c| c.is_ascii_digit() || *c == '.').collect();
            if !numeric.is_empty() { return format!("{}%", numeric); }
        }
    }
    String::new()
}

fn extract_id(text: &str, patterns: &[&str]) -> String {
    for pat in patterns {
        if let Some(caps) = Regex::new(pat).unwrap().captures(text) {
            return caps.get(1).unwrap().as_str().trim().to_string();
        }
    }
    String::new()
}

/// Clean an Egyptian-format number: commas may be decimal separators.
/// e.g. "1,145,393,00" → "1145393.00", "900,000,00" → "900000.00"
fn clean_eg_number(raw: &str) -> String {
    let cleaned: String = raw.chars().filter(|c| c.is_ascii_digit() || *c == '.' || *c == '-' || *c == ',').collect();
    if cleaned.is_empty() || cleaned == "." || cleaned == "-" { return String::new(); }
    // If no regular dot and comma appears as last separator with 2-digit fraction, treat as decimal
    if !cleaned.contains('.') && cleaned.contains(',') {
        if let Some(pos) = cleaned.rfind(',') {
            let after = &cleaned[pos+1..];
            if after.len() == 2 && after.chars().all(|c| c.is_ascii_digit()) {
                let int_part = cleaned[..pos].replace(",", "");
                return format!("{}.{}", int_part, after);
            }
        }
    }
    cleaned.replace(",", "")
}

/// Extract a value using a field-code regex pattern, using Egyptian number cleaning.
fn extract_fc(text: &str, pattern: &str) -> String {
    if let Some(caps) = Regex::new(pattern).unwrap().captures(text) {
        return clean_eg_number(caps.get(1).unwrap().as_str().trim());
    }
    String::new()
}

pub fn parse_text(text: &str) -> FormData {
    let t = &normalize_ocr(text);
    let mut data = FormData::default();

    // Document serial
    data.doc_serial = extract_id(t, &[
        r"(?i)表\s*格\s*编\s*号.*?S\.N[)\]]?\s*\n\s*([A-Za-z0-9_\-/]+)",
        r"(?i)表\s*格\s*编\s*号[^:：\n]*[:：]?\s*([A-Za-z0-9_\-]{4,})",
        r"(?i)(?:S\.N|SN|s\.n)\s*[:：]?\s*([A-Za-z0-9_\-/]{4,})",
        r"(?i)(?:Ref\.?\s*NO|Ref\.?\s*N\.?O)[.:：]*\s*([A-Za-z0-9_\-/]+)",
        r"(?i)(ADT[_-][A-Za-z0-9_\-/]+)",
        r"(?i)(CSCEC[/-][A-Za-z0-9_\-/]+)",
    ]);

    // Helper: try field-code first, fall back to label patterns
    macro_rules! try_fc {
        ($pat:expr) => { extract_fc(t, $pat) };
    }
    macro_rules! try_label {
        ($($p:expr),+) => { extract_val(t, &[$($p),+]) };
    }
    macro_rules! set_field {
        ($field:ident, $fc:expr, $($label:expr),+) => {
            data.$field = try_fc!($fc);
            if data.$field.is_empty() {
                data.$field = try_label!($($label),+);
            }
        };
    }

    // Card 1 – VAT base
    set_field!(val_1A,
        r"(?im)^\s*1[4A][.,;:\s].*?([\d,.\s]+)\s*$",
        r"(?i)期初累计[结算款额][^：:\n]*?([\d,.\-]+)",
        r"(?i)累计结算[额額][^：:\n]*?([\d,.\-]+)",
        r"(?i)val_1A[^:]*[:：\s]*([\d,.\-]+)"
    );

    set_field!(val_1B,
        r"(?im)^\s*1[8B][.,;:\s].*?([\d,.\s]+)\s*$",
        r"(?i)本期结算[金额额题][^：:\n]*?([\d,.\-]+)",
        r"(?i)本期结算[^：:\n]*?[:：]\s*([\d,.\-]+)",
        r"(?i)val_1B[^:]*[:：\s]*([\d,.\-]+)"
    );

    set_field!(val_1C,
        r"(?im)\b1C[.,;:\s].*?([\d,.\s]+)\s*$",
        r"(?i)本期其他增项[^：:\n]*?([\d,.\-]+)",
        r"(?i)other additions[^:]*[:：\s]*([\d,.\-]+)"
    );

    set_field!(val_1D,
        r"(?im)^\s*1D[.,;:\s].*?([\d,.\s]+)\s*$",
        r"(?i)本期扣除额[^：:\n]*?([\d,.\-]+)",
        r"(?i)current deductions[^:]*[:：\s]*([\d,.\-]+)"
    );

    data.vat_rate = extract_rate(t, &[
        r"(?i)增值税率[：:\s]*([\d.]+)\s*%",
        r"(?i)vat[_\s]*rate[^:]*[:：\s]*([\d.]+)\s*%",
        r"(?i)vat\s*[:：]\s*([\d.]+)\s*%",
        r"(?i)(?:value[- ]added|vat)\D{0,20}?([\d.]+)\s*%",
    ]);

    // Card 2 – Advance payment
    set_field!(val_2A,
        r"(?im)^\s*2[4A][.,;:\s].*?([\d,.\s]+)\s*$",
        r"(?i)预付款总额[^：:\n]*?([\d,.\-]+)",
        r"(?i)total advance[^:]*[:：\s]*([\d,.\-]+)"
    );

    set_field!(val_2B,
        r"(?im)^\s*2B[.,;:\s].*?([\d,.\s]+)\s*$",
        r"(?i)期初预付款扣除[^：:\n]*?([\d,.\-]+)",
        r"(?i)initial[_\s]*advance[_\s]*deduct[^:]*[:：\s]*([\d,.\-]+)"
    );

    set_field!(val_2C,
        r"(?im)\b2C[.,;:\s].*?([\d,.\s]+)\s*$",
        r"(?i)本期预付款扣除[^：:\n]*?([\d,.\-]+)",
        r"(?i)current[_\s]*advance[_\s]*deduct[^:]*[:：\s]*([\d,.\-]+)"
    );

    // Cross-check: if 2B is non-zero but ending balance (2D) is 0, OCR likely misread
    let ending_bal = try_fc!(r"(?im)^\s*2[0D][.,;:\s].*?([\d,.\s]+)\s*$");
    if !data.val_2B.is_empty() && data.val_2B != "0.00"
        && data.val_2C == "0.00" && ending_bal == "0.00"
    {
        data.val_2B = "0.00".into();
    }

    // Card 4 – Retention
    data.ret_rate = extract_rate(t, &[
        r"(?i)保留金率[：:\s]*([\d.]+%)",
        r"(?i)retention[_\s]*rate[^:]*[:：\s]*([\d.]+%)",
    ]);

    set_field!(val_4A,
        r"(?im)^\s*4[Ah][.,;:\s].*?([\d,.\s]+)\s*$",
        r"(?i)期初保留金[^：:\n]*?([\d,.\-]+)",
        r"(?i)initial[_\s]*retention[^:]*[:：\s]*([\d,.\-]+)"
    );

    set_field!(val_4B,
        r"(?im)\b4[8B][.,;:\s].*?([\d,.\s]+)\s*$",
        r"(?i)(?:本期应扣|保留金扣款)[^：:\n]*?([\d,.\-]+)",
        r"(?i)retention[_\s]*deduct[^:]*[:：\s]*([\d,.\-]+)"
    );

    set_field!(val_4C,
        r"(?im)^\s*4C[.,;:\s].*?([\d,.\s]+)\s*$",
        r"(?i)(?:本期返还|保留金返还)[^：:\n]*?([\d,.\-]+)",
        r"(?i)retention[_\s]*return[^:]*[:：\s]*([\d,.\-]+)"
    );

    // Card 5 – Temp labour
    data.temp_rate = extract_rate(t, &[
        r"(?i)临时工[社保率比例][^：:\n]*?([\d.]+)\s*%",
        r"(?i)temp[_\s]*labou?r[_\s]*rate[^:]*[:：\s]*([\d.]+)\s*%",
        r"(?i)temp\s*[:：]\s*([\d.]+)\s*%",
        r"(?i)temporary[_\s]*labou?r\D{0,20}?([\d.]+)\s*%",
    ]);

    set_field!(val_5A,
        r"(?im)^\s*[5S][4A][.,;:\s].*?([\d,.\s]+)\s*$",
        r"(?i)临时工[期初余额期初][^：:\n]*?([\d,.\-]+)",
        r"(?i)temp[_\s]*initial[_\s]*balance[^:]*[:：\s]*([\d,.\-]+)"
    );

    set_field!(val_5C,
        r"(?im)^\s*5C[.,;:\s].*?([\d,.\s]+)\s*$",
        r"(?i)临时工[本期返还返还][^：:\n]*?([\d,.\-]+)",
        r"(?i)temp[_\s]*current[_\s]*return[^:]*[:：\s]*([\d,.\-]+)"
    );

    // Card 6 – WHT
    data.wht_rate = extract_rate(t, &[
        r"(?i)预提税率[：:\s]*([\d.]+%)",
        r"(?i)wht[_\s]*rate[^:]*[:：\s]*([\d.]+%)",
    ]);

    set_field!(val_6A,
        r"(?im)\b6[Ah,][.,;:\s].*?([\d,.\s]+)\s*$",
        r"(?i)期初累计预提税[^：:\n]*?([\d,.\-]+)",
        r"(?i)initial[_\s]*accum[_\s]*wht[^:]*[:：\s]*([\d,.\-]+)",
        r"(?i)预提税[^：:\n]*?([\d,.\-]+)"
    );

    // Card 7 – Cumulative paid
    set_field!(val_7A,
        r"(?im)\b7[4A]\^?[.,;:\s].*?([\d,.\s]+)\s*$",
        r"(?i)期初累计实付[^：:\n]*?([\d,.\-]+)",
        r"(?i)initial[_\s]*accum[_\s]*paid[^:]*[:：\s]*([\d,.\-]+)",
        r"(?i)累计[已广]付[^：:\n]*?([\d,.\-]+)",
        r"(?i)累计已付款[^：:\n]*?([\d,.\-]+)",
        r"(?i)已付款[^：:\n]*?([\d,.\-]+)",
        r"(?i)val_7A[^:]*[:：\s]*([\d,.\-]+)"
    );

    // Card 8 – Other deductions
    data.oth_rate = extract_rate(t, &[
        r"(?i)其他扣费率[：:\s]*([\d.]+%)",
        r"(?i)other[_\s]*rate[^:]*[:：\s]*([\d.]+%)",
    ]);

    set_field!(val_8A,
        r"(?im)\b8[Ah,][.,;:\s].*?([\d,.\s]+)\s*$",
        r"(?i)期初其他扣款[^：:\n]*?([\d,.\-]+)",
        r"(?i)initial[_\s]*other[^:]*[:：\s]*([\d,.\-]+)"
    );

    // Card 12 – Social insurance
    data.soc_rate = extract_rate(t, &[
        r"(?i)社保比例[：:\s]*([\d.]+%)",
        r"(?i)social[_\s]*rate[^:]*[:：\s]*([\d.]+%)",
    ]);
    data.val_12A = extract_val(t, &[
        r"(?i)期初累计社保[^：:\n]*?([\d,.\-]+)",
        r"(?i)initial[_\s]*social[^:]*[:：\s]*([\d,.\-]+)",
    ]);

    // Card 10 – Current period paid
    set_field!(val_10A,
        r"(?im)\b10[Ah,][.,;:\s].*?([\d,.\s]+)\s*$",
        r"(?i)本期[实付应付][^：:\n]*?([\d,.\-]+)",
        r"(?i)current[_\s]*paid[^:]*[:：\s]*([\d,.\-]+)",
        r"(?i)本期应付[^：:\n]*?([\d,.\-]+)",
        r"(?i)小写[^：:\n]*?([\d,.\-]+)",
        r"(?i)val_10A[^:]*[:：\s]*([\d,.\-]+)",
        r"(?i)Invoice[_\s]*Amount[^:]*[:：\s]*([\d,.\-]+)",
        r"(?i)Invoice\s+Amount[^：:\n]*?([\d,.\-]+)"
    );

    // Net payable fallback for val_10A
    if data.val_10A.is_empty() {
        let net = extract_val(t, &[
            r"(?i)本次[批准批准扯准批准][^金]*?[金额额题][^：:\n]*?([\d,.\-]+)",
            r"(?i)net[_\s]*amount[_\s]*payable[^：:\n]*?([\d,.\-]+)",
            r"(?i)净应付[^：:\n]*?([\d,.\-]+)",
            r"(?i)approve[_\s]*amount[^：:\n]*?([\d,.\-]+)",
        ]);
        if !net.is_empty() && net != "0.00" { data.val_10A = net; }
    }

    // Budget amount → val_2A fallback
    if data.val_2A.is_empty() {
        let budget = extract_val(t, &[
            r"(?i)资金预算[金额金颜][^：:\n]*?([\d,.\-]+)",
            r"(?i)budget[_\s]*amount[^:]*[:：\s]*([\d,.\-]+)",
        ]);
        if !budget.is_empty() && budget != "0.00" { data.val_2A = budget; }
    }

    // Helper: extract spaced digits (OCR often inserts spaces: 1 0 0 4 8 9 0 9 5)
    let extract_tax_val = |text: &str, patterns: &[&str]| -> String {
        for pat in patterns {
            if let Some(caps) = Regex::new(pat).unwrap().captures(text) {
                let raw = caps.get(1).unwrap().as_str().trim();
                let cleaned: String = raw.chars().filter(|c| c.is_ascii_digit() || *c == '-' || *c == 'X' || *c == 'x').collect();
                if !cleaned.is_empty() && cleaned != "-" {
                    return cleaned;
                }
            }
        }
        String::new()
    };

    // Normalize text: remove spaces between digits (pdftotext may produce "1 00-489-095")
    // char-by-char to avoid lookbehind (not supported by Rust regex)
    let norm_text: String = {
        let mut result = String::with_capacity(text.len());
        let chars: Vec<char> = text.chars().collect();
        for i in 0..chars.len() {
            let c = chars[i];
            if c.is_ascii_whitespace() && i > 0 && i + 1 < chars.len()
                && chars[i - 1].is_ascii_digit() && chars[i + 1].is_ascii_digit()
            {
                continue;
            }
            result.push(c);
        }
        result
    };

    // Tax IDs — buyer is always CSCEC
    data.buyer_tax_id = "100489095".to_string();
    let detected_buyer = extract_tax_val(&norm_text, &[
        r"(?i)买方税号[：:\s]*([\w\-]+)",
        r"(?i)buyer[_\s]*tax[_\s]*id[^:]*[:：\s]*([\w\-]+)",
        r"(?i)购[买方][单]?位[^：:\n]{0,20}?(?:税|纳税人识别号)[：:\s]*([\w\-]+)",
        r"(?i)(?:纳税人识别号|税务登记号)[^：:\n]{0,10}?[:：]\s*([\w\-]+)",
    ]);
    // Flag if detected buyer tax ID differs from expected
    if !detected_buyer.is_empty() && detected_buyer != "100489095" {
        data.buyer_tax_id = detected_buyer;
    }
    data.seller_tax_id = extract_tax_val(&norm_text, &[
        r"(?i)卖方税号[：:\s]*([\w\-]+)",
        r"(?i)seller[_\s]*tax[_\s]*id[^:]*[:：\s]*([\w\-]+)",
        r"(?i)销[售货][单]?位[^：:\n]{0,20}?(?:税|纳税人识别号)[：:\s]*([\w\-]+)",
        r"(?i)供应[商][^：:\n]{0,20}?(?:税|纳税人识别号)[：:\s]*([\w\-]+)",
        r"(?i)(?:Registration|Registrat|Registration)[^:]*[:：\s]*#?\s*([\w\-]+)",
        r"(?i)tax[_\s]*(?:card|number|no)[^:]*[:：\s]*([\w\-]+)",
        r"(?i)统一社会信用代码[：:\s]*([\w\-]+)",
        // Fallback: any tax-related label followed by a number
        r"(?i)(?:Tax|TAX|税|Reg|CR|Commercial)[^：:\n]*?[:：]\s*([\w\-]+)",
    ]);
    // Additional patterns specifically for seller tax ID when not found yet
    if data.seller_tax_id.is_empty() {
        data.seller_tax_id = extract_tax_val(&norm_text, &[
            // English invoice pages: "RegIstratIon Number : #205174078"
            r"(?i)(?:registration|registrat)[^:]*[:：]\s*#?\s*([\w\-]+)",
            // "Tax card number: 205-174-078"
            r"(?i)tax\s+card\s+number[^:]*[:：]\s*([\w\-]+)",
            // "VAT No" or "Tax No" or "Tax ID"
            r"(?i)(?:vat|tax)\s*(?:no|number|id|card)[^:]*[:：]\s*([\w\-]+)",
            // Generic "Taxpayer Name" followed by registration number nearby
            r"(?i)taxpay(?:er|8r)\s+name[^:]*[:：]\s*\S+\s*\n\s*(?:registration|registrat)[^:]*[:：]\s*#?\s*([\w\-]+)",
        ]);
    }
    // Ensure buyer tax ID is the fixed expected value (override any OCR detection)
    if data.buyer_tax_id != "100489095" {
        data.buyer_tax_id = "100489095".to_string();
    }
    // Extract multiple seller tax IDs from invoice lines (only on original text for Chinese patterns)
    {
        let re = Regex::new(r"(?im)^.*?[销出]售[单]?方[^：:\n]{0,30}?(\d[\d\s\-]{8,17}).*$").unwrap();
        for caps in re.captures_iter(text) {
            let tid = caps.get(1).map(|m| m.as_str().trim()).unwrap_or("").to_string();
            if !tid.is_empty() && tid != "100489095" && !data.seller_tax_ids.contains(&tid) {
                data.seller_tax_ids.push(tid);
            }
        }
    }
    // Always include the main seller_tax_id in the list
    if !data.seller_tax_id.is_empty() && !data.seller_tax_ids.contains(&data.seller_tax_id) {
        data.seller_tax_ids.push(data.seller_tax_id.clone());
    }

    // Invoices from WHT certificate or invoice sections
    if let Ok(re) = Regex::new(r"(?i)Invoice[_\s]*(?:Number|No)[^:]*[:：\s]*([A-Za-z0-9_\-/]+)\s*Invoice[_\s]*Amount[^:]*[:：\s]*([\d,.\-]+)") {
        if let Some(caps) = re.captures(t) {
            let inv_no = caps.get(1).map(|m| m.as_str().trim()).unwrap_or("").to_string();
            let amount = caps.get(2).map(|m| m.as_str().replace(",", "")).unwrap_or("0.00".to_string());
            if !inv_no.is_empty() {
                data.invoices.push(InvoiceData { invoice_no: inv_no, seller_tax_id: String::new(), amount });
            }
        }
    }

    // Chinese invoice pattern: 发票号,RKL/...,发票金额 XX
    if data.invoices.is_empty() {
        if let Ok(re) = Regex::new(r"(?i)发票号[^，,\n]{0,20}?[，,]\s*([A-Za-z0-9_\-/]+).{0,40}?发票金额[：:\s]*([\d,.\-]+)") {
            if let Some(caps) = re.captures(t) {
                let inv_no = caps.get(1).map(|m| m.as_str().trim()).unwrap_or("").to_string();
                let amount = caps.get(2).map(|m| m.as_str().replace(",", "")).unwrap_or("0.00".to_string());
                if !inv_no.is_empty() && inv_no.len() > 3 {
                    data.invoices.push(InvoiceData { invoice_no: inv_no, seller_tax_id: String::new(), amount });
                }
            }
        }
    }

    if data.invoices.is_empty() {
        if let Ok(re) = Regex::new(r"(?i)(?:invoice|发票)[^\n]{0,60}?([A-Za-z0-9_\-/]{4,})") {
            let mut inv_count = 0;
            for caps in re.captures_iter(t) {
                if inv_count >= 5 { break; }
                let raw = caps.get(1).map(|m| m.as_str().trim()).unwrap_or("").to_string();
                // Skip label words (pure alpha) that the lazy regex captures instead of the value
                if raw.chars().all(|c| c.is_ascii_alphabetic()) { continue; }
                // Skip all-digit strings > 3 chars (likely an amount)
                if raw.chars().all(|c| c.is_ascii_digit()) && raw.len() > 3 { continue; }
                if !raw.is_empty() {
                    data.invoices.push(InvoiceData { invoice_no: raw, seller_tax_id: String::new(), amount: String::new() });
                    inv_count += 1;
                }
            }
        }
    }

    data
}

// ── Multi-pass OCR voting ─────────────────────────────────────

/// Run tesseract with a specific PSM mode and language.
fn ocr_page(image_path: &str, lang: &str, psm: &str) -> String {
    if let Ok(output) = create_command("tesseract")
        .args(&[image_path, "stdout", "-l", lang, "--psm", psm])
        .output()
    {
        if output.status.success() {
            let text = String::from_utf8_lossy(&output.stdout);
            let trimmed = text.trim();
            if trimmed.len() > 30 {
                return trimmed.to_string();
            }
        }
    }
    String::new()
}

/// Extract a value from each pass text, vote on most common result.
/// Returns (best_value, confidence_0to1, all_values_found).
/// Confidence is based on agreement ratio among passes that matched the field,
/// with a coverage penalty: conf = (agreement_ratio * 0.8 + coverage_ratio * 0.2).
fn vote_field(pass_texts: &[String], pattern: &str) -> (String, f64, Vec<String>) {
    let mut values: Vec<String> = Vec::new();
    for text in pass_texts {
        let v = extract_fc(text, pattern);
        if !v.is_empty() {
            values.push(v);
        }
    }
    if values.is_empty() {
        return (String::new(), 0.0, vec![]);
    }
    let total = pass_texts.len() as f64;
    let total_found = values.len() as f64;
    let mut freq: HashMap<String, u32> = HashMap::new();
    for v in &values {
        *freq.entry(v.clone()).or_insert(0) += 1;
    }
    let best = freq.iter().max_by_key(|e| e.1).unwrap();
    let agreement = *best.1 as f64 / total_found;
    let coverage = total_found / total;
    let confidence = agreement * 0.8 + coverage * 0.2;
    (best.0.clone(), confidence, values)
}

fn parse_f64(s: &str) -> f64 { s.parse().unwrap_or(0.0) }

/// If `ratio` matches a percentage in `known` (within tolerance), set `field` to that percentage.
/// Returns true if a known match was found and set.
fn match_ratio(ratio: f64, known: &[&str], field: &mut String) -> bool {
    if ratio <= 0.0 { return false; }
    let mut best_dist = f64::MAX;
    let mut best = known[0];
    for known_str in known {
        let pct: f64 = known_str.trim_end_matches('%').parse().unwrap_or(0.0);
        let expected = pct / 100.0;
        let dist = (ratio - expected).abs();
        if dist < 0.0003 {
            *field = known_str.to_string();
            return true;
        }
        if dist < best_dist {
            best_dist = dist;
            best = known_str;
        }
    }
    // Use closest known if within 10% of its value
    let best_pct: f64 = best.trim_end_matches('%').parse().unwrap_or(0.0);
    let best_expected = best_pct / 100.0;
    if best_expected > 0.0 && best_dist / best_expected < 0.1 {
        *field = best.to_string();
        return true;
    }
    false
}

/// Cross-validate using the form's own printed formulas.
/// `extras` contains formula-result fields extracted from OCR.
/// Corrects `data` fields in-place using formula relationships.
fn cross_validate(data: &mut FormData, extras: &HashMap<String, String>) {

    // 2D = 2B + 2C: if ending balance is 0 and 2C=0 but 2B≠0, correct 2B
    if let Some(v2d) = extras.get("val_2D").and_then(|v| v.parse::<f64>().ok()) {
        let v2b = parse_f64(&data.val_2B);
        let v2c = parse_f64(&data.val_2C);
        if v2d == 0.0 && v2c == 0.0 && v2b != 0.0 {
            data.val_2B = "0.00".into();
        }
    }

    // 4D = 4A + 4B - 4C
    if let Some(v4d) = extras.get("val_4D").and_then(|v| v.parse::<f64>().ok()) {
        let v4b = parse_f64(&data.val_4B);
        let v4a = parse_f64(&data.val_4A);
        let v4c = parse_f64(&data.val_4C);
        if v4d == 0.0 && v4a == 0.0 && v4c == 0.0 && v4b != 0.0 {
            data.val_4B = "0.00".into();
        }
    }

    // 5D = 5A + 5B - 5C
    if let Some(v5d) = extras.get("val_5D").and_then(|v| v.parse::<f64>().ok()) {
        let v5b = parse_f64(&data.val_5B);
        let v5a = parse_f64(&data.val_5A);
        let v5c = parse_f64(&data.val_5C);
        if v5d >= 0.0 && v5a == 0.0 && v5c == 0.0 && (v5b - v5d).abs() > 0.01 {
            data.val_5B = format!("{:.2}", v5d);
        }
    }

    // 6C = 6A + 6B: if 6A=0 and 6C extracted, use 6C as 6B
    if let Some(v6c) = extras.get("val_6C").and_then(|v| v.parse::<f64>().ok()) {
        let v6b = parse_f64(&data.val_6B);
        let v6a = parse_f64(&data.val_6A);
        if v6c > 0.0 && v6a == 0.0 && (v6b - v6c).abs() > 0.01 {
            data.val_6B = format!("{:.2}", v6c);
        }
    }

    // 11A = 7A + 10A + 2A
    if let Some(v11a) = extras.get("val_11A").and_then(|v| v.parse::<f64>().ok()) {
        let v7a = parse_f64(&data.val_7A);
        let v10a = parse_f64(&data.val_10A);
        let v2a = parse_f64(&data.val_2A);
        let expected = v7a + v10a + v2a;
        if v11a > 0.0 && expected > 0.0 && (v11a - expected).abs() > 0.01 {
            data.val_11A = format!("{:.2}", expected);
        }
    }

    // 11B = 11A + 6C
    if let Some(v11b) = extras.get("val_11B").and_then(|v| v.parse::<f64>().ok()) {
        let v11a = parse_f64(&data.val_11A);
        let v6c = parse_f64(&data.val_6C);
        let expected = v11a + v6c;
        if v11b > 0.0 && expected > 0.0 && (v11b - expected).abs() > 0.01 {
            data.val_11B = format!("{:.2}", expected);
        }
    }

    // ── Rate inference from deduction/base ratios ─────
    // Determine the base settlement amount (current period).
    let mut v1b = parse_f64(&data.val_1B);
    // If val_1B wasn't extracted, try ending accumulative supplier settlement
    if v1b <= 0.0 { v1b = parse_f64(&data.val_2B); }
    // If still 0, back-calculate from temp labour deduction (val_5B / 0.45%)
    if v1b <= 0.0 {
        let v5b = parse_f64(&data.val_5B);
        if v5b > 0.0 { v1b = (v5b / 0.0045 * 100.0).round() / 100.0; }
    }
    if v1b <= 0.0 { return; }

    fn fmt_pct(ratio: f64) -> String {
        format!("{:.4}%", ratio * 100.0)
    }

    if data.temp_rate == "0%" || data.temp_rate.is_empty() {
        let v = parse_f64(&data.val_5B);
        if v > 0.0 && !match_ratio(v / v1b, &["0.45%", "0%"], &mut data.temp_rate) {
            data.temp_rate = fmt_pct(v / v1b);
        }
    }

    if data.wht_rate == "0%" || data.wht_rate.is_empty() {
        let v = parse_f64(&data.val_6B);
        if v > 0.0 && !match_ratio(v / v1b, &["1%", "3%", "5%", "0%"], &mut data.wht_rate) {
            data.wht_rate = fmt_pct(v / v1b);
        }
    }

    if data.vat_rate == "14%" || data.vat_rate.is_empty() {
        let v = extras.get("val_1E").map(|s| parse_f64(s)).unwrap_or(0.0);
        if v > 0.0 && !match_ratio(v / v1b, &["5%", "9%", "10%", "14%", "0%"], &mut data.vat_rate) {
            data.vat_rate = fmt_pct(v / v1b);
        }
    }

    if data.oth_rate == "0%" || data.oth_rate.is_empty() {
        let v = extras.get("val_8B").map(|s| parse_f64(s)).unwrap_or(0.0);
        if v > 0.0 && !match_ratio(v / v1b, &["0.15%", "0.3%", "0%"], &mut data.oth_rate) {
            data.oth_rate = fmt_pct(v / v1b);
        }
    }

    if data.soc_rate == "0%" || data.soc_rate.is_empty() {
        let v = extras.get("val_12B").map(|s| parse_f64(s)).unwrap_or(0.0);
        if v > 0.0 && !match_ratio(v / v1b, &["3.3%", "3.6%", "11.86%", "0%"], &mut data.soc_rate) {
            data.soc_rate = fmt_pct(v / v1b);
        }
    }

}

/// Preprocess an image for better OCR using ImageMagick:
/// grayscale + local adaptive threshold + optional sharpen.
fn preprocess_image(input: &Path, output: &Path) -> Result<(), String> {
    let status = create_command("magick")
        .args([
            input.to_str().unwrap(),
            "-colorspace", "gray",
            "-negate",
            "-lat", "30x30+8%",
            "-negate",
            "-sharpen", "0x1",
            output.to_str().unwrap(),
        ])
        .status()
        .map_err(|e| format!("ImageMagick error: {}", e))?;

    if !status.success() {
        // Fallback: try "convert" (ImageMagick 6)
        let status2 = create_command("convert")
            .args([
                input.to_str().unwrap(),
                "-colorspace", "gray",
                "-negate",
                "-lat", "30x30+8%",
                "-negate",
                "-sharpen", "0x1",
                output.to_str().unwrap(),
            ])
            .status()
            .map_err(|e| format!("convert error: {}", e))?;
        if !status2.success() {
            // If preprocessing fails, use the original image
            let _ = std::fs::copy(input, output);
        }
    }
    Ok(())
}

/// Run tesseract with multiple PSM modes on a single image.
/// Returns a vector of (psm_label, text) for each successful pass.
#[allow(dead_code)]
fn ocr_multi_pass(image_path: &str, lang: &str) -> Vec<(String, String)> {
    let psms = &["6", "3", "4"];
    let mut results = Vec::new();
    for psm in psms {
        let text = ocr_page(image_path, lang, psm);
        if !text.is_empty() {
            results.push((format!("psm{}", psm), text));
        }
    }
    results
}

fn try_pdftotext(file_path: &str) -> Option<String> {
    let mut best = String::new();
    for flag in &["-layout", "-raw", "-table"] {
        if let Ok(output) = create_command("pdftotext")
            .args([flag, file_path, "-"])
            .output()
        {
            if output.status.success() {
                let text = String::from_utf8_lossy(&output.stdout).to_string();
                let chinese = text.chars().filter(|c| (*c as u32) >= 0x4E00 && (*c as u32) <= 0x9FFF).count();
                let ascii_len: usize = text.chars().filter(|c| c.is_ascii_graphic()).count();
                // Require some real content, not just whitespace
                if ascii_len > 100 && chinese >= 5 && text.len() > best.len() {
                    best = text;
                }
            }
        }
    }
    if best.is_empty() { None } else { Some(best) }
}

fn try_pdftotext_all(file_path: &str) -> Option<String> {
    let mut best = String::new();
    for flag in &["-layout", "-raw", "-table"] {
        if let Ok(output) = create_command("pdftotext")
            .args([flag, file_path, "-"])
            .output()
        {
            if output.status.success() {
                let text = String::from_utf8_lossy(&output.stdout).to_string();
                let ascii_len: usize = text.chars().filter(|c| c.is_ascii_graphic()).count();
                if ascii_len > 50 && text.len() > best.len() {
                    best = text;
                }
            }
        }
    }
    if best.is_empty() { None } else { Some(best) }
}

fn apply_fc_overrides(data: &mut FormData, field_values: &HashMap<String, String>) {
    for (name, val) in field_values {
        match name.as_str() {
            "val_1A" => data.val_1A = val.clone(),
            "val_1B" => data.val_1B = val.clone(),
            "val_1C" => data.val_1C = val.clone(),
            "val_1D" => data.val_1D = val.clone(),
            "val_2A" => data.val_2A = val.clone(),
            "val_2B" => data.val_2B = val.clone(),
            "val_2C" => data.val_2C = val.clone(),
            "val_4A" => data.val_4A = val.clone(),
            "val_4B" => data.val_4B = val.clone(),
            "val_4C" => data.val_4C = val.clone(),
            "val_5A" => data.val_5A = val.clone(),
            "val_5B" => data.val_5B = val.clone(),
            "val_5C" => data.val_5C = val.clone(),
            "val_6A" => data.val_6A = val.clone(),
            "val_6B" => data.val_6B = val.clone(),
            "val_7A" => data.val_7A = val.clone(),
            "val_8A" => data.val_8A = val.clone(),
            "val_10A" => data.val_10A = val.clone(),
            _ => {}
        }
    }
}

fn build_form_data(
    text: &str,
    pass_texts: &[String],
    high_quality_text: &str,
    progress: &Option<ProgressFn>,
) -> Result<FormData, String> {
    if text.trim().is_empty() {
        return Err("No text could be extracted from PDF".into());
    }

    call_progress(progress, "vote", "Extracting field-code values...");

    let has_voting = !pass_texts.is_empty();
    let fc_patterns: &[(&str, &str)] = &[
        ("val_1A", r"(?im)^\s*1[4A][.,;:\s].*?([\d,.\s]+)\s*$"),
        ("val_1B", r"(?im)^\s*1[8B][.,;:\s].*?([\d,.\s]+)\s*$"),
        ("val_1C", r"(?im)\b1[Cc][.,;:\s].*?([\d,.\s]+)\s*$"),
        ("val_1D", r"(?im)^\s*1[Dd][.,;:\s].*?([\d,.\s]+)\s*$"),
        ("val_2A", r"(?im)^\s*2[4A][.,;:\s].*?([\d,.\s]+)\s*$"),
        ("val_2B", r"(?im)^\s*2[8B][.,;:\s].*?([\d,.\s]+)\s*$"),
        ("val_2C", r"(?im)\b2[Cc][.,;:\s].*?([\d,.\s]+)\s*$"),
        ("val_4A", r"(?im)^\s*4[Ah][.,;:\s].*?([\d,.\s]+)\s*$"),
        ("val_4B", r"(?im)\b4[8B][.,;:\s].*?([\d,.\s]+)\s*$"),
        ("val_4C", r"(?im)^\s*4[Cc][.,;:\s].*?([\d,.\s]+)\s*$"),
        ("val_5A", r"(?im)^\s*[5S][4A][.,;:\s].*?([\d,.\s]+)\s*$"),
        ("val_5B", r"(?im)^\s*5[8B][.,;:\s].*?([\d,.\s]+)\s*$"),
        ("val_5C", r"(?im)^\s*5[Cc][.,;:\s].*?([\d,.\s]+)\s*$"),
        ("val_6A", r"(?im)\b6[Ah,&][.,;:\s].*?([\d,.\s]+)\s*$"),
        ("val_6B", r"(?im)\b6[8B][.,;:\s].*?([\d,.\s]+)\s*$"),
        ("val_6C", r"(?im)^\s*6[Cc][.,;:\s].*?([\d,.\s]+)\s*$"),
        ("val_7A", r"(?im)\b[7T][4A]\^?[.,;:\s].*?([\d,.\s]+)\s*$"),
        ("val_8A", r"(?im)\b[8B][4Ah,][.,;:\s].*?([\d,.\s]+)\s*$"),
        ("val_10A", r"(?im)\b10[Ah,][.,;:\s].*?([\d,.\s]+)\s*$"),
    ];
    let formula_patterns: &[(&str, &str)] = &[
        ("val_2D", r"(?im)^\s*2[0OD][.,;:\s].*?([\d,.\s]+)\s*$"),
        ("val_4D", r"(?im)^\s*4[DdO][.,;:\s].*?([\d,.\s]+)\s*$"),
        ("val_5D", r"(?im)^\s*5[DdO][.,;:\s].*?([\d,.\s]+)\s*$"),
        ("val_6C", r"(?im)^\s*6[Cc][.,;:\s].*?([\d,.\s]+)\s*$"),
        ("val_11A", r"(?im)^\s*1[1i][,A][.,;:\s].*?([\d,.\s]+)\s*$"),
        ("val_11B", r"(?im)^\s*11[8B][.,;:\s].*?([\d,.\s]+)\s*$"),
        ("val_1E", r"(?im)\b1E[,][.,;:\s].*?([\d,.\s]+)\s*$"),
        ("val_8B", r"(?im)\b8[8B][.,;:\s].*?([\d,.\s]+)\s*$"),
        ("val_12B", r"(?im)\b12[8B][.,;:\s].*?([\d,.\s]+)\s*$"),
    ];

    let mut field_values: HashMap<String, String> = HashMap::new();
    let mut field_confidence: HashMap<String, f64> = HashMap::new();
    let mut formula_values: HashMap<String, String> = HashMap::new();

    if has_voting {
        // Multi-pass OCR path: vote across passes
        for (name, pattern) in fc_patterns {
            let (best, conf, _vals) = vote_field(pass_texts, pattern);
            if !best.is_empty() {
                field_values.insert(name.to_string(), best);
                field_confidence.insert(name.to_string(), conf);
            }
        }
        for (name, pattern) in formula_patterns {
            let (best, _conf, _vals) = vote_field(pass_texts, pattern);
            if !best.is_empty() {
                formula_values.insert(name.to_string(), best);
            }
        }
        // Fallback to high_quality_text for missed formulas
        for (name, pattern) in formula_patterns {
            if !formula_values.contains_key(*name) && !high_quality_text.is_empty() {
                let v = extract_fc(high_quality_text, pattern);
                if !v.is_empty() {
                    formula_values.insert(name.to_string(), v);
                }
            }
        }
    } else {
        // pdftotext path: direct extraction with both strict and lenient patterns
        let pte_fc_patterns: &[(&str, &str)] = &[
            ("val_1A", r"(?im)^\s*1[4A][.,;:\s].*?([\d,.\s]+)\s*$"),
            ("val_1B", r"(?im)^\s*1[8B][.,;:\s].*?([\d,.\s]+)\s*$"),
            ("val_1C", r"(?im)\b1[Cc][.,;:\s].*?([\d,.\s]+)\s*$"),
            ("val_1D", r"(?im)^\s*1[Dd][.,;:\s].*?([\d,.\s]+)\s*$"),
            ("val_2A", r"(?im)^\s*2[4A][.,;:\s].*?([\d,.\s]+)\s*$"),
            ("val_2B", r"(?im)^\s*2[8B][.,;:\s].*?([\d,.\s]+)\s*$"),
            ("val_2C", r"(?im)\b2[Cc][.,;:\s].*?([\d,.\s]+)\s*$"),
            ("val_4A", r"(?im)^\s*4[Ah][.,;:\s].*?([\d,.\s]+)\s*$"),
            ("val_4B", r"(?im)\b4[8B][.,;:\s].*?([\d,.\s]+)\s*$"),
            ("val_4C", r"(?im)^\s*4[Cc][.,;:\s].*?([\d,.\s]+)\s*$"),
            ("val_5A", r"(?im)^\s*[5S][4A][.,;:\s].*?([\d,.\s]+)\s*$"),
            ("val_5B", r"(?im)^\s*5[8B][.,;:\s].*?([\d,.\s]+)\s*$"),
            ("val_5C", r"(?im)^\s*5[Cc][.,;:\s].*?([\d,.\s]+)\s*$"),
            ("val_6A", r"(?im)\b6[Ah,&][.,;:\s].*?([\d,.\s]+)\s*$"),
            ("val_6B", r"(?im)\b6[8B][.,;:\s].*?([\d,.\s]+)\s*$"),
            ("val_7A", r"(?im)\b[7T][4A]\^?[.,;:\s].*?([\d,.\s]+)\s*$"),
            ("val_8A", r"(?im)\b[8B][4Ah,][.,;:\s].*?([\d,.\s]+)\s*$"),
            ("val_10A", r"(?im)\b10[Ah,][.,;:\s].*?([\d,.\s]+)\s*$"),
        ];
        // Also try word-boundary patterns for tabular layout
        let pte_fc_loose: &[(&str, &str)] = &[
            ("val_1A", r"(?i)\b1[4A]\b\D*?([\d,.\s]+?)(?:\s|$)"),
            ("val_1B", r"(?i)\b1[8B]\b\D*?([\d,.\s]+?)(?:\s|$)"),
            ("val_2A", r"(?i)\b2[4A]\b\D*?([\d,.\s]+?)(?:\s|$)"),
            ("val_2B", r"(?i)\b2[8B]\b\D*?([\d,.\s]+?)(?:\s|$)"),
            ("val_4A", r"(?i)\b4[Ah]\b\D*?([\d,.\s]+?)(?:\s|$)"),
            ("val_4B", r"(?i)\b4[8B]\b\D*?([\d,.\s]+?)(?:\s|$)"),
            ("val_4C", r"(?i)\b4[Cc]\b\D*?([\d,.\s]+?)(?:\s|$)"),
            ("val_5A", r"(?i)\b[5S][4A]\b\D*?([\d,.\s]+?)(?:\s|$)"),
            ("val_5B", r"(?i)\b5[8B]\b\D*?([\d,.\s]+?)(?:\s|$)"),
            ("val_5C", r"(?i)\b5[Cc]\b\D*?([\d,.\s]+?)(?:\s|$)"),
            ("val_6A", r"(?i)\b6[Ah,&]\b\D*?([\d,.\s]+?)(?:\s|$)"),
            ("val_6B", r"(?i)\b6[8B]\b\D*?([\d,.\s]+?)(?:\s|$)"),
            ("val_7A", r"(?i)\b[7T][4A]\b\D*?([\d,.\s]+?)(?:\s|$)"),
            ("val_8A", r"(?i)\b[8B][4Ah,]\b\D*?([\d,.\s]+?)(?:\s|$)"),
            ("val_10A", r"(?i)\b10[Ah,]\b\D*?([\d,.\s]+?)(?:\s|$)"),
        ];
        for (name, pattern) in pte_fc_patterns.iter().chain(pte_fc_loose.iter()) {
            if field_values.contains_key(*name) { continue; }
            let v = extract_fc(text, pattern);
            if !v.is_empty() {
                field_values.insert(name.to_string(), v);
                field_confidence.insert(name.to_string(), 1.0);
            }
        }
        for (name, pattern) in formula_patterns {
            let v = extract_fc(text, pattern);
            if !v.is_empty() {
                formula_values.insert(name.to_string(), v);
            }
        }
    }

    let mut data = parse_text(text);
    apply_fc_overrides(&mut data, &field_values);

    for (name, val) in &formula_values {
        match name.as_str() {
            "val_6C" => data.val_6C = val.clone(),
            "val_11A" => data.val_11A = val.clone(),
            "val_11B" => data.val_11B = val.clone(),
            _ => {}
        }
    }

    call_progress(progress, "validate", "Cross-validating with form formulas...");
    cross_validate(&mut data, &formula_values);

    let sync_fields: &[&str] = &["val_2B", "val_4B", "val_5B", "val_6B", "val_11A", "val_11B"];
    for f in sync_fields {
        let v: String = match *f {
            "val_2B" => data.val_2B.clone(),
            "val_4B" => data.val_4B.clone(),
            "val_5B" => data.val_5B.clone(),
            "val_6B" => data.val_6B.clone(),
            "val_11A" => data.val_11A.clone(),
            "val_11B" => data.val_11B.clone(),
            _ => continue,
        };
        if let Some(existing) = field_values.get(*f) {
            if *existing != v {
                field_values.insert(f.to_string(), v);
            }
        }
    }

    let source_label = if has_voting { "fc" } else { "pdftotext" };
    let base_conf = if has_voting { 0.0 } else { 1.0 };
    let mut meta: Vec<OcrFieldInfo> = Vec::new();

    for (name, val) in &field_values {
        let conf = field_confidence.get(name).copied().unwrap_or(base_conf);
        let cross_validated = formula_values.contains_key(name) || {
            match name.as_str() {
                "val_2B" | "val_4B" | "val_5B" | "val_6B" => true,
                _ => false,
            }
        };
        let boost = if cross_validated && has_voting { 0.25 } else { 0.0 };
        meta.push(OcrFieldInfo {
            field: name.clone(),
            value: val.clone(),
            source: source_label.into(),
            confidence: (conf + boost).min(1.0),
            cross_validated,
            ocr_values: vec![],
        });
    }

    for (name, val) in &formula_values {
        if !meta.iter().any(|m| m.field == *name) {
            meta.push(OcrFieldInfo {
                field: name.clone(),
                value: val.clone(),
                source: "formula".into(),
                confidence: 0.6,
                cross_validated: false,
                ocr_values: vec![],
            });
        }
    }

    let label_fields: &[(&str, &str, &str)] = &[
        ("val_1A", &data.val_1A, "0.00"), ("val_1B", &data.val_1B, "0.00"),
        ("val_1C", &data.val_1C, "0.00"), ("val_1D", &data.val_1D, "0.00"),
        ("val_2A", &data.val_2A, "0.00"), ("val_2B", &data.val_2B, "0.00"),
        ("val_2C", &data.val_2C, "0.00"),
        ("val_4A", &data.val_4A, "0.00"), ("val_4B", &data.val_4B, "0.00"),
        ("val_4C", &data.val_4C, "0.00"),
        ("val_5A", &data.val_5A, "0.00"), ("val_5B", &data.val_5B, "0.00"), ("val_5C", &data.val_5C, "0.00"),
        ("val_6A", &data.val_6A, "0.00"), ("val_6B", &data.val_6B, "0.00"), ("val_6C", &data.val_6C, "0.00"),
        ("val_7A", &data.val_7A, "0.00"), ("val_8A", &data.val_8A, "0.00"),
        ("val_10A", &data.val_10A, "0.00"), ("val_11A", &data.val_11A, "0.00"),
        ("val_11B", &data.val_11B, "0.00"), ("val_12A", &data.val_12A, "0.00"),
        ("temp_rate", &data.temp_rate, "0%"),
        ("vat_rate", &data.vat_rate, "14%"),
        ("wht_rate", &data.wht_rate, "0%"),
        ("ret_rate", &data.ret_rate, "0%"),
        ("oth_rate", &data.oth_rate, "0%"),
        ("soc_rate", &data.soc_rate, "0%"),
    ];
    for (name, val, default) in label_fields {
        if !meta.iter().any(|m| m.field == *name) && !val.is_empty() && *val != *default {
            meta.push(OcrFieldInfo {
                field: name.to_string(),
                value: val.to_string(),
                source: "label".into(),
                confidence: 0.6,
                cross_validated: false,
                ocr_values: vec![],
            });
        }
    }

    call_progress(progress, "done", "Finalizing extracted data...");
    data.ocr_meta = meta;
    Ok(data)
}

#[allow(dead_code)]
pub fn import_pdf(file_path: &str) -> Result<FormData, String> {
    import_pdf_with_progress(file_path, &None)
}

#[allow(unused_mut)]
fn silent_cmd(program: &str) -> Command {
    let mut cmd = Command::new(program);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    cmd
}

fn find_tool(program: &str) -> String {
    if silent_cmd(program).arg("--version").output().is_ok() {
        return program.to_string();
    }
    #[cfg(target_os = "windows")]
    {
        let candidates: &[&str] = match program {
            "tesseract" => &[
                r"C:\Program Files\Tesseract-OCR\tesseract.exe",
                r"C:\Program Files (x86)\Tesseract-OCR\tesseract.exe",
                r"C:\Program Files\Tesseract-OCR\TesseractOCR\tesseract.exe",
            ],
            "pdftotext" | "pdftoppm" => &[],
            "magick" | "convert" => &[
                r"C:\Program Files\ImageMagick\magick.exe",
                r"C:\Program Files\ImageMagick\convert.exe",
            ],
            _ => &[],
        };
        for c in candidates {
            if std::path::Path::new(c).exists() {
                return c.to_string();
            }
        }
        // Search winget package dirs for poppler tools
        if matches!(program, "pdftotext" | "pdftoppm") {
            if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
                let winget_root = format!(r"{}\Microsoft\WinGet\Packages", local_app_data);
                if let Ok(entries) = std::fs::read_dir(&winget_root) {
                    for entry in entries.flatten() {
                        if entry.file_name().to_string_lossy().contains("poppler") {
                            let exe_name = format!("{}.exe", program);
                            if let Ok(found) = find_in_dir(entry.path(), &exe_name) {
                                return found;
                            }
                        }
                    }
                }
            }
        }
        // Search for ImageMagick in versioned directories
        if matches!(program, "magick" | "convert") {
            let exe_name = format!("{}.exe", program);
            for prog_files in &[r"C:\Program Files", r"C:\Program Files (x86)"] {
                if let Ok(entries) = std::fs::read_dir(prog_files) {
                    for entry in entries.flatten() {
                        if entry.file_name().to_string_lossy().starts_with("ImageMagick") {
                            let candidate = entry.path().join(&exe_name);
                            if candidate.exists() {
                                return candidate.to_string_lossy().to_string();
                            }
                        }
                    }
                }
            }
        }
    }
    program.to_string()
}

#[cfg(target_os = "windows")]
fn find_in_dir(dir: std::path::PathBuf, target: &str) -> Result<String, ()> {
    let mut stack = vec![dir];
    for _ in 0..5 {
        if let Some(current) = stack.pop() {
            if let Ok(entries) = std::fs::read_dir(&current) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        stack.push(path);
                    } else if let Some(name) = path.file_name() {
                        if name.to_string_lossy().eq_ignore_ascii_case(target) {
                            return Ok(path.to_string_lossy().to_string());
                        }
                    }
                }
            }
        }
    }
    Err(())
}

#[allow(unused_mut)]
fn create_command(program: &str) -> Command {
    let tool_path = find_tool(program);
    silent_cmd(&tool_path)
}

fn check_tool(tool: &str) -> bool {
    let found = find_tool(tool);
    silent_cmd(&found).arg("--version").output().is_ok()
}

fn check_required_tools() -> Result<(), String> {
    let mut missing = Vec::new();
    if !check_tool("pdftotext") { missing.push("pdftotext (install poppler-utils)"); }
    if !check_tool("pdftoppm") { missing.push("pdftoppm (install poppler-utils)"); }
    if !check_tool("tesseract") { missing.push("tesseract (install tesseract-ocr)"); }
    if !check_tool("magick") && !check_tool("convert") { missing.push("ImageMagick (install imagemagick)"); }
    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!("Missing required tools:\n  {}\n\nInstall them and ensure they are on your PATH, then restart the application.", missing.join("\n  ")))
    }
}

pub fn import_pdf_with_progress(file_path: &str, progress: &Option<ProgressFn>) -> Result<FormData, String> {
    // Check required tools first
    call_progress(progress, "check", "Checking required tools...");
    if let Err(e) = check_required_tools() {
        return Err(e);
    }

    // Try pdftotext first (instant) — save result for later merge
    call_progress(progress, "render", "Trying pdftotext extraction...");
    let pte_result: Option<FormData> = try_pdftotext(file_path)
        .and_then(|t| {
            let trimmed = t.trim().to_string();
            if !trimmed.is_empty() {
                // Debug: write pdftotext output for comparison
                let debug_pte = std::env::temp_dir().join("cscec_debug_pdftotext.txt");
                let _ = std::fs::write(&debug_pte, &trimmed);
                build_form_data(&trimmed, &[], "", progress).ok()
            } else {
                None
            }
        });

    // Fall back to OCR pipeline
    call_progress(progress, "render", "Rendering PDF pages with pdftoppm...");
    let tmp_dir = std::env::temp_dir().join(format!("cscec_ocr_{}", std::process::id()));
    std::fs::create_dir_all(&tmp_dir).map_err(|e| format!("Failed to create temp dir: {}", e))?;

    let output_prefix = tmp_dir.join("page");
    let status = create_command("pdftoppm")
        .args(["-png", "-r", "250", file_path])
        .arg(output_prefix.to_str().unwrap())
        .status()
        .map_err(|e| format!("Failed to run pdftoppm: {}", e))?;

    if !status.success() {
        let _ = std::fs::remove_dir_all(&tmp_dir);
        return Err("pdftoppm failed to render PDF".into());
    }

    let mut page_files: Vec<_> = std::fs::read_dir(&tmp_dir)
        .map_err(|e| format!("Failed to read temp dir: {}", e))?
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            let name = path.file_name()?.to_str()?;
            if name.starts_with("page-") && name.ends_with(".png") { Some(path) } else { None }
        })
        .collect();
    page_files.sort();

    if page_files.is_empty() {
        let _ = std::fs::remove_dir_all(&tmp_dir);
        return Err("No pages rendered from PDF".into());
    }

    let max_pages = page_files.len().min(2);
    let mut pass_texts: Vec<String> = Vec::new();
    let mut text = String::new();
    let mut high_quality_text = String::new();

    for i in 0..max_pages {
        let img = page_files[i].to_str().unwrap().to_string();
        let page_label = format!("page {}/{}", i + 1, max_pages);

        call_progress(progress, "ocr", &format!("{} — OCR pass 1/3 (PSM 6, Chinese)...", page_label));
        let t1 = ocr_page(&img, "chi_sim+eng", "6");
        if !t1.is_empty() { pass_texts.push(t1.clone()); text.push_str(&t1); text.push('\n'); }

        call_progress(progress, "ocr", &format!("{} — OCR pass 2/3 (PSM 3, Chinese)...", page_label));
        let t2 = ocr_page(&img, "chi_sim+eng", "3");
        if !t2.is_empty() { pass_texts.push(t2.clone()); text.push_str(&t2); text.push('\n'); }

        let main_len = t1.len() + t2.len();
        if main_len < 30 || t1.len() < 15 {
            call_progress(progress, "ocr", &format!("{} — OCR pass 3/3 (preprocessed, PSM 6)...", page_label));
            let preprocessed = tmp_dir.join(format!("pp-{}.png", i + 1));
            let _ = preprocess_image(Path::new(&img), &preprocessed);
            if preprocessed.exists() {
                let t3 = ocr_page(preprocessed.to_str().unwrap(), "chi_sim+eng", "6");
                if !t3.is_empty() { pass_texts.push(t3.clone()); text.push_str(&t3); text.push('\n'); }
            }
        }

        if high_quality_text.len() < t1.len() { high_quality_text = t1.clone(); }
        if high_quality_text.len() < t2.len() { high_quality_text = t2.clone(); }
    }

    let chinese_chars: usize = text.chars().filter(|c| (*c as u32) >= 0x4E00 && (*c as u32) <= 0x9FFF).count();
    if chinese_chars == 0 && !pass_texts.is_empty() {
        call_progress(progress, "ocr", "Low Chinese content — retrying with English OCR...");
        pass_texts.clear();
        text.clear();
        for i in 0..max_pages {
            let img = page_files[i].to_str().unwrap();
            let page_label = format!("page {}/{}", i + 1, max_pages);
            call_progress(progress, "ocr", &format!("{} — OCR pass 1/2 (PSM 6, English)...", page_label));
            let t1 = ocr_page(img, "eng", "6");
            if !t1.is_empty() { pass_texts.push(t1.clone()); text.push_str(&t1); text.push('\n'); }
            call_progress(progress, "ocr", &format!("{} — OCR pass 2/2 (PSM 3, English)...", page_label));
            let t2 = ocr_page(img, "eng", "3");
            if !t2.is_empty() { pass_texts.push(t2.clone()); text.push_str(&t2); text.push('\n'); }
            if t1.len() < 15 {
                let preprocessed = tmp_dir.join(format!("pp-{}.png", i + 1));
                let _ = preprocess_image(Path::new(img), &preprocessed);
                if preprocessed.exists() {
                    call_progress(progress, "ocr", &format!("{} — OCR pass 3/3 (preprocessed)...", page_label));
                    let t3 = ocr_page(preprocessed.to_str().unwrap(), "eng", "6");
                    if !t3.is_empty() { pass_texts.push(t3.clone()); text.push_str(&t3); text.push('\n'); }
                }
            }
        }
    }

    let _ = std::fs::remove_dir_all(&tmp_dir);

    if text.trim().is_empty() && pass_texts.is_empty() {
        return Err("No text could be extracted from PDF".into());
    }

    call_progress(progress, "vote", "Voting on field-code values across passes...");
    let debug_text: String = pass_texts.iter().enumerate()
        .map(|(i, t)| format!("=== PASS {} ===\n{}\n", i+1, t))
        .collect();
    let debug_path = std::env::temp_dir().join("cscec_debug_ocr.txt");
    let _ = std::fs::write(&debug_path, &debug_text);

    let mut data = build_form_data(&text, &pass_texts, &high_quality_text, progress)?;

    // Merge pdftotext results into OCR data: pdftotext values win when available
    call_progress(progress, "merge", "Merging pdftotext overrides into OCR data...");
    if let Some(ref pte) = pte_result {
        macro_rules! merge_field {
            ($field:ident, $default:expr) => {
                if !pte.$field.is_empty() && pte.$field != $default {
                    data.$field = pte.$field.clone();
                }
            };
        }
        merge_field!(val_1A, "0.00");
        merge_field!(val_1B, "0.00");
        merge_field!(val_1C, "0.00");
        merge_field!(val_1D, "0.00");
        merge_field!(val_2A, "0.00");
        merge_field!(val_2B, "0.00");
        merge_field!(val_2C, "0.00");
        merge_field!(val_4A, "0.00");
        merge_field!(val_4B, "0.00");
        merge_field!(val_4C, "0.00");
        merge_field!(val_5A, "0.00");
        merge_field!(val_5B, "0.00");
        merge_field!(val_5C, "0.00");
        merge_field!(val_6A, "0.00");
        merge_field!(val_6B, "0.00");
        merge_field!(val_6C, "0.00");
        merge_field!(val_7A, "0.00");
        merge_field!(val_8A, "0.00");
        merge_field!(val_10A, "0.00");
        merge_field!(val_11A, "0.00");
        merge_field!(val_11B, "0.00");
        merge_field!(val_12A, "0.00");
        merge_field!(vat_rate, "14%");
        merge_field!(temp_rate, "0%");
        merge_field!(wht_rate, "0%");
        merge_field!(ret_rate, "0%");
        merge_field!(oth_rate, "0%");
        merge_field!(soc_rate, "0%");
        if !pte.seller_tax_id.is_empty() && data.seller_tax_id.is_empty() {
            data.seller_tax_id = pte.seller_tax_id.clone();
        }
        // Merge seller_tax_ids from pdftotext (but don't deduplicate — already done)
        for tid in &pte.seller_tax_ids {
            if !data.seller_tax_ids.contains(tid) {
                data.seller_tax_ids.push(tid.clone());
            }
        }
    }

    // Also extract tax IDs from full pdftotext output (all pages)
    // English scanned PDFs have clean invoice pages at the end (e.g. 221123-1.pdf pages 11-12)
    // while OCR only processes first 2 pages — so tax IDs must come from pdftotext
    if let Some(pte) = try_pdftotext_all(file_path) {
        let pte_data = parse_text(&pte);
        if !pte_data.seller_tax_id.is_empty() && data.seller_tax_id.is_empty() {
            data.seller_tax_id = pte_data.seller_tax_id;
        }
        for tid in &pte_data.seller_tax_ids {
            if !data.seller_tax_ids.contains(tid) {
                data.seller_tax_ids.push(tid.clone());
            }
        }
        // Always use pdftotext invoice data when available — it comes from clean
        // pages (e.g. invoice pages 11-12) while OCR only processes the first 2 pages
        if !pte_data.invoices.is_empty() {
            data.invoices = pte_data.invoices;
        }
    }

    Ok(data)
}

fn call_progress(progress: &Option<ProgressFn>, stage: &str, message: &str) {
    if let Some(ref cb) = *progress {
        cb(stage, message);
    }
}