import { createClient, Session, User } from "@supabase/supabase-js";

const SUPABASE_URL = "https://ywvhaemmvqeqkridfptm.supabase.co";
const SUPABASE_ANON_KEY = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJpc3MiOiJzdXBhYmFzZSIsInJlZiI6Inl3dmhhZW1tdnFlcWtyaWRmcHRtIiwicm9sZSI6ImFub24iLCJpYXQiOjE3ODQ2Mjc4MjAsImV4cCI6MjEwMDIwMzgyMH0.xO7JUnAIigxxIOjlLvEOEJPzPNwKh6PPU7NtmKV9AuI";

export const supabase = createClient(SUPABASE_URL, SUPABASE_ANON_KEY);

export interface SnapshotRow {
  id: number;
  label: string;
  notes: string;
  data_json: string;
  created_at: string;
  user_id: string;
  delete_requested_at: string | null;
  delete_requested_by: string | null;
}

export async function signUp(email: string, password: string): Promise<User | null> {
  const { data, error } = await supabase.auth.signUp({ email, password });
  if (error) throw error;
  return data.user;
}

export async function signIn(email: string, password: string): Promise<Session | null> {
  const { data, error } = await supabase.auth.signInWithPassword({ email, password });
  if (error) throw error;
  return data.session;
}

export async function signOut(): Promise<void> {
  const { error } = await supabase.auth.signOut();
  if (error) throw error;
}

export async function getSession(): Promise<Session | null> {
  const { data } = await supabase.auth.getSession();
  return data.session;
}

export async function saveSnapshotRemote(label: string, notes: string, dataJson: string): Promise<number> {
  const session = await getSession();
  if (!session) throw new Error("Not authenticated");
  const { data, error } = await supabase
    .from("snapshots")
    .insert({ user_id: session.user.id, label, notes, data_json: dataJson })
    .select("id")
    .single();
  if (error) throw error;
  return data.id;
}

async function fetchAllPooled<T>(build: (from: number, to: number) => PromiseLike<T[]>, pageSize = 1000): Promise<T[]> {
  const all: T[] = [];
  for (let from = 0; ; from += pageSize) {
    const page = await build(from, from + pageSize - 1);
    if (!page) break;
    all.push(...page);
    if (page.length < pageSize) break;
  }
  return all;
}

export async function listSnapshotsRemote(search?: string): Promise<SnapshotRow[]> {
  const session = await getSession();
  if (!session) throw new Error("Not authenticated");
  const build = (from: number, to: number) => {
    let query = supabase
      .from("snapshots")
      .select("id, label, notes, data_json, created_at, user_id, delete_requested_at, delete_requested_by")
      .order("created_at", { ascending: false })
      .range(from, to);
    if (search) {
      query = query.ilike("label", `%${search}%`);
    }
    return query.then(({ data, error }) => {
      if (error) throw error;
      return data;
    });
  };
  return fetchAllPooled(build);
}

export async function loadSnapshotRemote(id: number): Promise<string> {
  const { data, error } = await supabase
    .from("snapshots")
    .select("data_json")
    .eq("id", id)
    .single();
  if (error) throw error;
  return data.data_json;
}

export async function deleteSnapshotRemote(id: number): Promise<void> {
  const { data, error } = await supabase
    .from("snapshots")
    .delete()
    .eq("id", id)
    .select("id");
  if (error) throw error;
  if (!data || data.length === 0) throw new Error("Delete had no effect — row may not exist or RLS blocked it");
}

export async function updateSnapshotRemote(id: number, label: string, notes: string, dataJson: string): Promise<void> {
  const session = await getSession();
  if (!session) throw new Error("Not authenticated");
  const { error } = await supabase
    .from("snapshots")
    .update({ label, notes, data_json: dataJson })
    .eq("id", id);
  if (error) throw error;
}

export async function changePassword(newPassword: string): Promise<void> {
  const { error } = await supabase.auth.updateUser({ password: newPassword });
  if (error) throw error;
}

export async function isAdmin(): Promise<boolean> {
  const session = await getSession();
  if (!session) return false;
  const role = session.user.app_metadata?.role ?? "";
  return role === "admin";
}

export async function requestDeleteSnapshot(id: number): Promise<void> {
  const session = await getSession();
  if (!session) throw new Error("Not authenticated");
  const { error } = await supabase
    .from("snapshots")
    .update({ delete_requested_at: new Date().toISOString(), delete_requested_by: session.user.id })
    .eq("id", id);
  if (error) throw error;
}

export async function approveDeleteSnapshot(id: number): Promise<void> {
  await deleteSnapshotRemote(id);
}

export async function rejectDeleteSnapshot(id: number): Promise<void> {
  const { error } = await supabase
    .from("snapshots")
    .update({ delete_requested_at: null, delete_requested_by: null })
    .eq("id", id);
  if (error) throw error;
}

export interface PoolInvoiceRow {
  id: number;
  invoice_id: string;
  uuid: string;
  seller_tax_id: string;
  seller_name: string;
  buyer_tax_id: string;
  buyer_name: string;
  issue_date: string;
  currency: string;
  net_amount: number;
  total_vat: number;
  total_wht: number;
  grand_total: number;
  lines_json: string;
  raw_xml: string;
  file_name: string;
  doc_status?: string;
  status: string;
  used_by_label: string;
  delete_requested_at: string | null;
  delete_requested_by: string | null;
  created_at: string;
}

// The doc_status column was added later than the base table; until the project
// has run the ALTER TABLE migration, queries naming it fail. Detect that once
// and fall back to queries/payloads without the column so sync keeps working.
let cloudHasDocStatus = true;

function isMissingDocStatusError(e: any): boolean {
  const msg = String((e && (e.message ?? e)) || "");
  return msg.includes("doc_status");
}

function poolSelect(colsBase: string): string {
  return cloudHasDocStatus ? `${colsBase}, doc_status` : colsBase;
}

export async function listPoolRemote(): Promise<PoolInvoiceRow[]> {
  const session = await getSession();
  if (!session) throw new Error("Not authenticated");
  const COLS = "id, invoice_id, uuid, seller_tax_id, seller_name, buyer_tax_id, buyer_name, issue_date, currency, net_amount, total_vat, total_wht, grand_total, lines_json, raw_xml, file_name, status, used_by_label, delete_requested_at, delete_requested_by";
  const build = (from: number, to: number) =>
    supabase
      .from("pool_invoices")
      .select(poolSelect(`${COLS}, created_at`))
      .order("created_at", { ascending: false })
      .range(from, to)
      .then(({ data, error }) => {
        if (error) {
          if (cloudHasDocStatus && isMissingDocStatusError(error)) {
            cloudHasDocStatus = false;
            return supabase
              .from("pool_invoices")
              .select(`${COLS}, created_at`)
              .order("created_at", { ascending: false })
              .range(from, to)
              .then((r2: any) => {
                if (r2.error) throw r2.error;
                return r2.data;
              });
          }
          throw error;
        }
        return data;
      });
  // Rows carry full raw_xml (tens of KB each), so keep pages small to stay
  // under Supabase's response size limit; the pagination loop still returns
  // every invoice.
  return fetchAllPooled(build, 50);
}

export async function listPoolRemoteByIds(uuids: string[]): Promise<PoolInvoiceRow[]> {
  const session = await getSession();
  if (!session) throw new Error("Not authenticated");
  if (uuids.length === 0) return [];
  const PAGE = 50;
  const all: PoolInvoiceRow[] = [];
  const COLS = "id, invoice_id, uuid, seller_tax_id, seller_name, buyer_tax_id, buyer_name, issue_date, currency, net_amount, total_vat, total_wht, grand_total, lines_json, file_name, status, used_by_label, delete_requested_at, delete_requested_by, created_at";
  for (let i = 0; i < uuids.length; i += PAGE) {
    const chunk = uuids.slice(i, i + PAGE);
    const run = (cols: string) =>
    supabase
      .from("pool_invoices")
      .select(cols)
      .in("uuid", chunk)
      .order("created_at", { ascending: false });
  let res: any = await run(poolSelect(COLS));
  if (res.error && cloudHasDocStatus && isMissingDocStatusError(res.error)) {
    cloudHasDocStatus = false;
    res = await run(COLS);
  }
  if (res.error) throw res.error;
  if (res.data) all.push(...(res.data as PoolInvoiceRow[]));
  }
  return all;
}

export async function listPoolRemoteMeta(): Promise<{
  uuid: string;
  invoice_id: string;
  seller_tax_id: string;
  doc_status?: string;
  status: string;
  used_by_label: string;
  delete_requested_at: string | null;
  delete_requested_by: string | null;
}[]> {
  const session = await getSession();
  if (!session) throw new Error("Not authenticated");
  const COLS = "uuid, invoice_id, seller_tax_id, status, used_by_label, delete_requested_at, delete_requested_by";
  const build = (from: number, to: number) =>
    supabase
      .from("pool_invoices")
      .select(poolSelect(COLS))
      .order("created_at", { ascending: false })
      .range(from, to)
      .then(({ data, error }) => {
        if (error) {
          if (cloudHasDocStatus && isMissingDocStatusError(error)) {
            cloudHasDocStatus = false;
            return supabase
              .from("pool_invoices")
              .select(COLS)
              .order("created_at", { ascending: false })
              .range(from, to)
              .then((r2: any) => {
                if (r2.error) throw r2.error;
                return r2.data;
              });
          }
          throw error;
        }
        return data;
      });
  return fetchAllPooled(build, 500);
}

export async function upsertPoolInvoicesRemote(rows: PoolInvoiceRow[]): Promise<void> {
  const session = await getSession();
  if (!session) throw new Error("Not authenticated");
  if (rows.length === 0) return;
  const upserts = rows.map((r) => ({
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
    doc_status: r.doc_status || "Valid",
    status: r.status || "available",
    used_by_label: r.used_by_label || "",
  }));
  // Chunk the upload: a single request carrying every invoice (with full
  // raw_xml) can exceed Supabase's request body limit and fail wholesale,
  // which previously left the cloud copy missing whichever device had more
  // invoices. Batches of ~20 keep each request well under the limit and let
  // partial failures be retried independently.
  const PAGE = 20;
  for (let i = 0; i < upserts.length; i += PAGE) {
    const chunk = upserts.slice(i, i + PAGE);
    const { error } = await supabase
      .from("pool_invoices")
      .upsert(chunk, { onConflict: "uuid" });
    if (error && cloudHasDocStatus && isMissingDocStatusError(error)) {
      // Cloud table predates the doc_status migration; strip and retry so
      // validity stays a local-only signal until the column exists.
      cloudHasDocStatus = false;
      const stripped = chunk.map(({ doc_status: _drop, ...rest }) => rest);
      const { error: retryError } = await supabase
        .from("pool_invoices")
        .upsert(stripped, { onConflict: "uuid" });
      if (retryError) throw retryError;
      continue;
    }
    if (error) throw error;
  }
}

export async function markPoolUsedRemote(uuid: string, usedByLabel: string): Promise<void> {
  const session = await getSession();
  if (!session) throw new Error("Not authenticated");
  const { error } = await supabase
    .from("pool_invoices")
    .update({ status: "used", used_by_label: usedByLabel })
    .eq("uuid", uuid);
  if (error) throw error;
}

export async function markPoolsUsedRemote(items: { uuid: string }[], usedByLabel: string): Promise<void> {
  const session = await getSession();
  if (!session) throw new Error("Not authenticated");
  if (items.length === 0) return;
  // Update each independently: one bad row must not abort the rest, or the
  // remaining claims silently never reach the cloud (devices then disagree on
  // how many invoices are claimed).
  const errors: string[] = [];
  for (const item of items) {
    if (!item.uuid) continue;
    const { error } = await supabase
      .from("pool_invoices")
      .update({ status: "used", used_by_label: usedByLabel })
      .eq("uuid", item.uuid);
    if (error) errors.push(`${item.uuid}: ${error.message || error}`);
  }
  if (errors.length > 0) throw new Error(`${errors.length} claim update(s) failed: ${errors[0]}`);
}

export async function markPoolAvailableRemote(uuid: string): Promise<void> {
  const session = await getSession();
  if (!session) throw new Error("Not authenticated");
  const { error } = await supabase
    .from("pool_invoices")
    .update({ status: "available", used_by_label: "" })
    .eq("uuid", uuid);
  if (error) throw error;
}

export async function markPoolsAvailableRemote(items: { uuid: string }[]): Promise<void> {
  const session = await getSession();
  if (!session) throw new Error("Not authenticated");
  if (items.length === 0) return;
  const errors: string[] = [];
  for (const item of items) {
    if (!item.uuid) continue;
    const { error } = await supabase
      .from("pool_invoices")
      .update({ status: "available", used_by_label: "" })
      .eq("uuid", item.uuid);
    if (error) errors.push(`${item.uuid}: ${error.message || error}`);
  }
  if (errors.length > 0) throw new Error(`${errors.length} unclaim update(s) failed: ${errors[0]}`);
}

/**
 * Release a claim only when the cloud row is still owned by the document that
 * is being released. A stale local "available" row must never clear a claim
 * another device has made since; in that case this returns false and leaves the
 * cloud claim untouched.
 */
export async function markPoolAvailableRemoteIfOwner(uuid: string, usedByLabel: string): Promise<boolean> {
  const session = await getSession();
  if (!session) throw new Error("Not authenticated");
  if (!uuid) return false;
  const { data, error } = await supabase
    .from("pool_invoices")
    .update({ status: "available", used_by_label: "" })
    .eq("uuid", uuid)
    .eq("status", "used")
    .eq("used_by_label", usedByLabel)
    .select("uuid");
  if (error) throw error;
  return (data?.length ?? 0) > 0;
}

/**
 * Claim an invoice only when it is still available (or already claimed by the
 * same document). Returns false when another document owns the cloud row, so a
 * stale local pool can never steal a claim.
 */
export async function markPoolUsedRemoteIfFree(uuid: string, usedByLabel: string): Promise<boolean> {
  const session = await getSession();
  if (!session) throw new Error("Not authenticated");
  if (!uuid) return false;
  const available = await supabase
    .from("pool_invoices")
    .update({ status: "used", used_by_label: usedByLabel })
    .eq("uuid", uuid)
    .eq("status", "available")
    .select("uuid");
  if (available.error) throw available.error;
  if ((available.data?.length ?? 0) > 0) return true;
  const mine = await supabase
    .from("pool_invoices")
    .update({ status: "used", used_by_label: usedByLabel })
    .eq("uuid", uuid)
    .eq("used_by_label", usedByLabel)
    .select("uuid");
  if (mine.error) throw mine.error;
  if ((mine.data?.length ?? 0) > 0) return true;
  // A "used" row with no label is not a real claim (the serial links the
  // invoice to a document); it may be taken over by a valid local claim.
  const orphan = await supabase
    .from("pool_invoices")
    .update({ status: "used", used_by_label: usedByLabel })
    .eq("uuid", uuid)
    .eq("status", "used")
    .eq("used_by_label", "")
    .select("uuid");
  if (orphan.error) throw orphan.error;
  if ((orphan.data?.length ?? 0) > 0) return true;
  // Neither available, ours, nor an orphaned claim. Distinguish "not uploaded
  // yet" (allow the local claim; the next sync will insert it) from a conflict.
  const { data: row, error } = await supabase
    .from("pool_invoices")
    .select("uuid")
    .eq("uuid", uuid)
    .maybeSingle();
  if (error) throw error;
  return !row;
}

// Soft-delete marker kept in the existing delete-request columns. Other
// devices see the tombstone, remove their local copy, and never re-upload it.
export const POOL_DELETED_MARKER = "__vouchify_deleted__";

export async function markPoolInvoiceDeletedRemote(uuid: string): Promise<void> {
  const session = await getSession();
  if (!session) throw new Error("Not authenticated");
  if (!uuid) throw new Error("Missing invoice uuid");
  const { data, error } = await supabase
    .from("pool_invoices")
    .update({
      delete_requested_at: new Date().toISOString(),
      delete_requested_by: POOL_DELETED_MARKER,
    })
    .eq("uuid", uuid)
    .select("uuid");
  if (error) throw error;
  if (!data || data.length === 0) {
    throw new Error("Invoice was not found in the shared pool");
  }
}

/**
 * Re-importing a document may derive a different (synthetic) uuid than the
 * tombstone the admin deleted. Clear the tombstone by document identity too so
 * the revived invoice is not deleted again on the next sync.
 */
export async function clearPoolDeletedForDocumentRemote(invoiceId: string, sellerTaxId: string): Promise<void> {
  const session = await getSession();
  if (!session) throw new Error("Not authenticated");
  if (!invoiceId) return;
  const { error } = await supabase
    .from("pool_invoices")
    .update({ delete_requested_at: null, delete_requested_by: null })
    .eq("invoice_id", invoiceId)
    .eq("seller_tax_id", sellerTaxId || "")
    .eq("delete_requested_by", POOL_DELETED_MARKER);
  if (error) throw error;
}

export async function deletePoolInvoiceRemote(uuid: string): Promise<void> {
  const session = await getSession();
  if (!session) throw new Error("Not authenticated");
  const { error } = await supabase
    .from("pool_invoices")
    .delete()
    .eq("uuid", uuid);
  if (error) throw error;
}

export async function requestPoolDeleteRemote(uuid: string): Promise<void> {
  const session = await getSession();
  if (!session) throw new Error("Not authenticated");
  const { error } = await supabase
    .from("pool_invoices")
    .update({ delete_requested_at: new Date().toISOString(), delete_requested_by: session.user.id })
    .eq("uuid", uuid)
    // Never overwrite an existing request or an approved soft-delete tombstone.
    .is("delete_requested_at", null);
  if (error) throw error;
}

export async function rejectPoolDeleteRemote(uuid: string): Promise<void> {
  const session = await getSession();
  if (!session) throw new Error("Not authenticated");
  const { error } = await supabase
    .from("pool_invoices")
    .update({ delete_requested_at: null, delete_requested_by: null })
    .eq("uuid", uuid);
  if (error) throw error;
}

// ── WHT-free certificates (shared across auditors) ──────────────────────────
export interface WhtCertRow {
  tax_id: string;
  supplier_name: string;
  valid_until: string;
  updated_at: string;
  updated_by: string;
}

export async function listWhtCertsRemote(): Promise<WhtCertRow[]> {
  const session = await getSession();
  if (!session) throw new Error("Not authenticated");
  const { data, error } = await supabase
    .from("wht_certificates")
    .select("tax_id, supplier_name, valid_until, updated_at, updated_by")
    .order("tax_id", { ascending: true });
  if (error) throw error;
  return (data || []) as WhtCertRow[];
}

export async function upsertWhtCertRemote(row: { tax_id: string; supplier_name: string; valid_until: string }): Promise<void> {
  const session = await getSession();
  if (!session) throw new Error("Not authenticated");
  const { error } = await supabase
    .from("wht_certificates")
    .upsert(
      {
        tax_id: row.tax_id,
        supplier_name: row.supplier_name || "",
        valid_until: row.valid_until || "",
        updated_at: new Date().toISOString(),
        updated_by: session.user.id,
      },
      { onConflict: "tax_id" }
    );
  if (error) throw error;
}

// ── Manually-added suppliers (shared across devices) ────────────────────────
export interface ManualSupplierRow {
  tax_id: string;
  name: string;
  updated_at: string;
  updated_by: string;
}

export async function listManualSuppliersRemote(): Promise<ManualSupplierRow[]> {
  const session = await getSession();
  if (!session) throw new Error("Not authenticated");
  const { data, error } = await supabase
    .from("manual_suppliers")
    .select("tax_id, name, updated_at, updated_by")
    .order("tax_id", { ascending: true });
  if (error) throw error;
  return (data || []) as ManualSupplierRow[];
}

export async function upsertManualSupplierRemote(row: { tax_id: string; name: string }): Promise<void> {
  const session = await getSession();
  if (!session) throw new Error("Not authenticated");
  const { error } = await supabase
    .from("manual_suppliers")
    .upsert(
      {
        tax_id: row.tax_id,
        name: row.name || "",
        updated_at: new Date().toISOString(),
        updated_by: session.user.id,
      },
      { onConflict: "tax_id" }
    );
  if (error) throw error;
}

export async function deleteManualSupplierRemote(taxId: string): Promise<void> {
  const session = await getSession();
  if (!session) throw new Error("Not authenticated");
  const { error } = await supabase.from("manual_suppliers").delete().eq("tax_id", taxId);
  if (error) throw error;
}

// ── Daily Entries (journal documents extracted from Daily Entries PDFs) ─────
export interface DailyEntrySyncRow {
  entry_key: string;
  source_file: string;
  source_page: number;
  pages: number;
  doc_no: string;
  serial: string;
  doc_date: string;
  journal_no: string;
  journal: string;
  branch: string;
  doc_source: string;
  contain_currency: string;
  local_amt: string;
  total_value: string;
  invoice_refs: string;
  from_documents: string;
  lines_json: string;
  deleted_at?: string | null;
  updated_at: string;
}

// The `deleted_at` tombstone column was added later than the table; until the
// project runs the ALTER, queries naming it fail. Detect once and fall back so
// sync keeps working (deletions then simply can't propagate cross-device).
let cloudHasDailyDeletedAt = true;

function isMissingDeletedAtError(e: any): boolean {
  const msg = String((e && (e.message ?? e)) || "");
  return msg.includes("deleted_at");
}

/** Daily-entry columns that must exist on every row we read. */
function dailyEntryCols(): string {
  const base =
    "entry_key, source_file, source_page, pages, doc_no, serial, doc_date, journal_no, journal, branch, doc_source, contain_currency, local_amt, total_value, invoice_refs, from_documents, lines_json, report_date, updated_at";
  return cloudHasDailyDeletedAt ? `${base}, deleted_at` : base;
}

export async function upsertDailyEntriesRemote(rows: DailyEntrySyncRow[]): Promise<void> {
  const session = await getSession();
  if (!session) throw new Error("Not authenticated");
  if (rows.length === 0) return;
  const PAGE = 50;
  const map = (r: DailyEntrySyncRow) => ({
    ...r,
    // A direct push (import or an edit) resurrects the entry: clear any tombstone.
    deleted_at: cloudHasDailyDeletedAt ? null : undefined,
    user_id: session.user.id,
  });
  for (let i = 0; i < rows.length; i += PAGE) {
    const chunk = rows.slice(i, i + PAGE).map(map);
    let { error } = await supabase
      .from("daily_entries")
      .upsert(chunk, { onConflict: "entry_key" });
    if (error && cloudHasDailyDeletedAt && isMissingDeletedAtError(error)) {
      cloudHasDailyDeletedAt = false;
      const retry = chunk.map(({ deleted_at, ...rest }: any) => ({ ...rest, user_id: session.user.id }));
      ({ error } = await supabase.from("daily_entries").upsert(retry, { onConflict: "entry_key" }));
    }
    if (error) throw error;
  }
}

export async function listDailyEntriesRemote(): Promise<DailyEntrySyncRow[]> {
  const session = await getSession();
  if (!session) throw new Error("Not authenticated");
  const build = (from: number, to: number): PromiseLike<any[]> =>
    supabase
      .from("daily_entries")
      .select(dailyEntryCols())
      .order("doc_date", { ascending: true })
      .order("entry_key", { ascending: true })
      .range(from, to)
      .then(({ data, error }) => {
        if (error) throw error;
        return (data || []) as any[];
      });
  return fetchAllPooled<any>(build, 500) as Promise<DailyEntrySyncRow[]>;
}

/** Keys + timestamps only (no lines_json) so the UI can decide what to pull. */
export interface DailyEntryRemoteMeta {
  entry_key: string;
  doc_no: string;
  serial: string;
  doc_date: string;
  updated_at: string;
  deleted_at?: string | null;
}

export async function listDailyEntriesRemoteMeta(): Promise<DailyEntryRemoteMeta[]> {
  const session = await getSession();
  if (!session) throw new Error("Not authenticated");
  // `entry_key` is the stable tiebreaker: without it, pagination over rows that
  // share an updated_at timestamp can repeat or skip entries.
  const baseCols = "entry_key, doc_no, serial, doc_date, updated_at";
  const build = (from: number, to: number) => {
    const cols = cloudHasDailyDeletedAt ? `${baseCols}, deleted_at` : baseCols;
    return supabase
      .from("daily_entries")
      .select(cols)
      .order("updated_at", { ascending: true })
      .order("entry_key", { ascending: true })
      .range(from, to)
      .then(({ data, error }) => {
        if (error) {
          if (cloudHasDailyDeletedAt && isMissingDeletedAtError(error)) {
            cloudHasDailyDeletedAt = false;
            return supabase
              .from("daily_entries")
              .select(baseCols)
              .order("updated_at", { ascending: true })
              .order("entry_key", { ascending: true })
              .range(from, to)
              .then((r2: any) => {
                if (r2.error) throw r2.error;
                return r2.data;
              });
          }
          throw error;
        }
        return data;
      });
  };
  return fetchAllPooled(build, 1000) as Promise<DailyEntryRemoteMeta[]>;
}

/**
 * Cheap change token for the whole shared store: row count, newest `updated_at`
 * and the number of tombstones. If it is unchanged since the last sync there is
 * nothing to pull, so the (much larger) per-key metadata fetch can be skipped.
 */
export async function dailyEntriesSyncToken(): Promise<string> {
  const session = await getSession();
  if (!session) throw new Error("Not authenticated");
  const { count, error } = await supabase
    .from("daily_entries")
    .select("entry_key", { count: "exact", head: true });
  if (error) throw error;
  const { data, error: e2 } = await supabase
    .from("daily_entries")
    .select("updated_at")
    .order("updated_at", { ascending: false })
    .limit(1);
  if (e2) throw e2;
  let deletedCount = 0;
  if (cloudHasDailyDeletedAt) {
    const r = await supabase
      .from("daily_entries")
      .select("entry_key", { count: "exact", head: true })
      .not("deleted_at", "is", null);
    if (r.error && isMissingDeletedAtError(r.error)) cloudHasDailyDeletedAt = false;
    else if (r.error) throw r.error;
    else deletedCount = r.count ?? 0;
  }
  return `${count ?? 0}|${data?.[0]?.updated_at ?? ""}|${deletedCount}`;
}

/** Full rows for just the given keys (chunked), used to pull only what's new. */
export async function listDailyEntriesRemoteByKeys(keys: string[]): Promise<DailyEntrySyncRow[]> {
  const session = await getSession();
  if (!session) throw new Error("Not authenticated");
  if (keys.length === 0) return [];
  const all: DailyEntrySyncRow[] = [];
  const PAGE = 200;
  for (let i = 0; i < keys.length; i += PAGE) {
    const chunk = keys.slice(i, i + PAGE);
    let { data, error }: any = await supabase.from("daily_entries").select(dailyEntryCols()).in("entry_key", chunk);
    if (error && cloudHasDailyDeletedAt && isMissingDeletedAtError(error)) {
      cloudHasDailyDeletedAt = false;
      ({ data, error } = await supabase
        .from("daily_entries")
        .select(dailyEntryCols())
        .in("entry_key", chunk));
    }
    if (error) throw error;
    if (data) all.push(...(data as DailyEntrySyncRow[]));
  }
  return all;
}

/**
 * Tombstone the given entries in the cloud. Soft-delete (rather than a hard
 * delete) is what lets every other device learn the entry is gone and stops it
 * from being re-uploaded. Falls back to a hard delete if the column is absent.
 */
export async function softDeleteDailyEntriesRemoteByKeys(keys: string[]): Promise<void> {
  const session = await getSession();
  if (!session) throw new Error("Not authenticated");
  if (keys.length === 0) return;
  const PAGE = 200;
  for (let i = 0; i < keys.length; i += PAGE) {
    const chunk = keys.slice(i, i + PAGE);
    if (!cloudHasDailyDeletedAt) {
      throw new Error("Daily Entries tombstone column is missing. Run supabase/daily_entries.sql in the Supabase SQL Editor.");
    }
    const { error } = await supabase
      .from("daily_entries")
      .update({ deleted_at: new Date().toISOString() })
      .in("entry_key", chunk);
    if (error && isMissingDeletedAtError(error)) {
      cloudHasDailyDeletedAt = false;
      throw new Error("Daily Entries tombstone column is missing. Run supabase/daily_entries.sql in the Supabase SQL Editor.");
    }
    if (error) throw error;
  }
}

export async function deleteDailyEntriesRemote(sourceFile: string): Promise<void> {
  const session = await getSession();
  if (!session) throw new Error("Not authenticated");
  const { error } = await supabase.from("daily_entries").delete().eq("source_file", sourceFile);
  if (error) throw error;
}
