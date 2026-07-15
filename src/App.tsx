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

interface InvoiceData {
  invoice_no: string;
  seller_tax_id: string;
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
  check_cover: boolean; check_invoices: boolean; audit_notes: string;
  vat_manual: boolean; wht_manual: boolean; oth_manual: boolean; soc_manual: boolean;
  invoices: InvoiceData[];
  vat_rows: RateRow[]; wht_rows: RateRow[]; oth_rows: RateRow[]; soc_rows: RateRow[];
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
}

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
  check_cover: false, check_invoices: false, audit_notes: "",
  vat_manual: false, wht_manual: false, oth_manual: false, soc_manual: false,
  invoices: [],
  vat_rows: [{ amount: "0.00", rate: "0%" }],
  wht_rows: [{ amount: "0.00", rate: "0%" }],
  oth_rows: [{ amount: "0.00", rate: "0%" }],
  soc_rows: [{ amount: "0.00", rate: "0%" }],
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
};

const fmt = (v: number) => `EGP ${v.toLocaleString("en-US", { minimumFractionDigits: 2, maximumFractionDigits: 2 })}`;

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

interface HistoryEntry {
  id: number; label: string; notes: string; created_at: string;
}

function App() {
  const [tab, setTab] = useState<"settlement" | "audit">("settlement");
  const [lang, setLang] = useState<"zh" | "en">("zh");
  const t = useCallback((zh: string, en: string) => lang === "zh" ? zh : en, [lang]);
  const formRef = useRef<FormData>(DEFAULT_FORM);
  const [computed, setComputed] = useState<CalcResult>(EMPTY_CALC);
  const overlayRef = useRef<HTMLDivElement>(null);
  const [progressMsg, setProgressMsg] = useState("");

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

  // Manual wheel handler + heartbeat to keep event loop alive
  useEffect(() => {
    const el = document.querySelector('.content');
    if (!el) return;
    const handler: EventListener = (e) => {
      (el as HTMLElement).scrollTop += (e as WheelEvent).deltaY;
      e.preventDefault();
    };
    el.addEventListener('wheel', handler, { passive: false });
    const id = setInterval(() => { invoke("ping"); }, 4000);
    return () => { el.removeEventListener('wheel', handler); clearInterval(id); };
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
      alert(t("PDF导入成功", "PDF imported successfully"));
    });
    return () => { unlisten.then(f => f()); };
  }, [recalc, t, saveConfig, hideOverlay]);

  // Listen for background import error
  useEffect(() => {
    const unlisten = listen<{status: string, message: string}>("import-error", (event) => {
      hideOverlay();
      alert(`${t("导入失败", "Import failed")}: ${event.payload.message}`);
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
          <input className="field-input small" type="text" value={r.amount} onChange={e => updateNested(parent, i, "amount", e.target.value)} />
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

  const AuditTab = () => (
    <div className="audit-tab">
      <div className="card">
        <h3>{t("文件信息", "Document Information")}</h3>
        <Input label={t("文档编号", "Doc Serial")} value={data.doc_serial} onChange={v => updateField("doc_serial", v)} />
        <div className="field"><label className="field-label">{t("日期", "Date")}</label><div className="computed-value">{new Date().toLocaleDateString()}</div></div>
        <Input label={t("买方税号", "Buyer TAX ID")} value={data.buyer_tax_id} onChange={v => updateField("buyer_tax_id", v)} />
        {data.buyer_tax_id && data.buyer_tax_id !== "100489095" && (
          <div className="field-warning">{t("警告: 买方税号不是 100489095!", "Warning: Buyer TAX ID is not 100489095!")}</div>
        )}
        <div className="field"><label className="field-label">{t("卖方税号", "Seller TAX IDs")}</label></div>
        {data.seller_tax_ids.map((tid, i) => (
          <div key={i} className="invoice-row" style={{marginTop: 4}}>
            <input className="field-input" value={tid} onChange={e => updSellerTaxId(i, e.target.value)} />
            <button className="btn-danger" onClick={() => delSellerTaxId(i)}>✕</button>
          </div>
        ))}
        <button className="btn-add" onClick={addSellerTaxId}>+ {t("添加卖方税号", "Add Seller TAX ID")}</button>
      </div>
      <div className="card">
        <h3>{t("发票", "Invoices")}</h3>
        <div className="invoice-header">
          <span>{t("发票号", "Invoice No")}</span><span>{t("金额", "Amount")}</span><span></span>
        </div>
        {data.invoices.map((inv, i) => (
          <div key={i} className="invoice-row">
            <input className="field-input" value={inv.invoice_no} onChange={e => updInv(i, "invoice_no", e.target.value)} />
            <input className="field-input" type="text" value={inv.amount} onChange={e => updInv(i, "amount", e.target.value)} />
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
      <div className="card">
        <h3>{t("核对清单", "Verification Checklist")}</h3>
        <label className="check-row">
          <input type="checkbox" checked={data.check_cover} onChange={e => updateField("check_cover", e.target.checked)} />
          {t("封面及结算核对", "Cover & Settlement Check")}
        </label>
        <label className="check-row">
          <input type="checkbox" checked={data.check_invoices} onChange={e => updateField("check_invoices", e.target.checked)} />
          {t("发票金额核对", "Invoices Match Amount")}
        </label>
      </div>
      <div className="card">
        <h3>{t("审计备注", "Audit Notes")}</h3>
        <textarea className="audit-notes" value={data.audit_notes} onChange={e => updateField("audit_notes", e.target.value)} rows={5}
          onKeyDown={e => { if (e.key === 'Escape') { e.currentTarget.blur(); } }} />
      </div>
    </div>
  );

  const exportExcel = async () => {
    const path = await save({ defaultPath: `${data.doc_serial || "CSCEC_Settlement"}.xlsx`, filters: [{ name: "Excel", extensions: ["xlsx"] }] });
    if (path) {
      await invoke("export_excel", { data, computed, filePath: path });
      alert(t("导出成功", "Export successful"));
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

  const [showHistory, setShowHistory] = useState(false);
  const [historyList, setHistoryList] = useState<HistoryEntry[]>([]);
  const [historySearch, setHistorySearch] = useState("");

  const loadHistoryList = async (search = "") => {
    const list = await invoke<HistoryEntry[]>("list_history", { search });
    setHistoryList(list);
  };

  const saveSnapshot = async () => {
    const label = data.doc_serial || `Snapshot-${new Date().toLocaleDateString()}`;
    const dataJson = JSON.stringify(data);
    await invoke("save_history", { label, notes: "", dataJson });
    alert(`${t("快照已保存", "Snapshot saved")} (${label})`);
  };

  const loadSnapshot = async (id: number) => {
    const dataJson = await invoke<string>("load_history", { id });
    const parsed = JSON.parse(dataJson);
    formRef.current = parsed;
    recalc(parsed);
    setShowHistory(false);
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
          <button className={tab === "settlement" ? "active" : ""} onClick={() => setTab("settlement")}>{t("结算", "Settlement")}</button>
          <button className={tab === "audit" ? "active" : ""} onClick={() => setTab("audit")}>{t("审计", "Audit")}</button>
        </nav>
        <div className="sidebar-lang">
          <button onClick={() => setLang(lang === "zh" ? "en" : "zh")}>
            {lang === "zh" ? "English" : "中文"}
          </button>
        </div>
        <div className="sidebar-actions">
          <button onClick={saveSnapshot}>{t("💾 保存快照", "💾 Save Snapshot")}</button>
          <button onClick={exportExcel}>{t("📥 导出Excel", "📥 Export Excel")}</button>
          <button onClick={importPdf}>{t("📄 导入PDF", "📄 Import PDF")}</button>
          <button onClick={() => { loadHistoryList(""); setShowHistory(true); }}>{t("📂 历史记录", "📂 History")}</button>
        </div>
      </aside>
      <main className="content">
        {tab === "settlement" ? (
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
        ) : (
          AuditTab()
        )}
      </main>

      {showHistory && (
        <div className="modal-overlay" onClick={() => setShowHistory(false)}>
          <div className="modal" onClick={e => e.stopPropagation()}>
            <div className="modal-header">
              <h3>{t("历史记录", "History Browser")}</h3>
              <button className="modal-close" onClick={() => setShowHistory(false)}>✕</button>
            </div>
            <div className="modal-search">
              <input className="field-input" placeholder={t("搜索快照...", "Search snapshots...")} value={historySearch} onChange={e => { setHistorySearch(e.target.value); loadHistoryList(e.target.value); }} />
            </div>
            <div className="history-list">
              {historyList.map(h => (
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
      )}
    </div>
  );
}

export default App;
