use serde::{Deserialize, Serialize};
use quick_xml::Reader;
use quick_xml::events::Event;
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EtaInvoiceLine {
    pub description: String,
    pub quantity: f64,
    pub unit_price: f64,
    pub line_total: f64,
    pub vat_rate: f64,
    pub vat_amount: f64,
    pub item_code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EtaInvoice {
    pub invoice_id: String,
    pub uuid: String,
    pub issue_date: String,
    pub invoice_type_code: String,
    pub seller_tax_id: String,
    pub seller_name: String,
    pub buyer_tax_id: String,
    pub buyer_name: String,
    /// Document state from the ETA portal wrapper: Valid / Rejected / Cancelled.
    /// Anything other than "Valid" makes the invoice visible but unusable.
    #[serde(default)]
    pub doc_status: String,
    pub currency: String,
    pub net_amount: f64,
    pub total_vat: f64,
    pub total_wht: f64,
    pub grand_total: f64,
    /// ETA rate used to convert the document's EGP figures into the invoice's
    /// sold currency (1.0 for an EGP invoice). Derived from `amountEGP /
    /// amountSold` when the document omits `currencyExchangeRate`, so every
    /// foreign invoice can be compared in either currency.
    #[serde(default)]
    pub exchange_rate: f64,
    pub lines: Vec<EtaInvoiceLine>,
    /// ETA `references` array: for a credit note, the UUID(s) of the invoice(s)
    /// it credits/cancels. Empty for ordinary invoices.
    #[serde(default)]
    pub references: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationIssue {
    pub field: String,
    pub xml_value: String,
    pub form_value: String,
    pub severity: String, // "error" or "warning"
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    pub invoice: EtaInvoice,
    pub issues: Vec<ValidationIssue>,
    pub is_valid: bool,
    #[serde(default)]
    pub matched_entry_indices: Vec<usize>,
    /// The pool row id (autoincrement PK) so the frontend can reference the
    /// exact row even when two invoices share the same invoice_id under
    /// different sellers.
    #[serde(default)]
    pub pool_id: Option<i64>,
}

fn parse_f64(s: &str) -> f64 {
    s.trim().replace(',', "").parse().unwrap_or(0.0)
}

fn normalize_id(s: &str) -> String {
    s.trim().to_uppercase().chars().filter(|c| !c.is_whitespace()).collect()
}

/// ETA document type: `I` invoice, `C` credit note, `D` debit note. Unknown or
/// missing values fall back to `I` so every legacy pool row stays payable.
pub fn normalize_doc_kind(s: &str) -> String {
    match s.trim().to_uppercase().as_str() {
        "" | "I" | "INVOICE" => "I".to_string(),
        "C" | "CREDIT" | "CREDIT_NOTE" | "CREDITNOTE" => "C".to_string(),
        "D" | "DEBIT" | "DEBIT_NOTE" | "DEBITNOTE" => "D".to_string(),
        other => other.chars().next().map(|c| c.to_string()).unwrap_or_else(|| "I".to_string()),
    }
}

/// Lightweight extraction of the two metadata fields the pool needs from the
/// *raw* XML, without parsing the whole document. Portal exports encode the
/// embedded JSON with HTML entities (`&#34;`) while others use plain nested XML,
/// so both shapes are handled.
pub fn parse_eta_meta(raw_xml: &str) -> (String, Vec<String>) {
    let normalized = raw_xml
        .replace("&#34;", "\"")
        .replace("&#x22;", "\"")
        .replace("&quot;", "\"");
    let kind = json_string_value(&normalized, "documentType")
        .or_else(|| xml_text(&normalized, "documentType"))
        .map(|s| normalize_doc_kind(&s))
        .unwrap_or_else(|| "I".to_string());
    let refs = extract_references(&normalized);
    (kind, refs)
}

fn json_string_value(hay: &str, key: &str) -> Option<String> {
    let needle = format!("\"{}\"", key);
    let mut from = 0usize;
    while let Some(pos) = hay[from..].find(&needle) {
        let after_key = &hay[from + pos + needle.len()..];
        if let Some(colon) = after_key.find(':') {
            let after_colon = &after_key[colon + 1..];
            if let Some(open) = after_colon.find('"') {
                let value = &after_colon[open + 1..];
                if let Some(close) = value.find('"') {
                    return Some(value[..close].trim().to_string());
                }
            }
        }
        from += pos + needle.len();
    }
    None
}

fn xml_text(hay: &str, tag: &str) -> Option<String> {
    let open = format!("<{}>", tag);
    let close = format!("</{}>", tag);
    let start = hay.find(&open)? + open.len();
    let end = hay[start..].find(&close)? + start;
    Some(hay[start..end].trim().to_string())
}

fn looks_like_reference(s: &str) -> bool {
    let t = s.trim();
    t.len() >= 16 && t.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn extract_quoted_refs(arr: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = arr;
    while let Some(open) = rest.find('"') {
        let after = &rest[open + 1..];
        let Some(close) = after.find('"') else { break };
        let value = after[..close].trim();
        if looks_like_reference(value) {
            out.push(value.to_string());
        }
        rest = &after[close + 1..];
    }
    out
}

fn extract_references(hay: &str) -> Vec<String> {
    // JSON: "references": ["uuid", ...]
    if let Some(pos) = hay.find("\"references\"") {
        let rest = &hay[pos..];
        if let Some(open) = rest.find('[') {
            if let Some(close) = rest[open..].find(']') {
                let refs = extract_quoted_refs(&rest[open + 1..open + close]);
                if !refs.is_empty() {
                    return refs;
                }
            }
        }
    }
    // XML: <references><reference>uuid</reference></references>
    if let Some(pos) = hay.find("<references>") {
        let rest = &hay[pos..];
        if let Some(end) = rest.find("</references>") {
            let block = &rest[..end];
            let mut out = Vec::new();
            let mut cursor = block;
            while let Some(open) = cursor.find("<reference>") {
                let after = &cursor[open + "<reference>".len()..];
                let Some(close) = after.find("</reference>") else { break };
                let value = after[..close].trim();
                if looks_like_reference(value) {
                    out.push(value.to_string());
                }
                cursor = &after[close + "</reference>".len()..];
            }
            if !out.is_empty() {
                return out;
            }
            // Some exports put the UUID directly inside <references>…</references>.
            let inner = block.trim_start_matches("<references>").trim();
            if looks_like_reference(inner) {
                out.push(inner.to_string());
            }
            if !out.is_empty() {
                return out;
            }
        }
    }
    Vec::new()
}

fn service_matches_invoice(service_name: &str, invoice_id: &str) -> bool {
    let invoice_norm = normalize_id(invoice_id);
    if invoice_norm.is_empty() {
        return false;
    }
    let upper = service_name.to_uppercase();
    let mut candidates = vec![normalize_id(&upper)];
    if let Some(pos) = upper.find("INV:") {
        candidates.push(normalize_id(&upper[pos + 4..]));
    }
    // Match any standalone alphanumeric token (covers numbers typed without "Inv:",
    // even when embedded in the middle of the text)
    for token in upper.split(|c: char| !c.is_alphanumeric()) {
        if normalize_id(token) == invoice_norm {
            return true;
        }
    }
    candidates.iter().any(|c| {
        if c.is_empty() {
            return false;
        }
        c == &invoice_norm
            || (invoice_norm.len() >= 4 && (c.ends_with(&invoice_norm) || c.starts_with(&invoice_norm)))
    })
}

/// Some ETA portal exports embed the real document as an *escaped XML string*
/// inside the outer `<document>` element instead of the JSON block the SDK
/// emits. This converts that inner XML into the same JSON shape the parser
/// already reads, so its line items and tax totals are captured instead of
/// being silently dropped (which left those invoices with no VAT at all).
fn xml_document_to_json(xml: &str) -> Option<serde_json::Value> {
    use serde_json::json;

    fn blocks<'a>(xml: &'a str, tag: &str) -> Vec<&'a str> {
        let open = format!("<{}>", tag);
        let close = format!("</{}>", tag);
        let mut out = Vec::new();
        let mut rest = xml;
        while let Some(s) = rest.find(&open) {
            let start = s + open.len();
            match rest[start..].find(&close) {
                Some(e) => { out.push(&rest[start..start + e]); rest = &rest[start + e + close.len()..]; }
                None => break,
            }
        }
        out
    }
    fn text<'a>(xml: &'a str, tag: &str) -> Option<&'a str> {
        let open = format!("<{}>", tag);
        let close = format!("</{}>", tag);
        let s = xml.find(&open)? + open.len();
        let e = xml[s..].find(&close)? + s;
        Some(xml[s..e].trim())
    }
    fn num(xml: &str, tag: &str) -> f64 {
        text(xml, tag).and_then(|t| t.parse::<f64>().ok()).unwrap_or(0.0)
    }

    // Only treat this as a real document when it actually carries invoice lines.
    let line_blocks = blocks(xml, "invoiceLine");
    if line_blocks.is_empty() {
        return None;
    }

    let mut lines: Vec<serde_json::Value> = Vec::new();
    for lb in &line_blocks {
        let unit = text(lb, "unitValue").unwrap_or("");
        let taxable: Vec<serde_json::Value> = blocks(lb, "taxableItem")
            .iter()
            .map(|tb| json!({
                "taxType": text(tb, "taxType").unwrap_or(""),
                "subType": text(tb, "subType").unwrap_or(""),
                "rate": num(tb, "rate"),
                "amount": num(tb, "amount"),
            }))
            .collect();
        lines.push(json!({
            "description": text(lb, "description").unwrap_or(""),
            "quantity": num(lb, "quantity"),
            "unitValue": {
                "currencySold": text(unit, "currencySold").unwrap_or(""),
                "amountSold": num(unit, "amountSold"),
                "amountEGP": num(unit, "amountEGP"),
                "currencyExchangeRate": num(unit, "currencyExchangeRate"),
            },
            "netTotal": num(lb, "netTotal"),
            "taxableItems": taxable,
        }));
    }

    let tax_totals: Vec<serde_json::Value> = blocks(xml, "taxTotal")
        .iter()
        .map(|tb| json!({
            "taxType": text(tb, "taxType").unwrap_or(""),
            "subType": text(tb, "subType").unwrap_or(""),
            "rate": num(tb, "rate"),
            "amount": num(tb, "amount"),
        }))
        .collect();

    let issuer = text(xml, "issuer").unwrap_or("");
    let receiver = text(xml, "receiver").unwrap_or("");
    Some(json!({
        "uuid": text(xml, "uuid").unwrap_or(""),
        "internalID": text(xml, "internalID").or_else(|| text(xml, "internalId")).unwrap_or(""),
        "documentType": text(xml, "documentType").unwrap_or(""),
        "dateTimeIssued": text(xml, "dateTimeIssued").unwrap_or(""),
        "netAmount": num(xml, "netAmount"),
        "totalAmount": num(xml, "totalAmount"),
        "issuer": { "id": text(issuer, "id").unwrap_or(""), "name": text(issuer, "name").unwrap_or("") },
        "receiver": { "id": text(receiver, "id").unwrap_or(""), "name": text(receiver, "name").unwrap_or("") },
        "invoiceLines": lines,
        "taxTotals": tax_totals,
        "references": blocks(xml, "reference")
            .iter()
            .map(|r| r.trim().to_string())
            .filter(|r| !r.is_empty())
            .collect::<Vec<_>>(),
    }))
}

pub fn parse_eta_xml(xml_content: &str) -> Result<EtaInvoice, String> {
    let mut reader = Reader::from_str(xml_content);
    reader.config_mut().trim_text(true);

    let mut invoice = EtaInvoice {
        invoice_id: String::new(),
        uuid: String::new(),
        issue_date: String::new(),
        invoice_type_code: "I".to_string(),
        seller_tax_id: String::new(),
        seller_name: String::new(),
        buyer_tax_id: String::new(),
        buyer_name: String::new(),
        doc_status: "Valid".to_string(),
        currency: "EGP".to_string(),
        net_amount: 0.0,
        total_vat: 0.0,
        total_wht: 0.0,
        grand_total: 0.0,
        exchange_rate: 1.0,
        lines: Vec::new(),
        references: Vec::new(),
    };

    let mut exchange_rate = 0.0;
    let mut current_text = String::new();
    let mut in_document_json = false;
    let mut document_json = String::new();
    let mut doc_depth = 0;
    let mut validation_depth = 0u32;

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                current_text.clear();

                if tag == "document" {
                    if doc_depth > 0 {
                        // Inner <document> tag with JSON
                        in_document_json = true;
                    }
                    doc_depth += 1;
                }

                if tag == "validationResults" {
                    validation_depth += 1;
                }
            }
            Ok(Event::Text(e)) => {
                let text = e.unescape().map_err(|e| e.to_string())?.to_string();
                if in_document_json {
                    document_json.push_str(&text);
                } else {
                    current_text = text;
                }
            }
            Ok(Event::CData(e)) => {
                if in_document_json {
                    document_json.push_str(&String::from_utf8_lossy(e.as_ref()));
                }
            }
            Ok(Event::End(e)) => {
                let tag_name = String::from_utf8_lossy(e.name().as_ref()).to_string();

                if tag_name == "document" {
                    doc_depth -= 1;
                    if doc_depth == 1 && in_document_json {
                        // Closed the inner <document> that contains JSON
                        in_document_json = false;
                    }
                }

                if !in_document_json {
                    let text = current_text.trim().to_string();
                    match tag_name.as_str() {
                        "uuid" => { invoice.uuid = text; }
                        "internalId" => { invoice.invoice_id = text; }
                        "issuerId" => { invoice.seller_tax_id = text; }
                        "issuerName" => { invoice.seller_name = text; }
                        "receiverId" => { invoice.buyer_tax_id = text; }
                        "receiverName" => { invoice.buyer_name = text; }
                        "dateTimeIssued" => { invoice.issue_date = text; }
                        "status" => {
                            // Portal wrapper status: Valid / Rejected / Cancelled.
                            // Ignore <status> inside <validationResults> — that block
                            // records the ORIGINAL validation outcome (often still
                            // "Valid" for documents cancelled later) and must not
                            // override the authoritative wrapper state.
                            if validation_depth == 0 {
                                let lower = text.to_lowercase();
                                if lower.contains("reject") { invoice.doc_status = "Rejected".to_string(); }
                                else if lower.contains("cancel") || lower.contains("void") { invoice.doc_status = "Cancelled".to_string(); }
                                else if !text.is_empty() { invoice.doc_status = "Valid".to_string(); }
                            }
                        }
                        "validationResults" => { validation_depth = validation_depth.saturating_sub(1); }
                        "netAmount" => { invoice.net_amount = parse_f64(&text); }
                        "total" => { invoice.grand_total = parse_f64(&text); }
                        _ => {}
                    }
                }
                current_text.clear();
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("XML parse error: {:?}", e)),
            _ => {}
        }
    }

    // Parse the embedded document for line items and tax totals. The ETA SDK
    // emits it as JSON inside <document>; some portal exports instead wrap an
    // escaped XML document there, and others nest a plain XML document. Cover
    // all three so no invoice is imported without its lines/VAT.
    {
        // Fix HTML entities first
        let json_str = document_json.replace("&#34;", "\"");
        let parsed_value = serde_json::from_str::<serde_json::Value>(&json_str)
            .ok()
            .or_else(|| xml_document_to_json(&json_str))
            .or_else(|| xml_document_to_json(xml_content));
        if let Some(parsed) = parsed_value {
            // Determine which tax types are withholding by inspecting line-level subtypes.
            // WHT entries carry a "W"-prefixed subtype (e.g. W004); added taxes (VAT-like)
            // like Tbl01 must NOT be summed into WHT. If no subtype info exists at all
            // (legacy format), fall back to treating T2/T4/T5 as WHT.
            let mut wht_tax_types: HashSet<String> = HashSet::new();
            let mut has_any_subtype = false;
            if let Some(lines) = parsed.get("invoiceLines").and_then(|v| v.as_array()) {
                for line in lines {
                    if let Some(taxable) = line.get("taxableItems").and_then(|v| v.as_array()) {
                        for tax_item in taxable {
                            let sub = tax_item.get("subType").and_then(|v| v.as_str()).unwrap_or("");
                            if !sub.is_empty() {
                                has_any_subtype = true;
                            }
                            if sub.starts_with('W') {
                                if let Some(tt) = tax_item.get("taxType").and_then(|v| v.as_str()) {
                                    wht_tax_types.insert(tt.to_string());
                                }
                            }
                        }
                    }
                }
            }

            // Extract tax totals
            if let Some(tax_totals) = parsed.get("taxTotals").and_then(|v| v.as_array()) {
                for tax in tax_totals {
                    let tax_type = tax.get("taxType").and_then(|v| v.as_str()).unwrap_or("");
                    let amt = tax.get("amount").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    if wht_tax_types.contains(tax_type) {
                        // Withholding tax (subtype W-prefixed)
                        invoice.total_wht += amt;
                    } else if has_any_subtype || tax_type == "T1" {
                        // Subtypes are meaningful: everything not W is VAT-like (T1 standard,
                        // T2/Tbl01 table tax, etc.). T1 is always VAT.
                        invoice.total_vat += amt;
                    } else if tax_type == "T2" || tax_type == "T4" || tax_type == "T5" {
                        // Legacy fallback (no subtype info present): treat as WHT
                        invoice.total_wht += amt;
                    } else {
                        invoice.total_vat += amt;
                    }
                }
            }

            // Extract line items
            if let Some(lines) = parsed.get("invoiceLines").and_then(|v| v.as_array()) {
                for line in lines {
                    let desc = line.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let qty = line.get("quantity").and_then(|v| v.as_f64()).unwrap_or(1.0);
                    let unit_value = line.get("unitValue");
                    let cur_sold = unit_value.and_then(|v| v.get("currencySold")).and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let rate = unit_value.and_then(|v| v.get("currencyExchangeRate")).and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let amount_sold = unit_value.and_then(|v| v.get("amountSold")).and_then(|v| v.as_f64());
                    let amount_egp = unit_value.and_then(|v| v.get("amountEGP")).and_then(|v| v.as_f64());

                    // If the invoice is sold in a foreign currency (e.g. USD), keep the
                    // invoice currency and capture the exchange rate so totals can be
                    // converted to the sold currency below (amounts in the XML are EGP).
                    if !cur_sold.is_empty() && cur_sold != "EGP" {
                        if invoice.currency == "EGP" {
                            invoice.currency = cur_sold.clone();
                        }
                        // Prefer the declared rate; when the document omits it (some
                        // portal exports do) derive it from amountEGP / amountSold so
                        // the totals are still converted into the sold currency.
                        let mut line_rate = rate;
                        if line_rate <= 0.0 {
                            let sold = amount_sold.unwrap_or(0.0);
                            let egp = amount_egp.unwrap_or(0.0);
                            if sold > 0.0 && egp > 0.0 {
                                line_rate = egp / sold;
                            }
                        }
                        if line_rate > 0.0 {
                            exchange_rate = line_rate;
                        }
                    }

                    let unit_val = if invoice.currency == "EGP" {
                        amount_egp.or(amount_sold).unwrap_or(0.0)
                    } else {
                        amount_sold.or(amount_egp).unwrap_or(0.0)
                    };
                    let net_total = line.get("netTotal").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let item_code = line.get("itemCode").and_then(|v| v.as_str()).unwrap_or("").to_string();

                    // Extract the effective (VAT-like) tax from taxable items; skip WHT
                    // entries. When a line carries several non-WHT taxes (e.g. T2 10%
                    // plus a zero-rated T1), use the one with the largest amount.
                    let mut line_vat_rate = 0.0;
                    let mut line_vat_amount = 0.0;
                    if let Some(taxable) = line.get("taxableItems").and_then(|v| v.as_array()) {
                        for tax_item in taxable {
                            let tax_type = tax_item.get("taxType").and_then(|v| v.as_str()).unwrap_or("");
                            if !wht_tax_types.contains(tax_type) {
                                let rate = tax_item.get("rate").and_then(|v| v.as_f64()).unwrap_or(0.0);
                                let amount = tax_item.get("amount").and_then(|v| v.as_f64()).unwrap_or(0.0);
                                if amount > line_vat_amount {
                                    line_vat_rate = rate;
                                    line_vat_amount = amount;
                                }
                            }
                        }
                    }

                    invoice.lines.push(EtaInvoiceLine {
                        description: desc,
                        quantity: qty,
                        unit_price: unit_val,
                        line_total: net_total,
                        vat_rate: line_vat_rate,
                        vat_amount: line_vat_amount,
                        item_code,
                    });
                }
            }

            // If grand_total is still 0, get from totalAmount in JSON
            if invoice.grand_total == 0.0 {
                invoice.grand_total = parsed.get("totalAmount").and_then(|v| v.as_f64()).unwrap_or(0.0);
            }
            // If net_amount is still 0, get from netAmount in JSON
            if invoice.net_amount == 0.0 {
                invoice.net_amount = parsed.get("netAmount").and_then(|v| v.as_f64()).unwrap_or(0.0);
            }
            // Identity fields may only exist in the embedded JSON. Values are
            // trimmed: ETA data often carries stray leading/trailing spaces or
            // tabs inside JSON string values, which would otherwise create
            // duplicate identities (' ADT-494' vs 'ADT-494').
            if invoice.invoice_id.is_empty() {
                invoice.invoice_id = parsed.get("internalId").and_then(|v| v.as_str())
                    .or_else(|| parsed.get("internalID").and_then(|v| v.as_str()))
                    .map(|s| s.trim().to_string())
                    .unwrap_or_default();
            }
            if invoice.uuid.is_empty() {
                invoice.uuid = parsed.get("uuid").and_then(|v| v.as_str())
                    .map(|s| s.trim().to_string())
                    .unwrap_or_default();
            }
            if invoice.seller_tax_id.is_empty() {
                invoice.seller_tax_id = parsed.get("issuerId").and_then(|v| v.as_str())
                    .or_else(|| parsed.pointer("/issuer/id").and_then(|v| v.as_str()))
                    .map(|s| s.trim().to_string())
                    .unwrap_or_default();
            }
            if invoice.seller_name.is_empty() {
                invoice.seller_name = parsed.get("issuerName").and_then(|v| v.as_str())
                    .or_else(|| parsed.pointer("/issuer/name").and_then(|v| v.as_str()))
                    .map(|s| s.trim().to_string())
                    .unwrap_or_default();
            }
            if invoice.buyer_tax_id.is_empty() {
                invoice.buyer_tax_id = parsed.get("receiverId").and_then(|v| v.as_str())
                    .or_else(|| parsed.pointer("/receiver/id").and_then(|v| v.as_str()))
                    .map(|s| s.trim().to_string())
                    .unwrap_or_default();
            }
            if invoice.buyer_name.is_empty() {
                invoice.buyer_name = parsed.get("receiverName").and_then(|v| v.as_str())
                    .or_else(|| parsed.pointer("/receiver/name").and_then(|v| v.as_str()))
                    .map(|s| s.trim().to_string())
                    .unwrap_or_default();
            }
            if invoice.issue_date.is_empty() {
                invoice.issue_date = parsed.get("dateTimeIssued").and_then(|v| v.as_str()).unwrap_or("").to_string();
            }
        }
    }

    // The ETA document always reports amounts in EGP (converted at the invoice's
    // exchange rate). When the invoice is sold in a foreign currency (e.g. USD),
    // convert the totals back to the sold currency so they match the actual
    // invoice figures (amountSold) and compare correctly against the form.
    if invoice.currency != "EGP" && exchange_rate > 0.0 {
        invoice.net_amount = (invoice.net_amount / exchange_rate * 100.0).round() / 100.0;
        invoice.total_vat = (invoice.total_vat / exchange_rate * 100.0).round() / 100.0;
        invoice.total_wht = (invoice.total_wht / exchange_rate * 100.0).round() / 100.0;
        invoice.grand_total = (invoice.grand_total / exchange_rate * 100.0).round() / 100.0;
        for line in invoice.lines.iter_mut() {
            line.line_total = (line.line_total / exchange_rate * 100.0).round() / 100.0;
            line.vat_amount = (line.vat_amount / exchange_rate * 100.0).round() / 100.0;
        }
    }
    // Record the rate the figures are expressed at (1.0 for an EGP invoice), so
    // a bank document can compare the invoice in either currency.
    invoice.exchange_rate = if invoice.currency != "EGP" && exchange_rate > 0.0 {
        exchange_rate
    } else {
        1.0
    };

    // Central identity normalization: never let stray whitespace into IDs.
    invoice.invoice_id = invoice.invoice_id.trim().to_string();
    invoice.uuid = invoice.uuid.trim().to_string();

    // Document type + credit-note references. Extracted from the raw export so
    // both the escaped-JSON and plain-XML shapes are covered.
    let (doc_kind, references) = parse_eta_meta(xml_content);
    invoice.invoice_type_code = doc_kind;
    invoice.references = references;

    Ok(invoice)
}

pub fn validate_eta_against_form(invoice: &EtaInvoice, form_json: &str) -> Result<ValidationResult, String> {
    let mut issues: Vec<ValidationIssue> = Vec::new();

    let form: serde_json::Value = serde_json::from_str(form_json)
        .map_err(|e| format!("Invalid form JSON: {}", e))?;

    let get_str = |key: &str| -> String {
        form.get(key).and_then(|v| v.as_str()).unwrap_or("").to_string()
    };

    let doc_type = get_str("doc_type");

    let mut matched_entry_indices: Vec<usize> = Vec::new();

    // ── Currency alignment ──
    // The invoice is stored in its sold currency (e.g. USD), and the Import tab
    // keeps that invoice's EGP figures converted with the exchange rate typed in
    // the document. Only the Import tab has such a rate, so only convert there.
    // The Bank tab stores each invoice in its own currency (the pool's amount is
    // the sold-currency figure), so compare the invoice's sold-currency amounts
    // directly — converting with a document rate would use a different rate and
    // report false mismatches for USD invoices.
    let imported_rate = get_str("import_commercial_rate")
        .chars().filter(|c| !c.is_whitespace()).collect::<String>()
        .replace(',', "")
        .parse::<f64>()
        .unwrap_or(0.0);
    let fx_factor = if doc_type == "import" && invoice.currency != "EGP" && imported_rate > 0.0 {
        imported_rate
    } else {
        1.0
    };
    let to_egp = |v: f64| (v * fx_factor * 100.0).round() / 100.0;
    let it_net = to_egp(invoice.net_amount);
    let it_vat = to_egp(invoice.total_vat);
    let it_wht = to_egp(invoice.total_wht);
    let it_grand = to_egp(invoice.grand_total);

    // ── Common checks ──

    // 1. Buyer tax ID
    let form_buyer = get_str("buyer_tax_id");
    if !form_buyer.is_empty() && !invoice.buyer_tax_id.is_empty() && form_buyer != invoice.buyer_tax_id {
        issues.push(ValidationIssue {
            field: "Buyer Tax ID".into(),
            xml_value: invoice.buyer_tax_id.clone(),
            form_value: form_buyer,
            severity: "error".into(),
            message: "Buyer tax ID does not match the XML invoice".into(),
        });
    }

    // 2. Seller tax IDs
    let seller_ids: Vec<String> = form.get("seller_tax_ids")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
        .unwrap_or_default();
    if !seller_ids.is_empty() && !invoice.seller_tax_id.is_empty() {
        if !seller_ids.iter().any(|id| id == &invoice.seller_tax_id) {
            issues.push(ValidationIssue {
                field: "Seller Tax ID".into(),
                xml_value: invoice.seller_tax_id.clone(),
                form_value: seller_ids.join(", "),
                severity: "error".into(),
                message: "Seller tax ID in XML does not match any seller tax ID in the form".into(),
            });
        }
    }

    // 3. Missing data in XML
    if invoice.invoice_id.is_empty() {
        issues.push(ValidationIssue {
            field: "Invoice ID".into(),
            xml_value: "(empty)".into(),
            form_value: "-".into(),
            severity: "warning".into(),
            message: "XML invoice has no Invoice ID".into(),
        });
    }
    if invoice.seller_tax_id.is_empty() {
        issues.push(ValidationIssue {
            field: "Seller Tax ID".into(),
            xml_value: "(empty)".into(),
            form_value: "-".into(),
            severity: "warning".into(),
            message: "XML invoice has no Seller Tax ID".into(),
        });
    }
    if invoice.buyer_tax_id.is_empty() {
        issues.push(ValidationIssue {
            field: "Buyer Tax ID".into(),
            xml_value: "(empty)".into(),
            form_value: "-".into(),
            severity: "warning".into(),
            message: "XML invoice has no Buyer Tax ID".into(),
        });
    }

    // ── Bank document checks ──
    if doc_type != "import" {
        let invoices = form.get("invoices").and_then(|v| v.as_array());

        // Find matching invoice in the form
        if let Some(inv_list) = invoices {
            let matching_inv = inv_list.iter().find(|inv| {
                inv.get("invoice_no").and_then(|v| v.as_str()) == Some(&invoice.invoice_id)
            });

            if let Some(inv) = matching_inv {
                let inv_amt = inv.get("amount").and_then(|v| v.as_str())
                    .and_then(|s| s.replace(',', "").parse::<f64>().ok())
                    .unwrap_or(0.0);

                // Check net amount (amount before VAT)
                if it_net > 0.0 && inv_amt > 0.0 {
                    // Form amount could be net or gross — check both
                    let diff_net = (it_net - inv_amt).abs();
                    let diff_gross = (it_grand - inv_amt).abs();
                    if diff_net > 0.5 && diff_gross > 0.5 {
                        issues.push(ValidationIssue {
                            field: "Invoice Amount".into(),
                            xml_value: format!("Net: {:.2} / Gross: {:.2}", it_net, it_grand),
                            form_value: format!("{:.2}", inv_amt),
                            severity: "error".into(),
                            message: format!("Invoice amount differs — XML net {:.2} / gross {:.2} vs form {:.2}", it_net, it_grand, inv_amt),
                        });
                    }
                }

                // Check VAT amount
                if it_vat > 0.0 {
                    let form_vat_total = form.get("import_total_vat").and_then(|v| v.as_str())
                        .and_then(|s| s.replace(',', "").parse::<f64>().ok())
                        .unwrap_or(0.0);
                    if form_vat_total > 0.0 {
                        let diff = (it_vat - form_vat_total).abs();
                        if diff > 0.5 {
                            issues.push(ValidationIssue {
                                field: "Total VAT".into(),
                                xml_value: format!("{:.2}", it_vat),
                                form_value: format!("{:.2}", form_vat_total),
                                severity: "error".into(),
                                message: format!("Total VAT differs by {:.2}", diff),
                            });
                        }
                    }
                }

                // Check WHT amount
                if it_wht > 0.0 {
                    let form_wht_total = form.get("import_total_wht").and_then(|v| v.as_str())
                        .and_then(|s| s.replace(',', "").parse::<f64>().ok())
                        .unwrap_or(0.0);
                    if form_wht_total > 0.0 {
                        let diff = (it_wht - form_wht_total).abs();
                        if diff > 0.5 {
                            issues.push(ValidationIssue {
                                field: "Total WHT".into(),
                                xml_value: format!("{:.2}", it_wht),
                                form_value: format!("{:.2}", form_wht_total),
                                severity: "error".into(),
                                message: format!("Total WHT differs by {:.2}", diff),
                            });
                        }
                    }
                }

                // Check per-line VAT rates
                for (i, xml_line) in invoice.lines.iter().enumerate() {
                    if xml_line.vat_rate > 0.0 {
                        // Check if 14% is expected for general services, 10% for inspection/clearance/testing
                        let desc = xml_line.description.to_lowercase();
                        let expected_rate = if desc.contains("inspection") || desc.contains("clearance") || desc.contains("testing") || desc.contains("custom") {
                            10.0
                        } else {
                            14.0
                        };
                        if (xml_line.vat_rate - expected_rate).abs() > 0.5 {
                            issues.push(ValidationIssue {
                                field: format!("Line {} VAT Rate", i + 1),
                                xml_value: format!("{:.0}%", xml_line.vat_rate),
                                form_value: format!("{:.0}%", expected_rate),
                                severity: "warning".into(),
                                message: format!("Line '{}' has VAT {:.0}% — expected {:.0}% based on service type", xml_line.description, xml_line.vat_rate, expected_rate),
                            });
                        }
                    }
                }
            } else {
                // No matching invoice found
                let form_inv_ids: Vec<String> = inv_list.iter()
                    .filter_map(|inv| inv.get("invoice_no").and_then(|v| v.as_str()).map(|s| s.to_string()))
                    .collect();
                if !form_inv_ids.is_empty() {
                    issues.push(ValidationIssue {
                        field: "Invoice Number".into(),
                        xml_value: invoice.invoice_id.clone(),
                        form_value: form_inv_ids.join(", "),
                        severity: "warning".into(),
                        message: "Invoice number in XML does not match any invoice number in the form".into(),
                    });
                }
            }
        }
    }

    // ── Import document checks ──
    if doc_type == "import" {
        let import_entries = form.get("import_entries").and_then(|v| v.as_array());

        if let Some(entries) = import_entries {
            // Collect ALL form entries matching this XML invoice.
            // Multiple services can reference the same invoice (e.g. two lines with
            // different VAT rates), so we match every entry whose service_name contains
            // the invoice id, then validate the aggregated totals and each line.
            matched_entry_indices = entries.iter().enumerate()
                .filter(|(_, entry)| {
                    let name = entry.get("service_name").and_then(|v| v.as_str()).unwrap_or("");
                    service_matches_invoice(name, &invoice.invoice_id)
                })
                .map(|(i, _)| i)
                .collect();

            if !matched_entry_indices.is_empty() {
                let matched: Vec<&serde_json::Value> = matched_entry_indices.iter()
                    .map(|&i| &entries[i])
                    .collect();

                let entry_numbers = |entry: &serde_json::Value| -> (f64, f64, f64, f64, f64, bool) {
                    let amt = entry.get("amount").and_then(|v| v.as_str())
                        .and_then(|s| s.replace(',', "").parse::<f64>().ok())
                        .unwrap_or(0.0);
                    let rate = entry.get("rate").and_then(|v| v.as_str())
                        .and_then(|s| s.replace(',', "").parse::<f64>().ok())
                        .unwrap_or(1.0);
                    let vat_rate = entry.get("vat_rate").and_then(|v| v.as_str())
                        .and_then(|s| s.trim_end_matches('%').parse::<f64>().ok())
                        .unwrap_or(0.0);
                    let wht_rate = entry.get("wht_rate").and_then(|v| v.as_str())
                        .and_then(|s| s.trim_end_matches('%').parse::<f64>().ok())
                        .unwrap_or(0.0);
                    let free_wht = entry.get("free_wht").and_then(|v| v.as_bool()).unwrap_or(false);
                    (amt * rate, amt, rate, vat_rate, wht_rate, free_wht)
                };

                // Aggregate totals across all matched entries
                let mut form_total = 0.0;
                let mut form_vat_total = 0.0;
                let mut form_wht_total = 0.0;
                for e in &matched {
                    let (egp, _amt, _rate, vat_rate, wht_rate, free_wht) = entry_numbers(e);
                    form_total += egp;
                    form_vat_total += egp * vat_rate / 100.0;
                    if !free_wht {
                        form_wht_total += egp * wht_rate / 100.0;
                    }
                }

                // Check net amount against the sum of matching entries
                if it_net > 0.0 && form_total > 0.0 {
                    let diff = (it_net - form_total).abs();
                    if diff > 0.5 {
                        issues.push(ValidationIssue {
                            field: "Total Net Amount".into(),
                            xml_value: format!("{:.2}", it_net),
                            form_value: format!("{:.2}", form_total),
                            severity: "error".into(),
                            message: format!("Total net amount differs by {:.2} — sum of matching services is {:.2}", diff, form_total),
                        });
                    }
                }

                // Check total VAT
                if it_vat > 0.0 && form_vat_total > 0.0 {
                    let diff = (it_vat - form_vat_total).abs();
                    if diff > 0.5 {
                        issues.push(ValidationIssue {
                            field: "Total VAT".into(),
                            xml_value: format!("{:.2}", it_vat),
                            form_value: format!("{:.2}", form_vat_total),
                            severity: "error".into(),
                            message: format!("Total VAT differs by {:.2} — expected from matching services", diff),
                        });
                    }
                }

                // Check total WHT
                if it_wht > 0.0 && form_wht_total > 0.0 {
                    let diff = (it_wht - form_wht_total).abs();
                    if diff > 0.5 {
                        issues.push(ValidationIssue {
                            field: "Total WHT".into(),
                            xml_value: format!("{:.2}", it_wht),
                            form_value: format!("{:.2}", form_wht_total),
                            severity: "error".into(),
                            message: format!("Total WHT differs by {:.2} — expected from matching services", diff),
                        });
                    }
                } else if it_wht > 0.0 && form_wht_total == 0.0 {
                    issues.push(ValidationIssue {
                        field: "Total WHT".into(),
                        xml_value: format!("{:.2}", it_wht),
                        form_value: "0.00".into(),
                        severity: "warning".into(),
                        message: "XML invoice has WHT but matching services have none (0% or free_wht)".into(),
                    });
                } else if form_wht_total > 0.0 && it_wht == 0.0 {
                    issues.push(ValidationIssue {
                        field: "Total WHT".into(),
                        xml_value: "0.00".into(),
                        form_value: format!("{:.2}", form_wht_total),
                        severity: "warning".into(),
                        message: "Matching services expect WHT but the XML invoice has none".into(),
                    });
                }

                // Per-line checks. Strict positional matching is only meaningful when
                // the XML line count equals the number of matched services. A single
                // XML line is often split across multiple services (e.g. 2600 →
                // 1000 + 1600), or many XML lines may be aggregated into one service
                // (e.g. a total line). In those cases the aggregate totals above
                // already validate the amounts, so we skip positional line/entry
                // comparisons and avoid a confusing line-count warning.
                if invoice.lines.len() == matched.len() {
                    for (i, xml_line) in invoice.lines.iter().enumerate() {
                        let entry = matched[i];
                        let (egp, _amt, _rate, vat_rate, _wht_rate, _free_wht) = entry_numbers(entry);
                        let line_ttl = to_egp(xml_line.line_total);
                        if line_ttl > 0.0 && egp > 0.0 && (line_ttl - egp).abs() > 0.5 {
                            issues.push(ValidationIssue {
                                field: format!("Line {} Amount", i + 1),
                                xml_value: format!("{:.2}", line_ttl),
                                form_value: format!("{:.2}", egp),
                                severity: "error".into(),
                                message: format!("Line amount differs by {:.2} (form amount × rate)", (line_ttl - egp).abs()),
                            });
                        }
                        if xml_line.vat_rate > 0.0 && vat_rate > 0.0 && (xml_line.vat_rate - vat_rate).abs() > 0.5 {
                            issues.push(ValidationIssue {
                                field: format!("Line {} VAT Rate", i + 1),
                                xml_value: format!("{:.0}%", xml_line.vat_rate),
                                form_value: format!("{:.0}%", vat_rate),
                                severity: "warning".into(),
                                message: format!("VAT rate in XML ({:.0}%) differs from form ({:.0}%)", xml_line.vat_rate, vat_rate),
                            });
                        }
                    }
                }
            } else {
                // No form entry matches this invoice
                let form_names: Vec<String> = entries.iter()
                    .filter_map(|e| e.get("service_name").and_then(|v| v.as_str()).map(|s| s.to_string()))
                    .collect();
                if !form_names.is_empty() {
                    issues.push(ValidationIssue {
                        field: "Invoice Number".into(),
                        xml_value: invoice.invoice_id.clone(),
                        form_value: form_names.join(", "),
                        severity: "warning".into(),
                        message: "Invoice number in XML does not match any service name in the import tab".into(),
                    });
                }
            }
        }

        // Check commercial invoice amounts for import
        let invoices = form.get("invoices").and_then(|v| v.as_array());
        if let Some(inv_list) = invoices {
            let matching_inv = inv_list.iter().find(|inv| {
                inv.get("invoice_no").and_then(|v| v.as_str()) == Some(&invoice.invoice_id)
            });
            if let Some(inv) = matching_inv {
                let inv_amt = inv.get("amount").and_then(|v| v.as_str())
                    .and_then(|s| s.replace(',', "").parse::<f64>().ok())
                    .unwrap_or(0.0);
                if inv_amt > 0.0 && it_grand > 0.0 {
                    // Form amount could be net or gross — check both
                    let diff_net = (it_net - inv_amt).abs();
                    let diff_gross = (it_grand - inv_amt).abs();
                    if diff_net > 0.5 && diff_gross > 0.5 {
                        issues.push(ValidationIssue {
                            field: "Commercial Invoice Amount".into(),
                            xml_value: format!("Net: {:.2} / Gross: {:.2}", it_net, it_grand),
                            form_value: format!("{:.2}", inv_amt),
                            severity: "error".into(),
                            message: format!("Commercial invoice amount differs — XML net {:.2} / gross {:.2} vs form {:.2}", it_net, it_grand, inv_amt),
                        });
                    }
                }
            }
        }
    }

    let is_valid = issues.iter().all(|i| i.severity != "error");

    Ok(ValidationResult {
        invoice: invoice.clone(),
        issues,
        is_valid,
        matched_entry_indices,
        pool_id: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_inv_in_free_text() {
        assert!(service_matches_invoice("A4 KPI CC TAX ID: 721067026 Inv: 0206 ADT", "0206ADT"));
        assert!(service_matches_invoice("A4 KPI CC TAX ID: 721067026 Inv: 0206 ADT", "0206 ADT"));
        assert!(service_matches_invoice("0206ADT", "0206ADT"));
        assert!(service_matches_invoice("0206 ADT", "0206ADT"));
        assert!(service_matches_invoice("SERVICE 0206ADT 1234", "0206ADT"));
        assert!(service_matches_invoice("0206ADT TAX ID 721067026", "0206ADT"));
        assert!(service_matches_invoice("A4 KPI CC 0206ADT", "0206ADT"));
        assert!(!service_matches_invoice("A4 KPI CC TAX ID: 721067026 Inv: 0206 ADT", "0205ADT"));
        assert!(!service_matches_invoice("A4 KPI CC TAX ID: 721067026", "0206ADT"));
    }

    #[test]
    fn parses_extension_bundle_format() {
        // Browser-extension exports wrap the ETA JSON in <document><document>…
        // with the JSON FIRST and no <uuid>/<internalId>/<issuerId> wrapper
        // elements; identity must fall back to the nested JSON keys.
        let xml = r#"<document><document>{
      "issuer": { "type": "B", "id": "761776869", "name": "تويار لتجاره الجمله والتجزئه" },
      "receiver": { "type": "B", "id": "100489095", "name": "الشركه الصينيه" },
      "documentType": "i",
      "dateTimeIssued": "2026-07-24T14:29:00Z",
      "internalID": "62",
      "invoiceLines": [
        {"description":"cable","quantity":10,"unitValue":{"currencySold":"EGP","amountEGP":105,"amountSold":0},
         "netTotal":1050,"taxableItems":[{"taxType":"T1","amount":147,"subType":"V009","rate":14}]}
      ],
      "taxTotals":[{"taxType":"T1","amount":147,"subType":"V009"}],
      "netAmount":1050,"totalAmount":1197
    }</document><issuerName>تويار لتجاره الجمله والتجزئه</issuerName><receiverId>100489095</receiverId><receiverName>الشركه الصينيه</receiverName><dateTimeIssued>2026-07-24T14:29:00Z</dateTimeIssued><totalSales>1050</totalSales><netAmount>1050</netAmount><total>1197</total><status>Rejected</status></document>"#;
        let inv = parse_eta_xml(xml).unwrap();
        assert_eq!(inv.invoice_id, "62");
        assert_eq!(inv.seller_tax_id, "761776869");
        assert_eq!(inv.buyer_tax_id, "100489095");
        assert_eq!(inv.doc_status, "Rejected", "wrapper status after inner JSON must be picked up");
        assert_eq!(inv.net_amount, 1050.0);
        assert_eq!(inv.total_vat, 147.0);
        assert_eq!(inv.grand_total, 1197.0);
        assert_eq!(inv.lines.len(), 1);
    }

    #[test]
    fn parses_escaped_xml_document_export() {
        // ETA portal "document" downloads wrap the real document as an escaped
        // XML string inside <document> (not the SDK's JSON block). Those files
        // used to import with no lines and no VAT at all.
        let inner = r#"<document><issuer><type>B</type><id>100443648</id><name>Warehouse Co</name></issuer><receiver><type>B</type><id>100489095</id><name>Buyer Co</name></receiver><documentType>I</documentType><dateTimeIssued>2026-03-14T12:12:00Z</dateTimeIssued><internalID>I1895071</internalID><invoiceLines><invoiceLine><description>Open safe</description><quantity>1</quantity><unitValue><currencySold>EGP</currencySold><amountEGP>30</amountEGP><amountSold>0</amountSold><currencyExchangeRate>0</currencyExchangeRate></unitValue><netTotal>30</netTotal><taxableItems><taxableItem><taxType>T1</taxType><amount>4.2</amount><subType>V009</subType><rate>14</rate></taxableItem></taxableItems></invoiceLine><invoiceLine><description>Storage</description><quantity>1</quantity><unitValue><currencySold>EGP</currencySold><amountEGP>1400</amountEGP><amountSold>0</amountSold><currencyExchangeRate>0</currencyExchangeRate></unitValue><netTotal>1400</netTotal><taxableItems><taxableItem><taxType>T1</taxType><amount>196</amount><subType>V009</subType><rate>14</rate></taxableItem></taxableItems></invoiceLine></invoiceLines><netAmount>1430</netAmount><taxTotals><taxTotal><taxType>T1</taxType><amount>200.2</amount></taxTotal></taxTotals><totalAmount>1630.2</totalAmount></document>"#;
        let escaped = inner.replace('<', "&lt;").replace('>', "&gt;");
        let xml = format!(
            r#"<document><uuid>Z45YBJ5HB3ZQS0060VAQAPKK10</uuid><internalId>I1895071</internalId><issuerId>100443648</issuerId><issuerName>Warehouse Co</issuerName><receiverId>100489095</receiverId><receiverName>Buyer Co</receiverName><dateTimeIssued>2026-03-14T12:12:00</dateTimeIssued><netAmount>1430</netAmount><total>1630.2</total><status>Valid</status><document>{}</document></document>"#,
            escaped
        );
        let inv = parse_eta_xml(&xml).unwrap();
        assert_eq!(inv.invoice_id, "I1895071");
        assert_eq!(inv.seller_tax_id, "100443648");
        assert_eq!(inv.lines.len(), 2, "escaped-XML lines must be captured");
        assert_eq!(inv.lines[0].description, "Open safe");
        assert_eq!(inv.lines[0].line_total, 30.0);
        assert_eq!(inv.lines[0].vat_rate, 14.0);
        assert_eq!(inv.lines[0].vat_amount, 4.2);
        assert_eq!(inv.total_vat, 200.2, "VAT from taxTotals must be captured");
        assert_eq!(inv.net_amount, 1430.0);
        assert_eq!(inv.grand_total, 1630.2);
    }

    #[test]
    fn wrapper_status_cancelled_not_overridden_by_validation_results() {
        // ETA portal downloads carry the authoritative top-level <status> (here
        // Cancelled) plus a <validationResults> block whose <status> reflects the
        // ORIGINAL validation outcome (Valid). The validation block used to win
        // because it came last; it must not override the wrapper state.
        let xml = r#"<document><uuid>BSHTQNS7J3X6RYRF0SCH8DEK10</uuid>
<internalId>165</internalId>
<issuerId>699633362</issuerId><issuerName>S</issuerName>
<receiverId>100489095</receiverId><receiverName>R</receiverName>
<dateTimeIssued>2026-01-07T22:16:13</dateTimeIssued>
<netAmount>60000</netAmount><total>68400</total>
<status>Cancelled</status>
<document>{}</document>
<validationResults><status>Valid</status><validationSteps><name>Step-03.ITIDA Signature Validator</name><status>Valid</status></validationSteps></validationResults>
</document>"#;
        let inv = parse_eta_xml(xml).unwrap();
        assert_eq!(inv.doc_status, "Cancelled");
    }

    #[test]
    fn matches_shared_invoice_across_entries() {
        // Two services reference the same invoice id (different VAT rates).
        // Both entries must match and totals must be validated against the sum.
        let form_json = r#"{
            "doc_type": "import",
            "buyer_tax_id": "100489095",
            "seller_tax_ids": ["721067026"],
            "invoices": [{"invoice_no": "0205 ADT", "seller_tax_id": "721067026", "amount": "26250"}],
            "import_entries": [
                {"service_name": "Customs A Inv: 0205 ADT", "amount": "10000", "rate": "1", "vat_rate": "14%", "wht_rate": "3%", "free_wht": false},
                {"service_name": "Customs B Inv: 0205 ADT", "amount": "13500", "rate": "1", "vat_rate": "10%", "wht_rate": "0%", "free_wht": true}
            ]
        }"#;
        let xml = r#"<document>
  <internalId>0205 ADT</internalId>
  <issuerId>721067026</issuerId>
  <receiverId>100489095</receiverId>
  <netAmount>23500</netAmount>
  <total>26250</total>
  <document>
    {"invoiceLines":[
      {"description":"A","quantity":1,"netTotal":10000,"taxableItems":[
        {"taxType":"T2","amount":1400,"subType":"Tbl01","rate":14},
        {"taxType":"T4","amount":300,"subType":"W004","rate":3}]},
      {"description":"B","quantity":1,"netTotal":13500,"taxableItems":[
        {"taxType":"T2","amount":1350,"subType":"Tbl01","rate":10}]}],
     "taxTotals":[{"taxType":"T2","amount":2750},{"taxType":"T4","amount":300}],
     "netAmount":23500,"totalAmount":26250}
  </document>
</document>"#;
        let inv = parse_eta_xml(xml).unwrap();
        assert_eq!(inv.net_amount, 23500.0);
        assert_eq!(inv.total_vat, 2750.0);
        assert_eq!(inv.total_wht, 300.0);
        let res = validate_eta_against_form(&inv, form_json).unwrap();
        assert_eq!(res.matched_entry_indices, vec![0, 1], "both entries reference invoice 0205 ADT");
        assert!(res.is_valid, "totals match the sum of both entries: {:#?}", res.issues);
    }

    #[test]
    fn bank_compares_usd_invoice_in_sold_currency() {
        // A USD invoice (sold at 50 EGP/USD) attached to a Bank document must be
        // compared in USD, ignoring any import rate on the form.
        let xml = r#"<document>
  <internalId>T-USD-1</internalId>
  <issuerId>111111111</issuerId><receiverId>222222222</receiverId>
  <netAmount>5000</netAmount><total>5700</total>
  <document>
    {"invoiceLines":[{"description":"A","quantity":1,
       "unitValue":{"currencySold":"USD","currencyExchangeRate":50,"amountSold":100,"amountEGP":5000}}],
     "taxTotals":[{"taxType":"T1","amount":700}],
     "netAmount":5000,"totalAmount":5700}
  </document>
</document>"#;
        let inv = parse_eta_xml(xml).unwrap();
        assert_eq!(inv.currency, "USD");
        assert_eq!(inv.net_amount, 100.0);
        assert_eq!(inv.grand_total, 114.0);

        // Bank form stores the invoice amount in its own (USD) currency; the
        // import rate present on the form must NOT be applied.
        let bank_form = r#"{
            "doc_type": "bank",
            "import_commercial_rate": "50",
            "invoices": [{"invoice_no": "T-USD-1", "seller_tax_id": "111111111", "amount": "100"}]
        }"#;
        let res = validate_eta_against_form(&inv, bank_form).unwrap();
        assert!(res.is_valid, "USD invoice should match in sold currency: {:#?}", res.issues);

        // Import form still converts with the document rate.
        let import_form = r#"{
            "doc_type": "import",
            "import_commercial_rate": "50",
            "import_entries": [
                {"service_name": "Service Inv: T-USD-1", "amount": "100", "rate": "50", "vat_rate": "14%", "wht_rate": "0%", "free_wht": true}
            ]
        }"#;
        let res2 = validate_eta_against_form(&inv, import_form).unwrap();
        assert!(res2.is_valid, "import doc should convert to EGP at the document rate: {:#?}", res2.issues);
    }

    #[test]
    fn matches_line_split_across_services() {
        // One XML line of 2600 is split into two services (1000 + 1600).
        // Positional line checks must not fire; the aggregate must pass.
        let form_json = r#"{
            "doc_type": "import",
            "buyer_tax_id": "100489095",
            "seller_tax_ids": ["721067026"],
            "invoices": [{"invoice_no": "0205 ADT", "seller_tax_id": "721067026", "amount": "2964"}],
            "import_entries": [
                {"service_name": "Clearing A Inv: 0205 ADT", "amount": "1000", "rate": "1", "vat_rate": "14%", "wht_rate": "3%", "free_wht": false},
                {"service_name": "Clearing B Inv: 0205 ADT", "amount": "1600", "rate": "1", "vat_rate": "14%", "wht_rate": "3%", "free_wht": false}
            ]
        }"#;
        let xml = r#"<document>
  <internalId>0205 ADT</internalId>
  <issuerId>721067026</issuerId>
  <receiverId>100489095</receiverId>
  <netAmount>2600</netAmount>
  <total>2964</total>
  <document>
    {"invoiceLines":[{"description":"Clearing","quantity":1,"netTotal":2600,"taxableItems":[
      {"taxType":"T2","amount":364,"subType":"Tbl01","rate":14},
      {"taxType":"T4","amount":78,"subType":"W004","rate":3}]}],
     "taxTotals":[{"taxType":"T2","amount":364},{"taxType":"T4","amount":78}],
     "netAmount":2600,"totalAmount":2964}
  </document>
</document>"#;
        let inv = parse_eta_xml(xml).unwrap();
        assert_eq!(inv.net_amount, 2600.0);
        assert_eq!(inv.total_vat, 364.0);
        assert_eq!(inv.total_wht, 78.0);
        let res = validate_eta_against_form(&inv, form_json).unwrap();
        assert_eq!(res.matched_entry_indices, vec![0, 1]);
        assert!(res.is_valid, "split line must validate against the sum: {:#?}", res.issues);
    }

    #[test]
    fn parses_mixed_vat_no_wht() {
        // Invoice with T1 (14%) and T2 Tbl01 (10%) taxes, NO W-prefixed subtypes,
        // and genuinely no withholding. Everything not W is VAT-like, so WHT must be 0
        // and VAT must include the T2 table tax.
        let xml = r#"<document>
  <internalId>A0910185</internalId>
  <issuerId>200201336</issuerId>
  <receiverId>100489095</receiverId>
  <netAmount>80252</netAmount>
  <total>90427.68</total>
  <document>
    {"invoiceLines":[
      {"description":"Delivery via Truck","quantity":1,"unitValue":{"amountEGP":51162},"netTotal":51162,"taxableItems":[{"taxType":"T1","rate":14,"amount":7162.68,"subType":"V009"}]},
      {"description":"Cargo Reporting Fee","quantity":1,"unitValue":{"amountEGP":2600},"netTotal":2600,"taxableItems":[
        {"taxType":"T2","rate":10,"amount":260,"subType":"Tbl01"},
        {"taxType":"T1","rate":0,"amount":0,"subType":"V009"}]},
      {"description":"Import Temporary Clearance","quantity":1,"unitValue":{"amountEGP":2600},"netTotal":2600,"taxableItems":[
        {"taxType":"T2","rate":10,"amount":260,"subType":"Tbl01"},
        {"taxType":"T1","rate":0,"amount":0,"subType":"V009"}]},
      {"description":"Fiscal Representation","quantity":1,"unitValue":{"amountEGP":10000},"netTotal":10000,"taxableItems":[
        {"taxType":"T2","rate":10,"amount":1000,"subType":"Tbl01"},
        {"taxType":"T1","rate":0,"amount":0,"subType":"V009"}]},
      {"description":"Delivery Order Change","quantity":1,"unitValue":{"amountEGP":2600},"netTotal":2600,"taxableItems":[{"taxType":"T1","rate":14,"amount":364,"subType":"V009"}]},
      {"description":"Import Customs Clearance","quantity":1,"unitValue":{"amountEGP":8690},"netTotal":8690,"taxableItems":[
        {"taxType":"T2","rate":10,"amount":869,"subType":"Tbl01"},
        {"taxType":"T1","rate":0,"amount":0,"subType":"V009"}]},
      {"description":"Customs Document Issuing","quantity":1,"unitValue":{"amountEGP":2600},"netTotal":2600,"taxableItems":[
        {"taxType":"T2","rate":10,"amount":260,"subType":"Tbl01"},
        {"taxType":"T1","rate":0,"amount":0,"subType":"V009"}]}],
     "taxTotals":[{"taxType":"T1","amount":7526.68},{"taxType":"T2","amount":2649}],
     "netAmount":80252,"totalAmount":90427.68}
  </document>
</document>"#;
        let inv = parse_eta_xml(xml).unwrap();
        assert_eq!(inv.invoice_id, "A0910185");
        assert_eq!(inv.net_amount, 80252.0);
        assert_eq!(inv.grand_total, 90427.68);
        assert_eq!(inv.total_vat, 10175.68, "VAT must include T1 + T2 table tax (no W subtypes)");
        assert_eq!(inv.total_wht, 0.0, "no W-prefixed subtype => no withholding");
        assert_eq!(inv.lines.len(), 7);
        assert_eq!(inv.lines[1].vat_rate, 10.0, "T2 10% must win over zero-rated T1");
        assert_eq!(inv.lines[1].vat_amount, 260.0);
    }

    #[test]
    fn parses_wht_by_subtype() {
        let xml = r#"<document>
  <internalId>0205 ADT</internalId>
  <issuerId>721067026</issuerId>
  <receiverId>100489095</receiverId>
  <netAmount>23500</netAmount>
  <total>25145</total>
  <document>
    {"invoiceLines":[{"description":"Customs Clearance","quantity":1,"netTotal":23500,"taxableItems":[
      {"taxType":"T2","amount":2350,"subType":"Tbl01","rate":10},
      {"taxType":"T4","amount":705,"subType":"W004","rate":3}]}],
     "taxTotals":[{"taxType":"T2","amount":2350},{"taxType":"T4","amount":705}],
     "netAmount":23500,"totalAmount":25145}
  </document>
</document>"#;
        let inv = parse_eta_xml(xml).unwrap();
        assert_eq!(inv.invoice_id, "0205 ADT");
        assert_eq!(inv.net_amount, 23500.0);
        assert_eq!(inv.grand_total, 25145.0);
        assert_eq!(inv.total_wht, 705.0, "WHT should be only the W-subtype entry (T4)");
        assert_eq!(inv.total_vat, 2350.0, "Tbl01 (T2) is an added tax, not WHT");
        assert_eq!(inv.lines.len(), 1);
        assert_eq!(inv.lines[0].vat_rate, 10.0);
        assert_eq!(inv.lines[0].vat_amount, 2350.0);
    }

    #[test]
    fn converts_usd_invoice_to_sold_currency() {
        // ETA reports amounts in EGP converted at the invoice exchange rate; the
        // actual bill is in USD (amountSold). Totals must be converted to USD.
        let xml = r#"<document>
  <internalId>EGSOKAM260000812</internalId>
  <issuerId>202487288</issuerId>
  <receiverId>100489095</receiverId>
  <netAmount>17933.19</netAmount>
  <total>20443.84</total>
  <document>
    {"invoiceLines":[{"description":"Import shipment services","quantity":1,
      "unitValue":{"currencySold":"USD","amountEGP":17933.19,"amountSold":341,"currencyExchangeRate":52.59},
      "netTotal":17933.19,"taxableItems":[{"taxType":"T1","amount":2510.65,"subType":"V009","rate":14}]}],
     "taxTotals":[{"taxType":"T1","amount":2510.65}],
     "netAmount":17933.19,"totalAmount":20443.84}
  </document>
</document>"#;
        let inv = parse_eta_xml(xml).unwrap();
        assert_eq!(inv.currency, "USD");
        assert_eq!(inv.net_amount, 341.0, "net converted from EGP 17933.19 / 52.59");
        assert_eq!(inv.total_vat, 47.74, "VAT converted from EGP 2510.65 / 52.59");
        assert_eq!(inv.grand_total, 388.74, "total converted from EGP 20443.84 / 52.59");
        assert_eq!(inv.lines.len(), 1);
        assert_eq!(inv.lines[0].line_total, 341.0);
        assert_eq!(inv.lines[0].unit_price, 341.0);
        assert_eq!(inv.lines[0].vat_amount, 47.74);
    }

    #[test]
    fn derives_exchange_rate_when_xml_omits_it() {
        // Some portal exports set currencySold but omit currencyExchangeRate,
        // which used to leave EGP totals under a "USD" label. The rate is derived
        // from amountEGP / amountSold so the totals convert correctly.
        let xml = r#"<document>
  <internalId>EGALM26024</internalId>
  <issuerId>91310115662480226X</issuerId>
  <netAmount>17933.19</netAmount>
  <total>20443.84</total>
  <document>
    {"invoiceLines":[{"description":"Import shipment services","quantity":1,
      "unitValue":{"currencySold":"USD","amountEGP":17933.19,"amountSold":341},
      "netTotal":17933.19,"taxableItems":[{"taxType":"T1","amount":2510.65,"subType":"V009","rate":14}]}],
     "taxTotals":[{"taxType":"T1","amount":2510.65}],
     "netAmount":17933.19,"totalAmount":20443.84}
  </document>
</document>"#;
        let inv = parse_eta_xml(xml).unwrap();
        assert_eq!(inv.currency, "USD");
        assert!((inv.exchange_rate - 52.59).abs() < 0.01, "derived rate: {}", inv.exchange_rate);
        assert!((inv.net_amount - 341.0).abs() < 0.01, "net in sold currency: {}", inv.net_amount);
        assert!((inv.grand_total - 388.74).abs() < 0.05, "total in sold currency: {}", inv.grand_total);
    }

    #[test]
    fn leaves_egp_invoice_unchanged() {
        let xml = r#"<document>
  <internalId>EGSOC000000001</internalId>
  <netAmount>1000</netAmount>
  <total>1140</total>
  <document>
    {"invoiceLines":[{"description":"Service","quantity":1,
      "unitValue":{"currencySold":"EGP","amountEGP":1000,"amountSold":1000,"currencyExchangeRate":1},
      "netTotal":1000,"taxableItems":[{"taxType":"T1","amount":140,"subType":"V009","rate":14}]}],
     "taxTotals":[{"taxType":"T1","amount":140}],
     "netAmount":1000,"totalAmount":1140}
  </document>
</document>"#;
        let inv = parse_eta_xml(xml).unwrap();
        assert_eq!(inv.currency, "EGP");
        assert_eq!(inv.net_amount, 1000.0);
        assert_eq!(inv.total_vat, 140.0);
        assert_eq!(inv.grand_total, 1140.0);
        assert_eq!(inv.lines[0].line_total, 1000.0);
    }

    #[test]
    fn validates_usd_invoice_in_egp_against_form_rate() {
        // USD invoice (341 net, 47.74 VAT). The import doc is filled in EGP using
        // import_commercial_rate. Validation must convert the invoice back to EGP
        // at that rate before comparing.
        let xml = r#"<document>
  <internalId>EGSOKAM260000812</internalId>
  <issuerId>202487288</issuerId>
  <receiverId>100489095</receiverId>
  <netAmount>17933.19</netAmount>
  <total>20443.84</total>
  <document>
    {"invoiceLines":[{"description":"Import shipment services","quantity":1,
      "unitValue":{"currencySold":"USD","amountEGP":17933.19,"amountSold":341,"currencyExchangeRate":52.59},
      "netTotal":17933.19,"taxableItems":[{"taxType":"T1","amount":2510.65,"subType":"V009","rate":14}]}],
     "taxTotals":[{"taxType":"T1","amount":2510.65}],
     "netAmount":17933.19,"totalAmount":20443.84}
  </document>
</document>"#;
        let inv = parse_eta_xml(xml).unwrap();
        assert_eq!(inv.currency, "USD");

        // Form is in EGP. Rate 54.66 typed on the import doc. 341 * 54.66 = 18639.06.
        let form = r#"{
            "doc_type": "import",
            "buyer_tax_id": "100489095",
            "seller_tax_ids": ["202487288"],
            "import_commercial_rate": "54.66",
            "import_total_vat": "2609.47",
            "import_total_wht": "0",
            "invoices": [{"invoice_no": "EGSOKAM260000812", "amount": "18639.06"}],
            "import_entries": [
                {"service_name": "MSC Import Inv: EGSOKAM260000812", "amount": "341", "rate": "54.66", "vat_rate": "14%", "wht_rate": "0%", "free_wht": false}
            ]
        }"#;
        let res = validate_eta_against_form(&inv, form).unwrap();
        assert!(res.is_valid, "should pass after EGP conversion: {:#?}", res.issues);
    }

    #[test]
    fn parses_credit_note_type_and_reference() {
        // Escaped-JSON ETA export (the portal form) carrying a credit note.
        let xml = r#"<document><document>{&#34;internalID&#34;:&#34;CN-1&#34;,&#34;documentType&#34;:&#34;c&#34;,&#34;references&#34;:[&#34;ZNT9AQ7BA0BJB5T3VZEGYEYK10&#34;],&#34;netAmount&#34;:100,&#34;totalAmount&#34;:100}</document></document>"#;
        let inv = parse_eta_xml(xml).unwrap();
        assert_eq!(inv.invoice_id, "CN-1");
        assert_eq!(inv.invoice_type_code, "C");
        assert_eq!(inv.references, vec!["ZNT9AQ7BA0BJB5T3VZEGYEYK10"]);
    }

    #[test]
    fn parses_plain_xml_document_type_and_references() {
        let xml = r#"<document><document><internalID>DN-1</internalID><documentType>D</documentType><references><reference>ABC12345678901234567890123</reference></references><invoiceLines><invoiceLine><description>x</description><quantity>1</quantity><unitValue><currencySold>EGP</currencySold><amountEGP>10</amountEGP></unitValue><netTotal>10</netTotal></invoiceLine></invoiceLines><netAmount>10</netAmount><totalAmount>10</totalAmount></document></document>"#;
        let inv = parse_eta_xml(xml).unwrap();
        assert_eq!(inv.invoice_type_code, "D");
        assert_eq!(inv.references, vec!["ABC12345678901234567890123"]);
    }

    #[test]
    fn meta_defaults_to_invoice_without_document_type() {
        let (kind, refs) = parse_eta_meta("<document><internalID>X</internalID></document>");
        assert_eq!(kind, "I");
        assert!(refs.is_empty());
    }
}
