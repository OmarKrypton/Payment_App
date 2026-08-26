import { useState, useCallback, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getVersion } from "@tauri-apps/api/app";
import { emit, listen } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";
import "./App.css";
import { supabase, signIn, signOut, getSession, saveSnapshotRemote, listSnapshotsRemote, loadSnapshotRemote, updateSnapshotRemote, deleteSnapshotRemote, changePassword, requestDeleteSnapshot, approveDeleteSnapshot, rejectDeleteSnapshot, listPoolRemoteByIds, listPoolRemoteMeta, upsertPoolInvoicesRemote, markPoolUsedRemote, markPoolAvailableRemote, markPoolsAvailableRemote, deletePoolInvoiceRemote, requestPoolDeleteRemote, rejectPoolDeleteRemote, markPoolsUsedRemote } from "./supabase";
import { IconSave, IconHistory, IconNewSession, IconImport, IconExport, IconChevronDown, IconReport, IconInvoice } from "./icons";
import { checkForUpdate, performUpdate } from "./update";

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
  attached_invoice: string;
  seller_tax_id?: string;
}

interface InvoiceData {
  invoice_no: string;
  seller_tax_id: string;
  amount: string;
  vat?: string;
  wht?: string;
  company_name?: string;
  attached_invoice?: string;
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
  final_decision: string; conditional_reason: string; reject_reason: string; auditor: string;
  vat_manual: boolean; wht_manual: boolean; oth_manual: boolean; soc_manual: boolean;
  wht_manual_amount: boolean;
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
  final_decision: "", conditional_reason: "", reject_reason: "", auditor: "",
  vat_manual: false, wht_manual: false, oth_manual: false, soc_manual: false, wht_manual_amount: false,
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
  final_decision: "", conditional_reason: "", reject_reason: "", auditor: "",
  vat_manual: false, wht_manual: false, oth_manual: false, soc_manual: false, wht_manual_amount: false,
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

function FastInput({ value, onChange, className, type, rows, style }: {
  value: string; onChange: (v: string) => void; className?: string; type?: string; rows?: number; style?: React.CSSProperties;
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
      style={{ resize: 'none', overflowY: 'hidden', minHeight: '32px', ...style }}
      onChange={e => { setLocal(e.target.value); onChange(e.target.value); }}
      onKeyDown={e => { if (e.key === 'Escape') e.currentTarget.blur(); }}
    />;
  }
  return <input className={className || "field-input"} type={type || "text"} value={local}
    style={style}
    onChange={e => { setLocal(e.target.value); onChange(e.target.value); }} />;
}

interface HistoryEntry {
  id: number; label: string; notes: string; created_at: string; owner?: string;
  delete_requested_at?: string | null; delete_requested_by?: string | null;
  data_json?: string; final_decision?: string; doc_type?: string; auditor?: string;
}

const normalizeId = (s: string) => (s || "").toUpperCase().replace(/\s+/g, "");

// Mirrors service_matches_invoice in src-tauri/src/eta_xml.rs: returns true when an
// invoice id appears in a service name (as a whole "Inv:" suffix or any standalone token).
const serviceNameContainsInvoice = (serviceName: string, invoiceId: string): boolean => {
  const invNorm = normalizeId(invoiceId);
  if (!invNorm) return false;
  const upper = (serviceName || "").toUpperCase();
  const candidates: string[] = [normalizeId(upper)];
  const invIdx = upper.indexOf("INV:");
  if (invIdx >= 0) candidates.push(normalizeId(upper.slice(invIdx + 4)));
  for (const token of upper.split(/[^A-Z0-9]+/)) {
    if (normalizeId(token) === invNorm) return true;
  }
  return candidates.some((c) => {
    if (!c) return false;
    return c === invNorm || (invNorm.length >= 4 && (c.endsWith(invNorm) || c.startsWith(invNorm)));
  });
};

function App() {
  const [tab, setTab] = useState<"bank" | "final_decision" | "import">("bank");
  const [lang, setLang] = useState<"zh" | "en">("zh");
  const [appVersion, setAppVersion] = useState("");
  const [checkingUpdate, setCheckingUpdate] = useState(false);
  const t = useCallback((zh: string, en: string) => lang === "zh" ? zh : en, [lang]);
  const formRef = useRef<FormData>(DEFAULT_FORM);
  const [computed, setComputed] = useState<CalcResult>(EMPTY_CALC);
  const overlayRef = useRef<HTMLDivElement>(null);
  const [progressMsg, setProgressMsg] = useState("");
  const [modalMsg, setModalMsg] = useState<string | null>(null);
  const [authEmail, setAuthEmail] = useState("");
  const [authPassword, setAuthPassword] = useState("");
  const [newPassword, setNewPassword] = useState("");
  const [showChangePw, setShowChangePw] = useState(false);
  const [showInvoiceExport, setShowInvoiceExport] = useState(false);
  const [showExportMenu, setShowExportMenu] = useState(false);
  const [showImportMenu, setShowImportMenu] = useState(false);
  const [invoiceExportFrom, setInvoiceExportFrom] = useState(new Date(new Date().getFullYear(), 0, 1).toISOString().slice(0, 10)); // Jan 1 of current year
  const [invoiceExportTo, setInvoiceExportTo] = useState(new Date().toISOString().slice(0, 10)); // Today
  const [authUser, setAuthUser] = useState<string | null>(null); // email of logged-in user
  const [authUserId, setAuthUserId] = useState<string | null>(null); // uuid for ownership checks
  const [isAdminUser, setIsAdminUser] = useState(false);
  const [synced, setSynced] = useState(false);
  const [rateVisible, setRateVisible] = useState(true);
  const [historyFilter, setHistoryFilter] = useState<"all" | "bank" | "import">("all");
  const [etaResult, setEtaResult] = useState<any[] | null>(null);
  const [showPool, setShowPool] = useState(false);
  const [poolList, setPoolList] = useState<any[]>([]);
  const [poolLoading, setPoolLoading] = useState(false);
  const [poolSyncInfo, setPoolSyncInfo] = useState<{ ok: boolean; local: number; cloud: number; pushed: number; pulled: number; error?: string }>({ ok: true, local: 0, cloud: 0, pushed: 0, pulled: 0 });
  const [poolSearch, setPoolSearch] = useState("");
  const [poolTab, setPoolTab] = useState<"unclaimed" | "claimed">("unclaimed");
  const [poolMode, setPoolMode] = useState<"validate" | "select">("validate");
  const [poolSelected, setPoolSelected] = useState<Set<number>>(new Set());
  const [poolDateFrom, setPoolDateFrom] = useState("");
  const [poolDateTo, setPoolDateTo] = useState("");
  const [poolSeller, setPoolSeller] = useState("all");
  const [poolCurrency, setPoolCurrency] = useState("all");
  const [poolDocFilter, setPoolDocFilter] = useState<"all" | "Valid" | "Rejected" | "Cancelled">("all");
  const [poolImportProgress, setPoolImportProgress] = useState<{ processed: number; total: number; file: string } | null>(null);
  const [resultSearch, setResultSearch] = useState("");
  const [overwriteTarget, setOverwriteTarget] = useState<{ id: number; label: string; remote: boolean } | null>(null);

  // #6 VAT/WHT rate memory per seller tax ID (persisted locally so re-imports
  // prefill with the last-used rates for that seller).
  const [sellerRates, setSellerRates] = useState<Record<string, { vat: string; wht: string; rate: string }>>(() => {
    try {
      return JSON.parse(localStorage.getItem("seller_rates") || "{}");
    } catch {
      return {};
    }
  });
  const rememberSellerRates = (taxId: string, rates: { vat: string; wht: string; rate: string }) => {
    if (!taxId) return;
    setSellerRates(prev => {
      const next = { ...prev, [taxId]: rates };
      try { localStorage.setItem("seller_rates", JSON.stringify(next)); } catch {}
      return next;
    });
  };

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

  // Init: load local config and restore Supabase session
  useEffect(() => {
    (async () => {
      try {
        const cfg = await invoke<FormData>("load_config");
        formRef.current = cfg;
        recalc(cfg);
      } catch (e) {
        console.error("load_config failed", e);
      }
      // Restore Supabase session
      try {
        const session = await getSession();
        if (session?.user?.email) {
          setAuthUser(session.user.email);
          setAuthUserId(session.user.id);
          setIsAdminUser(session.user.app_metadata?.role === "admin");
          setSynced(true);
        }
      } catch {}
    })();
  }, []);

  // Load app version for display in the sidebar
  useEffect(() => {
    getVersion().then(setAppVersion).catch(() => {});
  }, []);

  const handleCheckUpdate = async () => {
    if (checkingUpdate) return;
    setCheckingUpdate(true);
    try {
      const res = await checkForUpdate();
      if (res.available) {
        await performUpdate();
      } else {
        showAlert(t("已是最新版本", "You're up to date"));
      }
    } catch (e: any) {
      showAlert(`${t("检查更新失败", "Update check failed")}: ${e.message || e}`);
    } finally {
      setCheckingUpdate(false);
    }
  };

  // Close export dropdown when clicking outside
  useEffect(() => {
    if (!showExportMenu) return;
    const handler = (e: MouseEvent) => {
      const target = e.target as HTMLElement;
      if (!target.closest('.sidebar-export-group')) setShowExportMenu(false);
    };
    document.addEventListener('click', handler);
    return () => document.removeEventListener('click', handler);
  }, [showExportMenu]);

  // Close import dropdown when clicking outside
  useEffect(() => {
    if (!showImportMenu) return;
    const handler = (e: MouseEvent) => {
      const target = e.target as HTMLElement;
      if (!target.closest('.sidebar-export-group')) setShowImportMenu(false);
    };
    document.addEventListener('click', handler);
    return () => document.removeEventListener('click', handler);
  }, [showImportMenu]);

  // Listen for Supabase auth changes
  useEffect(() => {
    const { data: { subscription } } = supabase.auth.onAuthStateChange((_event, session) => {
      if (session?.user?.email) {
        setAuthUser(session.user.email);
        setAuthUserId(session.user.id);
        setIsAdminUser(session.user.app_metadata?.role === "admin");
        setSynced(true);
      } else {
        setAuthUser(null);
        setAuthUserId(null);
        setIsAdminUser(false);
        setSynced(false);
      }
    });
    return () => { subscription.unsubscribe(); };
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
      showAlert(t("PDF上传成功", "PDF uploaded successfully"));
    });
    return () => { unlisten.then(f => f()); };
  }, [recalc, t, saveConfig, hideOverlay]);

  // Listen for background import error
  useEffect(() => {
    const unlisten = listen<{status: string, message: string}>("import-error", (event) => {
      hideOverlay();
      showAlert(`${t("上传失败", "Upload failed")}: ${event.payload.message}`);
    });
    return () => { unlisten.then(f => f()); };
  }, [t, hideOverlay]);

  // Listen for pool import progress updates
  useEffect(() => {
    const unlisten = listen<{ processed: number; total: number; file: string }>("pool-import-progress", (event) => {
      setPoolImportProgress(event.payload);
    });
    return () => { unlisten.then(f => f()); };
  }, []);

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
      <Select label={t("保留金率", "Retention rate")} value={data.ret_rate} options={["0%", "0.5%", "3%", "5%", "10%", "15%"]} onChange={v => updateField("ret_rate", v)} />
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
      <div className="section-header" style={{flexWrap:'wrap',gap:8}}>
        <label className="toggle">
          <input type="checkbox" checked={data.wht_manual} onChange={() => toggleManual("wht_manual")} />
          {t("多税率", "Multi-Rate")}
        </label>
        <label className="toggle">
          <input type="checkbox" checked={data.wht_manual_amount} onChange={() => toggleManual("wht_manual_amount")} />
          {t("手动金额", "Manual Amount")}
        </label>
      </div>
      {data.wht_manual_amount ? (
        <>
          <Input label={t("本期预提税（手动，可为负值）", "Current WHT (manual, may be negative)")} value={data.val_6B} onChange={v => updateField("val_6B", v)} confidence={ocrConf("val_6B")} />
          <Computed label={t("本期预提税", "Current WHT")} value={computed.c_6B} />
        </>
      ) : data.wht_manual ? (
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
      <Select label={t("扣除费率", "Other rate")} value={data.oth_rate} options={["0%", "0.15%", "0.3%", "0.45%"]} onChange={v => updateField("oth_rate", v)} />
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

  const InvoicesCard = () => {
    const totals = (data.invoices ?? []).reduce((acc, inv) => {
      acc.net += parseFloat(inv.amount) || 0;
      acc.vat += parseFloat(inv.vat || "0") || 0;
      acc.wht += parseFloat(inv.wht || "0") || 0;
      return acc;
    }, { net: 0, vat: 0, wht: 0 });
    const invVatRate = totals.net > 0 ? (totals.vat / totals.net) * 100 : 0;
    const invWhtRate = totals.net > 0 ? (totals.wht / totals.net) * 100 : 0;
    const docVatRate = parseFloat((data.vat_rate || "0").replace('%', '')) || 0;
    const docWhtRate = parseFloat((data.wht_rate || "0").replace('%', '')) || 0;
    const matcher = (label: string, inv: number, doc: number, fmtV: (n: number) => string, active = true) => ({
      label, active, inv, doc, ok: Math.abs(inv - doc) <= 0.5, fmtV,
    });
    const comparisons = [
      matcher(t("净额 vs 结算−扣除(折扣)", "Net vs Settlement−Deductions"), totals.net, computed.c_1B - computed.c_1D, fmtShort),
      matcher(t("VAT vs 本期VAT", "VAT vs Current VAT Amount"), totals.vat, computed.c_1E, fmtShort),
      matcher(t("VAT率 vs 设定税率", "VAT Rate vs Set VAT Rate"), invVatRate, docVatRate, n => `${n.toFixed(1)}%`, docVatRate > 0),
      matcher(t("WHT vs 本期WHT", "WHT vs Current WHT Amount"), totals.wht, computed.c_6B, fmtShort, computed.c_6B > 0 || totals.wht > 0),
      matcher(t("WHT率 vs 设定税率", "WHT Rate vs Set WHT Rate"), invWhtRate, docWhtRate, n => `${n.toFixed(1)}%`, docWhtRate > 0),
      {
        label: t("本期实付 vs 净应付(9A)", "Current Paid vs Net Payable (9A)"),
        active: computed.c_9A > 0 || computed.c_10A > 0,
        inv: computed.c_10A,
        doc: computed.c_9A,
        ok: computed.c_10A <= computed.c_9A + 0.5,
        fmtV: fmtShort,
      },
    ];
    const shown = comparisons.filter(c => c.active);
    const hasInvoices = (data.invoices ?? []).length > 0;
    return (
    <div className="card">
      <h3>{t("发票", "Invoices")}</h3>
      <div className="invoice-header">
        <span>{t("公司名", "Company Name")}</span>
        <span>{t("发票号", "Invoice No")}</span>
        <span>{t("销售方税号", "Seller TAX ID")}</span>
        <span>{t("净额", "Net")}</span>
        <span>VAT</span>
        <span>WHT</span>
        <span></span>
      </div>
      {data.invoices.map((inv, i) => (
        <div key={i} className={`invoice-row${inv.attached_invoice ? ' has-pill' : ''}`}>
          <div className="invoice-company">
            {inv.attached_invoice && (
              <div className="attached-pill" title={`${t("已附加发票", "Attached invoice")}: ${inv.attached_invoice}`}>✓ {inv.attached_invoice}</div>
            )}
            <FastInput value={inv.company_name || ""} onChange={v => updInv(i, "company_name", v)} />
          </div>
          <FastInput value={inv.invoice_no} onChange={v => updInv(i, "invoice_no", v)} />
          <FastInput value={inv.seller_tax_id || ""} onChange={v => updInv(i, "seller_tax_id", v)} />
          <FastInput value={inv.amount} onChange={v => updInv(i, "amount", v)} />
          <FastInput value={inv.vat || "0.00"} onChange={v => updInv(i, "vat", v)} />
          <FastInput value={inv.wht || "0.00"} onChange={v => updInv(i, "wht", v)} />
          <button className="btn-danger" onClick={() => delInv(i)}>✕</button>
        </div>
      ))}
      <div className="invoice-row invoice-totals">
        <div className="invoice-company" style={{fontWeight:600}}>{t("合计", "Totals")}</div>
        <div />
        <div />
        <div className="computed-value" style={{fontWeight:700}}>{fmtShort(totals.net)}</div>
        <div className="computed-value" style={{fontWeight:700}}>{fmtShort(totals.vat)}</div>
        <div className="computed-value" style={{fontWeight:700}}>{fmtShort(totals.wht)}</div>
        <div />
      </div>
      <div style={{display:'flex',gap:8,marginTop:8}}>
        <button className="btn-add" onClick={addInvoice}>+ {t("手动发票", "Manual Invoice")}</button>
        <button className="btn-add" onClick={openPoolForSelect}>{t("从发票池添加", "Add from Pool")}</button>
      </div>
      {hasInvoices && shown.length > 0 && (
        <div className="invoice-compare">
          <div style={{fontWeight:600,fontSize:11,marginBottom:6}}>{t("与文档字段对比", "Comparison vs Document Fields")}</div>
          {shown.map((c, i) => (
            <div key={i} className={`invoice-compare-row ${c.ok ? 'ok' : 'bad'}`}>
              <span>{c.label}</span>
              <span>{c.fmtV(c.inv)}</span>
              <span className="arrow">→</span>
              <span>{c.fmtV(c.doc)}</span>
              <span className="flag">{c.ok ? '✓' : '✗'}</span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
  }

  const addInvoice = () => {
    const arr = [...data.invoices, { invoice_no: `Invoice-${data.invoices.length + 1}`, seller_tax_id: "", amount: "0.00", vat: "0.00", wht: "0.00", company_name: "" }];
    formRef.current = { ...formRef.current, invoices: arr };
    recalc(formRef.current);
  };
  const updInv = (i: number, k: string, v: string) => updateNested("invoices", i, k, v);
  const delInv = (i: number) => {
    const inv = (formRef.current.invoices ?? [])[i];
    delRow("invoices", i);
    if (inv && inv.invoice_no) {
      const p = poolList.find((x: any) => x.invoice_id === inv.invoice_no && x.status === 'used');
      if (p) {
        invoke("mark_pool_invoice_available", { id: p.id }).catch(() => {});
        try { if (authUser) markPoolAvailableRemote(inv.invoice_no, p.seller_tax_id || ""); } catch (e: any) { console.error("markPoolAvailableRemote failed", e); }
      }
    }
  };

  const sellerTaxForService = (serviceName: string): string => {
    const name = serviceName || "";
    const found = poolList.find((p: any) =>
      p.invoice_id && name.toUpperCase().includes((p.invoice_id).toUpperCase())
    );
    return found?.seller_tax_id || "";
  };

  const addImportEntry = () => {
    const arr = [...(data.import_entries ?? []), { service_name: "", amount: "0.00", rate: "", free_wht: false, wht_rate: "0%", vat_rate: "14%", temp_labour: false, attached_invoice: "", seller_tax_id: "" }];
    formRef.current = { ...formRef.current, import_entries: arr };
    recalc(formRef.current);
  };
  const updImportEntry = (i: number, k: string, v: any) => {
    updateNested("import_entries", i, k, v);
    const entry = { ...(formRef.current.import_entries ?? [])[i], [k]: v };
    if (k === "service_name") {
      // When a service names an invoice from the pool, prefill VAT/WHT/rate from
      // the memory of the matching seller (last used).
      const taxId = sellerTaxForService(v);
      if (taxId && sellerRates[taxId]) {
        const mem = sellerRates[taxId];
        updateNested("import_entries", i, "vat_rate", mem.vat || entry.vat_rate);
        updateNested("import_entries", i, "wht_rate", mem.wht || entry.wht_rate);
        if (mem.rate) updateNested("import_entries", i, "rate", mem.rate);
        updateNested("import_entries", i, "seller_tax_id", taxId);
        entry.vat_rate = mem.vat || entry.vat_rate;
        entry.wht_rate = mem.wht || entry.wht_rate;
        entry.seller_tax_id = taxId;
      }
      updateNested("import_entries", i, "attached_invoice", "");
    }
    if (k === "vat_rate" || k === "wht_rate" || k === "rate") {
      const taxId = entry.seller_tax_id || sellerTaxForService(entry.service_name);
      if (taxId) rememberSellerRates(taxId, { vat: entry.vat_rate || "14%", wht: entry.wht_rate || "0%", rate: entry.rate || "" });
    }
  };
  const delImportEntry = (i: number) => {
    const inv = (formRef.current.import_entries ?? [])[i];
    const attached = inv?.attached_invoice;
    delRow("import_entries", i);
    if (attached) {
      const p = poolList.find((x: any) => x.invoice_id === attached && x.status === 'used');
      if (p) {
        invoke("mark_pool_invoice_available", { id: p.id }).catch(() => {});
        try { if (authUser) markPoolAvailableRemote(attached, p.seller_tax_id || ""); } catch (e: any) { console.error("markPoolAvailableRemote failed", e); }
      }
    }
  };
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

  const isImport = data.doc_type === "import";
  const AuditTab = () => {
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
      <div className="card">
        <h3>{t("最终决定", "Final Decision")}</h3>
        <div className="decision-options">
          <label className={`decision-option ${data.final_decision === "approve" ? "decision-selected approve" : ""}`}>
            <input type="radio" name="final_decision" checked={data.final_decision === "approve"} onChange={() => updateField("final_decision", "approve")} />
            <span className="decision-label">{t("批准", "Approve")}</span>
          </label>
          <label className={`decision-option ${data.final_decision === "conditional" ? "decision-selected conditional" : ""}`}>
            <input type="radio" name="final_decision" checked={data.final_decision === "conditional"} onChange={() => updateField("final_decision", "conditional")} />
            <span className="decision-label">{t("有条件批准", "Conditional Approve")}</span>
          </label>
          <label className={`decision-option ${data.final_decision === "reject" ? "decision-selected reject" : ""}`}>
            <input type="radio" name="final_decision" checked={data.final_decision === "reject"} onChange={() => updateField("final_decision", "reject")} />
            <span className="decision-label">{t("拒绝", "Reject")}</span>
          </label>
        </div>
        {data.final_decision === "conditional" && (
          <div className="field" style={{marginTop: 12}}>
            <label className="field-label">{t("有条件批准原因", "Reason for Conditional Approve")}</label>
            <FastInput className="audit-notes" value={data.conditional_reason} onChange={v => updateField("conditional_reason", v)} rows={3} />
          </div>
        )}
        {data.final_decision === "reject" && (
          <div className="field" style={{marginTop: 12}}>
            <label className="field-label">{t("拒绝原因", "Reason for Reject")}</label>
            <FastInput className="audit-notes" value={data.reject_reason} onChange={v => updateField("reject_reason", v)} rows={3} />
          </div>
        )}
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
              <div style={{paddingTop:14,paddingLeft:10,display:'flex',alignItems:'center',gap:4}}>
                {t("汇率", "Rate")}
                <input type="checkbox" checked={rateVisible} onChange={() => setRateVisible(v => !v)} style={{width:14,height:14,accentColor:'var(--accent)',cursor:'pointer'}} title={rateVisible ? "Showing EGP (rate applied)" : "Showing USD (rate ignored)"} />
              </div>
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
            const rate = parseFloat(e.rate) || 1;
            const egpAmt = amt * rate;
            const displayAmt = rateVisible ? egpAmt : amt;
            const vatRate = parseFloat((e.vat_rate || "0%").replace('%', '')) || 0;
            const vat = Math.round(displayAmt * vatRate / 100 * 100) / 100;
            const whtRate = parseFloat((e.wht_rate || "0%").replace('%', '')) || 0;
            const wht = e.free_wht ? 0 : Math.round(displayAmt * whtRate / 100 * 100) / 100;
            return (
              <div key={i} className="invoice-row" style={{display:'grid',gridTemplateColumns:'minmax(200px, 1.5fr) minmax(110px, 1fr) 80px 50px 80px 80px 50px 1fr 100px 100px 110px 110px 30px',gap:6,alignItems:'center'}}>
                <div style={{minWidth:0}}>
                  {e.attached_invoice && (
                    <div className="attached-pill" title={`${t("已附加发票", "Attached invoice")}: ${e.attached_invoice}`}>✓ {e.attached_invoice}</div>
                  )}
                  <FastInput value={e.service_name} onChange={v => updImportEntry(i, "service_name", v)} rows={1} />
                </div>
                <FastInput value={e.amount} onChange={v => updImportEntry(i, "amount", v)} />
                <FastInput value={e.rate} onChange={v => updImportEntry(i, "rate", v)} style={rateVisible ? {} : {opacity: 0.4, textDecoration: 'line-through'}} />
                <input type="checkbox" checked={e.free_wht} onChange={() => updImportEntry(i, "free_wht", !e.free_wht)} style={{margin:'auto'}} />
                <select className="field-select" style={{padding:'7px 4px 7px 8px',fontSize:11}} value={e.vat_rate} onChange={ev => updImportEntry(i, "vat_rate", ev.target.value)}>
                  {["0%","5%","9%","10%","14%"].map(o => <option key={o} value={o}>{o}</option>)}
                </select>
                <select className="field-select" style={{padding:'7px 4px 7px 8px',fontSize:11}} value={e.wht_rate} onChange={ev => updImportEntry(i, "wht_rate", ev.target.value)}>
                  {["0%","1%","3%","5%","10%"].map(o => <option key={o} value={o}>{o}</option>)}
                </select>
                <input type="checkbox" checked={e.temp_labour} onChange={() => updImportEntry(i, "temp_labour", !e.temp_labour)} style={{margin:'auto'}} />
                <div></div> {/* Spacer cell */}
                <div className="computed-value" style={{fontSize:11,padding:'7px 10px',wordBreak:'break-all'}}>{fmtShort(vat)}</div>
                <div className="computed-value" style={{fontSize:11,padding:'7px 10px',wordBreak:'break-all'}}>{fmtShort(wht)}</div>
                <div className="computed-value" style={{fontSize:11,fontWeight:600,padding:'7px 10px',wordBreak:'break-all'}}>{fmtShort(displayAmt + vat - wht)}</div>
                <div className="computed-value" style={{fontSize:11,fontWeight:600,padding:'7px 10px',wordBreak:'break-all'}}>{fmtShort(displayAmt + vat)}</div>
                <button className="btn-danger" style={{padding:'7px 10px'}} onClick={() => delImportEntry(i)}>✕</button>
              </div>
            );
          })}
          <div style={{display:'flex',gap:8,marginTop:8}}>
          <button className="btn-add" onClick={addImportEntry}>+ {t("添加服务商", "Add Provider")}</button>
          <button className="btn-add" onClick={openPoolForSelect}>{t("从发票池添加", "Add from Pool")}</button>
        </div>
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
    setOverwriteTarget(null);
    await recalc(formRef.current);
    try { await invoke("save_config", { data: formRef.current }); } catch {}
  }, [t, recalc]);

  const exportExcel = async () => {
    try {
      const path = await save({ defaultPath: `${data.doc_serial || "Vouchify_Settlement"}.xlsx`, filters: [{ name: "Excel", extensions: ["xlsx"] }] });
      if (path) {
        await invoke("export_excel", { data, computed, filePath: path });
        showAlert(t("导出成功", "Export successful"));
      }
    } catch (e) {
      console.error("export_excel failed", e);
      showAlert(`${t("导出失败", "Export failed")}: ${String(e)}`);
    }
  };

  const exportInvoiceSummary = async () => {
    const startDate = new Date(invoiceExportFrom);
    const endDate = new Date(invoiceExportTo);
    // Set endDate to end of that day
    endDate.setHours(23, 59, 59, 999);
    const startStr = startDate.toISOString();
    const endStr = endDate.toISOString();

    let allInvoices: { serial: string; invoice_no: string; seller_tax_id: string; amount: number; doc_type: string }[] = [];

    // If logged in, query Supabase
    if (authUser) {
      try {
        const rows = await listSnapshotsRemote("");
        for (const r of rows) {
          if (r.created_at >= startStr && r.created_at <= endStr) {
            try {
              const parsed = JSON.parse(r.data_json);
              const serial = parsed.doc_serial || r.label;
              const invs = parsed.invoices || [];
              const dt = parsed.doc_type || "bank";
              for (const inv of invs) {
                allInvoices.push({
                  serial,
                  invoice_no: inv.invoice_no || "",
                  seller_tax_id: inv.seller_tax_id || "",
                  amount: parseFloat(inv.amount) || 0,
                  doc_type: dt,
                });
              }
              // For import entries, also include them
              const entries = parsed.import_entries || [];
              if (entries.length > 0 && dt === "import") {
                for (const e of entries) {
                  allInvoices.push({
                    serial,
                    invoice_no: e.service_name || "",
                    seller_tax_id: "",
                    amount: parseFloat(e.amount) || 0,
                    doc_type: "import",
                  });
                }
              }
            } catch {}
          }
        }
      } catch (e) {
        console.error("Supabase invoice query failed", e);
      }
    }

    // Also query local SQLite for any data not in cloud
    try {
      const localList = await invoke<HistoryEntry[]>("list_history", { search: "" });
      for (const entry of localList) {
        try {
          const dataJson = await invoke<string>("load_history", { id: entry.id });
          const parsed = JSON.parse(dataJson);
          const entryDate = new Date(entry.created_at);
          if (entryDate >= startDate && entryDate <= endDate) {
            // Check if already in Supabase (avoid duplicates)
            if (authUser && allInvoices.some(i => i.serial === (parsed.doc_serial || entry.label))) continue;
            const serial = parsed.doc_serial || entry.label;
            const invs = parsed.invoices || [];
            const dt = parsed.doc_type || "bank";
            for (const inv of invs) {
              allInvoices.push({
                serial,
                invoice_no: inv.invoice_no || "",
                seller_tax_id: inv.seller_tax_id || "",
                amount: parseFloat(inv.amount) || 0,
                doc_type: dt,
              });
            }
            const entries = parsed.import_entries || [];
            if (entries.length > 0 && dt === "import") {
              for (const e of entries) {
                allInvoices.push({
                  serial,
                  invoice_no: e.service_name || "",
                  seller_tax_id: "",
                  amount: parseFloat(e.amount) || 0,
                  doc_type: "import",
                });
              }
            }
          }
        } catch {}
      }
    } catch {}

    if (allInvoices.length === 0) {
      showAlert(t("没有找到该时间段内的发票数据", "No invoice data found for this period"));
      return;
    }

    const filePath = await save({
      defaultPath: `Invoice_Registry_${invoiceExportFrom}_to_${invoiceExportTo}.xlsx`,
      filters: [{ name: "Excel", extensions: ["xlsx"] }],
    });
    if (filePath) {
      await invoke("export_invoice_summary", { invoices: allInvoices, dateFrom: invoiceExportFrom, dateTo: invoiceExportTo, filePath });
      showAlert(t("发票清单导出成功", "Invoice registry exported successfully"));
      setShowInvoiceExport(false);
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
      setProgressMsg(t("正在准备上传...", "Preparing upload..."));
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

  const showHistoryModal = async () => {
    if (historyOverlayRef.current) {
      historyOverlayRef.current.style.visibility = 'visible';
      historyOverlayRef.current.style.opacity = '1';
      historyOverlayRef.current.style.pointerEvents = 'auto';
    }
    // Auto-sync local snapshots to cloud when logged in
    if (authUser) {
      try {
        const localList = await invoke<HistoryEntry[]>("list_history", { search: "" });
        if (localList.length > 0) {
          const remoteRows = await listSnapshotsRemote("");
          const remoteLabels = new Set(remoteRows.map(r => r.label));
          let synced = 0;
          for (const entry of localList) {
            if (remoteLabels.has(entry.label)) continue; // skip duplicates
            try {
              const dataJson = await invoke<string>("load_history", { id: entry.id });
              await saveSnapshotRemote(entry.label, entry.notes || "", dataJson);
              synced++;
            } catch (e) {
              console.error("Failed to sync entry", entry.id, e);
            }
          }
          if (synced > 0) {
            console.log(`Auto-synced ${synced} local snapshots to cloud`);
          }
        }
      } catch (e) {
        console.error("Auto-sync failed", e);
      }
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
    if (authUser) {
      try {
        const rows = await listSnapshotsRemote(search);
        setHistoryList(rows.map(r => {
          let final_decision = "";
          let doc_type = "bank";
          let auditor = "";
          try {
            const parsed = JSON.parse(r.data_json);
            final_decision = parsed.final_decision || "";
            doc_type = parsed.doc_type || "bank";
            auditor = parsed.auditor || "";
          } catch {}
          return {
            id: r.id,
            label: r.label,
            notes: r.notes,
            data_json: r.data_json,
            created_at: new Date(r.created_at).toLocaleString(),
            owner: r.user_id,
            delete_requested_at: r.delete_requested_at,
            delete_requested_by: r.delete_requested_by,
            final_decision,
            doc_type,
            auditor,
          };
        }));
      } catch (e) {
        console.error("listSnapshotsRemote failed", e);
        try {
          const list = await invoke<HistoryEntry[]>("list_history", { search });
          setHistoryList(list.map(h => {
            let final_decision = "";
            let doc_type = "bank";
            let auditor = "";
            try {
              const parsed = JSON.parse(h.data_json || "{}");
              final_decision = parsed.final_decision || "";
              doc_type = parsed.doc_type || "bank";
              auditor = parsed.auditor || "";
            } catch {}
            return { ...h, final_decision, doc_type, auditor };
          }));
        } catch (e2) {
          console.error("list_history fallback failed", e2);
          setHistoryList([]);
        }
      }
    } else {
      try {
        const list = await invoke<HistoryEntry[]>("list_history", { search });
        setHistoryList(list.map(h => {
          let final_decision = "";
          let doc_type = "bank";
          let auditor = "";
          try {
            const parsed = JSON.parse(h.data_json || "{}");
            final_decision = parsed.final_decision || "";
            doc_type = parsed.doc_type || "bank";
            auditor = parsed.auditor || "";
          } catch {}
          return { ...h, final_decision, doc_type, auditor };
        }));
      } catch (e) {
        console.error("list_history failed", e);
        setHistoryList([]);
      }
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
    if (overwriteTarget) {
      // Overwrite an existing snapshot (maker fixed a rejected document).
      // Preserve the decision chosen in the Final Decision card (e.g. an approved
      // doc stays approved; for a rejected doc the maker sets Approve before saving).
      const saveData = {
        ...data,
        final_decision: data.final_decision || "",
        conditional_reason: data.final_decision === "conditional" ? (data.conditional_reason || "") : "",
        reject_reason: data.final_decision === "reject" ? (data.reject_reason || "") : "",
        auditor: data.auditor || authUser || "",
      };
      const dataJson = JSON.stringify(saveData);
      const target = overwriteTarget;
      if (target.remote && authUser) {
        try {
          await updateSnapshotRemote(target.id, target.label, "", dataJson);
          setSynced(true);
          showAlert(`${t("已覆盖并同步", "Overwritten & synced")} (${target.label})`);
        } catch (e: any) {
          showAlert(`${t("覆盖失败", "Overwrite failed")}: ${e.message || e}`);
          return;
        }
      } else {
        try {
          await invoke("update_history", { id: target.id, label: target.label, notes: "", dataJson });
          showAlert(`${t("已覆盖", "Overwritten")} (${target.label})`);
        } catch (e: any) {
          showAlert(`${t("覆盖失败", "Overwrite failed")}: ${e.message || e}`);
          return;
        }
      }
      setOverwriteTarget(null);
      loadHistoryList(historySearch);
      return;
    }
    const serial = data.doc_serial;
    if (serial) {
      // Check for duplicate serial number in Supabase
      if (authUser) {
        try {
          const remoteRows = await listSnapshotsRemote(serial);
          if (remoteRows.some(r => r.label === serial)) {
            showAlert(t("该文档编号已存在，无法重复保存", "This document serial already exists, cannot save duplicate"));
            return;
          }
        } catch (e) {
          console.error("Supabase serial check failed", e);
        }
      }
      // Also check local SQLite
      try {
        const localExists = await invoke<boolean>("check_serial_exists", { serial });
        if (localExists) {
          showAlert(t("该文档编号已存在，无法重复保存", "This document serial already exists, cannot save duplicate"));
          return;
        }
      } catch {}
    }
    const label = serial || `Snapshot-${new Date().toLocaleDateString()}`;
    const saveData = { ...data, auditor: data.auditor || authUser || "" };
    const dataJson = JSON.stringify(saveData);
    if (authUser) {
      try {
        await saveSnapshotRemote(label, "", dataJson);
        setSynced(true);
        showAlert(`${t("快照已保存并同步", "Snapshot saved & synced")} (${label})`);
      } catch (e: any) {
        console.error("saveSnapshotRemote failed", e);
        try {
          await invoke("save_history", { label, notes: "", dataJson });
          setSynced(false);
          showAlert(`${t("快照已保存(本地)", "Snapshot saved (local)")} (${label})`);
        } catch (e2) {
          console.error("save_history fallback failed", e2);
        }
      }
    } else {
      await invoke("save_history", { label, notes: "", dataJson });
      showAlert(`${t("快照已保存", "Snapshot saved")} (${label})`);
    }
  };

  const loadSnapshot = async (id: number) => {
    try {
      let dataJson: string;
      if (authUser) {
        try {
          dataJson = await loadSnapshotRemote(id);
        } catch (e) {
          console.error("loadSnapshotRemote failed, falling back", e);
          dataJson = await invoke<string>("load_history", { id });
        }
      } else {
        dataJson = await invoke<string>("load_history", { id });
      }
      const parsed = JSON.parse(dataJson);
      if (!parsed.import_costs && (parsed.import_cost_1 !== undefined)) {
        parsed.import_costs = [
          { name: parsed.import_cost_1_label || "Foreign Cost", amount: parsed.import_cost_1 || "0.00" },
          { name: parsed.import_cost_2_label || "Domestic Cost", amount: parsed.import_cost_2 || "0.00" },
          { name: parsed.import_cost_3_label || "Nafeza Paper", amount: parsed.import_cost_3 || "0.00" },
        ];
      }
      if (!parsed.import_costs) parsed.import_costs = [...DEFAULT_FORM.import_costs];
      if (!parsed.check_form_4_6) parsed.check_form_4_6 = false;
      if (!parsed.seller_tax_ids) parsed.seller_tax_ids = [];
      if (!parsed.final_decision) parsed.final_decision = "";
      if (!parsed.conditional_reason) parsed.conditional_reason = "";
      if (!parsed.reject_reason) parsed.reject_reason = "";
      if (!parsed.auditor) parsed.auditor = "";
      formRef.current = parsed;
      await recalc(parsed);
      await reconcilePillsFromPool();
      await restoreClaimsFromDocument();
    } catch (e) {
      console.error("loadSnapshot failed", e);
    }
    hideHistoryModal();
  };

  const startOverwrite = async (id: number) => {
    const entry = historyList.find(h => h.id === id);
    await loadSnapshot(id);
    setOverwriteTarget({ id, label: entry?.label || "", remote: !!authUser });
  };

  const historySearchTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const onHistorySearch = (v: string) => {
    setHistorySearch(v);
    if (historySearchTimer.current) clearTimeout(historySearchTimer.current);
    historySearchTimer.current = setTimeout(() => loadHistoryList(v), 300);
  };

  const deleteSnapshot = async (id: number) => {
    if (authUser) {
      // Admin: delete freely
      if (isAdminUser) {
        try {
          await deleteSnapshotRemote(id);
          showAlert(t("快照已删除", "Snapshot deleted"));
        } catch (e: any) {
          console.error("deleteSnapshotRemote failed", e);
          showAlert(`${t("删除失败", "Delete failed")}: ${e.message || e}`);
        }
      } else {
        // Normal user: request deletion
        try {
          await requestDeleteSnapshot(id);
          showAlert(t("删除请求已提交，等待管理员确认", "Delete request submitted, awaiting admin approval"));
        } catch (e: any) {
          showAlert(`${t("请求失败", "Request failed")}: ${e.message || e}`);
        }
      }
    } else {
      await invoke("delete_history", { id });
    }
    loadHistoryList(historySearch);
  };

  const approveDelete = async (id: number) => {
    try {
      await approveDeleteSnapshot(id);
      showAlert(t("快照已删除", "Snapshot deleted"));
    } catch (e: any) {
      showAlert(`${t("删除失败", "Delete failed")}: ${e.message || e}`);
    }
    loadHistoryList(historySearch);
  };

  const attachValidatedInvoices = async (results: any[]) => {
    const serial = data.doc_serial || "draft";
    const updated = [...(formRef.current.import_entries ?? [])];
    for (const r of results) {
      const indices: number[] = Array.isArray(r.matched_entry_indices)
        ? r.matched_entry_indices
        : r.matched_entry_index != null ? [r.matched_entry_index] : [];
      if (r.is_valid && indices.length > 0) {
        for (const idx of indices) {
          if (updated[idx] && !updated[idx].attached_invoice) {
            updated[idx] = { ...updated[idx], attached_invoice: r.invoice.invoice_id, seller_tax_id: r.invoice.seller_tax_id || updated[idx].seller_tax_id || "" };
          }
        }
        try { if (r.pool_id) await invoke("mark_pool_invoice_used", { id: r.pool_id, snapshotId: 0, snapshotLabel: serial }); } catch {}
        try { if (authUser) await markPoolUsedRemote(r.invoice.invoice_id, r.invoice.seller_tax_id || "", serial); } catch (e) { console.error("markPoolUsedRemote failed", e); }
      }
    }
    if (updated.some((e, i) => e && e.attached_invoice !== (formRef.current.import_entries ?? [])[i]?.attached_invoice)) {
      formRef.current = { ...formRef.current, import_entries: updated };
      await recalc(formRef.current);
    }
    try { await loadPool(); } catch {}
  };

  // Non-destructive: keep the form consistent with the pool by showing a pill on
  // any service whose name references an invoice already marked "used" in the pool.
  // It never changes pool status, so invoices stay used across documents.
  const reconcilePillsFromPool = async () => {
    try {
      const entries = formRef.current.import_entries ?? [];
      const list = await invoke<any[]>("list_invoice_pool");
      const used = list.filter((p: any) => p.status === 'used');
      if (used.length === 0) return;
      let changed = false;
      const updated = entries.map((e: any) => {
        if (!e || e.attached_invoice || !e.service_name) return e;
        const match = used.find((p: any) => serviceNameContainsInvoice(e.service_name, p.invoice_id));
        if (match) {
          changed = true;
          return { ...e, attached_invoice: match.invoice_id };
        }
        return e;
      });
      if (changed) {
        formRef.current = { ...formRef.current, import_entries: updated };
        await recalc(formRef.current);
      }
    } catch {}
  };

  // The document is the source of truth for claims. When a pool is wiped or a
  // sync is lost, the pool may show "available" invoices that this document
  // already references. This re-asserts those claims in the local pool and in
  // Supabase so they never have to be manually reclaimed after a restore.
  const restoreClaimsFromDocument = async (): Promise<number> => {
    const form = formRef.current;
    const serial = ((form.doc_serial) || "draft").trim();
    const ids = new Set<string>();
    (form.invoices || []).forEach((inv: any) => { if (inv?.invoice_no) ids.add(`${inv.invoice_no}||${inv.seller_tax_id || ""}`); });
    (form.import_entries || []).forEach((e: any) => { if (e?.attached_invoice) ids.add(`${e.attached_invoice}||${e.seller_tax_id || ""}`); });
    if (ids.size === 0) return 0;
    let list: any[] = [];
    try { list = await invoke<any[]>("list_invoice_pool"); } catch { return 0; }
    const poolByInvId = new Map(list.map((p: any) => [`${p.invoice_id}||${p.seller_tax_id}`, p]));
    const toClaim = [...ids].map(k => poolByInvId.get(k)).filter(Boolean);
    if (toClaim.length === 0) return 0;
    try {
      await invoke("mark_pool_invoices_used", { ids: toClaim.map((p: any) => p.id), snapshotId: 0, snapshotLabel: serial });
    } catch {}
    try {
      if (authUser) await markPoolsUsedRemote(toClaim.map((p: any) => ({ invoice_id: p.invoice_id, seller_tax_id: p.seller_tax_id || "" })), serial);
    } catch (e) { console.error("restore claims remote failed", e); }
    try { await loadPool(); } catch {}
    return toClaim.length;
  };

  // Bulk re-assert claims across every saved document (local + remote) so that
  // claims and their serial indicators survive a pool wipe, a fresh install, or
  // a sync failure. Scans all snapshots, extracts invoice references + serial,
  // and marks those invoices used in the local pool and in Supabase.
  const restoreAllClaims = async (): Promise<number> => {
    let claimed = 0;
    let scanned = 0;
    let docs: { serial: string; ids: Set<string> }[] = [];
    // Local snapshots
    try {
      const local = await invoke<HistoryEntry[]>("list_history", { search: "" });
      for (const h of local) {
        try {
          const json = await invoke<string>("load_history", { id: h.id });
          const parsed = JSON.parse(json);
          const serial = ((parsed.doc_serial) || h.label || "draft").trim();
          const ids = new Set<string>();
          (parsed.invoices || []).forEach((inv: any) => { if (inv?.invoice_no) ids.add(`${inv.invoice_no}||${inv.seller_tax_id || ""}`); });
          (parsed.import_entries || []).forEach((e: any) => { if (e?.attached_invoice) ids.add(`${e.attached_invoice}||${e.seller_tax_id || ""}`); });
          if (ids.size > 0) docs.push({ serial, ids });
        } catch (e) { console.error("parse local snapshot failed", h.id, e); }
      }
    } catch (e) { console.error("list_history failed in restoreAllClaims", e); }
    // Remote snapshots
    if (authUser) {
      try {
        const remote = await listSnapshotsRemote("");
        for (const r of remote) {
          try {
            const parsed = JSON.parse(r.data_json);
            const serial = ((parsed.doc_serial) || r.label || "draft").trim();
            const ids = new Set<string>();
            (parsed.invoices || []).forEach((inv: any) => { if (inv?.invoice_no) ids.add(`${inv.invoice_no}||${inv.seller_tax_id || ""}`); });
            (parsed.import_entries || []).forEach((e: any) => { if (e?.attached_invoice) ids.add(`${e.attached_invoice}||${e.seller_tax_id || ""}`); });
            if (ids.size > 0) docs.push({ serial, ids });
          } catch (e) { console.error("parse remote snapshot failed", r.id, e); }
        }
      } catch (e) { console.error("listSnapshotsRemote failed in restoreAllClaims", e); }
    }
    if (docs.length === 0) return 0;
    // Make sure the local pool is up to date first so claims can target
    // invoices that exist in the cloud but not yet in local SQLite.
    try { await syncPoolRemote(); } catch {}
    let list: any[] = [];
    try { list = await invoke<any[]>("list_invoice_pool"); } catch { return 0; }
    const poolByComposite = new Map(list.map((p: any) => [`${p.invoice_id}||${p.seller_tax_id || ""}`, p]));
    for (const doc of docs) {
      const toClaim = [...doc.ids].map(k => poolByComposite.get(k)).filter(Boolean) as any[];
      if (toClaim.length === 0) continue;
      scanned += toClaim.length;
      try {
        await invoke("mark_pool_invoices_used", { ids: toClaim.map((p: any) => p.id), snapshotId: 0, snapshotLabel: doc.serial });
      } catch {}
      try {
        if (authUser) await markPoolsUsedRemote(toClaim.map((p: any) => ({ invoice_id: p.invoice_id, seller_tax_id: p.seller_tax_id || "" })), doc.serial);
      } catch (e) { console.error("restoreAllClaims remote failed", e); }
      claimed += toClaim.length;
    }
    try { await loadPool(); } catch {}
    return claimed;
  };

  // Bidirectional pool sync: pull the cloud pool into local SQLite AND push any
  // local-only invoices up to Supabase. Runs on login/session-restore and when
  // the pool modal opens, so invoices imported on one device (even while logged
  // out) eventually reach every other device.
  // Bidirectional pool sync. Claim state is STICKY: a "used" claim (with
  // serial) is never downgraded by stale "available" local state from another
  // device. Local-only invoices are pushed up; local claims propagate up; but
  // an available local row never overwrites an existing remote claim. The pull
  // then brings the authoritative cloud state (including every claim) down to
  // local SQLite, converging every device to the full union.
  const syncPoolRemote = async (): Promise<void> => {
    if (poolSyncInFlight.current) return;
    poolSyncInFlight.current = true;
    const info: { ok: boolean; local: number; cloud: number; pushed: number; pulled: number; error?: string } = { ok: true, local: 0, cloud: 0, pushed: 0, pulled: 0 };
    if (!authUser) {
      info.error = "not logged in (authUser null)";
      info.ok = false;
      setPoolSyncInfo(info);
      poolSyncInFlight.current = false;
      return;
    }
    try {
      // 1) Push local invoices up, but only when safe:
      //    - invoice not yet in the cloud (local-only import), or
      //    - local is a claim (used) that the cloud doesn't have yet.
      //    Never push "available" over an existing cloud claim.
      const localAll = await invoke<any[]>("list_invoice_pool");
      info.local = localAll.length;
      const remoteMeta = await listPoolRemoteMeta();
      info.cloud = remoteMeta.length;
      const remoteByComposite = new Map(remoteMeta.map((r) => [`${r.invoice_id}||${r.seller_tax_id || ""}`, r]));
      const toPush = localAll.filter((l) => {
        const r = remoteByComposite.get(`${l.invoice_id}||${l.seller_tax_id || ""}`);
        if (!r) return true;
        if (l.status === "used") {
          return r.status !== "used" || (r.used_by_label || "") !== (l.used_by_label || "");
        }
        return false;
      });
      info.pushed = toPush.length;
      if (toPush.length > 0) {
        try {
          await upsertPoolInvoicesRemote(toPush);
        } catch (e) { console.error("upsert pool remote failed", e); info.error = `upsert: ${(e as any)?.message || e}`; }
      }
      // 2) Pull only the remote rows that differ from local state (missing
      //    locally, or claim state changed). This avoids re-downloading the
      //    entire pool (with full raw_xml) on every sync, which was slowing
      //    the app down as the pool grew.
      const localByComposite = new Map(localAll.map((l) => [`${l.invoice_id}||${l.seller_tax_id || ""}`, l]));
      const changedRemote = remoteMeta.filter((r) => {
        const l = localByComposite.get(`${r.invoice_id}||${r.seller_tax_id || ""}`);
        if (!l) return true;
        return (l.status || "available") !== (r.status || "available") ||
               (l.used_by_label || "") !== (r.used_by_label || "") ||
               (l.doc_status || "Valid") !== (r.doc_status || "Valid");
      });
      const remote = changedRemote.length > 0 ? await listPoolRemoteByIds(changedRemote.map((r) => r.invoice_id)) : [];
      info.cloud = remoteMeta.length;
      info.pulled = remote.length;
      if (remote.length > 0) {
        const mapped = remote.map((r: any) => ({
          invoice_id: r.invoice_id,
          uuid: r.uuid || "",
          seller_tax_id: r.seller_tax_id || "",
          seller_name: r.seller_name || "",
          buyer_tax_id: r.buyer_tax_id || "",
          buyer_name: r.buyer_name || "",
          issue_date: r.issue_date || "",
          currency: r.currency || "",
          net_amount: r.net_amount || 0,
          total_vat: r.total_vat || 0,
          total_wht: r.total_wht || 0,
          grand_total: r.grand_total || 0,
          lines_json: r.lines_json || "[]",
          raw_xml: r.raw_xml || "",
          file_name: r.file_name || "",
          ...(r.doc_status ? { doc_status: r.doc_status } : {}),
          status: r.status || "available",
          used_by_snapshot_id: null,
          used_by_label: r.used_by_label || "",
          delete_requested_at: r.delete_requested_at || null,
          delete_requested_by: r.delete_requested_by || "",
          created_at: r.created_at || new Date().toISOString(),
        }));
        // Chunk the pull too: sending every invoice (with full raw_xml) in a
        // single Tauri invoke produces a multi-MB IPC payload that can fail
        // silently, leaving the cloud rows stranded. Small batches keep each
        // invoke well within IPC limits.
        const PAGE = 10;
        for (let i = 0; i < mapped.length; i += PAGE) {
          const chunk = mapped.slice(i, i + PAGE);
          await invoke("sync_pool_from_remote", { invoices: chunk });
        }
      }
      // 3) Clean up claims that have no serial. A "used" row without a serial
      //    label is meaningless (the serial links the invoice to a document),
      //    so downgrade them to available locally AND in the cloud so stale
      //    "claimed, no serial" rows never linger across devices.
      const cleaned = await invoke<number>("clean_unlabelled_claims");
      if (cleaned > 0) console.log(`downgraded ${cleaned} unlabelled claims to available`);
      const unlabelledRemote = remoteMeta.filter((r) => r.status === "used" && !(r.used_by_label || ""));
      if (unlabelledRemote.length > 0) {
        try { await markPoolsAvailableRemote(unlabelledRemote.map((u) => ({ invoice_id: u.invoice_id, seller_tax_id: u.seller_tax_id || "" }))); } catch (e) { console.error("clean remote claims failed", e); }
      }
    } catch (e) {
      console.error("sync remote pool failed", e);
      info.error = `sync: ${(e as any)?.message || e}`;
    } finally {
      info.ok = !info.error;
      setPoolSyncInfo(info);
      poolSyncInFlight.current = false;
    }
  };

  const loadPool = async () => {
    setPoolLoading(true);
    try {
      await syncPoolRemote();
      const list = await invoke<any[]>("list_invoice_pool");
      setPoolList(list);
    } catch (e) {
      console.error("list_invoice_pool failed", e);
      setPoolList([]);
    }
    setPoolLoading(false);
  };

  // Auto-sync the pool whenever a user logs in (or a saved session is restored)
  // so invoices imported on any device reach the cloud without needing to open
  // the pool modal first. Also refreshes periodically so a device that was
  // offline (or opened before the cloud was updated) still converges.
  const didInitPoolSync = useRef<string | null>(null);
  const poolSyncInFlight = useRef(false);
  useEffect(() => {
    if (!authUser) { didInitPoolSync.current = null; return; }
    if (didInitPoolSync.current === authUser) return;
    didInitPoolSync.current = authUser;
    const run = async () => {
      try { await syncPoolRemote(); } catch (e) { console.error("auto pool sync failed", e); }
    };
    run();
  }, [authUser]);

  useEffect(() => {
    if (!authUser) return;
    const id = setInterval(() => {
      syncPoolRemote().catch((e) => console.error("periodic pool sync failed", e));
    }, 60_000);
    return () => clearInterval(id);
  }, [authUser]);

  const openPool = () => {
    setPoolMode("validate");
    setShowPool(true);
    setPoolSearch("");
    setPoolTab("unclaimed");
    setPoolSelected(new Set());
    loadPool();
  };

  const openPoolForSelect = () => {
    setPoolMode("select");
    setShowPool(true);
    setPoolSearch("");
    setPoolTab("unclaimed");
    setPoolSelected(new Set());
    loadPool();
  };

  const poolToImportEntry = (p: any) => {
    const net = p.net_amount ?? 0;
    const vatPct = net > 0 ? Math.round(((p.total_vat ?? 0) / net) * 100) : 14;
    const whtPct = net > 0 ? Math.round(((p.total_wht ?? 0) / net) * 100) : 0;
    return {
      service_name: `${p.seller_name || p.invoice_id} ${p.invoice_id}`.trim(),
      amount: net.toFixed(2),
      rate: "",
      free_wht: false,
      wht_rate: `${whtPct}%`,
      vat_rate: `${vatPct}%`,
      temp_labour: false,
      attached_invoice: p.invoice_id,
      seller_tax_id: p.seller_tax_id || "",
    };
  };

  const attachImportEntryFromPool = async (id: number) => {
    const p = poolList.find((x: any) => x.id === id);
    if (!p) return;
    if (p.doc_status && p.doc_status !== "Valid") {
      showAlert(`${t("发票已被拒绝或取消，无法使用", "Rejected/cancelled invoices cannot be used")}: ${p.invoice_id}`);
      return;
    }
    const current = [...(formRef.current.import_entries ?? [])];
    if (current.some((e: any) => e.attached_invoice === p.invoice_id)) {
      showAlert(t("该发票已在此文档中", "This invoice is already in this document"));
      return;
    }
    const arr = [...current, poolToImportEntry(p)];
    formRef.current = { ...formRef.current, import_entries: arr };
    await recalc(formRef.current);
    await markPoolClaimed(id);
  };

  const attachBatchImportEntriesFromPool = async (rowIds: number[]) => {
    if (rowIds.length === 0) return;
    const current = [...(formRef.current.import_entries ?? [])];
    const items = rowIds.map(id => poolList.find((x: any) => x.id === id)).filter(Boolean) as any[];
    const already = items.filter(p => current.some((e: any) => e.attached_invoice === p.invoice_id));
    const fresh = items.filter(p => !already.some((a: any) => a.invoice_id === p.invoice_id) && isInvoiceUsable(p.id));
    const blockedCount = items.length - already.length - fresh.length;
    if (blockedCount > 0) {
      showAlert(`${t("发票已被拒绝或取消，无法使用", "Rejected/cancelled invoices cannot be used")} (${blockedCount})`);
    }
    if (fresh.length === 0) {
      if (blockedCount === 0) {
        showAlert(t("选中的发票已在此文档中", "Selected invoices are already in this document"));
      }
      return;
    }
    const freshEntries = fresh.map((p: any) => poolToImportEntry(p));
    const arr = [...current, ...freshEntries];
    formRef.current = { ...formRef.current, import_entries: arr };
    await recalc(formRef.current);
    const serial = ((formRef.current.doc_serial) || "draft").trim();
    const freshIds = fresh.map((p: any) => p.id);
    try {
      await invoke("mark_pool_invoices_used", { ids: freshIds, snapshotId: 0, snapshotLabel: serial });
    } catch {}
    try {
      if (authUser) await markPoolsUsedRemote(fresh.map((p: any) => ({ invoice_id: p.invoice_id, seller_tax_id: p.seller_tax_id || "" })), serial);
    } catch (e) { console.error("markPoolsUsedRemote failed", e); }
    await loadPool();
    if (fresh.length > 0) {
      try {
        const poolIds = new Set(poolList.map((x: any) => x.invoice_id));
        const attached = arr.filter((e: any) => poolIds.has(e.attached_invoice));
        if (attached.length > 0) {
          const poolRowIds = attached.map((e: any) => {
            const pi = poolList.find((x: any) => x.invoice_id === e.attached_invoice);
            return pi?.id;
          }).filter((x): x is number => x != null);
          if (poolRowIds.length > 0) {
            const formJson = JSON.stringify(formRef.current);
            const results = await invoke<any[]>("validate_from_pool", { ids: poolRowIds, formJson });
            setEtaResult(results);
          }
        }
      } catch (e: any) {
        showAlert(`${t("验证失败", "Validation failed")}: ${e.message || e}`);
      }
    }
  };

  const markPoolClaimed = async (id: number) => {
    const p = poolList.find((x: any) => x.id === id);
    if (!p) return;
    const serial = ((formRef.current.doc_serial) || "draft").trim();
    try {
      await invoke("mark_pool_invoice_used", { id, snapshotId: 0, snapshotLabel: serial });
    } catch {}
    try {
      if (authUser) await markPoolUsedRemote(p.invoice_id, p.seller_tax_id || "", serial);
    } catch (e) { console.error("markPoolUsedRemote failed", e); }
    await loadPool();
  };

  const attachFromPool = async (id: number) => {
    if (data.doc_type === "import") return attachImportEntryFromPool(id);
    const p = poolList.find((x: any) => x.id === id);
    if (!p) return;
    const invoiceId = p.invoice_id;
    if (p.doc_status && p.doc_status !== "Valid") {
      showAlert(`${t("发票已被拒绝或取消，无法使用", "Rejected/cancelled invoices cannot be used")}: ${invoiceId}`);
      return;
    }
    const currentInvoices = [...(formRef.current.invoices ?? [])];
    if (currentInvoices.some(inv => inv.invoice_no === invoiceId)) {
      showAlert(t("该发票已在此文档中", "This invoice is already in this document"));
      return;
    }
    const arr = [...currentInvoices, {
      invoice_no: invoiceId,
      seller_tax_id: p.seller_tax_id || "",
      amount: (p.net_amount ?? 0).toFixed(2),
      vat: (p.total_vat ?? 0).toFixed(2),
      wht: (p.total_wht ?? 0).toFixed(2),
      company_name: p.seller_name || "",
      attached_invoice: invoiceId,
    }];
    formRef.current = { ...formRef.current, invoices: arr };
    await recalc(formRef.current);
    await markPoolClaimed(id);
    // Compare every pool-attached invoice still in this document against the
    // bank document fields, so adding multiple invoices one-by-one always shows
    // the full set of comparisons together.
    try {
      const poolIds = new Set(poolList.map((x: any) => x.invoice_id));
      const attached = arr.filter(inv => poolIds.has(inv.invoice_no));
      if (attached.length > 0) {
        const poolRowIds = attached.map(inv => {
          const pi = poolList.find((x: any) => x.invoice_id === inv.invoice_no && x.seller_tax_id === (inv.seller_tax_id || ""));
          return pi?.id;
        }).filter((x): x is number => x != null);
        if (poolRowIds.length > 0) {
          const formJson = JSON.stringify(formRef.current);
          const results = await invoke<any[]>("validate_from_pool", { ids: poolRowIds, formJson });
          setEtaResult(results);
        }
      }
    } catch (e: any) {
      showAlert(`${t("验证失败", "Validation failed")}: ${e.message || e}`);
    }
  };

  const attachBatchFromPool = async (rowIds: number[]) => {
    if (data.doc_type === "import") return attachBatchImportEntriesFromPool(rowIds);
    if (rowIds.length === 0) return;
    const currentInvoices = [...(formRef.current.invoices ?? [])];
    const items = rowIds.map(id => poolList.find((x: any) => x.id === id)).filter(Boolean);
    const already = items.filter(p => currentInvoices.some(inv => inv.invoice_no === p!.invoice_id));
    const fresh = items.filter(p => !already.some(a => a!.invoice_id === p!.invoice_id) && isInvoiceUsable(p!.id));
    const blockedCount = items.length - already.length - fresh.length;
    if (blockedCount > 0) {
      showAlert(`${t("发票已被拒绝或取消，无法使用", "Rejected/cancelled invoices cannot be used")} (${blockedCount})`);
    }
    if (fresh.length === 0) {
      if (blockedCount === 0) {
        showAlert(t("选中的发票已在此文档中", "Selected invoices are already in this document"));
      }
      return;
    }
    const arr = [
      ...currentInvoices,
      ...fresh.map((p) => ({
        invoice_no: p!.invoice_id,
        seller_tax_id: p!.seller_tax_id || "",
        amount: (p!.net_amount ?? 0).toFixed(2),
        vat: (p!.total_vat ?? 0).toFixed(2),
        wht: (p!.total_wht ?? 0).toFixed(2),
        company_name: p!.seller_name || "",
        attached_invoice: p!.invoice_id,
      })),
    ];
    formRef.current = { ...formRef.current, invoices: arr };
    await recalc(formRef.current);
    const serial = ((formRef.current.doc_serial) || "draft").trim();
    const freshIds = fresh.map(p => p!.id);
    try {
      await invoke("mark_pool_invoices_used", { ids: freshIds, snapshotId: 0, snapshotLabel: serial });
    } catch {}
    try {
      if (authUser) await markPoolsUsedRemote(fresh.map(p => ({ invoice_id: p!.invoice_id, seller_tax_id: p!.seller_tax_id || "" })), serial);
    } catch (e) { console.error("markPoolsUsedRemote failed", e); }
    await loadPool();
    if (fresh.length > 0) {
      try {
        const poolIds = new Set(poolList.map((x: any) => x.invoice_id));
        const attached = arr.filter(inv => poolIds.has(inv.invoice_no));
        if (attached.length > 0) {
          const poolRowIds = attached.map(inv => {
            const pi = poolList.find((x: any) => x.invoice_id === inv.invoice_no && x.seller_tax_id === (inv.seller_tax_id || ""));
            return pi?.id;
          }).filter((x): x is number => x != null);
          if (poolRowIds.length > 0) {
            const formJson = JSON.stringify(formRef.current);
            const results = await invoke<any[]>("validate_from_pool", { ids: poolRowIds, formJson });
            setEtaResult(results);
          }
        }
      } catch (e: any) {
        showAlert(`${t("验证失败", "Validation failed")}: ${e.message || e}`);
      }
    }
  };

  const unclaimPoolInvoice = async (id: number) => {
    const p = poolList.find((x: any) => x.id === id);
    try {
      await invoke("mark_pool_invoice_available", { id });
      try {
        if (authUser && p) await markPoolAvailableRemote(p.invoice_id, p.seller_tax_id || "");
      } catch (e) { console.error("markPoolAvailableRemote failed", e); }
      loadPool();
    } catch (e: any) {
      showAlert(`${t("解除认领失败", "Unclaim failed")}: ${e.message || e}`);
    }
  };

  const importToPool = async () => {
    if (poolImportProgress) return;
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({ multiple: true, filters: [{ name: "XML", extensions: ["xml"] }] });
      if (!selected) return;
      const filePaths: string[] = (Array.isArray(selected) ? selected : [selected])
        .map((f: any) => typeof f === "string" ? f : f.path)
        .filter(Boolean);
      if (filePaths.length === 0) return;
      setPoolImportProgress({ processed: 0, total: filePaths.length, file: "" });
      const res = await invoke<any>("import_to_pool", { filePaths });
      const imported: any[] = res.imported || [];
      const failed: any[] = res.failed || [];
      let msg = `${t("已上传", "Uploaded")} ${imported.length} ${t("发票到池", "invoice(s) to pool")}`;
      if (failed.length > 0) {
        const names = failed.slice(0, 3).map((f: any) => f.file).join(", ");
        msg += `\n${t("跳过", "Skipped")} ${failed.length}: ${names}${failed.length > 3 ? "…" : ""}`;
      }
      showAlert(msg);
      if (authUser && imported.length > 0) {
        try {
          const local = await invoke<any[]>("list_invoice_pool");
          const rows = local.filter((l: any) => imported.some((i: any) => i.invoice_id === l.invoice_id && i.seller_tax_id === l.seller_tax_id));
          await upsertPoolInvoicesRemote(rows);
        } catch (e) { console.error("upsert pool remote failed", e); }
      }
      loadPool();
    } catch (e: any) {
      showAlert(`${t("上传失败", "Upload failed")}: ${e.message || e}`);
    }
    setPoolImportProgress(null);
  };

  const isInvoiceUsable = (id: number) => {
    const p = poolList.find((x: any) => x.id === id);
    return !p || !p.doc_status || p.doc_status === "Valid";
  };

  const validateFromPool = async (ids: number[]) => {
    if (!(data.doc_serial || "").trim()) {
      showAlert(t("请先输入本文档的文档编号（序列号）", "Please enter a serial number for this document first"));
      return;
    }
    const unusable = ids.filter(id => !isInvoiceUsable(id));
    if (unusable.length > 0) {
      const labels = unusable.map(id => { const p = poolList.find((x: any) => x.id === id); return p?.invoice_id || id; });
      showAlert(`${t("发票已被拒绝或取消，无法使用", "Rejected/cancelled invoices cannot be used")}: ${labels.join(", ")}`);
      return;
    }
    try {
      const formJson = JSON.stringify(formRef.current);
      const result = await invoke<any[]>("validate_from_pool", { ids, formJson });
      await attachValidatedInvoices(result);
      setEtaResult(result);
      setShowPool(false);
    } catch (e: any) {
      showAlert(`${t("验证失败", "Validation failed")}: ${e.message || e}`);
    }
  };

  const exportValidationReport = async () => {
    if (!etaResult || etaResult.length === 0) return;
    try {
      const path = await save({ defaultPath: "Validation_Report.xlsx", filters: [{ name: "Excel", extensions: ["xlsx"] }] });
      if (!path) return;
      await invoke("export_validation_report", { results: etaResult, filePath: path });
      showAlert(t("报告已导出", "Report exported"));
    } catch (e: any) {
      showAlert(`${t("导出失败", "Export failed")}: ${e.message || e}`);
    }
  };

  const deletePoolInvoice = async (id: number) => {
    try {
      const p = poolList.find((x: any) => x.id === id);
      const invId = p?.invoice_id;
      const sid = p?.seller_tax_id || "";
      if (authUser) {
        if (isAdminUser) {
          await invoke("delete_pool_invoice", { id });
          try { if (invId) await deletePoolInvoiceRemote(invId, sid); } catch (e) { console.error("deletePoolInvoiceRemote failed", e); }
          showAlert(t("发票已删除", "Invoice deleted"));
        } else {
          await invoke("request_pool_delete", { id, requestedBy: authUserId || authUser });
          try { if (invId) await requestPoolDeleteRemote(invId, sid); } catch (e) { console.error("requestPoolDeleteRemote failed", e); }
          showAlert(t("删除请求已提交，等待管理员确认", "Delete request submitted, awaiting admin approval"));
        }
      } else {
        await invoke("delete_pool_invoice", { id });
      }
      loadPool();
    } catch (e: any) {
      showAlert(`${t("删除失败", "Delete failed")}: ${e.message || e}`);
    }
  };

  const approvePoolDelete = async (id: number) => {
    try {
      const p = poolList.find((x: any) => x.id === id);
      const invId = p?.invoice_id;
      const sid = p?.seller_tax_id || "";
      await invoke("delete_pool_invoice", { id });
      try { if (invId) await deletePoolInvoiceRemote(invId, sid); } catch (e) { console.error("deletePoolInvoiceRemote failed", e); }
      showAlert(t("发票已删除", "Invoice deleted"));
      loadPool();
    } catch (e: any) {
      showAlert(`${t("删除失败", "Delete failed")}: ${e.message || e}`);
    }
  };

  const rejectPoolDelete = async (id: number) => {
    try {
      const p = poolList.find((x: any) => x.id === id);
      const invId = p?.invoice_id;
      const sid = p?.seller_tax_id || "";
      await invoke("reject_pool_delete", { id });
      try { if (invId) await rejectPoolDeleteRemote(invId, sid); } catch (e) { console.error("rejectPoolDeleteRemote failed", e); }
      showAlert(t("删除请求已拒绝", "Delete request rejected"));
      loadPool();
    } catch (e: any) {
      showAlert(`${t("拒绝失败", "Reject failed")}: ${e.message || e}`);
    }
  };

  const rejectDelete = async (id: number) => {
    try {
      await rejectDeleteSnapshot(id);
      showAlert(t("删除请求已拒绝", "Delete request rejected"));
    } catch (e: any) {
      showAlert(`${t("拒绝失败", "Reject failed")}: ${e.message || e}`);
    }
    loadHistoryList(historySearch);
  };

  return (
    <div className="app">
      <div ref={overlayRef} className="loading-overlay" style={{ display: 'none' }}>
        <div className="loading-modal">
          <div className="loading-spinner" />
          <p className="loading-message">{progressMsg || t("正在上传PDF，请稍候...", "Uploading PDF, please wait...")}</p>
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
              <img src="/assets/vouchify.svg" alt="Vouchify" />
            </div>
            <div className="brand-text">
              <h2>Vouchify</h2>
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
        <div className="sidebar-sync" style={{padding:'12px',borderTop:'1px solid rgba(255,255,255,0.08)',marginTop:4}}>
          {authUser ? (
            <div>
              <div style={{display:'flex',alignItems:'center',gap:10}}>
                <div style={{width:32,height:32,borderRadius:'50%',background:'var(--accent)',display:'flex',alignItems:'center',justifyContent:'center',fontSize:14,fontWeight:700,color:'#fff'}}>
                  {authUser.charAt(0).toUpperCase()}
                </div>
                <div style={{flex:1,minWidth:0}}>
                  <div style={{display:'flex',alignItems:'center',gap:5}}>
                    <span style={{width:7,height:7,borderRadius:'50%',background:synced?'var(--green)':'var(--red)',display:'inline-block',boxShadow:synced?'0 0 4px var(--green)':'none'}} />
                    <span style={{fontSize:12,fontWeight:600,overflow:'hidden',textOverflow:'ellipsis',whiteSpace:'nowrap',maxWidth:130}}>{authUser}</span>
                  </div>
                  <span style={{fontSize:10,color:synced?'var(--green)':'var(--red)',opacity:0.8}}>{synced ? t("已同步", "Synced") : t("未同步", "Not synced")}</span>
                </div>
                <button style={{fontSize:11,padding:'4px 12px',borderRadius:6,border:'1px solid rgba(255,255,255,0.15)',background:'rgba(255,255,255,0.05)',color:'rgba(255,255,255,0.7)',cursor:'pointer',transition:'all 0.15s'}} onClick={async () => { await signOut(); setAuthUser(null); setAuthUserId(null); setSynced(false); setShowChangePw(false); }}
                  onMouseEnter={e => {(e.target as HTMLElement).style.background='rgba(255,255,255,0.1)';(e.target as HTMLElement).style.color='rgba(255,255,255,0.9)'}}
                  onMouseLeave={e => {(e.target as HTMLElement).style.background='rgba(255,255,255,0.05)';(e.target as HTMLElement).style.color='rgba(255,255,255,0.7)'}}
                >
                  {t("登出", "Logout")}
                </button>
              </div>
              {!showChangePw && (
                <button style={{fontSize:10,padding:'3px 0',border:'none',background:'transparent',color:'rgba(255,255,255,0.35)',cursor:'pointer',marginTop:6,width:'100%',textAlign:'center'}} onClick={() => setShowChangePw(true)}>
                  {t("修改密码", "Change password")}
                </button>
              )}
              {showChangePw && (
                <div style={{display:'flex',flexDirection:'column',gap:6,marginTop:8}}>
                  <input style={{fontSize:11,padding:'6px 10px',borderRadius:8,border:'1px solid rgba(255,255,255,0.12)',background:'rgba(255,255,255,0.06)',color:'rgba(255,255,255,0.85)',width:'100%',outline:'none'}}
                    type="password"
                    placeholder={t("新密码", "New password")}
                    value={newPassword}
                    onChange={e => setNewPassword(e.target.value)} />
                  <div style={{display:'flex',gap:6}}>
                    <button style={{fontSize:11,padding:'5px 0',borderRadius:8,border:'none',background:'var(--accent)',color:'#fff',cursor:'pointer',fontWeight:600,flex:1}} onClick={async () => {
                      try { await changePassword(newPassword); setNewPassword(""); setShowChangePw(false); showAlert(t("密码已修改", "Password changed")); } catch (e: any) { showAlert(`${t("修改失败", "Change failed")}: ${e.message || e}`); }
                    }}>{t("确认", "Confirm")}</button>
                    <button style={{fontSize:11,padding:'5px 0',borderRadius:8,border:'1px solid rgba(255,255,255,0.12)',background:'transparent',color:'rgba(255,255,255,0.5)',cursor:'pointer',flex:1}} onClick={() => { setShowChangePw(false); setNewPassword(""); }}>
                      {t("取消", "Cancel")}
                    </button>
                  </div>
                </div>
              )}
            </div>
          ) : (
            <div style={{display:'flex',flexDirection:'column',gap:10}}>
              <div style={{textAlign:'center',marginBottom:2}}>
                <div style={{width:40,height:40,borderRadius:'50%',background:'rgba(255,255,255,0.06)',border:'1px solid rgba(255,255,255,0.1)',display:'flex',alignItems:'center',justifyContent:'center',fontSize:18,color:'rgba(255,255,255,0.4)',margin:'0 auto 8px'}}>🔒</div>
                <div style={{fontSize:13,fontWeight:600,color:'rgba(255,255,255,0.85)',marginBottom:2}}>{t("登录以同步", "Sign in to sync")}</div>
                <div style={{fontSize:10,color:'rgba(255,255,255,0.4)',lineHeight:1.4}}>{t("跨设备共享快照数据", "Share snapshots across devices")}</div>
              </div>
              <input style={{fontSize:12,padding:'8px 10px',borderRadius:8,border:'1px solid rgba(255,255,255,0.12)',background:'rgba(255,255,255,0.06)',color:'rgba(255,255,255,0.85)',width:'100%',outline:'none',transition:'border 0.15s'}}
                placeholder={t("邮箱", "Email")}
                value={authEmail}
                onChange={e => setAuthEmail(e.target.value)}
                onFocus={e => {(e.target as HTMLElement).style.borderColor='var(--accent)'}}
                onBlur={e => {(e.target as HTMLElement).style.borderColor='rgba(255,255,255,0.12)'}} />
              <input style={{fontSize:12,padding:'8px 10px',borderRadius:8,border:'1px solid rgba(255,255,255,0.12)',background:'rgba(255,255,255,0.06)',color:'rgba(255,255,255,0.85)',width:'100%',outline:'none',transition:'border 0.15s'}}
                type="password"
                placeholder={t("密码", "Password")}
                value={authPassword}
                onChange={e => setAuthPassword(e.target.value)}
                onFocus={e => {(e.target as HTMLElement).style.borderColor='var(--accent)'}}
                onBlur={e => {(e.target as HTMLElement).style.borderColor='rgba(255,255,255,0.12)'}} />
              <button style={{fontSize:12,padding:'8px 0',borderRadius:8,border:'none',background:'var(--accent)',color:'#fff',cursor:'pointer',fontWeight:600,transition:'background 0.15s'}} onClick={async () => {
                try { await signIn(authEmail, authPassword); setAuthPassword(""); } catch (e: any) { showAlert(`${t("登录失败", "Login failed")}: ${e.message || e}`); }
              }}
                onMouseEnter={e => {(e.target as HTMLElement).style.background='var(--accent-hover)'}}
                onMouseLeave={e => {(e.target as HTMLElement).style.background='var(--accent)'}}
              >{t("登录", "Sign In")}</button>
              <span style={{fontSize:10,color:'rgba(255,255,255,0.3)',textAlign:'center',marginTop:2}}>{t("账号由管理员创建", "Accounts created by admin")}</span>
            </div>
          )}
          <div style={{marginTop:10,paddingTop:8,borderTop:'1px solid rgba(255,255,255,0.06)',textAlign:'center'}}>
            <div style={{display:'flex',alignItems:'center',justifyContent:'center',gap:8,marginBottom:6}}>
              <span style={{fontSize:10,color:'rgba(255,255,255,0.35)'}}>{appVersion ? `v${appVersion}` : ""}</span>
              <button style={{fontSize:10,padding:'2px 8px',borderRadius:6,border:'1px solid rgba(255,255,255,0.12)',background:'transparent',color:'rgba(255,255,255,0.4)',cursor:'pointer'}} onClick={handleCheckUpdate} disabled={checkingUpdate}>
                {checkingUpdate ? t("检查中…", "Checking…") : t("检查更新", "Check Update")}
              </button>
            </div>
            <button style={{fontSize:10,padding:'4px 12px',borderRadius:6,border:'1px solid rgba(255,255,255,0.12)',background:'transparent',color:'rgba(255,255,255,0.4)',cursor:'pointer'}} onClick={() => setLang(lang === "zh" ? "en" : "zh")}>
              {lang === "zh" ? "English" : "中文"}
            </button>
          </div>
        </div>
        <div className="sidebar-actions">
          <button onClick={saveSnapshot} style={{background:'var(--accent)',color:'#fff',fontWeight:600}}>
            <IconSave color="#fff" /> {t("保存", "Save")}
          </button>
          <button onClick={showHistoryModal}>
            <IconHistory /> {t("历史记录", "History")}
          </button>
          <div className="sidebar-actions-divider" />
          <button onClick={newSession}>
            <IconNewSession /> {t("新会话", "New Session")}
          </button>
          <div className="sidebar-export-group">
            <button onClick={() => setShowImportMenu(!showImportMenu)} style={{width:'100%'}}>
              <IconImport /> {t("上传", "Upload")} <IconChevronDown size={12} style={{marginLeft:'auto', transition:'transform 0.2s', transform: showImportMenu ? 'rotate(0deg)' : 'rotate(180deg)'}} />
            </button>
            {showImportMenu && (
              <div className="sidebar-export-dropdown">
                <button onClick={() => { setShowImportMenu(false); importPdf(); }}>
                  <IconReport /> {t("上传PDF", "Upload PDF")}
                </button>
                <button onClick={() => { setShowImportMenu(false); openPool(); }}>
                  <IconInvoice /> {t("发票池", "Invoice Pool")}
                </button>
              </div>
            )}
          </div>
          <div className="sidebar-actions-divider" />
          <div className="sidebar-export-group">
            <button onClick={() => setShowExportMenu(!showExportMenu)} style={{width:'100%'}}>
              <IconExport /> {t("导出", "Export")} <IconChevronDown size={12} style={{marginLeft:'auto', transition:'transform 0.2s', transform: showExportMenu ? 'rotate(0deg)' : 'rotate(180deg)'}} />
            </button>
            {showExportMenu && (
              <div className="sidebar-export-dropdown">
                <button onClick={() => { setShowExportMenu(false); exportExcel(); }}>
                  <IconReport /> {t("结算报告", "Settlement Report")}
                </button>
                <button onClick={() => { setShowExportMenu(false); setShowInvoiceExport(true); }}>
                  <IconInvoice /> {t("发票清单", "Invoice Registry")}
                </button>
              </div>
            )}
          </div>
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
            {!isImport && InvoicesCard()}
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
          <div className="history-filters">
            <button className={historyFilter === "all" ? "active" : ""} onClick={() => setHistoryFilter("all")}>{t("全部", "All")}</button>
            <button className={historyFilter === "bank" ? "active" : ""} onClick={() => setHistoryFilter("bank")}>{t("银行", "Bank")}</button>
            <button className={historyFilter === "import" ? "active" : ""} onClick={() => setHistoryFilter("import")}>{t("进口", "Import")}</button>
          </div>
          <div className="history-list" style={historyLoading ? { opacity: 0.5 } : {}}>
            {historyLoading ? (
              <div className="history-empty">{t("加载中...", "Loading...")}</div>
            ) : (() => {
              const filtered = historyFilter === "all" ? historyList : historyList.filter(h => historyFilter === "import" ? h.doc_type === "import" : h.doc_type !== "import");
              return filtered.length === 0 ? <div className="history-empty">{t("未找到快照", "No snapshots found")}</div> : filtered.map(h => {
                const isOwn = !h.owner || h.owner === authUserId;
                const pendingDelete = h.delete_requested_at != null;
                const decisionColor = h.final_decision === "approve" ? "var(--green)" : h.final_decision === "conditional" ? "var(--orange)" : h.final_decision === "reject" ? "var(--red)" : "";
                const auditorName = h.auditor ? h.auditor.split("@")[0] : "";
                const snapInfo = (() => {
                  try {
                    const p = JSON.parse(h.data_json || "{}");
                    const isImport = p.doc_type === "import";
                    const taxSet = new Set<string>();
                    const coSet = new Set<string>();
                    const invSet = new Set<string>();
                    if (isImport) {
                      (p.import_entries || []).forEach((e: any) => {
                        if (e.attached_invoice) invSet.add(e.attached_invoice);
                        if (e.seller_tax_id) taxSet.add(e.seller_tax_id);
                      });
                    } else {
                      (p.seller_tax_ids || []).forEach((x: string) => x && taxSet.add(x));
                      if (p.seller_tax_id) taxSet.add(p.seller_tax_id);
                      (p.invoices || []).forEach((inv: any) => {
                        if (inv.invoice_no) invSet.add(inv.invoice_no);
                        if (inv.company_name) coSet.add(inv.company_name);
                        if (inv.seller_tax_id) taxSet.add(inv.seller_tax_id);
                      });
                    }
                    const rows: { label: string; value: string }[] = [];
                    if (taxSet.size > 0) rows.push({ label: t("卖方税号", "Seller TAX ID"), value: Array.from(taxSet).join(", ") });
                    if (coSet.size > 0) rows.push({ label: t("公司名称", "Company"), value: Array.from(coSet).join(", ") });
                    if (invSet.size > 0) rows.push({ label: t("发票", "Invoices"), value: Array.from(invSet).join(", ") });
                    return rows;
                  } catch { return [] as { label: string; value: string }[]; }
                })();
                return (
                <div key={h.id} className="history-item" style={{flexDirection:'column',alignItems:'stretch',justifyContent:'flex-start',gap:6,...(pendingDelete ? {background:'rgba(239,68,68,0.08)',borderLeft:'3px solid #ef4444'} : {})}}>
                  <div style={{display:'flex',justifyContent:'space-between',alignItems:'center',gap:8,minWidth:0}}>
                    <div style={{display:'flex',alignItems:'center',gap:8,minWidth:0}}>
                      {decisionColor && <span className="decision-dot" style={{background:decisionColor,flexShrink:0}} title={h.final_decision} />}
                      <div style={{minWidth:0}}>
                        <strong style={{display:'block',overflow:'hidden',textOverflow:'ellipsis',whiteSpace:'nowrap'}}>{h.label}</strong>
                        <p><small>
                          {h.created_at}
                          {auditorName ? ` · ${auditorName}` : ''}
                          {h.owner && authUserId && !isOwn ? ` · ${t("他人", "Other user")}` : ''}
                          {pendingDelete ? ` · ⚠️ ${t("待删除", "Pending delete")}` : ''}
                        </small></p>
                      </div>
                    </div>
                    <div className="history-actions">
                      <button className="btn-load" onClick={() => loadSnapshot(h.id)}>Load</button>
                      {(isOwn || isAdminUser) && !pendingDelete && (
                        <button className="btn-approve" onClick={() => startOverwrite(h.id)} title={t("加载并覆盖此快照（重置审核决定）", "Load & overwrite this snapshot (resets decision)")}>{t("覆盖", "Overwrite")}</button>
                      )}
                      {isAdminUser && pendingDelete && (
                        <>
                          <button className="btn-approve" onClick={() => approveDelete(h.id)}>{t("批准", "Approve")}</button>
                          <button className="btn-reject" onClick={() => rejectDelete(h.id)}>{t("拒绝", "Reject")}</button>
                        </>
                      )}
                      {isAdminUser && !pendingDelete && (
                        <button className="btn-delete" onClick={() => deleteSnapshot(h.id)}>Delete</button>
                      )}
                      {!isAdminUser && authUser && !pendingDelete && (
                        <button className="btn-delete" onClick={() => deleteSnapshot(h.id)}>{t("请求删除", "Request delete")}</button>
                      )}
                      {!authUser && <button className="btn-delete" onClick={() => deleteSnapshot(h.id)}>Delete</button>}
                    </div>
                  </div>
                  {snapInfo.length > 0 && (
                    <div style={{borderTop:'1px dashed var(--border)',paddingTop:6,fontSize:11,color:'var(--text-secondary)',display:'grid',gap:2,wordBreak:'break-all'}}>
                      {snapInfo.map((r, i) => (
                        <div key={i}>
                          <span style={{color:'var(--accent)',fontWeight:600}}>{r.label}: </span>
                          <span style={{userSelect:'text'}}>{r.value}</span>
                        </div>
                      ))}
                    </div>
                  )}
                </div>
                );
              });
            })()}
          </div>
</div>
        </div>

      {showInvoiceExport && (
        <div style={{
          position: 'fixed', top: 0, left: 0, right: 0, bottom: 0,
          background: 'rgba(0,0,0,0.45)', display: 'flex', alignItems: 'center', justifyContent: 'center',
          zIndex: 9999,
        }} onClick={() => setShowInvoiceExport(false)}>
          <div style={{
            background: 'var(--bg-card, #fff)', borderRadius: 12, padding: '28px 36px',
            boxShadow: '0 8px 32px rgba(0,0,0,0.18)', maxWidth: 420, minWidth: 320,
            border: '1px solid var(--border, #e0e0e0)',
          }} onClick={e => e.stopPropagation()}>
            <h3 style={{marginBottom:20,fontSize:16}}>{t("发票清单导出", "Invoice Registry Export")}</h3>
            <div style={{marginBottom:16}}>
              <label style={{fontSize:12,fontWeight:600,marginBottom:6,display:'block'}}>{t("开始日期", "From date")}</label>
              <input type="date" style={{width:'100%',padding:'8px 10px',borderRadius:8,border:'1px solid var(--border)',fontSize:13}}
                value={invoiceExportFrom} onChange={e => setInvoiceExportFrom(e.target.value)} />
            </div>
            <div style={{marginBottom:20}}>
              <label style={{fontSize:12,fontWeight:600,marginBottom:6,display:'block'}}>{t("结束日期", "To date")}</label>
              <input type="date" style={{width:'100%',padding:'8px 10px',borderRadius:8,border:'1px solid var(--border)',fontSize:13}}
                value={invoiceExportTo} onChange={e => setInvoiceExportTo(e.target.value)} />
            </div>
            <div style={{display:'flex',gap:8}}>
              <button style={{padding:'8px 20px',borderRadius:8,border:'none',background:'var(--accent)',color:'#fff',cursor:'pointer',fontWeight:600,fontSize:14}}
                onClick={exportInvoiceSummary}>{t("导出", "Export")}</button>
              <button style={{padding:'8px 20px',borderRadius:8,border:'1px solid var(--border)',background:'transparent',color:'inherit',cursor:'pointer',fontSize:14}}
                onClick={() => setShowInvoiceExport(false)}>{t("取消", "Cancel")}</button>
            </div>
          </div>
        </div>
      )}

      {etaResult && etaResult.length > 0 && (
        <div className="modal-overlay" style={{position:'fixed'}} onClick={() => setEtaResult(null)}>
          <div className="modal" style={{width:700, maxHeight:'85vh'}} onClick={e => e.stopPropagation()}>
            <div className="modal-header">
              <h3>{t("ETA XML 验证结果", "ETA XML Validation Result")} ({etaResult.length} {t("发票", "invoices")})</h3>
              <div style={{display:'flex',gap:8,alignItems:'center'}}>
                <button className="btn-add" style={{background:'var(--accent)'}} onClick={exportValidationReport}>
                  {t("导出报告", "Export Report")}
                </button>
                <button className="modal-close" onClick={() => setEtaResult(null)}>✕</button>
              </div>
            </div>

            <div style={{marginBottom:10}}>
              <input
                className="field-input"
                style={{width:'100%',boxSizing:'border-box'}}
                placeholder={t("搜索发票ID或卖方...", "Search invoices...")}
                value={resultSearch}
                onChange={e => setResultSearch(e.target.value)}
              />
            </div>

            {(() => {
              const rq = resultSearch.trim().toLowerCase();
              const shownResults = rq ? etaResult.filter((r: any) => {
                const hay = [r.invoice.invoice_id || "", r.invoice.seller_name || "", r.invoice.seller_tax_id || ""].join(" ").toLowerCase();
                return hay.includes(rq);
              }) : etaResult;
              const totalErrors = shownResults.reduce((s: number, r: any) => s + r.issues.filter((i: any) => i.severity === "error").length, 0);
              const totalWarnings = shownResults.reduce((s: number, r: any) => s + r.issues.filter((i: any) => i.severity === "warning").length, 0);
              const allValid = shownResults.every((r: any) => r.is_valid);
              if (shownResults.length !== etaResult.length) {
                return (
                  <div style={{fontSize:11,color:'var(--text-secondary)',marginBottom:10}}>
                    {t("显示", "Showing")} {shownResults.length} / {etaResult.length}
                  </div>
                );
              }
              return (
                <div style={{display:'flex',gap:12,marginBottom:16}}>
                  <div style={{flex:1,padding:'10px 14px',borderRadius:8,background:allValid?'var(--green-bg)':'var(--red-bg)',border:`1px solid ${allValid?'#bbf7d0':'#fecaca'}`,textAlign:'center'}}>
                    <div style={{fontSize:18,fontWeight:700,color:allValid?'var(--green)':'var(--red)'}}>{allValid ? '✓' : '✗'}</div>
                    <div style={{fontSize:11,color:'var(--text-secondary)'}}>{allValid ? t("全部通过", "All Passed") : t("存在问题", "Issues Found")}</div>
                  </div>
                  <div style={{flex:1,padding:'10px 14px',borderRadius:8,background:'var(--bg-input)',border:'1px solid var(--border)',textAlign:'center'}}>
                    <div style={{fontSize:18,fontWeight:700,color:'var(--red)'}}>{totalErrors}</div>
                    <div style={{fontSize:11,color:'var(--text-secondary)'}}>{t("错误", "Errors")}</div>
                  </div>
                  <div style={{flex:1,padding:'10px 14px',borderRadius:8,background:'var(--bg-input)',border:'1px solid var(--border)',textAlign:'center'}}>
                    <div style={{fontSize:18,fontWeight:700,color:'var(--orange)'}}>{totalWarnings}</div>
                    <div style={{fontSize:11,color:'var(--text-secondary)'}}>{t("警告", "Warnings")}</div>
                  </div>
                </div>
              );
            })()}

            <div style={{display:'flex',flexDirection:'column',gap:12,maxHeight:'calc(85vh - 180px)',overflowY:'auto'}}>
              {(() => {
                const rq = resultSearch.trim().toLowerCase();
                const shownResults = rq ? etaResult.filter((r: any) => {
                  const hay = [r.invoice.invoice_id || "", r.invoice.seller_name || "", r.invoice.seller_tax_id || ""].join(" ").toLowerCase();
                  return hay.includes(rq);
                }) : etaResult;
                return shownResults.map((r: any, idx: number) => {
                const errorCount = r.issues.filter((i: any) => i.severity === "error").length;
                const warnCount = r.issues.filter((i: any) => i.severity === "warning").length;
                return (
                <div key={idx} style={{border:'1px solid var(--border)',borderRadius:8,overflow:'hidden'}}>
                  <div style={{padding:'8px 12px',background:'var(--bg-input)',display:'flex',alignItems:'center',gap:8,borderBottom:'1px solid var(--border)'}}>
                    <span style={{width:8,height:8,borderRadius:'50%',background:r.is_valid?'var(--green)':errorCount>0?'var(--red)':'var(--orange)',flexShrink:0}} />
                    <strong style={{fontSize:12}}>{r.invoice.invoice_id || `Invoice ${idx + 1}`}</strong>
                    <span style={{fontSize:11,color:'var(--text-muted)',marginLeft:'auto'}}>
                      {r.invoice.seller_name || r.invoice.seller_tax_id}
                      {errorCount > 0 && <span style={{color:'var(--red)',marginLeft:8}}>{errorCount} {t("错误", "errors")}</span>}
                      {warnCount > 0 && <span style={{color:'var(--orange)',marginLeft:8}}>{warnCount} {t("警告", "warnings")}</span>}
                    </span>
                  </div>
                  <div style={{padding:'8px 12px',fontSize:12}}>
                    <div style={{display:'grid',gridTemplateColumns:'repeat(auto-fit,minmax(90px,1fr))',gap:4,marginBottom:8}}>
                      {r.invoice.currency && r.invoice.currency !== 'EGP' ? (
                        <div><span style={{color:'var(--accent)',fontWeight:700}}>[{r.invoice.currency}]</span></div>
                      ) : null}
                      <div><span style={{color:'var(--text-muted)'}}>{t("净额", "Net")}:</span> {r.invoice.net_amount.toFixed(2)}</div>
                      <div><span style={{color:'var(--text-muted)'}}>VAT:</span> {r.invoice.total_vat.toFixed(2)}</div>
                      <div><span style={{color:'var(--text-muted)'}}>WHT:</span> {r.invoice.total_wht.toFixed(2)}</div>
                      <div><span style={{color:'var(--text-muted)'}}>{t("合计", "Total")}:</span> {r.invoice.grand_total.toFixed(2)}</div>
                    </div>
                    {r.issues.length === 0 ? (
                      <div style={{color:'var(--green)',fontWeight:600,fontSize:11,textAlign:'center',padding:4}}>✓ {t("所有检查均通过", "All checks passed")}</div>
                    ) : (
                      <div style={{display:'flex',flexDirection:'column',gap:4}}>
                        {r.issues.map((issue: any, i: number) => (
                          <div key={i} style={{
                            display:'flex',alignItems:'flex-start',gap:6,padding:'6px 8px',
                            borderRadius:4,fontSize:11,
                            background: issue.severity === 'error' ? 'var(--red-bg)' : '#fffbeb',
                            border: `1px solid ${issue.severity === 'error' ? '#fecaca' : '#fde68a'}`,
                          }}>
                            <span style={{color: issue.severity === 'error' ? 'var(--red)' : 'var(--orange)',fontWeight:700,flexShrink:0}}>
                              {issue.severity === 'error' ? '✗' : '⚠'}
                            </span>
                            <div>
                              <strong>{issue.field}</strong>
                              <span style={{color:'var(--text-secondary)',marginLeft:6}}>{issue.message}</span>
                              <div style={{fontSize:10,color:'var(--text-muted)',marginTop:1}}>
                                XML: {issue.xml_value} → {t("表单", "Form")}: {issue.form_value}
                              </div>
                            </div>
                          </div>
                        ))}
                      </div>
                    )}
                  </div>
                </div>
                );
                });
              })()}
            </div>
          </div>
        </div>
      )}

      {showPool && (
        <div className="modal-overlay" style={{position:'fixed'}} onClick={() => setShowPool(false)}>
          <div className="modal" style={{width:700, maxHeight:'85vh'}} onClick={e => e.stopPropagation()}>
            <div className="modal-header">
              <h3>{t("发票池", "Invoice Pool")}</h3>
              <button className="modal-close" onClick={() => setShowPool(false)}>✕</button>
            </div>
            <div className="pool-toolbar">
              <button className="btn-add" onClick={importToPool} disabled={!!poolImportProgress}>
                {poolImportProgress ? t("上传中…", "Uploading…") : `+ ${t("上传XML", "Upload XML")}`}
              </button>
              <button className="pool-btn" style={{background:'#0ea5e9',color:'#fff'}} onClick={async () => {
                setPoolLoading(true);
                await syncPoolRemote();
                try { setPoolList(await invoke<any[]>("list_invoice_pool")); } catch (e) { console.error(e); }
                setPoolLoading(false);
              }}>{t("同步云端", "Sync Now")}</button>
              <div style={{flex:1}} />
              <button className="pool-btn" onClick={async () => {
                const n = await restoreClaimsFromDocument();
                showAlert(n > 0
                  ? `${t("已认领", "Claimed")} ${n} ${t("张本文档引用的发票", "invoice(s) referenced by this document")}`
                  : t("本文档没有可恢复的发票", "No recoverable invoices for this document"));
              }}>{t("恢复本文档认领", "Restore Doc Claims")}</button>
              <button className="pool-btn" onClick={async () => {
                const n = await restoreAllClaims();
                showAlert(n > 0
                  ? `${t("已恢复", "Restored")} ${n} ${t("张发票在所有已保存文档中的认领", "invoice claim(s) across all saved documents")}`
                  : t("没有可恢复的发票", "No recoverable invoices"));
              }}>{t("恢复全部文档认领", "Restore All Claims")}</button>
            </div>
            <div className={`pool-syncinfo ${poolSyncInfo.ok ? 'ok' : 'err'}`}>
              <span>sync: local={poolSyncInfo.local} · cloud={poolSyncInfo.cloud} · ↑{poolSyncInfo.pushed} ↓{poolSyncInfo.pulled}</span>
              {poolSyncInfo.error && <span style={{marginLeft:8}}>ERROR: {poolSyncInfo.error}</span>}
            </div>
            {poolImportProgress && (() => {
              const pp = poolImportProgress;
              const pct = pp.total > 0 ? Math.round((pp.processed / pp.total) * 100) : 0;
              return (
                <div style={{marginBottom:10}}>
                  <div style={{display:'flex',justifyContent:'space-between',fontSize:11,marginBottom:4,color:'var(--text-secondary)'}}>
                    <span>{t("正在导入发票到池", "Importing invoices into pool")}…</span>
                    <span style={{fontFamily:'var(--font-mono)',fontWeight:600}}>{pp.processed}/{pp.total} ({pct}%)</span>
                  </div>
                  <div className="pool-progress-track">
                    <div className="pool-progress-fill" style={{width:`${pct}%`}} />
                  </div>
                  {pp.file && (
                    <div style={{fontSize:10,color:'var(--text-muted)',marginTop:3,overflow:'hidden',textOverflow:'ellipsis',whiteSpace:'nowrap'}}>📄 {pp.file}</div>
                  )}
                </div>
              );
            })()}
            {poolMode === 'select' && (
              <div style={{fontSize:11,color:'var(--text-secondary)',marginBottom:8}}>
                {t("选择要附加到本文档的发票（点击“添加”将其加入发票列表并标记为已认领）", "Select invoices to attach to this document. Click \"Add\" to include them in the invoice list and mark them as claimed.")}
              </div>
            )}
            <input
              className="field-input"
              style={{width:'100%', marginBottom:10, boxSizing:'border-box'}}
              placeholder={t("搜索发票ID、卖方税号、文件名或序列号...", "Search invoice ID, seller tax ID, file name, or serial...")}
              value={poolSearch}
              onChange={e => setPoolSearch(e.target.value)}
            />
            {(() => {
              const q = poolSearch.trim().replace(/\s+/g, " ").toLowerCase();
              const sellers = Array.from(new Set(poolList.map((p: any) => p.seller_tax_id).filter(Boolean))) as string[];
              const currencies = Array.from(new Set(poolList.map((p: any) => p.currency || 'EGP').filter(Boolean))) as string[];
              const base = poolList.filter((p: any) => {
                if (q) {
                  const hay = [p.invoice_id, p.seller_tax_id, p.seller_name, p.file_name || "", p.used_by_label || ""].join(" ").replace(/\s+/g, " ").toLowerCase();
                  if (!hay.includes(q)) return false;
                }
                if (poolSeller !== 'all' && (p.seller_tax_id || '') !== poolSeller) return false;
                if (poolCurrency !== 'all' && (p.currency || 'EGP') !== poolCurrency) return false;
                if (poolDateFrom && p.issue_date && p.issue_date < poolDateFrom) return false;
                if (poolDateTo && p.issue_date && p.issue_date > poolDateTo) return false;
                return true;
              });
              const statusCounts = { Valid: 0, Rejected: 0, Cancelled: 0 };
              for (const p of base) {
                const s = (p.doc_status || "Valid") as "Valid" | "Rejected" | "Cancelled";
                if (s in statusCounts) statusCounts[s]++;
              }
              // An explicit search spans every tab and status chip so a pasted
              // invoice ID is always found; filters only apply while browsing.
              const searching = q.length > 0;
              const effDocFilter = searching ? "all" : poolDocFilter;
              const filtered = effDocFilter === 'all' ? base : base.filter((p: any) => (p.doc_status || "Valid") === effDocFilter);
              const unclaimed = filtered.filter((p: any) => p.status === 'available');
              const claimed = filtered.filter((p: any) => p.status === 'used');
              const shown = searching ? filtered : (poolTab === 'unclaimed' ? unclaimed : claimed);
              const selectedIds = shown.filter((p: any) => p.status === 'available' && (p.doc_status || "Valid") === "Valid" && poolSelected.has(p.id)).map((p: any) => p.id);
              return (
                <>
                  <div className="pool-seg" style={{marginBottom:10}}>
                    <button className={poolTab === 'unclaimed' ? 'active' : ''} onClick={() => setPoolTab('unclaimed')}>
                      {t("未认领", "Unclaimed")} ({unclaimed.length})
                    </button>
                    <button className={poolTab === 'claimed' ? 'active' : ''} onClick={() => setPoolTab('claimed')}>
                      {t("已认领", "Claimed")} ({claimed.length})
                    </button>
                  </div>
                  <div style={{display:'flex',gap:6,marginBottom:10,flexWrap:'wrap', opacity: searching ? 0.45 : 1, pointerEvents: searching ? 'none' : 'auto', transition:'opacity .15s'}}>
                    {([
                      ["all", t("全部", "All"), base.length, "#3b82f6"],
                      ["Valid", t("有效", "Valid"), statusCounts.Valid, "var(--green)"],
                      ["Rejected", t("已拒绝", "Rejected"), statusCounts.Rejected, "var(--red)"],
                      ["Cancelled", t("已取消", "Cancelled"), statusCounts.Cancelled, "#64748b"],
                    ] as [string, string, number, string][]).map(([key, label, count, color]) => (
                      <button key={key} className={`pool-chip${poolDocFilter === key ? ' pool-chip-active' : ''}`} style={poolDocFilter === key
                        ? { background: color, borderColor: color, color: '#fff' }
                        : undefined} onClick={() => setPoolDocFilter(key as any)}>
                        <span className="dot" style={{ background: poolDocFilter === key ? '#fff' : color }} />
                        {label} ({count})
                      </button>
                    ))}
                  </div>
                  {searching && (
                    <div style={{fontSize:10,color:'var(--text-muted)',marginTop:-6,marginBottom:10}}>
                      {t("搜索覆盖所有状态和标签", "Search spans all statuses and tabs")}
                    </div>
                  )}
                  {selectedIds.length > 0 && (
                    <div style={{display:'flex',alignItems:'center',gap:8,marginBottom:10}}>
                      {poolMode === 'validate' ? (
                        <button className="btn-add" onClick={() => validateFromPool(selectedIds)}>
                          {t("验证选中", "Validate Selected")} ({selectedIds.length})
                        </button>
                      ) : (
                        <button className="btn-add" onClick={() => attachBatchFromPool(selectedIds)}>
                          {t("添加选中", "Attach Selected")} ({selectedIds.length})
                        </button>
                      )}
                    </div>
                  )}
                  {poolTab === 'unclaimed' && (
                    <div className="pool-filterbar" style={{marginBottom:10}}>
                      <select className="field-input" style={{width:120}} value={poolCurrency} onChange={e => setPoolCurrency(e.target.value)}>
                        <option value="all">{t("所有货币", "All currencies")}</option>
                        {currencies.map(c => <option key={c} value={c}>{c}</option>)}
                      </select>
                      <select className="field-input" style={{width:160}} value={poolSeller} onChange={e => setPoolSeller(e.target.value)}>
                        <option value="all">{t("所有卖方", "All sellers")}</option>
                        {sellers.map(s => <option key={s} value={s}>{s}</option>)}
                      </select>
                      <input className="field-input" style={{width:110}} type="date" value={poolDateFrom} onChange={e => setPoolDateFrom(e.target.value)} />
                      <span style={{color:'var(--text-muted)',fontSize:11}}>–</span>
                      <input className="field-input" style={{width:110}} type="date" value={poolDateTo} onChange={e => setPoolDateTo(e.target.value)} />
                      {(poolSeller !== 'all' || poolCurrency !== 'all' || poolDateFrom || poolDateTo) && (
                        <button className="pool-chip" style={{marginLeft:'auto'}} onClick={() => { setPoolSeller('all'); setPoolCurrency('all'); setPoolDateFrom(''); setPoolDateTo(''); }}>
                          ✕ {t("重置筛选", "Reset filters")}
                        </button>
                      )}
                    </div>
                  )}
                  <div className="history-list" style={poolLoading ? {opacity:0.5} : {}}>
                    {poolLoading ? (
                      <div className="history-empty">{t("加载中...", "Loading...")}</div>
                    ) : poolList.length === 0 ? (
                      <div className="history-empty">{t("发票池为空，上传XML发票以开始", "Pool is empty. Upload XML invoices to get started.")}</div>
                    ) : shown.length === 0 ? (
                      <div className="history-empty">{t("无匹配结果", "No matching invoices")}</div>
                    ) : shown.map((p: any) => {
                      const pendingDelete = p.delete_requested_at != null;
                      const docStatus = p.doc_status || "Valid";
                      const unusable = docStatus !== "Valid";
                      return (
                      <div key={p.id} className="history-item" style={pendingDelete ? {background:'rgba(239,68,68,0.08)',borderLeft:'3px solid #ef4444'} : unusable ? {opacity:0.6, borderLeft:'3px solid var(--red)'} : (p.status === 'used' ? {opacity:0.55, borderLeft:'3px solid var(--orange)'} : {borderLeft:'3px solid var(--green)'})}>
                        {poolTab === 'unclaimed' && p.status === 'available' && !unusable && (
                          <input
                            type="checkbox"
                            style={{margin:2,flexShrink:0,cursor:'pointer',width:14,height:14}}
                            checked={poolSelected.has(p.id)}
                            onChange={() => {
                              const next = new Set(poolSelected);
                              if (next.has(p.id)) next.delete(p.id);
                              else next.add(p.id);
                              setPoolSelected(next);
                            }}
                          />
                        )}
                        <div style={{minWidth:0}}>
                          <div style={{display:'flex',alignItems:'center',gap:6,flexWrap:'wrap'}}>
                            <strong style={{fontSize:12}}>{p.invoice_id}</strong>
                            {unusable ? (
                              docStatus === "Cancelled" ? (
                                <span style={{fontSize:9,padding:'2px 6px',borderRadius:4,fontWeight:700,
                                  background:'#f1f5f9',color:'#64748b',border:'1px solid #cbd5e1'
                                }}>{t("已取消", "Cancelled")}</span>
                              ) : (
                                <span style={{fontSize:9,padding:'2px 6px',borderRadius:4,fontWeight:700,
                                  background:'#fef2f2',color:'var(--red)',border:'1px solid #fecaca'
                                }}>{t("已拒绝", "Rejected")}</span>
                              )
                            ) : p.status === 'used' ? (
                              <span style={{fontSize:9,padding:'2px 6px',borderRadius:4,fontWeight:600,
                                background:'#fffbeb',color:'var(--orange)',border:'1px solid #fde68a'
                              }}>{t("已认领", "Claimed")}</span>
                            ) : (
                              <span style={{fontSize:9,padding:'2px 6px',borderRadius:4,fontWeight:600,
                                background:'var(--green-bg)',color:'var(--green)',border:'1px solid #bbf7d0'
                              }}>{t("未认领", "Unclaimed")}</span>
                            )}
                            {pendingDelete && (
                              <span style={{fontSize:9,padding:'2px 6px',borderRadius:4,fontWeight:700,
                                background:'#fef2f2',color:'var(--red)',border:'1px solid #fecaca'
                              }}>⚠️ {t("待删除", "Pending delete")}</span>
                            )}
                            {p.status === 'used' && p.used_by_label && (
                              <span style={{fontSize:9,padding:'2px 6px',borderRadius:4,fontWeight:700,
                                background:'var(--accent-light)',color:'var(--accent)',
                                border:'1px solid var(--accent)'
                              }}>{t("序列号", "Serial")}: {p.used_by_label}</span>
                            )}
                          </div>
                          <p style={{fontSize:11,color:'var(--text-secondary)',marginTop:2}}>
                            {p.seller_name || p.seller_tax_id}
                            {p.seller_name && p.seller_tax_id ? ` · ${p.seller_tax_id}` : ''}
                            {p.issue_date ? ` · ${p.issue_date}` : ''}
                          </p>
                          {p.file_name && (
                            <p style={{fontSize:10,color:'var(--text-muted)',marginTop:1}}>📄 {p.file_name}</p>
                          )}
                          <p style={{fontSize:11,color:'var(--text-muted)',marginTop:1}}>
                            {p.currency && p.currency !== 'EGP' ? (
                              <span style={{fontWeight:700,color:'var(--accent)',marginRight:4}}>[{p.currency}]</span>
                            ) : null}
                            {t("净额", "Net")}: {p.net_amount.toFixed(2)} · VAT: {p.total_vat.toFixed(2)}{p.total_wht > 0 ? ` · WHT: ${p.total_wht.toFixed(2)}` : ''} · {t("合计", "Total")}: {p.grand_total.toFixed(2)}
                          </p>
                        </div>
                        <div className="history-actions">
                          {poolMode === 'select' && p.status === 'available' && !unusable && (
                            <button className="btn-load" onClick={() => attachFromPool(p.id)}>
                              {t("添加", "Add")}
                            </button>
                          )}
                          {poolMode === 'validate' && p.status === 'available' && !unusable && (
                            <button className="btn-load" onClick={() => validateFromPool([p.id])}>
                              {t("验证", "Validate")}
                            </button>
                          )}
                          {poolMode !== 'select' && p.status === 'used' && !pendingDelete && (
                            <button className="btn-load" onClick={() => unclaimPoolInvoice(p.id)}>
                              {t("解除认领", "Unclaim")}
                            </button>
                          )}
                          {isAdminUser && pendingDelete && (
                            <>
                              <button className="btn-approve" onClick={() => approvePoolDelete(p.id)}>{t("批准", "Approve")}</button>
                              <button className="btn-reject" onClick={() => rejectPoolDelete(p.id)}>{t("拒绝", "Reject")}</button>
                            </>
                          )}
                          {!pendingDelete && (
                            <button className="btn-delete" onClick={() => deletePoolInvoice(p.id)}>✕</button>
                          )}
                        </div>
                      </div>
                      );
                    })}
                  </div>
                </>
              );
            })()}
          </div>
        </div>
      )}
    </div>
  );
}

export default App;
