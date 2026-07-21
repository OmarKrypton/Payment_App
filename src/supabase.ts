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

export async function listSnapshotsRemote(search?: string): Promise<SnapshotRow[]> {
  const session = await getSession();
  if (!session) throw new Error("Not authenticated");
  let query = supabase
    .from("snapshots")
    .select("id, label, notes, data_json, created_at, user_id")
    .order("created_at", { ascending: false });
  if (search) {
    query = query.ilike("label", `%${search}%`);
  }
  const { data, error } = await query;
  if (error) throw error;
  return data ?? [];
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
  const { error } = await supabase
    .from("snapshots")
    .delete()
    .eq("id", id);
  if (error) throw error;
}
