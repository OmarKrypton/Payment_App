import { useState, useCallback, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { emit, listen } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";
import "./App.css";

interface OcrFieldInfo {
  field: string;
  value: string;
  source: string;
  confidence: number;
  cross_validated: boolean;
}

interface RateRow {
  amount: string;
  rate: string;
}

interface ImportEntry {
  service_name: string;
  amount: string;
  rate: string;
  free_wht: boolean;
  wht_rate: string;
  vat_rate: string;
  temp_labour: boolean;
}

interface InvoiceData {
  invoice_no: string;
  seller_tax_id: string;
  amount: string;
}

interface ImportCostRow {
  name: string;
  amount: string;
}

interface FormData {
  val_1A: string; val_1B: string; val_1C: string; val_1D: string;
  vat_rate: string; val_2A: string; val_2B: string; val_2C: string;
  ret_rate: string; val_4A: string; val_4B: string; val_4C: string;
  temp_rate: string; val_5A: string; val_5B: string; val_5C: string;
  wht_rate: string; val_6A: string; val_6B: string; val_6C: string;
  oth_rate: string; val_8A: string;
  soc_rate: string; val_12A: string;
  val_7A: string; val_10A: string; val_11A: string; val_11B: string;
  doc_serial: string; buyer_tax_id: string; seller_tax_id: string; seller_tax_ids: string[];
  check_cover: boolean; check_invoices: boolean; check_company_name: boolean; check_wht_cert: boolean; audit_notes: string;
  check_sad: boolean; check_import_invoice: boolean; check_bill_lading: boolean; check_packing_list: boolean; check_cert_origin: boolean; check_nafeza: boolean; check_form_4_6: boolean;
  vat_manual: boolean; wht_manual: boolean; oth_manual: boolean; soc_manual: boolean;
  invoices: InvoiceData[];
  vat_rows: RateRow[]; wht_rows: RateRow[]; oth_rows: RateRow[]; soc_rows: RateRow[];
  import_commercial_amount: string; import_cost_1: string; import_cost_2: string; import_cost_3: string;
  import_commercial_rate: string;
  import_entries: ImportEntry[];
  import_costs: ImportCostRow[];
  doc_type: string;
  ocr_meta: OcrFieldInfo[];
}

interface CalcResult {
  c_1A: number; c_1B: number; c_1C: number; c_1D: number;
  c_1E: number; c_1F: number; c_1G: number;
  c_2A: number; c_2B: number; c_2C: number; c_2D: number; c_2E: number;
  c_3A: number;
  c_4A: number; c_4B: number; c_4C: number; c_4D: number;
  c_5A: number; c_5B: number; c_5C: number; c_5D: number;
  c_6A: number; c_6B: number; c_6C: number; c_6D: number;
  c_8A: number; c_8B: number; c_8C: number;
  c_12A: number; c_12B: number; c_12C: number;
  total_deductions: number;
  c_7A: number; c_7B: number; c_9A: number;
  c_10A: number; c_11A: number; c_11B: number;
  import_total_costs: number; import_gross_amount: number;
  import_total_vat: number; import_total_wht: number;
  import_grand_total: number; import_grand_net: number;
  import_temp_labour: number;
}

const EMPTY_FORM: FormData = {
  val_1A: "0.00", val_1B: "0.00", val_1C: "0.00", val_1D: "0.00",
  vat_rate: "14%", val_2A: "0.00", val_2B: "0.00", val_2C: "0.00",
  ret_rate: "0%", val_4A: "0.00", val_4B: "0.00", val_4C: "0.00",
  temp_rate: "0%", val_5A: "0.00", val_5B: "0.00", val_5C: "0.00",
  wht_rate: "0%", val_6A: "0.00", val_6B: "0.00", val_6C: "0.00",
  oth_rate: "0%", val_8A: "0.00",
  soc_rate: "0%", val_12A: "0.00",
  val_7A: "0.00", val_10A: "0.00", val_11A: "0.00", val_11B: "0.00",
  doc_serial: "", buyer_tax_id: "", seller_tax_id: "", seller_tax_ids: [],
  check_cover: false, check_invoices: false, check_company_name: false, check_wht_cert: false, audit_notes: "",
  check_sad: false, check_import_invoice: false, check_bill_lading: false, check_packing_list: false, check_cert_origin: false, check_nafeza: false, check_form_4_6: false,
  vat_manual: false, wht_manual: false, oth_manual: false, soc_manual: false,
  invoices: [],
  vat_rows: [{ amount: "0.00", rate: "0%" }],
  wht_rows: [{ amount: "0.00", rate: "0%" }],
  oth_rows: [{ amount: "0.00", rate: "0%" }],
  soc_rows: [{ amount: "0.00", rate: "0%" }],
  import_commercial_amount: "0.00", import_cost_1: "0.00", import_cost_2: "0.00", import_cost_3: "0.00",
  import_commercial_rate: "",
  import_entries: [],
  import_costs: [{ name: "Foreign Cost", amount: "0.00" }, { name: "Domestic Cost", amount: "0.00" }, { name: "Nafeza Paper", amount: "0.00" }],
  doc_type: "bank",
  ocr_meta: [],
};

const DEFAULT_FORM: FormData = {
  val_1A: "0.00", val_1B: "0.00", val_1C: "0.00", val_1D: "0.00",
  vat_rate: "14%", val_2A: "0.00", val_2B: "0.00", val_2C: "0.00",
  ret_rate: "0%", val_4A: "0.00", val_4B: "0.00", val_4C: "0.00",
  temp_rate: "0%", val_5A: "0.00", val_5B: "0.00", val_5C: "0.00",
  wht_rate: "0%", val_6A: "0.00", val_6B: "0.00", val_6C: "0.00",
  oth_rate: "0%", val_8A: "0.00",
  soc_rate: "0%", val_12A: "0.00",
  val_7A: "0.00", val_10A: "0.00", val_11A: "0.00", val_11B: "0.00",
  doc_serial: "", buyer_tax_id: "", seller_tax_id: "", seller_tax_ids: [],
  check_cover: false, check_invoices: false, check_company_name: false, check_wht_cert: false, audit_notes: "",
  check_sad: false, check_import_invoice: false, check_bill_lading: false, check_packing_list: false, check_cert_origin: false, check_nafeza: false, check_form_4_6: false,
  vat_manual: false, wht_manual: false, oth_manual: false, soc_manual: false,
  invoices: [],
  vat_rows: [{ amount: "0.00", rate: "0%" }],
  wht_rows: [{ amount: "0.00", rate: "0%" }],
  oth_rows: [{ amount: "0.00", rate: "0%" }],
  soc_rows: [{ amount: "0.00", rate: "0%" }],
  import_commercial_amount: "0.00", import_cost_1: "0.00", import_cost_2: "0.00", import_cost_3: "0.00",
  import_commercial_rate: "",
  import_entries: [],
  import_costs: [{ name: "Foreign Cost", amount: "0.00" }, { name: "Domestic Cost", amount: "0.00" }, { name: "Nafeza Paper", amount: "0.00" }],
  doc_type: "bank",
  ocr_meta: [],
};

const EMPTY_CALC: CalcResult = {
  c_1A: 0, c_1B: 0, c_1C: 0, c_1D: 0, c_1E: 0, c_1F: 0, c_1G: 0,
  c_2A: 0, c_2B: 0, c_2C: 0, c_2D: 0, c_2E: 0, c_3A: 0,
  c_4A: 0, c_4B: 0, c_4C: 0, c_4D: 0,
  c_5A: 0, c_5B: 0, c_5C: 0, c_5D: 0,
  c_6A: 0, c_6B: 0, c_6C: 0, c_6D: 0,
  c_8A: 0, c_8B: 0, c_8C: 0,
  c_12A: 0, c_12B: 0, c_12C: 0, total_deductions: 0,
  c_7A: 0, c_7B: 0, c_9A: 0, c_10A: 0, c_11A: 0, c_11B: 0,
  import_total_costs: 0, import_gross_amount: 0,
  import_total_vat: 0, import_total_wht: 0,
  import_grand_total: 0, import_grand_net: 0,
  import_temp_labour: 0,
};

const fmt = (v: number) => `EGP ${v.toLocaleString("en-US", { minimumFractionDigits: 2, maximumFractionDigits: 2 })}`;
const fmtShort = (v: number) => v.toLocaleString("en-US", { minimumFractionDigits: 2, maximumFractionDigits: 2 });

function focusNext(current: HTMLElement) {
  const fields = document.querySelectorAll<HTMLElement>('.field-input, .field-select, button, textarea');
  const idx = Array.from(fields).indexOf(current);
  if (idx >= 0 && idx < fields.length - 1) {
    fields[idx + 1].focus();
  }
}

function Input({ label, sub, value, onChange, width, confidence }: {
  label: string; sub?: string; value: string; onChange: (v: string) => void; width?: number; confidence?: number;
}) {
  const [local, setLocal] = useState(value);
  useEffect(() => setLocal(value), [value]);
  const dot = confidence !== undefined ? (
    <span className={`conf-dot ${confidence >= 0.67 ? "conf-high" : confidence >= 0.33 ? "conf-med" : "conf-low"}`}
          title={`OCR confidence: ${(confidence * 100).toFixed(0)}%${confidence >= 0.67 ? " ✓" : confidence >= 0.33 ? " ~" : " ✗"}`}
    />
  ) : null;
  return (
    <div className="field" style={width ? { maxWidth: width } : {}}>
      <label className="field-label">{dot}{label}{sub ? <><br /><span className="field-sub">{sub}</span></> : null}</label>
      <input
        className={"field-input" + (confidence !== undefined && confidence < 0.33 ? " conf-low-input" : "")}
        type="text" value={local}
        onChange={e => { setLocal(e.target.value); onChange(e.target.value); }}
        onKeyDown={e => {
          if (e.key === 'Enter') { e.preventDefault(); focusNext(e.currentTarget); }
          if (e.key === 'Escape') { e.currentTarget.blur(); }
        }}
      />
    </div>
  );
}

function Select({ label, sub, value, options, onChange }: {
  label: string; sub?: string; value: string; options: string[]; onChange: (v: string) => void;
}) {
  return (
    <div className="field">
      <label className="field-label">{label}{sub ? <><br /><span className="field-sub">{sub}</span></> : null}</label>
      <select
        className="field-select" value={value}
        onChange={e => onChange(e.target.value)}
        onKeyDown={e => {
          if (e.key === 'Escape') { e.currentTarget.blur(); }
        }}
      >
        {options.map(o => <option key={o} value={o}>{o}</option>)}
      </select>
    </div>
  );
}

function Computed({ label, sub, value, highlight }: { label: string; sub?: string; value: number; highlight?: boolean }) {
  const negative = value < 0;
  const cls = `computed-value${highlight ? " highlight" : ""}${negative ? " negative" : ""}`;
  const color = negative ? "var(--red)" : "var(--green)";
  return (
    <div className="field">
      <label className="field-label">{label}{sub ? <><br /><span className="field-sub">{sub}</span></> : null}</label>
      <div className={cls} style={{ color, borderColor: color }}>{fmt(value)}</div>
    </div>
  );
}

function FastInput({ value, onChange, className, type, rows }: {
  value: string; onChange: (v: string) => void; className?: string; type?: string; rows?: number;
}) {
  const [local, setLocal] = useState(value);
  const ref = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    setLocal(value);
  }, [value]);

  useEffect(() => {
    if (rows && ref.current) {
      ref.current.style.height = 'auto';
      ref.current.style.height = ref.current.scrollHeight + 'px';
    }
  }, [local, rows]);

  if (rows) {
    return <textarea
      ref={ref}
      className={className || "field-input"}
      value={local}
      rows={rows}
      style={{ resize: 'none', overflowY: 'hidden', minHeight: '32px' }}
      onChange={e => { setLocal(e.target.value); onChange(e.target.value); }}
      onKeyDown={e => { if (e.key === 'Escape') e.currentTarget.blur(); }}
    />;
  }
  return <input className={className || "field-input"} type={type || "text"} value={local}
    onChange={e => { setLocal(e.target.value); onChange(e.target.value); }} />;
}

interface HistoryEntry {
  id: number; label: string; notes: string; created_at: string;
}

function App() {
  const [tab, setTab] = useState<"bank" | "final_decision" | "import">("bank");
  const [lang, setLang] = useState<"zh" | "en">("zh");
  const t = useCallback((zh: string, en: string) => lang === "zh" ? zh : en, [lang]);
  const formRef = useRef<FormData>(DEFAULT_FORM);
  const [computed, setComputed] = useState<CalcResult>(EMPTY_CALC);
  const overlayRef = useRef<HTMLDivElement>(null);
  const [progressMsg, setProgressMsg] = useState("");
  const [modalMsg, setModalMsg] = useState<string | null>(null);

  const showAlert = useCallback((msg: string) => setModalMsg(msg), []);

  const recalc = useCallback(async (data: FormData) => {
    const result = await invoke<CalcResult>("recalculate", { data });
    setComputed(result);
  }, []);

  const saveConfig = useCallback(async () => {
    try { await invoke("save_config", { data: formRef.current }); } catch {}
  }, []);

  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const queueFlush = useCallback(() => {
    if (debounceRef.current) clearTimeout(debounceRef.current);
    debounceRef.current = setTimeout(async () => {
      debounceRef.current = null;
      const d = formRef.current;
      const result = await invoke<CalcResult>("recalculate", { data: d });
      setComputed(result);
      try { await invoke("save_config", { data: d }); } catch {}
    }, 300);
  }, []);

  const updateField = useCallback((key: keyof FormData, value: any) => {
    formRef.current = { ...formRef.current, [key]: value };
    queueFlush();
  }, [queueFlush]);

  const updateNested = useCallback((parent: string, index: number, key: string, value: string) => {
    const arr = [...(formRef.current as any)[parent]];
    arr[index] = { ...arr[index], [key]: value };
    formRef.current = { ...formRef.current, [parent]: arr };
    queueFlush();
  }, [queueFlush]);

  const addRow = useCallback((parent: string) => {
    const arr = [...(formRef.current as any)[parent], { amount: "0.00", rate: "0%" }];
    formRef.current = { ...formRef.current, [parent]: arr };
    queueFlush();
  }, [queueFlush]);

  const delRow = useCallback((parent: string, index: number) => {
    const arr = [...(formRef.current as any)[parent]];
    arr.splice(index, 1);
    formRef.current = { ...formRef.current, [parent]: arr };
    queueFlush();
  }, [queueFlush]);

  const toggleManual = useCallback((key: string) => {
    const current = !(formRef.current as any)[key];
    formRef.current = { ...formRef.current, [key]: current };
    queueFlush();
  }, [queueFlush]);

  // ── Overlay safety ──
  const importTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const hideOverlay = useCallback(() => {
    if (importTimerRef.current) { clearTimeout(importTimerRef.current); importTimerRef.current = null; }
    setProgressMsg("");
    if (overlayRef.current) overlayRef.current.style.display = 'none';
  }, []);

  // Init
  useEffect(() => {
    (async () => {
      try {
        const cfg = await invoke<FormData>("load_config");
        formRef.current = cfg;
        recalc(cfg);
      } catch (e) {
        console.error("load_config failed", e);
      }
    })();
  }, []);

  // Periodic noop to keep WebKit event loop alive on Linux
  useEffect(() => {
    const ping = () => { invoke("ping"); };
    ping();
    const id = setInterval(ping, 4000);
    return () => clearInterval(id);
  }, []);

  // Listen for import progress updates
  useEffect(() => {
    const unlisten = listen<{status: string, message: string}>("import-progress", (event) => {
      setProgressMsg(event.payload.message);
    });
    return () => { unlisten.then(f => f()); };
  }, []);

  // Listen for background import completion
  useEffect(() => {
    const unlisten = listen<FormData>("import-complete", (event) => {
      const parsed = event.payload;
      formRef.current = parsed;
      recalc(parsed);
      saveConfig();
      hideOverlay();
      showAlert(t("PDF导入成功", "PDF imported successfully"));
    });
    return () => { unlisten.then(f => f()); };
  }, [recalc, t, saveConfig, hideOverlay]);

  // Listen for background import error
  useEffect(() => {
    const unlisten = listen<{status: string, message: string}>("import-error", (event) => {
      hideOverlay();
      showAlert(`${t("导入失败", "Import failed")}: ${event.payload.message}`);
    });
    return () => { unlisten.then(f => f()); };
  }, [t, hideOverlay]);

  const data = formRef.current;

  const ocrConf = (field: string): number | undefined => {
    if (!data.ocr_meta) return undefined;
    const m = data.ocr_meta.find(o => o.field === field);
    return m ? m.confidence : undefined;
  };

  const renderRateRows = (rows: RateRow[], parent: string, rates: string[]) => (
    <div className="rate-rows">
      {rows.map((r, i) => (
        <div key={i} className="rate-row">
          <FastInput className="field-input small" value={r.amount} onChange={v => updateNested(parent, i, "amount", v)} />
          <select className="field-select small" value={r.rate} onChange={e => updateNested(parent, i, "rate", e.target.value)}>
            {rates.map(o => <option key={o} value={o}>{o}</option>)}
          </select>
          {rows.length > 1 && <button className="btn-danger" onClick={() => delRow(parent, i)}>✕</button>}
        </div>
      ))}
      <button className="btn-add" onClick={() => addRow(parent)}>+ Add Row</button>
    </div>
  );

  const renderAutoRate = (key: string, value: string, rates: string[]) => (
    <Select label="" value={value} options={rates} onChange={v => updateField(key as any, v)} />
  );

  // ── Cards ──
  const Card1 = () => (
    <div className="card">
      <h3>{t("1. 供应商结算", "1. Supplier Settlement")}</h3>
      <div className="card-grid">
        <div className="card-left">
          <Input label={t("期初累计结算款", "Opening accum.")} value={data.val_1A} onChange={v => updateField("val_1A", v)} confidence={ocrConf("val_1A")} />
          <Input label={t("本期结算金额", "Current settlement")} value={data.val_1B} onChange={v => { updateField("val_1B", v); }} confidence={ocrConf("val_1B")} />
          <Input label={t("本期其他增项", "Other additions")} value={data.val_1C} onChange={v => updateField("val_1C", v)} confidence={ocrConf("val_1C")} />
          <Input label={t("本期扣除额", "Current deductions")} value={data.val_1D} onChange={v => updateField("val_1D", v)} confidence={ocrConf("val_1D")} />
          <Computed label={t("本期供应商结算 (含税)", "Settlement incl. VAT")} value={computed.c_1G} />
          <Computed label={t("期末累计供应商结算", "Ending accumulative")} value={computed.c_1F} highlight />
        </div>
        <div className="card-right">
          <div className="section-header">
            <span>{t("增值税率", "VAT Rates")}</span>
            <label className="toggle">
              <input type="checkbox" checked={data.vat_manual} onChange={() => toggleManual("vat_manual")} />
              {t("多税率模式", "Multi-Rate")}
            </label>
          </div>
          {data.vat_manual ? (
            <>
              {renderRateRows(data.vat_rows, "vat_rows", ["0%", "5%", "9%", "10%", "14%"])}
              <div className="rate-total">VAT: {fmt(computed.c_1E)}</div>
            </>
          ) : (
            renderAutoRate("vat_rate", data.vat_rate, ["0%", "5%", "9%", "10%", "14%"])
          )}
          <Computed label={t("本期增值税额", "Current VAT amount")} sub={t("", "VAT")} value={computed.c_1E} />
        </div>
      </div>
    </div>
  );

  const Card2 = () => (
    <div className="card">
      <h3>{t("2. 预付款", "2. Advance Payment")}</h3>
      <Input label={t("预付款总额", "Total advance")} value={data.val_2A} onChange={v => updateField("val_2A", v)} confidence={ocrConf("val_2A")} />
      <Input label={t("期初预付款扣除", "Initial deduction")} value={data.val_2B} onChange={v => updateField("val_2B", v)} confidence={ocrConf("val_2B")} />
      <Input label={t("本期预付款扣除", "Current deduction")} value={data.val_2C} onChange={v => updateField("val_2C", v)} confidence={ocrConf("val_2C")} />
      <Computed label={t("期末预付款扣除金额", "Ending deduction")} value={computed.c_2D} highlight />
      <Computed label={t("期末预付款余额", "Ending balance")} value={computed.c_2E} highlight />
    </div>
  );

  const Card3 = () => (
    <div className="card">
      <h3>{t("3. 应付金额", "3. Amount Payable")}</h3>
      <Computed label={t("期末累计应付金额", "Ending accum. payable")} value={computed.c_3A} highlight />
    </div>
  );

  const Card4 = () => (
    <div className="card">
      <h3>{t("4. 保留金", "4. Retention")}</h3>
      <Input label={t("期初保留金", "Initial retention")} value={data.val_4A} onChange={v => updateField("val_4A", v)} confidence={ocrConf("val_4A")} />
      <Select label={t("保留金率", "Retention rate")} value={data.ret_rate} options={["0%", "0.5%", "3%", "5%", "10%"]} onChange={v => updateField("ret_rate", v)} />
      <Computed label={t("本期应扣保留金", "Current deduction")} value={computed.c_4B} />
      <Input label={t("本期返还保留金", "Current return")} value={data.val_4C} onChange={v => updateField("val_4C", v)} confidence={ocrConf("val_4C")} />
      <Computed label={t("期末保留金余额", "Ending balance")} value={computed.c_4D} highlight />
    </div>
  );

  const Card5 = () => (
    <div className="card">
      <h3>{t("5. 临时工社保", "5. Temp. Labour Insurance")}</h3>
      <Input label={t("期初余额", "Initial balance")} value={data.val_5A} onChange={v => updateField("val_5A", v)} confidence={ocrConf("val_5A")} />
      <Select label={t("临时工社保率", "Temp labour rate")} value={data.temp_rate} options={["0%", "0.45%"]} onChange={v => updateField("temp_rate", v)} />
      <Computed label={t("本期应扣", "Current deductible")} value={computed.c_5B} />
      <Input label={t("本期返还", "Current return")} value={data.val_5C} onChange={v => updateField("val_5C", v)} confidence={ocrConf("val_5C")} />
      <Computed label={t("期末余额", "Ending balance")} value={computed.c_5D} highlight />
    </div>
  );

  const Card6 = () => (
    <div className="card">
      <h3>{t("6. 预提税", "6. WHT")}</h3>
      <Input label={t("期初累计预提税", "Initial accum. WHT")} value={data.val_6A} onChange={v => updateField("val_6A", v)} confidence={ocrConf("val_6A")} />
      <div className="section-header">
        <label className="toggle">
          <input type="checkbox" checked={data.wht_manual} onChange={() => toggleManual("wht_manual")} />
          {t("多税率", "Multi-Rate")}
        </label>
      </div>
      {data.wht_manual ? (
        <>
          {renderRateRows(data.wht_rows, "wht_rows", ["0%", "1%", "3%", "5%"])}
          <Computed label={t("本期预提税", "Current WHT")} value={computed.c_6B} />
        </>
      ) : (
        <>
          <Select label={t("预提税率", "WHT Rate")} value={data.wht_rate} options={["0%", "1%", "3%", "5%"]} onChange={v => updateField("wht_rate", v)} />
          <Computed label={t("本期预提税", "Current WHT")} value={computed.c_6B} />
        </>
      )}
      <Computed label={t("期末累计预提税", "Ending WHT")} value={computed.c_6C} />
      <Computed label={t("决算含税减去预提税", "Settlement minus WHT")} value={computed.c_6D} highlight />
    </div>
  );

  const Card78 = () => (
    <div className="card">
      <h3>{t("8 & 12. 其他扣款与社保", "8 & 12. Others & Social")}</h3>
      <h4>{t("其他扣款", "Other Deductions")}</h4>
      <Input label={t("期初其他扣款", "Initial other")} value={data.val_8A} onChange={v => updateField("val_8A", v)} />
      <Select label={t("扣除费率", "Other rate")} value={data.oth_rate} options={["0%", "0.15%", "0.3%"]} onChange={v => updateField("oth_rate", v)} />
      <Computed label={t("本期其他扣款", "Current other")} value={computed.c_8B} />
      <Computed label={t("期末累计其他扣款", "Ending other")} value={computed.c_8C} highlight />

      <h4>{t("社保", "Social Insurance")}</h4>
      <Input label={t("期初累计社保", "Initial social")} value={data.val_12A} onChange={v => updateField("val_12A", v)} />
      <Select label={t("社保比例", "Social rate")} value={data.soc_rate} options={["0%", "3.3%", "3.6%", "11.86%"]} onChange={v => updateField("soc_rate", v)} />
      <Computed label={t("本期应扣社保", "Current social")} value={computed.c_12B} />
      <Computed label={t("期末累计社保", "Ending social")} value={computed.c_12C} highlight />
    </div>
  );

  const Card9 = () => (
    <div className="card">
      <h3>{t("7. 已付款", "7. Amount Paid")}</h3>
      <Input label={t("期初累计实付", "Initial accum. paid")} value={data.val_7A} onChange={v => updateField("val_7A", v)} confidence={ocrConf("val_7A")} />
      <Computed label={t("期初累计已付款项合计", "Total initial paid")} value={computed.c_7B} />
    </div>
  );

  const Card10 = () => (
    <div className="card">
      <h3>{t("9. 净应付", "9. Net Payable")}</h3>
      <Computed label={t("最终净应付金额", "Net amount payable")} value={computed.c_9A} highlight />
    </div>
  );

  const Card11 = () => (
    <div className="card">
      <h3>{t("10 & 11. 实付合计", "10 & 11. Paid Totals")}</h3>
      <Input label={t("本期实付", "Current paid")} value={data.val_10A} onChange={v => updateField("val_10A", v)} confidence={ocrConf("val_10A")} />
      <Computed label={t("期末累计实付", "Ending accum. paid")} value={computed.c_11A} highlight />
      <Computed label={t("期末累计已付合计", "Ending total paid")} value={computed.c_11B} highlight />
    </div>
  );

  const addInvoice = () => {
    const arr = [...data.invoices, { invoice_no: `Invoice-${data.invoices.length + 1}`, seller_tax_id: "", amount: "0.00" }];
    formRef.current = { ...formRef.current, invoices: arr };
    recalc(formRef.current);
  };
  const updInv = (i: number, k: string, v: string) => updateNested("invoices", i, k, v);
  const delInv = (i: number) => delRow("invoices", i);

  const addImportEntry = () => {
    const arr = [...(data.import_entries ?? []), { service_name: "", amount: "0.00", rate: "", free_wht: false, wht_rate: "0%", vat_rate: "14%", temp_labour: false }];
    formRef.current = { ...formRef.current, import_entries: arr };
    recalc(formRef.current);
  };
  const updImportEntry = (i: number, k: string, v: any) => updateNested("import_entries", i, k, v);
  const delImportEntry = (i: number) => delRow("import_entries", i);
  const addCostRow = () => {
    const arr = [...(data.import_costs ?? []), { name: "", amount: "0.00" }];
    formRef.current = { ...formRef.current, import_costs: arr };
    recalc(formRef.current);
  };
  const updCostRow = (i: number, k: string, v: any) => updateNested("import_costs", i, k, v);
  const delCostRow = (i: number) => delRow("import_costs", i);

  const addSellerTaxId = () => {
    const arr = [...data.seller_tax_ids, ""];
    formRef.current = { ...formRef.current, seller_tax_ids: arr };
    recalc(formRef.current);
  };
  const updSellerTaxId = (i: number, v: string) => {
    const arr = [...data.seller_tax_ids];
    arr[i] = v;
    formRef.current = { ...formRef.current, seller_tax_ids: arr };
    recalc(formRef.current);
  };
  const delSellerTaxId = (i: number) => {
    const arr = data.seller_tax_ids.filter((_, idx) => idx !== i);
    formRef.current = { ...formRef.current, seller_tax_ids: arr };
    recalc(formRef.current);
  };

  const AuditTab = () => {
    const isImport = data.doc_type === "import";
    return (
    <div className="audit-tab">
      <div className="card">
        <h3>{t("文件信息", "Document Information")}</h3>
        <div className="field"><label className="field-label">{t("文档类型", "Doc Type")}</label>
          <div style={{display:'flex',gap:8}}>
            <button onClick={() => updateField("doc_type", "bank")} style={{padding:'4px 12px',border:'1px solid var(--border)',borderRadius:4,background:data.doc_type==="bank"?'var(--accent)':'transparent',color:data.doc_type==="bank"?'#fff':'inherit',cursor:'pointer'}}>
              {t("银行", "Bank")}
            </button>
            <button onClick={() => updateField("doc_type", "import")} style={{padding:'4px 12px',border:'1px solid var(--border)',borderRadius:4,background:data.doc_type==="import"?'var(--accent)':'transparent',color:data.doc_type==="import"?'#fff':'inherit',cursor:'pointer'}}>
              {t("进口", "Import")}
            </button>
          </div>
        </div>
        <Input label={t("文档编号", "Doc Serial")} value={data.doc_serial} onChange={v => updateField("doc_serial", v)} />
        {isSerialDuplicate && (
          <div className="field-warning" style={{color: 'var(--red)'}}>
            {t("警告: 该文档编号已存在!", "Warning: This document serial already exists!")}
          </div>
        )}
        <div className="field"><label className="field-label">{t("日期", "Date")}</label><div className="computed-value">{new Date().toLocaleDateString()}</div></div>
        <Input label={t("买方税号", "Buyer TAX ID")} value={data.buyer_tax_id} onChange={v => updateField("buyer_tax_id", v)} />
        {data.buyer_tax_id && data.buyer_tax_id !== "100489095" && (
          <div className="field-warning">{t("警告: 买方税号不是 100489095!", "Warning: Buyer TAX ID is not 100489095!")}</div>
        )}
        <div className="field"><label className="field-label">{t("卖方税号", "Seller TAX IDs")}</label></div>
        {data.seller_tax_ids.map((tid, i) => (
          <div key={i} className="invoice-row" style={{marginTop: 4}}>
            <FastInput value={tid} onChange={v => updSellerTaxId(i, v)} />
            <button className="btn-danger" onClick={() => delSellerTaxId(i)}>✕</button>
          </div>
        ))}
        <button className="btn-add" onClick={addSellerTaxId}>+ {t("添加卖方税号", "Add Seller TAX ID")}</button>
      </div>

      {!isImport && (
        <>
          <div className="card">
            <h3>{t("发票", "Invoices")}</h3>
            <div className="invoice-header">
              <span>{t("发票号", "Invoice No")}</span>
              <span>{t("销售方税号", "Seller TAX ID")}</span>
              <span>{t("金额", "Amount")}</span>
              <span></span>
            </div>
            {data.invoices.map((inv, i) => (
              <div key={i} className="invoice-row">
                <FastInput value={inv.invoice_no} onChange={v => updInv(i, "invoice_no", v)} />
                <FastInput value={inv.seller_tax_id || ""} onChange={v => updInv(i, "seller_tax_id", v)} />
                <FastInput value={inv.amount} onChange={v => updInv(i, "amount", v)} />
                <button className="btn-danger" onClick={() => delInv(i)}>✕</button>
              </div>
            ))}
            <button className="btn-add" onClick={addInvoice}>+ {t("添加发票", "Add Invoice")}</button>
          </div>
          <div className="card">
            <h3>{t("计算值", "Computed Values")}</h3>
            <Computed label={t("净应付金额 (9A)", "Net Amount Payable (9A)")} value={computed.c_9A} />
            <Computed label={t("本期实付 (10A)", "Current Paid (10A)")} value={computed.c_10A} />
            <Computed label={t("期末累计实付 (11A)", "Ending Accum. Paid (11A)")} value={computed.c_11A} />
            <Computed label={t("期末已付合计 (11B)", "Ending Total Paid (11B)")} value={computed.c_11B} />
          </div>
        </>
      )}

      <div className="card">
        <h3>{t("核对清单", "Verification Checklist")}</h3>
        {isImport ? (
          <>
            <label className="check-row">
              <input type="checkbox" checked={data.check_sad} onChange={e => updateField("check_sad", e.target.checked)} />
              {t("SAD 报关单", "SAD Customs Declaration")}
            </label>
            <label className="check-row">
              <input type="checkbox" checked={data.check_import_invoice} onChange={e => updateField("check_import_invoice", e.target.checked)} />
              {t("商业发票", "Commercial Invoice")}
            </label>
            <label className="check-row">
              <input type="checkbox" checked={data.check_bill_lading} onChange={e => updateField("check_bill_lading", e.target.checked)} />
              {t("提单", "Bill of Lading")}
            </label>
            <label className="check-row">
              <input type="checkbox" checked={data.check_packing_list} onChange={e => updateField("check_packing_list", e.target.checked)} />
              {t("装箱单", "Packing List")}
            </label>
            <label className="check-row">
              <input type="checkbox" checked={data.check_cert_origin} onChange={e => updateField("check_cert_origin", e.target.checked)} />
              {t("原产地证明", "Certificate of Origin")}
            </label>
            <label className="check-row">
              <input type="checkbox" checked={data.check_nafeza} onChange={e => updateField("check_nafeza", e.target.checked)} />
              {t("Nafeza 文件", "Nafeza Paper")}
            </label>
            <label className="check-row">
              <input type="checkbox" checked={data.check_form_4_6} onChange={e => updateField("check_form_4_6", e.target.checked)} />
              {t("Form 4 或 6", "Form 4 or 6")}
            </label>
          </>
        ) : (
          <>
            <label className="check-row">
              <input type="checkbox" checked={data.check_cover} onChange={e => updateField("check_cover", e.target.checked)} />
              {t("封面及结算核对", "Cover & Settlement Check")}
            </label>
            <label className="check-row">
              <input type="checkbox" checked={data.check_invoices} onChange={e => updateField("check_invoices", e.target.checked)} />
              {t("发票金额核对", "Invoices Match Amount")}
            </label>
            <label className="check-row">
              <input type="checkbox" checked={data.check_company_name} onChange={e => updateField("check_company_name", e.target.checked)} />
              {t("封面公司名与发票公司名一致", "Company Name on Cover Matches Invoices")}
            </label>
            <label className="check-row">
              <input type="checkbox" checked={data.check_wht_cert} onChange={e => updateField("check_wht_cert", e.target.checked)} />
              {t("免WHT公司提供WHT证明", "WHT-Free Company Provided WHT Certificate")}
            </label>
          </>
        )}
      </div>
      <div className="card">
        <h3>{t("审计备注", "Audit Notes")}</h3>
        <FastInput className="audit-notes" value={data.audit_notes} onChange={v => updateField("audit_notes", v)} rows={5} />
      </div>
    </div>
    );
  };

  const ImportTab = () => {
    return (
      <div className="import-tab">
        <div className="card" style={{background:'linear-gradient(135deg, var(--bg-card) 0%, rgba(59,130,246,0.03) 100%)'}}>
          <h3>{t("进口文件信息", "Import Document Info")}</h3>

          {/* Commercial Invoice Row */}
          <div style={{display:'grid',gridTemplateColumns:'1fr 130px',gap:12,marginBottom:16}}>
            <div style={{display:'grid',gridTemplateColumns:'1fr 130px',gap:8,alignItems:'end'}}>
              <Input label={t("商业发票金额", "Commercial Invoice Amount")} value={data.import_commercial_amount} onChange={v => updateField("import_commercial_amount", v)} />
              <Input label={t("汇率", "Rate")} value={data.import_commercial_rate} onChange={v => updateField("import_commercial_rate", v)} />
            </div>
            <div className="field" style={{alignSelf:'end'}}>
              <label className="field-label" style={{color:'var(--accent)',fontWeight:700}}>{t("总额 (EGP)", "Total (EGP)")}</label>
              <div className="computed-value highlight" style={{fontSize:14,fontWeight:700,padding:'9px 12px',wordBreak:'break-word',overflowWrap:'anywhere',whiteSpace:'normal',minWidth:0}}>{fmt((parseFloat(data.import_commercial_amount)||0) * ((parseFloat(data.import_commercial_rate)||0) || 1))}</div>
            </div>
          </div>

          {data.import_commercial_rate && (data.import_entries ?? []).some((e: any) => e.rate && e.rate !== data.import_commercial_rate) && (
            <div className="field-warning" style={{color:'var(--red)',marginBottom:12}}>
              {t("警告: 汇率与其他发票不一致!", "Warning: Rate differs from other invoices!")}
            </div>
          )}

          {/* Cost Breakdown */}
          <div style={{borderTop:'1px solid var(--border)',paddingTop:14,marginTop:4}}>
            <div style={{display:'flex',alignItems:'center',justifyContent:'space-between',marginBottom:10}}>
              <h4 style={{fontSize:11,fontWeight:700,color:'var(--text-muted)',textTransform:'uppercase',letterSpacing:'0.06em',margin:0}}>{t("成本拆分", "Cost Breakdown")}</h4>
              <button className="btn-add" style={{marginTop:0,padding:'4px 10px',fontSize:11}} onClick={addCostRow}>+ {t("添加费用", "Add Cost")}</button>
            </div>
            {(data.import_costs ?? [{name:"",amount:"0.00"}]).map((c: any, i: number) => (
              <div key={i} style={{display:'grid',gridTemplateColumns:'1fr 130px 30px',gap:8,alignItems:'center',marginBottom:6}}>
                <FastInput value={c.name} onChange={v => updCostRow(i, "name", v)} rows={1} />
                <FastInput value={c.amount} onChange={v => updCostRow(i, "amount", v)} />
                {(data.import_costs ?? []).length > 1 ? (
                  <button className="btn-danger" style={{padding:'6px 8px',height:32}} onClick={() => delCostRow(i)}>✕</button>
                ) : <div />}
              </div>
            ))}
            <div style={{display:'flex',justifyContent:'flex-end',marginTop:10}}>
              <div style={{minWidth:280}}>
                <Computed label={t("成本合计", "Total Costs")} value={computed.import_total_costs} highlight />
              </div>
            </div>
          </div>
        </div>
          <div className="card" style={{overflowX:'auto',background:'linear-gradient(135deg, var(--bg-card) 0%, rgba(59,130,246,0.03) 100%)'}}>
          <h3>{t("服务商", "Service Providers")}</h3>
            <div className="invoice-header" style={{display:'grid',gridTemplateColumns:'minmax(200px, 1.5fr) minmax(110px, 1fr) 80px 50px 80px 80px 50px 1fr 100px 100px 110px 110px 30px',gap:6,fontSize:11,fontWeight:600,marginBottom:8,alignItems:'end'}}>
              <div style={{paddingTop:14,paddingLeft:10}}>{t("服务名称", "Service")}</div>
              <div style={{paddingTop:14,paddingLeft:10}}>{t("金额", "Amount")}</div>
              <div style={{paddingTop:14,paddingLeft:10}}>{t("汇率", "Rate")}</div>
              <div style={{paddingTop:14,textAlign:'center'}}>{t("免WHT", "Free")}</div>
              <div style={{paddingTop:14,paddingLeft:10}}>{t("VAT率", "VAT")}</div>
              <div style={{paddingTop:14,paddingLeft:10}}>{t("WHT率", "WHT")}</div>
              <div style={{paddingTop:14,textAlign:'center'}}>{t("临时工", "Temp")}</div>
              <div></div> {/* Spacer header */}
              <div style={{paddingTop:14,paddingLeft:10}}>{t("VAT", "VAT")}</div>
              <div style={{paddingTop:14,paddingLeft:10}}>{t("WHT", "WHT")}</div>
              <div style={{paddingTop:14,paddingLeft:10}}>{t("净额", "Net")}</div>
              <div style={{paddingTop:14,paddingLeft:10}}>{t("含税合计", "+VAT")}</div>
              <div></div>
            </div>
          {(data.import_entries ?? []).map((e: any, i: number) => {
            const amt = parseFloat(e.amount) || 0;
            const r = parseFloat(e.rate) || 1;
            const egpAmt = r ? amt * r : amt;
            const vatRate = parseFloat((e.vat_rate || "0%").replace('%', '')) || 0;
            const vat = Math.round(egpAmt * vatRate / 100 * 100) / 100;
            const whtRate = parseFloat((e.wht_rate || "0%").replace('%', '')) || 0;
            const wht = e.free_wht ? 0 : Math.round(egpAmt * whtRate / 100 * 100) / 100;
            return (
              <div key={i} className="invoice-row" style={{display:'grid',gridTemplateColumns:'minmax(200px, 1.5fr) minmax(110px, 1fr) 80px 50px 80px 80px 50px 1fr 100px 100px 110px 110px 30px',gap:6,alignItems:'center'}}>
                <FastInput value={e.service_name} onChange={v => updImportEntry(i, "service_name", v)} rows={1} />
                <FastInput value={e.amount} onChange={v => updImportEntry(i, "amount", v)} />
                <FastInput value={e.rate} onChange={v => updImportEntry(i, "rate", v)} />
                <input type="checkbox" checked={e.free_wht} onChange={() => updImportEntry(i, "free_wht", !e.free_wht)} style={{margin:'auto'}} />
                <select className="field-select" style={{padding:'7px 4px 7px 8px',fontSize:11}} value={e.vat_rate} onChange={ev => updImportEntry(i, "vat_rate", ev.target.value)}>
                  {["0%","5%","9%","10%","14%"].map(o => <option key={o} value={o}>{o}</option>)}
                </select>
                <select className="field-select" style={{padding:'7px 4px 7px 8px',fontSize:11}} value={e.wht_rate} onChange={ev => updImportEntry(i, "wht_rate", ev.target.value)}>
                  {["0%","0.5%","3%","5%","10%"].map(o => <option key={o} value={o}>{o}</option>)}
                </select>
                <input type="checkbox" checked={e.temp_labour} onChange={() => updImportEntry(i, "temp_labour", !e.temp_labour)} style={{margin:'auto'}} />
                <div></div> {/* Spacer cell */}
                <div className="computed-value" style={{fontSize:11,padding:'7px 10px',wordBreak:'break-all'}}>{fmtShort(vat)}</div>
                <div className="computed-value" style={{fontSize:11,padding:'7px 10px',wordBreak:'break-all'}}>{fmtShort(wht)}</div>
                <div className="computed-value" style={{fontSize:11,fontWeight:600,padding:'7px 10px',wordBreak:'break-all'}}>{fmtShort(egpAmt + vat - wht)}</div>
                <div className="computed-value" style={{fontSize:11,fontWeight:600,padding:'7px 10px',wordBreak:'break-all'}}>{fmtShort(egpAmt + vat)}</div>
                <button className="btn-danger" style={{padding:'7px 10px'}} onClick={() => delImportEntry(i)}>✕</button>
              </div>
            );
          })}
          <button className="btn-add" onClick={addImportEntry} style={{marginTop:8}}>+ {t("添加服务商", "Add Provider")}</button>
        </div>
        <div className="card" style={{background:'linear-gradient(135deg, var(--bg-card) 0%, rgba(59,130,246,0.03) 100%)'}}>
          <h3>{t("进口汇总", "Import Summary")}</h3>
          <div style={{display:'grid',gridTemplateColumns:'repeat(auto-fit, minmax(220px, 1fr))',gap:16,marginTop:12}}>
            <Computed label={t("总额 (金额+VAT)", "Grand Total (Amount+VAT)")} value={computed.import_grand_total} highlight />
            <Computed label={t("净额 (总额-WHT)", "Grand Net (Total-WHT)")} value={computed.import_grand_net} highlight />
            <Computed label={t("临时工社保 (服务金额 × 0.45%)", "Temp Labour (Services × 0.45%)")} value={computed.import_temp_labour} highlight />
          </div>
        </div>
      </div>
    );
  };

  const newSession = useCallback(async () => {
    const confirmed = window.confirm(t("确定要开始新会话吗？当前未保存的更改将丢失。", "Start a new session? Any unsaved changes will be lost."));
    if (!confirmed) return;
    formRef.current = { ...EMPTY_FORM };
    setComputed(EMPTY_CALC);
    setTab("bank");
    await recalc(formRef.current);
    try { await invoke("save_config", { data: formRef.current }); } catch {}
  }, [t, recalc]);

  const exportExcel = async () => {
    const path = await save({ defaultPath: `${data.doc_serial || "CSCEC_Settlement"}.xlsx`, filters: [{ name: "Excel", extensions: ["xlsx"] }] });
    if (path) {
      await invoke("export_excel", { data, computed, filePath: path });
      showAlert(t("导出成功", "Export successful"));
    }
  };

  const importPdf = async () => {
    const path = await open({ filters: [{ name: "PDF", extensions: ["pdf"] }], multiple: false });
    if (path) {
      const overlay = overlayRef.current;
      if (overlay) {
        overlay.style.display = 'flex';
        void overlay.offsetHeight;
      }
      setProgressMsg(t("正在准备导入...", "Preparing import..."));
      importTimerRef.current = setTimeout(() => {
        hideOverlay();
        console.warn("import timed out, overlay auto-hidden");
      }, 60000);
      emit("start-import", { filePath: path });
    }
  };

  const [historyList, setHistoryList] = useState<HistoryEntry[]>([]);
  const [historySearch, setHistorySearch] = useState("");
  const [historyLoading, setHistoryLoading] = useState(false);
  const [isSerialDuplicate, setIsSerialDuplicate] = useState(false);
  const historyOverlayRef = useRef<HTMLDivElement>(null);

  const showHistoryModal = () => {
    if (historyOverlayRef.current) {
      historyOverlayRef.current.style.visibility = 'visible';
      historyOverlayRef.current.style.opacity = '1';
      historyOverlayRef.current.style.pointerEvents = 'auto';
    }
    loadHistoryList("");
  };

  const hideHistoryModal = () => {
    if (historyOverlayRef.current) {
      historyOverlayRef.current.style.visibility = 'hidden';
      historyOverlayRef.current.style.opacity = '0';
      historyOverlayRef.current.style.pointerEvents = 'none';
    }
  };

  const loadHistoryList = async (search = "") => {
    setHistoryLoading(true);
    try {
      const list = await invoke<HistoryEntry[]>("list_history", { search });
      setHistoryList(list);
    } catch (e) {
      console.error("list_history failed", e);
      setHistoryList([]);
    }
    setHistoryLoading(false);
  };

  // Debounced duplicate serial check
  const serialCheckTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(() => {
    const serial = data.doc_serial;
    if (!serial) { setIsSerialDuplicate(false); return; }
    if (serialCheckTimer.current) clearTimeout(serialCheckTimer.current);
    serialCheckTimer.current = setTimeout(async () => {
      try {
        const exists = await invoke<boolean>("check_serial_exists", { serial });
        setIsSerialDuplicate(exists);
      } catch { setIsSerialDuplicate(false); }
    }, 500);
    return () => { if (serialCheckTimer.current) clearTimeout(serialCheckTimer.current); };
  }, [data.doc_serial]);

  const saveSnapshot = async () => {
    const serial = data.doc_serial;
    if (serial) {
      const exists = await invoke<boolean>("check_serial_exists", { serial });
      if (exists) {
        showAlert(t("该文档编号已存在，无法重复保存", "This document serial already exists, cannot save duplicate"));
        return;
      }
    }
    const label = serial || `Snapshot-${new Date().toLocaleDateString()}`;
    const dataJson = JSON.stringify(data);
    await invoke("save_history", { label, notes: "", dataJson });
    showAlert(`${t("快照已保存", "Snapshot saved")} (${label})`);
  };

  const loadSnapshot = async (id: number) => {
    try {
      const dataJson = await invoke<string>("load_history", { id });
      const parsed = JSON.parse(dataJson);
      if (!parsed.import_costs && (parsed.import_cost_1 !== undefined)) {
        parsed.import_costs = [
          { name: parsed.import_cost_1_label || "Foreign Cost", amount: parsed.import_cost_1 || "0.00" },
          { name: parsed.import_cost_2_label || "Domestic Cost", amount: parsed.import_cost_2 || "0.00" },
          { name: parsed.import_cost_3_label || "Nafeza Paper", amount: parsed.import_cost_3 || "0.00" },
        ];
      }
      if (!parsed.import_costs) parsed.import_costs = [...DEFAULT_FORM.import_costs];
      formRef.current = parsed;
      await recalc(parsed);
    } catch (e) {
      console.error("loadSnapshot failed", e);
    }
    hideHistoryModal();
  };

  const historySearchTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const onHistorySearch = (v: string) => {
    setHistorySearch(v);
    if (historySearchTimer.current) clearTimeout(historySearchTimer.current);
    historySearchTimer.current = setTimeout(() => loadHistoryList(v), 300);
  };

  const deleteSnapshot = async (id: number) => {
    await invoke("delete_history", { id });
    loadHistoryList(historySearch);
  };

  return (
    <div className="app">
      <div ref={overlayRef} className="loading-overlay" style={{ display: 'none' }}>
        <div className="loading-modal">
          <div className="loading-spinner" />
          <p className="loading-message">{progressMsg || t("正在导入PDF，请稍候...", "Importing PDF, please wait...")}</p>
        </div>
      </div>
      {modalMsg !== null && (
        <div style={{
          position: 'fixed', top: 0, left: 0, right: 0, bottom: 0,
          background: 'rgba(0,0,0,0.45)', display: 'flex', alignItems: 'center', justifyContent: 'center',
          zIndex: 10000,
        }} onClick={() => setModalMsg(null)}>
          <div style={{
            background: 'var(--bg-card, #fff)', borderRadius: 12, padding: '28px 36px',
            boxShadow: '0 8px 32px rgba(0,0,0,0.18)', maxWidth: 420, minWidth: 280,
            textAlign: 'center', fontSize: 15, color: 'var(--text-primary, #222)',
            border: '1px solid var(--border, #e0e0e0)',
          }} onClick={e => e.stopPropagation()}>
            <div style={{ marginBottom: 20, whiteSpace: 'pre-wrap', lineHeight: 1.6 }}>{modalMsg}</div>
            <button style={{
              padding: '8px 32px', borderRadius: 6, border: 'none',
              background: 'var(--accent, #00529B)', color: '#fff', fontWeight: 600,
              cursor: 'pointer', fontSize: 14,
            }} onClick={() => setModalMsg(null)}>OK</button>
          </div>
        </div>
      )}
      <aside className="sidebar">
        <div className="sidebar-header">
          <div className="brand">
            <div className="brand-icon">
              <img src="/assets/cscec.png" alt="CSCEC" />
            </div>
            <div className="brand-text">
              <h2>CSCEC</h2>
              <p>{t("付款凭证", "Payment Voucher")}</p>
            </div>
          </div>
        </div>
        <div className="sidebar-metrics">
          <div className="metric">
            <span className={`metric-label${computed.c_9A < 0 ? ' negative' : ''}`}>{fmt(computed.c_9A)}</span>
            <span className="metric-sub">{t("应付净额", "Net Payable")}</span>
          </div>
          <div className="metric">
            <span className={`metric-label${computed.total_deductions < 0 ? ' negative' : ''}`} style={{color: '#f59e0b'}}>{fmt(computed.total_deductions)}</span>
            <span className="metric-sub">{t("扣款合计", "Deductions")}</span>
          </div>
          <div className="metric">
            <span className={`metric-label${computed.c_11B < 0 ? ' negative' : ''}`}>{fmt(computed.c_11B)}</span>
            <span className="metric-sub">{t("期末已付", "Accum. Paid")}</span>
          </div>
        </div>
        <nav className="sidebar-nav">
          <button className={tab === "bank" ? "active" : ""} onClick={() => setTab("bank")}>{t("银行", "Bank")}</button>
          <button className={tab === "import" ? "active" : ""} onClick={() => setTab("import")}>{t("进口", "Import")}</button>
          <button className={tab === "final_decision" ? "active" : ""} onClick={() => setTab("final_decision")}>{t("最终决定", "Final Decision")}</button>
        </nav>
        <div className="sidebar-lang">
          <button onClick={() => setLang(lang === "zh" ? "en" : "zh")}>
            {lang === "zh" ? "English" : "中文"}
          </button>
        </div>
        <div className="sidebar-actions">
          <button onClick={newSession}>{t("🆕 新会话", "🆕 New Session")}</button>
          <button onClick={saveSnapshot}>{t("💾 保存快照", "💾 Save Snapshot")}</button>
          <button onClick={exportExcel}>{t("📥 导出Excel", "📥 Export Excel")}</button>
          <button onClick={importPdf}>{t("📄 导入PDF", "📄 Import PDF")}</button>
          <button onClick={showHistoryModal}>{t("📂 历史记录", "📂 History")}</button>
        </div>
      </aside>
      <main className="content">
        {tab === "bank" ? (
          <>
            {Card1()}
            <div className="card-row">
              {Card2()}{Card3()}
            </div>
            <div className="card-row">
              {Card4()}{Card5()}
            </div>
            <div className="card-row">
              {Card6()}{Card78()}
            </div>
            <div className="card-row">
              {Card9()}{Card10()}
            </div>
            {Card11()}
          </>
        ) : tab === "import" ? (
          ImportTab()
        ) : (
          AuditTab()
        )}
      </main>

      <div ref={historyOverlayRef} className="modal-overlay" style={{ visibility: 'hidden', opacity: 0, pointerEvents: 'none' }} onClick={hideHistoryModal}>
        <div className="modal" onClick={e => e.stopPropagation()}>
          <div className="modal-header">
            <h3>{t("历史记录", "History Browser")}</h3>
            <button className="modal-close" onClick={hideHistoryModal}>✕</button>
          </div>
          <div className="modal-search">
            <input className="field-input" placeholder={t("搜索快照...", "Search snapshots...")} value={historySearch} onChange={e => onHistorySearch(e.target.value)} />
          </div>
          <div className="history-list" style={historyLoading ? { opacity: 0.5 } : {}}>
            {historyLoading ? (
              <div className="history-empty">{t("加载中...", "Loading...")}</div>
            ) : historyList.map(h => (
              <div key={h.id} className="history-item">
                <div>
                  <strong>{h.label}</strong>
                  <p><small>{h.created_at}</small></p>
                </div>
                <div className="history-actions">
                  <button className="btn-load" onClick={() => loadSnapshot(h.id)}>Load</button>
                  <button className="btn-delete" onClick={() => deleteSnapshot(h.id)}>Delete</button>
                </div>
              </div>
            ))}
            {historyList.length === 0 && <div className="history-empty">{t("未找到快照", "No snapshots found")}</div>}
          </div>
        </div>
      </div>
    </div>
  );
}

export default App;
