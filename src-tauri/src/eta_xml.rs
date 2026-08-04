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
    pub currency: String,
    pub net_amount: f64,
    pub total_vat: f64,
    pub total_wht: f64,
    pub grand_total: f64,
    pub lines: Vec<EtaInvoiceLine>,
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
}

fn parse_f64(s: &str) -> f64 {
    s.trim().replace(',', "").parse().unwrap_or(0.0)
}

fn normalize_id(s: &str) -> String {
    s.trim().to_uppercase().chars().filter(|c| !c.is_whitespace()).collect()
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

pub fn parse_eta_xml(xml_content: &str) -> Result<EtaInvoice, String> {
    let mut reader = Reader::from_str(xml_content);
    reader.config_mut().trim_text(true);

    let mut invoice = EtaInvoice {
        invoice_id: String::new(),
        uuid: String::new(),
        issue_date: String::new(),
        invoice_type_code: String::new(),
        seller_tax_id: String::new(),
        seller_name: String::new(),
        buyer_tax_id: String::new(),
        buyer_name: String::new(),
        currency: "EGP".to_string(),
        net_amount: 0.0,
        total_vat: 0.0,
        total_wht: 0.0,
        grand_total: 0.0,
        lines: Vec::new(),
    };

    let mut current_text = String::new();
    let mut in_document_json = false;
    let mut document_json = String::new();
    let mut doc_depth = 0;

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

    // Parse the embedded JSON for line items and tax totals
    if !document_json.is_empty() {
        // Fix HTML entities first
        let json_str = document_json.replace("&#34;", "\"");
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&json_str) {
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
                    let unit_val = line.get("unitValue").and_then(|v| v.get("amountEGP")).and_then(|v| v.as_f64())
                        .or_else(|| line.get("unitValue").and_then(|v| v.get("amountSold")).and_then(|v| v.as_f64()))
                        .unwrap_or(0.0);
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
            // Identity fields may only exist in the embedded JSON
            if invoice.invoice_id.is_empty() {
                invoice.invoice_id = parsed.get("internalId").and_then(|v| v.as_str()).unwrap_or("").to_string();
            }
            if invoice.uuid.is_empty() {
                invoice.uuid = parsed.get("uuid").and_then(|v| v.as_str()).unwrap_or("").to_string();
            }
            if invoice.seller_tax_id.is_empty() {
                invoice.seller_tax_id = parsed.get("issuerId").and_then(|v| v.as_str()).unwrap_or("").to_string();
            }
            if invoice.seller_name.is_empty() {
                invoice.seller_name = parsed.get("issuerName").and_then(|v| v.as_str()).unwrap_or("").to_string();
            }
            if invoice.buyer_tax_id.is_empty() {
                invoice.buyer_tax_id = parsed.get("receiverId").and_then(|v| v.as_str()).unwrap_or("").to_string();
            }
            if invoice.buyer_name.is_empty() {
                invoice.buyer_name = parsed.get("receiverName").and_then(|v| v.as_str()).unwrap_or("").to_string();
            }
            if invoice.issue_date.is_empty() {
                invoice.issue_date = parsed.get("dateTimeIssued").and_then(|v| v.as_str()).unwrap_or("").to_string();
            }
        }
    }

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
                if invoice.net_amount > 0.0 && inv_amt > 0.0 {
                    // Form amount could be net or gross — check both
                    let diff_net = (invoice.net_amount - inv_amt).abs();
                    let diff_gross = (invoice.grand_total - inv_amt).abs();
                    if diff_net > 0.5 && diff_gross > 0.5 {
                        issues.push(ValidationIssue {
                            field: "Invoice Amount".into(),
                            xml_value: format!("Net: {:.2} / Gross: {:.2}", invoice.net_amount, invoice.grand_total),
                            form_value: format!("{:.2}", inv_amt),
                            severity: "error".into(),
                            message: format!("Invoice amount differs — XML net {:.2} / gross {:.2} vs form {:.2}", invoice.net_amount, invoice.grand_total, inv_amt),
                        });
                    }
                }

                // Check VAT amount
                if invoice.total_vat > 0.0 {
                    let form_vat_total = form.get("import_total_vat").and_then(|v| v.as_str())
                        .and_then(|s| s.replace(',', "").parse::<f64>().ok())
                        .unwrap_or(0.0);
                    if form_vat_total > 0.0 {
                        let diff = (invoice.total_vat - form_vat_total).abs();
                        if diff > 0.5 {
                            issues.push(ValidationIssue {
                                field: "Total VAT".into(),
                                xml_value: format!("{:.2}", invoice.total_vat),
                                form_value: format!("{:.2}", form_vat_total),
                                severity: "error".into(),
                                message: format!("Total VAT differs by {:.2}", diff),
                            });
                        }
                    }
                }

                // Check WHT amount
                if invoice.total_wht > 0.0 {
                    let form_wht_total = form.get("import_total_wht").and_then(|v| v.as_str())
                        .and_then(|s| s.replace(',', "").parse::<f64>().ok())
                        .unwrap_or(0.0);
                    if form_wht_total > 0.0 {
                        let diff = (invoice.total_wht - form_wht_total).abs();
                        if diff > 0.5 {
                            issues.push(ValidationIssue {
                                field: "Total WHT".into(),
                                xml_value: format!("{:.2}", invoice.total_wht),
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
                if invoice.net_amount > 0.0 && form_total > 0.0 {
                    let diff = (invoice.net_amount - form_total).abs();
                    if diff > 0.5 {
                        issues.push(ValidationIssue {
                            field: "Total Net Amount".into(),
                            xml_value: format!("{:.2}", invoice.net_amount),
                            form_value: format!("{:.2}", form_total),
                            severity: "error".into(),
                            message: format!("Total net amount differs by {:.2} — sum of matching services is {:.2}", diff, form_total),
                        });
                    }
                }

                // Check total VAT
                if invoice.total_vat > 0.0 && form_vat_total > 0.0 {
                    let diff = (invoice.total_vat - form_vat_total).abs();
                    if diff > 0.5 {
                        issues.push(ValidationIssue {
                            field: "Total VAT".into(),
                            xml_value: format!("{:.2}", invoice.total_vat),
                            form_value: format!("{:.2}", form_vat_total),
                            severity: "error".into(),
                            message: format!("Total VAT differs by {:.2} — expected from matching services", diff),
                        });
                    }
                }

                // Check total WHT
                if invoice.total_wht > 0.0 && form_wht_total > 0.0 {
                    let diff = (invoice.total_wht - form_wht_total).abs();
                    if diff > 0.5 {
                        issues.push(ValidationIssue {
                            field: "Total WHT".into(),
                            xml_value: format!("{:.2}", invoice.total_wht),
                            form_value: format!("{:.2}", form_wht_total),
                            severity: "error".into(),
                            message: format!("Total WHT differs by {:.2} — expected from matching services", diff),
                        });
                    }
                } else if invoice.total_wht > 0.0 && form_wht_total == 0.0 {
                    issues.push(ValidationIssue {
                        field: "Total WHT".into(),
                        xml_value: format!("{:.2}", invoice.total_wht),
                        form_value: "0.00".into(),
                        severity: "warning".into(),
                        message: "XML invoice has WHT but matching services have none (0% or free_wht)".into(),
                    });
                } else if form_wht_total > 0.0 && invoice.total_wht == 0.0 {
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
                        if xml_line.line_total > 0.0 && egp > 0.0 && (xml_line.line_total - egp).abs() > 0.5 {
                            issues.push(ValidationIssue {
                                field: format!("Line {} Amount", i + 1),
                                xml_value: format!("{:.2}", xml_line.line_total),
                                form_value: format!("{:.2}", egp),
                                severity: "error".into(),
                                message: format!("Line amount differs by {:.2} (form amount × rate)", (xml_line.line_total - egp).abs()),
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
                if inv_amt > 0.0 && invoice.grand_total > 0.0 {
                    let diff = (invoice.grand_total - inv_amt).abs();
                    if diff > 0.5 {
                        issues.push(ValidationIssue {
                            field: "Commercial Invoice Amount".into(),
                            xml_value: format!("{:.2}", invoice.grand_total),
                            form_value: format!("{:.2}", inv_amt),
                            severity: "error".into(),
                            message: format!("Commercial invoice amount differs by {:.2}", diff),
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
}
