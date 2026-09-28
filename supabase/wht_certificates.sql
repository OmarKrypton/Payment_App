-- Run this in the Supabase SQL Editor to create the shared WHT-free certificate
-- registry. WHT-free companies must renew their certificate every year, so each
-- supplier has a validity date; the Suppliers tab shows expired/expiring certs
-- and lets auditors record the renewal. Shared across all users like the pool.

create table if not exists public.wht_certificates (
  tax_id text primary key,
  supplier_name text not null default '',
  valid_until text not null default '',   -- ISO date (YYYY-MM-DD)
  updated_at timestamptz not null default now(),
  updated_by text not null default ''
);

alter table public.wht_certificates enable row level security;

-- All authenticated users can read and write the shared certificate registry.
create policy "wht certs read" on public.wht_certificates
  for select to authenticated using (true);

create policy "wht certs insert" on public.wht_certificates
  for insert to authenticated with check (true);

create policy "wht certs update" on public.wht_certificates
  for update to authenticated using (true);

create policy "wht certs delete" on public.wht_certificates
  for delete to authenticated using (true);
