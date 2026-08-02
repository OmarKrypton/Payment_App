use serde::{Deserialize, Serialize};
use quick_xml::Reader;
use quick_xml::events::Event;

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
}

fn strip_ns(tag: &str) -> &str {
    if let Some(pos) = tag.rfind(':') {
        &tag[pos + 1..]
    } else {
        tag
    }
}

fn parse_f64(s: &str) -> f64 {
    s.trim().replace(',', "").parse().unwrap_or(0.0)
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
        currency: String::new(),
        net_amount: 0.0,
        total_vat: 0.0,
        total_wht: 0.0,
        grand_total: 0.0,
        lines: Vec::new(),
    };

    // State machine for parsing
    let mut current_text = String::new();
    let mut in_supplier = false;
    let mut in_customer = false;
    let mut _in_tax_total = false;
    let mut in_tax_subtotal = false;
    let mut in_monetary_total = false;
    let mut in_invoice_line = false;
    let mut current_line = EtaInvoiceLine {
        description: String::new(),
        quantity: 0.0,
        unit_price: 0.0,
        line_total: 0.0,
        vat_rate: 0.0,
        vat_amount: 0.0,
        item_code: String::new(),
    };
    let mut tax_subtotal_count = 0;

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                let local = strip_ns(&tag).to_string();
                current_text.clear();

                match local.as_str() {
                    "AccountingSupplierParty" => in_supplier = true,
                    "AccountingCustomerParty" => in_customer = true,
                    "TaxTotal" => {
                        if !in_invoice_line {
                            _in_tax_total = true;
                            tax_subtotal_count = 0;
                        }
                    }
                    "TaxSubtotal" => {
                        in_tax_subtotal = true;
                        tax_subtotal_count += 1;
                    }
                    "LegalMonetaryTotal" => in_monetary_total = true,
                    "InvoiceLine" => {
                        in_invoice_line = true;
                        current_line = EtaInvoiceLine {
                            description: String::new(),
                            quantity: 0.0,
                            unit_price: 0.0,
                            line_total: 0.0,
                            vat_rate: 0.0,
                            vat_amount: 0.0,
                            item_code: String::new(),
                        };
                    }
                    _ => {}
                }
            }
            Ok(Event::Empty(_)) => {
                // Self-closing tags handled where needed
            }
            Ok(Event::Text(e)) => {
                current_text = e.unescape().map_err(|e| e.to_string())?.to_string();
            }
            Ok(Event::End(e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                let local = strip_ns(&tag).to_string();

                let text = current_text.trim().to_string();

                match local.as_str() {
                    "ID" => {
                        if in_supplier && invoice.seller_tax_id.is_empty() {
                            invoice.seller_tax_id = text.clone();
                        } else if in_customer && invoice.buyer_tax_id.is_empty() {
                            invoice.buyer_tax_id = text.clone();
                        } else if !in_supplier && !in_customer && !in_invoice_line {
                            // Top-level invoice ID
                            if invoice.invoice_id.is_empty() {
                                invoice.invoice_id = text.clone();
                            } else if invoice.uuid.is_empty() {
                                invoice.uuid = text.clone();
                            }
                        } else if in_invoice_line && current_line.item_code.is_empty() {
                            current_line.item_code = text.clone();
                        }
                    }
                    "UUID" => {
                        if !in_supplier && !in_customer && !in_invoice_line {
                            invoice.uuid = text;
                        }
                    }
                    "IssueDate" => {
                        invoice.issue_date = text;
                    }
                    "InvoiceTypeCode" => {
                        invoice.invoice_type_code = text;
                    }
                    "Name" => {
                        if in_supplier {
                            invoice.seller_name = text;
                        } else if in_customer {
                            invoice.buyer_name = text;
                        }
                    }
                    "Description" => {
                        if in_invoice_line && current_line.description.is_empty() {
                            current_line.description = text;
                        }
                    }
                    "InvoicedQuantity" | "InvoicedQuantityUnit" | "Quantity" => {
                        if in_invoice_line {
                            current_line.quantity = parse_f64(&text);
                        }
                    }
                    "PriceAmount" => {
                        if in_invoice_line {
                            current_line.unit_price = parse_f64(&text);
                        }
                    }
                    "LineExtensionAmount" => {
                        if in_invoice_line {
                            current_line.line_total = parse_f64(&text);
                        } else if in_monetary_total {
                            invoice.net_amount = parse_f64(&text);
                        }
                    }
                    "TaxExclusiveAmount" => {
                        if in_monetary_total {
                            invoice.net_amount = parse_f64(&text);
                        }
                    }
                    "TaxInclusiveAmount" => {
                        if in_monetary_total {
                            invoice.grand_total = parse_f64(&text);
                        }
                    }
                    "TaxAmount" => {
                        if in_tax_subtotal && !in_invoice_line {
                            if tax_subtotal_count == 1 {
                                // First subtotal is usually VAT
                                invoice.total_vat = parse_f64(&text);
                            } else if tax_subtotal_count == 2 {
                                // Second subtotal is usually WHT
                                invoice.total_wht = parse_f64(&text);
                            }
                        } else if in_invoice_line && in_tax_subtotal {
                            current_line.vat_amount = parse_f64(&text);
                        }
                    }
                    "TaxableAmount" => {
                        if in_invoice_line && in_tax_subtotal {
                            // Already captured in LineExtensionAmount
                        }
                    }
                    "Percent" => {
                        if in_tax_subtotal {
                            if in_invoice_line {
                                current_line.vat_rate = parse_f64(&text);
                            }
                            // For top-level, we could track VAT rate too
                        }
                    }
                    "DocumentCurrencyCode" => {
                        invoice.currency = text;
                    }
                    _ => {}
                }

                // Track section exits
                match local.as_str() {
                    "AccountingSupplierParty" => in_supplier = false,
                    "AccountingCustomerParty" => in_customer = false,
                    "TaxTotal" => {
                        if !in_invoice_line {
                            _in_tax_total = false;
                            tax_subtotal_count = 0;
                        } else {
                            // Line-level tax total
                        }
                    }
                    "TaxSubtotal" => in_tax_subtotal = false,
                    "LegalMonetaryTotal" => in_monetary_total = false,
                    "InvoiceLine" => {
                        invoice.lines.push(current_line.clone());
                        in_invoice_line = false;
                    }
                    _ => {}
                }
                current_text.clear();
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("XML parse error at position {}: {:?}", reader.error_position(), e)),
            _ => {}
        }
    }

    // If grand_total is 0 but net_amount + vat > 0, compute it
    if invoice.grand_total == 0.0 && invoice.net_amount > 0.0 {
        invoice.grand_total = invoice.net_amount + invoice.total_vat - invoice.total_wht;
    }

    Ok(invoice)
}

pub fn validate_eta_against_form(invoice: &EtaInvoice, form_json: &str) -> Result<ValidationResult, String> {
    let mut issues: Vec<ValidationIssue> = Vec::new();

    let form: serde_json::Value = serde_json::from_str(form_json)
        .map_err(|e| format!("Invalid form JSON: {}", e))?;

    // Helper to get form field as string
    let get_str = |key: &str| -> String {
        form.get(key).and_then(|v| v.as_str()).unwrap_or("").to_string()
    };

    // 1. Compare buyer tax ID
    let form_buyer = get_str("buyer_tax_id");
    if !form_buyer.is_empty() && !invoice.buyer_tax_id.is_empty() {
        if form_buyer != invoice.buyer_tax_id {
            issues.push(ValidationIssue {
                field: "Buyer Tax ID".into(),
                xml_value: invoice.buyer_tax_id.clone(),
                form_value: form_buyer,
                severity: "error".into(),
                message: "Buyer tax ID does not match the XML invoice".into(),
            });
        }
    }

    // 2. Compare seller tax IDs
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

    // 3. Check invoice amounts against import entries
    let import_entries = form.get("import_entries").and_then(|v| v.as_array());
    if let Some(entries) = import_entries {
        // For bank type, check commercial invoice amounts
        let doc_type = get_str("doc_type");
        if doc_type == "import" {
            // Check if total from XML matches total of import entries
            let xml_total = invoice.net_amount;
            let mut form_total = 0.0;
            for entry in entries {
                let amt = entry.get("amount").and_then(|v| v.as_str())
                    .and_then(|s| s.replace(',', "").parse::<f64>().ok())
                    .unwrap_or(0.0);
                let rate = entry.get("rate").and_then(|v| v.as_str())
                    .and_then(|s| s.replace(',', "").parse::<f64>().ok())
                    .unwrap_or(1.0);
                form_total += amt * rate;
            }
            let diff = (xml_total - form_total).abs();
            if diff > 0.5 {
                issues.push(ValidationIssue {
                    field: "Total Amount".into(),
                    xml_value: format!("{:.2}", xml_total),
                    form_value: format!("{:.2}", form_total),
                    severity: "error".into(),
                    message: format!("Total amount differs by {:.2}", diff),
                });
            }
        }
    }

    // 4. Check commercial invoices (bank type)
    let invoices = form.get("invoices").and_then(|v| v.as_array());
    if let Some(inv_list) = invoices {
        // Check if the XML invoice ID matches any invoice in the form
        let form_inv_ids: Vec<String> = inv_list.iter()
            .filter_map(|inv| inv.get("invoice_no").and_then(|v| v.as_str()).map(|s| s.to_string()))
            .collect();
        if !form_inv_ids.is_empty() && !invoice.invoice_id.is_empty() {
            if !form_inv_ids.iter().any(|id| id == &invoice.invoice_id) {
                issues.push(ValidationIssue {
                    field: "Invoice Number".into(),
                    xml_value: invoice.invoice_id.clone(),
                    form_value: form_inv_ids.join(", "),
                    severity: "warning".into(),
                    message: "Invoice number in XML does not match any invoice number in the form".into(),
                });
            }
        }

        // Check individual invoice amounts
        for inv in inv_list {
            let inv_no = inv.get("invoice_no").and_then(|v| v.as_str()).unwrap_or("");
            let inv_amt = inv.get("amount").and_then(|v| v.as_str())
                .and_then(|s| s.replace(',', "").parse::<f64>().ok())
                .unwrap_or(0.0);
            if inv_no == invoice.invoice_id && inv_amt > 0.0 {
                let diff = (invoice.grand_total - inv_amt).abs();
                if diff > 0.5 {
                    issues.push(ValidationIssue {
                        field: format!("Invoice {} Amount", inv_no),
                        xml_value: format!("{:.2}", invoice.grand_total),
                        form_value: format!("{:.2}", inv_amt),
                        severity: "error".into(),
                        message: format!("Invoice amount differs by {:.2}", diff),
                    });
                }
            }
        }
    }

    // 5. Check VAT rate
    let import_entries = form.get("import_entries").and_then(|v| v.as_array());
    if let Some(entries) = import_entries {
        for (i, entry) in entries.iter().enumerate() {
            let line_idx = if i < invoice.lines.len() { Some(i) } else { None };
            if let Some(li) = line_idx {
                let xml_line = &invoice.lines[li];
                let form_vat = entry.get("vat_rate").and_then(|v| v.as_str()).unwrap_or("0%");
                let form_vat_pct = form_vat.trim_end_matches('%').parse::<f64>().unwrap_or(0.0);
                if xml_line.vat_rate > 0.0 && form_vat_pct > 0.0 && (xml_line.vat_rate - form_vat_pct).abs() > 0.5 {
                    issues.push(ValidationIssue {
                        field: format!("Line {} VAT Rate", i + 1),
                        xml_value: format!("{:.0}%", xml_line.vat_rate),
                        form_value: form_vat.to_string(),
                        severity: "warning".into(),
                        message: format!("VAT rate in XML ({:.0}%) differs from form ({})", xml_line.vat_rate, form_vat),
                    });
                }
            }
        }
    }

    // 6. Check for missing data in XML
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

    let is_valid = issues.iter().all(|i| i.severity != "error");

    Ok(ValidationResult {
        invoice: invoice.clone(),
        issues,
        is_valid,
    })
}
