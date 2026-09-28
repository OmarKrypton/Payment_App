-- Run this in the Supabase SQL Editor to create the shared directory of
-- manually-added suppliers. Keeps "Add Supplier" entries in sync across all
-- devices/users (the supplier list itself is still derived from documents and
-- the invoice pool; this table only holds suppliers added by hand).

create table if not exists public.manual_suppliers (
  tax_id text primary key,
  name text not null default '',
  updated_at timestamptz not null default now(),
  updated_by text not null default ''
);

alter table public.manual_suppliers enable row level security;

-- All authenticated users can read and write the shared supplier directory.
create policy "manual suppliers read" on public.manual_suppliers
  for select to authenticated using (true);

create policy "manual suppliers insert" on public.manual_suppliers
  for insert to authenticated with check (true);

create policy "manual suppliers update" on public.manual_suppliers
  for update to authenticated using (true);

create policy "manual suppliers delete" on public.manual_suppliers
  for delete to authenticated using (true);
