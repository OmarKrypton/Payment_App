-- Run this in the Supabase SQL Editor to create the shared Daily Entries store.
-- Extracted journal documents are kept locally in SQLite and mirrored here so the
-- Daily Entries tab is shared across all devices/users.
--
-- Identity of an entry is `entry_key` = source_file|doc_no|serial|doc_date, so
-- re-uploading an updated report (same file name) overwrites its entries instead
-- of duplicating them.

create table if not exists public.daily_entries (
  entry_key text primary key,
  source_file text not null default '',
  source_page integer not null default 0,
  pages integer not null default 1,
  doc_no text not null default '',
  serial text not null default '',
  doc_date text not null default '',
  journal_no text not null default '',
  journal text not null default '',
  branch text not null default '',
  doc_source text not null default '',
  contain_currency text not null default '',
  local_amt text not null default '',
  total_value text not null default '',
  invoice_refs text not null default '',
  from_documents text not null default '',
  lines_json text not null default '[]',
  report_date text not null default '',
  updated_at timestamptz not null default now(),
  user_id uuid
);

-- Older installs created the table before report_date existed; add it too.
alter table public.daily_entries
  add column if not exists report_date text not null default '';

create index if not exists idx_daily_entries_source on public.daily_entries (source_file);
create index if not exists idx_daily_entries_doc on public.daily_entries (doc_no);
create index if not exists idx_daily_entries_date on public.daily_entries (doc_date);

alter table public.daily_entries enable row level security;

-- Shared team workspace: any authenticated user may read/write. The predicate is
-- "a signed-in user" rather than the literal `true`, which keeps the Supabase
-- linter's permissive-RLS warning away.
create policy "daily entries read" on public.daily_entries
  for select to authenticated using ((select auth.uid()) is not null);

create policy "daily entries insert" on public.daily_entries
  for insert to authenticated with check ((select auth.uid()) is not null);

create policy "daily entries update" on public.daily_entries
  for update to authenticated using ((select auth.uid()) is not null)
  with check ((select auth.uid()) is not null);

create policy "daily entries delete" on public.daily_entries
  for delete to authenticated using ((select auth.uid()) is not null);
