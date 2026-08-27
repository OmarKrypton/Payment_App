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
  const { error, count } = await supabase
    .from("snapshots")
    .delete()
    .eq("id", id)
    .select("id");
  if (error) throw error;
  if (count === 0) throw new Error("Delete had no effect — row may not exist or RLS blocked it");
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

export async function listPoolRemoteMeta(): Promise<{ uuid: string; invoice_id: string; seller_tax_id: string; doc_status?: string; status: string; used_by_label: string }[]> {
  const session = await getSession();
  if (!session) throw new Error("Not authenticated");
  const COLS = "uuid, invoice_id, seller_tax_id, status, used_by_label";
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
  return fetchAllPooled(build, 2000);
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
  for (const item of items) {
    const { error } = await supabase
      .from("pool_invoices")
      .update({ status: "used", used_by_label: usedByLabel })
      .eq("uuid", item.uuid);
    if (error) throw error;
  }
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
  for (const item of items) {
    const { error } = await supabase
      .from("pool_invoices")
      .update({ status: "available", used_by_label: "" })
      .eq("uuid", item.uuid);
    if (error) throw error;
  }
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
    .eq("uuid", uuid);
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
