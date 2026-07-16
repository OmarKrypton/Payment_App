use crate::models::{CalcResult, FormData};
use rust_xlsxwriter::*;
use std::collections::BTreeSet;

pub fn export_excel(data: &FormData, computed: &CalcResult, path: &str) -> Result<(), String> {
    let mut workbook = Workbook::new();

    // ── Shared formats ──
    let title_fmt = Format::new()
        .set_font_color(Color::White)
        .set_background_color(Color::RGB(0x00529B))
        .set_bold()
        .set_font_size(14);
    let label_fmt = Format::new()
        .set_font_size(10)
        .set_border(FormatBorder::Thin);
    let val_fmt = Format::new()
        .set_font_size(10)
        .set_num_format("#,##0.00")
        .set_border(FormatBorder::Thin);
    let section_fmt = Format::new()
        .set_bold()
        .set_font_size(10)
        .set_border(FormatBorder::Thin)
        .set_background_color(Color::RGB(0xD9E2F3));
    let calc_fmt = Format::new()
        .set_font_size(10)
        .set_num_format("#,##0.00")
        .set_background_color(Color::RGB(0xE2EFDA))
        .set_bold()
        .set_border(FormatBorder::Thin);

    // ── Sheet 1: Settlement Report ──
    let sheet1 = workbook.add_worksheet();
    sheet1.set_name("Settlement Report").map_err(|e| e.to_string())?;
    sheet1.set_column_width(0, 8).map_err(|e| e.to_string())?;
    sheet1.set_column_width(1, 55).map_err(|e| e.to_string())?;
    sheet1.set_column_width(2, 25).map_err(|e| e.to_string())?;

    // Title row
    sheet1.merge_range(0, 0, 0, 2, "CSCEC - Payment Voucher Settlement", &title_fmt)
        .map_err(|e| e.to_string())?;

    // Document info rows
    let info_fmt = Format::new().set_font_size(10).set_italic();
    sheet1.write_with_format(1, 1, format!("Doc: {}  |  Buyer TAX ID: {}  |  Seller TAX IDs: {}", data.doc_serial, data.buyer_tax_id, data.seller_tax_ids.join(", ")), &info_fmt)
        .map_err(|e| e.to_string())?;

    let sections: Vec<(String, String, &str)> = vec![
        ("1".into(), "Initial accumulative supplier settlement amount".into(), "1A"),
        ("".into(), "Current period supplier settlement without VAT".into(), "1B"),
        ("".into(), "Current period other-payables under contract".into(), "1C"),
        ("".into(), "Current period other-deductions under contract".into(), "1D"),
        ("".into(), "Current period VAT of supplier settlement amount".into(), "1E"),
        ("".into(), "Current period settlement incl. VAT".into(), "1G"),
        ("".into(), "Ending accumulative supplier settlement amount".into(), "1F"),
        ("2".into(), "Advance payment under contract".into(), "2A"),
        ("".into(), "Initial deduction from advance payment".into(), "2B"),
        ("".into(), "Current period deduction from advance payment".into(), "2C"),
        ("".into(), "Ending deduction from advance payment".into(), "2D"),
        ("".into(), "Ending balance of advance payment".into(), "2E"),
        ("3".into(), "Ending accumulative amount payable".into(), "3A"),
        ("4".into(), "Initial balance of retention amount".into(), "4A"),
        ("".into(), "Current period retention money to be deducted".into(), "4B"),
        ("".into(), "Current period return of retention money".into(), "4C"),
        ("".into(), "Ending balance of retention money".into(), "4D"),
        ("5".into(), "Initial balance of temporary labour social insurance".into(), "5A"),
        ("".into(), "Current temporary labour to deduct".into(), "5B"),
        ("".into(), "Current return of temporary labour social insurance".into(), "5C"),
        ("".into(), "Ending balance of temporary labour social insurance".into(), "5D"),
        ("6".into(), "Initial accumulative WHT".into(), "6A"),
        ("".into(), "Current period WHT".into(), "6B"),
        ("".into(), "Ending accumulative WHT".into(), "6C"),
        ("".into(), "Settlement incl. VAT minus WHT".into(), "6D"),
        ("8".into(), "Initial other-deductions".into(), "8A"),
        ("".into(), "Current other-deductions".into(), "8B"),
        ("".into(), "Ending accumulative other-deductions".into(), "8C"),
        ("12".into(), "Initial accumulative contract social insurance".into(), "12A"),
        ("".into(), "Current period contract social insurance".into(), "12B"),
        ("".into(), "Ending accumulative contract social insurance".into(), "12C"),
        ("7".into(), "Initial accumulative reality amount paid".into(), "7A"),
        ("".into(), "Initial total accumulative paid amount".into(), "7B"),
        ("9".into(), "Net amount payable".into(), "9A"),
        ("10".into(), "Current period reality amount paid".into(), "10A"),
        ("11".into(), "Ending accumulative reality amount paid".into(), "11A"),
        ("".into(), "Ending total accumulative amount paid".into(), "11B"),
    ];

    let highlighted: BTreeSet<&str> = [
        "1F", "1G", "2D", "2E", "3A", "4D", "5D", "6C", "6D", "8C", "12C", "9A", "11A", "11B",
    ]
    .into_iter()
    .collect();

    let get_val = |key: &str| -> f64 {
        match key {
            "1A" => computed.c_1A,
            "1B" => computed.c_1B,
            "1C" => computed.c_1C,
            "1D" => computed.c_1D,
            "1E" => computed.c_1E,
            "1F" => computed.c_1F,
            "1G" => computed.c_1G,
            "2A" => computed.c_2A,
            "2B" => computed.c_2B,
            "2C" => computed.c_2C,
            "2D" => computed.c_2D,
            "2E" => computed.c_2E,
            "3A" => computed.c_3A,
            "4A" => computed.c_4A,
            "4B" => computed.c_4B,
            "4C" => computed.c_4C,
            "4D" => computed.c_4D,
            "5A" => computed.c_5A,
            "5B" => computed.c_5B,
            "5C" => computed.c_5C,
            "5D" => computed.c_5D,
            "6A" => computed.c_6A,
            "6B" => computed.c_6B,
            "6C" => computed.c_6C,
            "6D" => computed.c_6D,
            "8A" => computed.c_8A,
            "8B" => computed.c_8B,
            "8C" => computed.c_8C,
            "12A" => computed.c_12A,
            "12B" => computed.c_12B,
            "12C" => computed.c_12C,
            "7A" => computed.c_7A,
            "7B" => computed.c_7B,
            "9A" => computed.c_9A,
            "10A" => computed.c_10A,
            "11A" => computed.c_11A,
            "11B" => computed.c_11B,
            _ => 0.0,
        }
    };

    for (i, (sec, desc, key)) in sections.iter().enumerate() {
        let row = (i + 3) as u32;
        if !sec.is_empty() {
            sheet1
                .write_with_format(row, 0, sec.parse::<i32>().unwrap_or(0), &section_fmt)
                .map_err(|e| e.to_string())?;
        } else {
            sheet1.write_with_format(row, 0, "", &label_fmt).map_err(|e| e.to_string())?;
        }
        sheet1
            .write_with_format(row, 1, desc.as_str(), &label_fmt)
            .map_err(|e| e.to_string())?;
        let v = get_val(key);
        if highlighted.contains(key) {
            sheet1.write_with_format(row, 2, v, &calc_fmt).map_err(|e| e.to_string())?;
        } else {
            sheet1.write_with_format(row, 2, v, &val_fmt).map_err(|e| e.to_string())?;
        }
    }

    // ── Sheet 2: Audit Checklist ──
    let sheet2 = workbook.add_worksheet();
    sheet2.set_name("Audit Checklist").map_err(|e| e.to_string())?;
    sheet2.set_column_width(0, 35).map_err(|e| e.to_string())?;
    sheet2.set_column_width(1, 55).map_err(|e| e.to_string())?;

    let section2_fmt = Format::new()
        .set_bold()
        .set_font_size(11)
        .set_font_color(Color::White)
        .set_background_color(Color::RGB(0x00529B))
        .set_border(FormatBorder::Thin);
    let bold_fmt = Format::new()
        .set_bold()
        .set_font_size(10)
        .set_border(FormatBorder::Thin);
    let normal_fmt = Format::new()
        .set_font_size(10)
        .set_border(FormatBorder::Thin);
    let pass_fmt = Format::new()
        .set_font_size(10)
        .set_font_color(Color::RGB(0x00B050))
        .set_bold()
        .set_border(FormatBorder::Thin);
    let pending_fmt = Format::new()
        .set_font_size(10)
        .set_font_color(Color::RGB(0xED7D31))
        .set_bold()
        .set_border(FormatBorder::Thin);

    let mut r = 0u32;

    // Section 1: Document Info
    sheet2
        .write_with_format(r, 0, "Document Information", &section2_fmt)
        .map_err(|e| e.to_string())?;
    r += 1;
    let today_str = chrono::Local::now().format("%Y-%m-%d").to_string();
    let doc_items: Vec<(&str, &str)> = vec![
        ("Document Serial Number", data.doc_serial.as_str()),
        ("Date", &today_str),
        ("Buyer TAX ID", data.buyer_tax_id.as_str()),
    ];
    for (label, val) in &doc_items {
        sheet2
            .write_with_format(r, 0, *label, &bold_fmt)
            .map_err(|e| e.to_string())?;
        sheet2
            .write_with_format(r, 1, *val, &normal_fmt)
            .map_err(|e| e.to_string())?;
        r += 1;
    }
    // Seller TAX IDs
    sheet2
        .write_with_format(r, 0, "Seller TAX ID(s)", &bold_fmt)
        .map_err(|e| e.to_string())?;
    let mut tax_ids: BTreeSet<&str> = BTreeSet::new();
    for tid in &data.seller_tax_ids {
        if !tid.is_empty() {
            tax_ids.insert(tid.as_str());
        }
    }
    for inv in &data.invoices {
        if !inv.seller_tax_id.is_empty() {
            tax_ids.insert(&inv.seller_tax_id);
        }
    }
    let tax_str = tax_ids.into_iter().collect::<Vec<_>>().join(", ");
    sheet2
        .write_with_format(r, 1, &tax_str, &normal_fmt)
        .map_err(|e| e.to_string())?;
    r += 2;

    // Section 2: Computed Values
    sheet2
        .write_with_format(r, 0, "Computed Values", &section2_fmt)
        .map_err(|e| e.to_string())?;
    r += 1;
    let computed_items: Vec<(&str, f64)> = vec![
        ("Net Amount Payable (9A)", computed.c_9A),
        ("Current Period Paid (10A)", computed.c_10A),
        ("Ending Accumulative Paid (11A)", computed.c_11A),
        ("Ending Total Paid (11B)", computed.c_11B),
    ];
    for (label, val) in &computed_items {
        sheet2
            .write_with_format(r, 0, *label, &bold_fmt)
            .map_err(|e| e.to_string())?;
        sheet2
            .write_with_format(r, 1, *val, &normal_fmt)
            .map_err(|e| e.to_string())?;
        r += 1;
    }
    r += 1;

    // Section 3: Invoice Details
    sheet2
        .write_with_format(r, 0, "Invoice Details", &section2_fmt)
        .map_err(|e| e.to_string())?;
    r += 1;
    sheet2
        .write_with_format(r, 0, "Invoice No", &bold_fmt)
        .map_err(|e| e.to_string())?;
    sheet2
        .write_with_format(r, 1, "Seller TAX ID", &bold_fmt)
        .map_err(|e| e.to_string())?;
    sheet2
        .write_with_format(r, 2, "Amount", &bold_fmt)
        .map_err(|e| e.to_string())?;
    r += 1;
    for inv in &data.invoices {
        sheet2
            .write_with_format(r, 0, &inv.invoice_no, &normal_fmt)
            .map_err(|e| e.to_string())?;
        sheet2
            .write_with_format(r, 1, &inv.seller_tax_id, &normal_fmt)
            .map_err(|e| e.to_string())?;
        let amt: f64 = inv.amount.parse().unwrap_or(0.0);
        sheet2
            .write_with_format(r, 2, amt, &normal_fmt)
            .map_err(|e| e.to_string())?;
        r += 1;
    }
    r += 1;

    // Section 4: Verification Checklist
    sheet2
        .write_with_format(r, 0, "Verification Checklist", &section2_fmt)
        .map_err(|e| e.to_string())?;
    r += 1;
    let check_items: Vec<(&str, bool)> = vec![
        ("Cover & Settlement Check", data.check_cover),
        ("Invoices Match Amount", data.check_invoices),
        ("Company Name on Cover Matches Invoices", data.check_company_name),
        ("WHT-Free Company Provided WHT Certificate", data.check_wht_cert),
    ];
    for (label, ok) in &check_items {
        sheet2
            .write_with_format(r, 0, *label, &normal_fmt)
            .map_err(|e| e.to_string())?;
        let status = if *ok { "✓ Passed" } else { "⏳ Pending" };
        let fmt: &Format = if *ok { &pass_fmt } else { &pending_fmt };
        sheet2
            .write_with_format(r, 1, status, fmt)
            .map_err(|e| e.to_string())?;
        r += 1;
    }
    r += 1;

    // Section 5: Audit Notes
    sheet2
        .write_with_format(r, 0, "Audit Notes", &section2_fmt)
        .map_err(|e| e.to_string())?;
    r += 1;
    sheet2
        .write_with_format(r, 0, &data.audit_notes, &normal_fmt)
        .map_err(|e| e.to_string())?;

    // ── Sheet 3: Import Calculation ──
    let sheet3 = workbook.add_worksheet();
    sheet3.set_name("Import Calculation").map_err(|e| e.to_string())?;
    sheet3.set_column_width(0, 35).map_err(|e| e.to_string())?;
    sheet3.set_column_width(1, 25).map_err(|e| e.to_string())?;
    sheet3.set_column_width(2, 18).map_err(|e| e.to_string())?;
    sheet3.set_column_width(3, 18).map_err(|e| e.to_string())?;
    sheet3.set_column_width(4, 18).map_err(|e| e.to_string())?;

    let mut r3 = 0u32;

    // Title
    sheet3.merge_range(r3, 0, r3, 4, "CSCEC - Import Calculation", &title_fmt)
        .map_err(|e| e.to_string())?;
    r3 += 1;

    // Document info
    sheet3.write_with_format(r3, 0, format!("Doc: {}", data.doc_serial), &info_fmt)
        .map_err(|e| e.to_string())?;
    r3 += 2;

    // Section 1: Cost Breakdown
    sheet3.write_with_format(r3, 0, "Cost Breakdown", &section_fmt)
        .map_err(|e| e.to_string())?;
    r3 += 1;
    let cost_items: Vec<(&str, f64)> = vec![
        ("Commercial Invoice Amount", parse_amt(&data.import_commercial_amount)),
        ("Cost 1", parse_amt(&data.import_cost_1)),
        ("Cost 2", parse_amt(&data.import_cost_2)),
        ("Cost 3", parse_amt(&data.import_cost_3)),
        ("Total Costs", computed.import_total_costs),
    ];
    for (label, val) in &cost_items {
        let is_total = *label == "Total Costs";
        sheet3.write_with_format(r3, 0, *label, if is_total { &bold_fmt } else { &normal_fmt })
            .map_err(|e| e.to_string())?;
        sheet3.write_with_format(r3, 1, *val, if is_total { &calc_fmt } else { &val_fmt })
            .map_err(|e| e.to_string())?;
        r3 += 1;
    }
    r3 += 1;

    // Section 2: Service Providers
    sheet3.write_with_format(r3, 0, "Service Providers", &section_fmt)
        .map_err(|e| e.to_string())?;
    r3 += 1;
    // Header
    let headers_import = ["Service", "Amount", "Free WHT", "VAT", "WHT", "Total (+VAT)"];
    for (ci, h) in headers_import.iter().enumerate() {
        sheet3.write_with_format(r3, ci as u16, *h, &bold_fmt)
            .map_err(|e| e.to_string())?;
    }
    r3 += 1;

    let vat_rate: f64 = parse_rate(&data.import_vat_rate);
    for entry in &data.import_entries {
        let amt: f64 = parse_amt(&entry.amount);
        let vat = (amt * vat_rate / 100.0 * 100.0).round() / 100.0;
        let wht_rate: f64 = parse_rate(&entry.wht_rate);
        let wht = if entry.free_wht { 0.0 } else { (amt * wht_rate / 100.0 * 100.0).round() / 100.0 };
        let total = amt + vat;
        sheet3.write_with_format(r3, 0, &entry.service_name, &normal_fmt)
            .map_err(|e| e.to_string())?;
        sheet3.write_with_format(r3, 1, amt, &val_fmt)
            .map_err(|e| e.to_string())?;
        sheet3.write_with_format(r3, 2, if entry.free_wht { "Yes" } else { "No" }, &normal_fmt)
            .map_err(|e| e.to_string())?;
        sheet3.write_with_format(r3, 3, vat, &val_fmt)
            .map_err(|e| e.to_string())?;
        sheet3.write_with_format(r3, 4, wht, &val_fmt)
            .map_err(|e| e.to_string())?;
        sheet3.write_with_format(r3, 5, total, &calc_fmt)
            .map_err(|e| e.to_string())?;
        r3 += 1;
    }
    r3 += 1;

    // Section 3: Summary
    sheet3.write_with_format(r3, 0, "Summary", &section_fmt)
        .map_err(|e| e.to_string())?;
    r3 += 1;
    let summary_items: Vec<(&str, f64)> = vec![
        ("Gross (Invoice+Costs+Services)", computed.import_gross_amount),
        ("Total VAT", computed.import_total_vat),
        ("Total WHT", computed.import_total_wht),
        ("Grand Total (Gross+VAT)", computed.import_grand_total),
        ("Grand Net (Gross+VAT-WHT)", computed.import_grand_net),
    ];
    for (label, val) in &summary_items {
        let is_grand = label.starts_with("Grand ");
        sheet3.write_with_format(r3, 0, *label, if is_grand { &bold_fmt } else { &normal_fmt })
            .map_err(|e| e.to_string())?;
        sheet3.write_with_format(r3, 1, *val, if is_grand { &calc_fmt } else { &val_fmt })
            .map_err(|e| e.to_string())?;
        r3 += 1;
    }

    workbook.save(path).map_err(|e| e.to_string())?;
    Ok(())
}

fn parse_amt(s: &str) -> f64 {
    s.replace(',', "").parse().unwrap_or(0.0)
}

fn parse_rate(s: &str) -> f64 {
    s.replace('%', "").parse().unwrap_or(0.0)
}
