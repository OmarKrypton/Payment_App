use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrFieldInfo {
    pub field: String,
    pub value: String,
    pub source: String,
    pub confidence: f64,
    pub cross_validated: bool,
    pub ocr_values: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(non_snake_case)]
pub struct FormData {
    pub val_1A: String,
    pub val_1B: String,
    pub val_1C: String,
    pub val_1D: String,
    pub vat_rate: String,
    pub val_2A: String,
    pub val_2B: String,
    pub val_2C: String,
    pub ret_rate: String,
    pub val_4A: String,
    pub val_4B: String,
    pub val_4C: String,
    pub temp_rate: String,
    pub val_5A: String,
    pub val_5B: String,
    pub val_5C: String,
    pub wht_rate: String,
    pub val_6A: String,
    pub val_6B: String,
    pub val_6C: String,
    pub oth_rate: String,
    pub val_8A: String,
    pub soc_rate: String,
    pub val_12A: String,
    pub val_7A: String,
    pub val_10A: String,
    pub val_11A: String,
    pub val_11B: String,
    pub doc_serial: String,
    pub buyer_tax_id: String,
    pub seller_tax_id: String,
    #[serde(default)]
    pub seller_tax_ids: Vec<String>,
    #[serde(default)]
    pub doc_type: String,
    pub check_cover: bool,
    pub check_invoices: bool,
    #[serde(default)]
    pub check_company_name: bool,
    #[serde(default)]
    pub check_wht_cert: bool,
    pub audit_notes: String,
    pub vat_manual: bool,
    pub wht_manual: bool,
    pub oth_manual: bool,
    pub soc_manual: bool,
    pub invoices: Vec<InvoiceData>,
    pub vat_rows: Vec<RateRow>,
    pub wht_rows: Vec<RateRow>,
    pub oth_rows: Vec<RateRow>,
    pub soc_rows: Vec<RateRow>,
    #[serde(default)]
    pub import_commercial_amount: String,
    #[serde(default)]
    pub import_commercial_rate: String,
    #[serde(default)]
    pub import_cost_1: String,
    #[serde(default)]
    pub import_cost_2: String,
    #[serde(default)]
    pub import_cost_3: String,
    #[serde(default)]
    pub import_vat_rate: String,
    #[serde(default)]
    pub import_entries: Vec<ImportEntry>,
    pub ocr_meta: Vec<OcrFieldInfo>,
}

impl Default for FormData {
    fn default() -> Self {
        Self {
            val_1A: "0.00".into(),
            val_1B: "0.00".into(),
            val_1C: "0.00".into(),
            val_1D: "0.00".into(),
            vat_rate: "14%".into(),
            val_2A: "0.00".into(),
            val_2B: "0.00".into(),
            val_2C: "0.00".into(),
            ret_rate: "0%".into(),
            val_4A: "0.00".into(),
            val_4B: "0.00".into(),
            val_4C: "0.00".into(),
            temp_rate: "0%".into(),
            val_5A: "0.00".into(),
            val_5B: "0.00".into(),
            val_5C: "0.00".into(),
            wht_rate: "0%".into(),
            val_6A: "0.00".into(),
            val_6B: "0.00".into(),
            val_6C: "0.00".into(),
            oth_rate: "0%".into(),
            val_8A: "0.00".into(),
            soc_rate: "0%".into(),
            val_12A: "0.00".into(),
            val_7A: "0.00".into(),
            val_10A: "0.00".into(),
            val_11A: "0.00".into(),
            val_11B: "0.00".into(),
            doc_serial: String::new(),
            buyer_tax_id: String::new(),
            seller_tax_id: String::new(),
            seller_tax_ids: vec![],
            doc_type: "bank".into(),
            check_cover: false,
            check_invoices: false,
            check_company_name: false,
            check_wht_cert: false,
            audit_notes: String::new(),
            vat_manual: false,
            wht_manual: false,
            oth_manual: false,
            soc_manual: false,
            invoices: vec![],
            vat_rows: vec![],
            wht_rows: vec![],
            oth_rows: vec![],
            soc_rows: vec![],
            import_commercial_amount: "0.00".into(),
            import_commercial_rate: String::new(),
            import_cost_1: "0.00".into(),
            import_cost_2: "0.00".into(),
            import_cost_3: "0.00".into(),
            import_vat_rate: "14%".into(),
            import_entries: vec![],
            ocr_meta: vec![],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvoiceData {
    pub invoice_no: String,
    pub seller_tax_id: String,
    pub amount: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateRow {
    pub amount: String,
    pub rate: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportEntry {
    pub service_name: String,
    pub amount: String,
    pub rate: String,
    pub free_wht: bool,
    pub wht_rate: String,
    pub vat_rate: String,
    pub temp_labour: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(non_snake_case)]
pub struct CalcResult {
    pub c_1A: f64,
    pub c_1B: f64,
    pub c_1C: f64,
    pub c_1D: f64,
    pub c_1E: f64,
    pub c_1F: f64,
    pub c_1G: f64,
    pub c_2A: f64,
    pub c_2B: f64,
    pub c_2C: f64,
    pub c_2D: f64,
    pub c_2E: f64,
    pub c_3A: f64,
    pub c_4A: f64,
    pub c_4B: f64,
    pub c_4C: f64,
    pub c_4D: f64,
    pub c_5A: f64,
    pub c_5B: f64,
    pub c_5C: f64,
    pub c_5D: f64,
    pub c_6A: f64,
    pub c_6B: f64,
    pub c_6C: f64,
    pub c_6D: f64,
    pub c_8A: f64,
    pub c_8B: f64,
    pub c_8C: f64,
    pub c_12A: f64,
    pub c_12B: f64,
    pub c_12C: f64,
    pub total_deductions: f64,
    pub c_7A: f64,
    pub c_7B: f64,
    pub c_9A: f64,
    pub c_10A: f64,
    pub c_11A: f64,
    pub c_11B: f64,
    pub import_total_costs: f64,
    pub import_gross_amount: f64,
    pub import_total_vat: f64,
    pub import_total_wht: f64,
    pub import_grand_total: f64,
    pub import_grand_net: f64,
    pub import_temp_labour: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: i64,
    pub label: String,
    pub notes: String,
    pub created_at: String,
}


