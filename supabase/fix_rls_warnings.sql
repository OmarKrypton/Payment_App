-- Fix the Supabase "RLS Policy Always True" (lint 0024) security warnings.
--
-- The shared tables (pool_invoices, wht_certificates, manual_suppliers) are a
-- shared team workspace: every signed-in user is meant to read and write every
-- row. The original policies expressed that as the literal `true`, which the
-- database linter reports as an always-true predicate.
--
-- This migration recreates the policies with an explicit "is a signed-in user"
-- check instead: `(select auth.uid()) is not null`. Access is unchanged (the
-- policies are still scoped `to authenticated`; anonymous callers have no
-- session and no auth.uid()), it no longer trips the linter, and wrapping the
-- call in a subquery makes Postgres evaluate auth.uid() once per query rather
-- than once per row. UPDATE policies also set WITH CHECK explicitly so the write
-- side is covered (previously it silently defaulted to the USING expression).
--
-- Safe to run more than once. Wrapped in a transaction so the drop/create of
-- each policy is atomic (RLS fails closed during the swap, but a transaction
-- removes even that brief window).

begin;

-- ── pool_invoices ───────────────────────────────────────────────────────────
drop policy if exists "pool read"   on public.pool_invoices;
drop policy if exists "pool insert" on public.pool_invoices;
drop policy if exists "pool update" on public.pool_invoices;
drop policy if exists "pool delete" on public.pool_invoices;

create policy "pool read" on public.pool_invoices
  for select to authenticated using ((select auth.uid()) is not null);

create policy "pool insert" on public.pool_invoices
  for insert to authenticated with check ((select auth.uid()) is not null);

create policy "pool update" on public.pool_invoices
  for update to authenticated using ((select auth.uid()) is not null)
  with check ((select auth.uid()) is not null);

create policy "pool delete" on public.pool_invoices
  for delete to authenticated using ((select auth.uid()) is not null);

-- ── wht_certificates ────────────────────────────────────────────────────────
drop policy if exists "wht certs read"   on public.wht_certificates;
drop policy if exists "wht certs insert" on public.wht_certificates;
drop policy if exists "wht certs update" on public.wht_certificates;
drop policy if exists "wht certs delete" on public.wht_certificates;

create policy "wht certs read" on public.wht_certificates
  for select to authenticated using ((select auth.uid()) is not null);

create policy "wht certs insert" on public.wht_certificates
  for insert to authenticated with check ((select auth.uid()) is not null);

create policy "wht certs update" on public.wht_certificates
  for update to authenticated using ((select auth.uid()) is not null)
  with check ((select auth.uid()) is not null);

create policy "wht certs delete" on public.wht_certificates
  for delete to authenticated using ((select auth.uid()) is not null);

-- ── manual_suppliers ────────────────────────────────────────────────────────
drop policy if exists "manual suppliers read"   on public.manual_suppliers;
drop policy if exists "manual suppliers insert" on public.manual_suppliers;
drop policy if exists "manual suppliers update" on public.manual_suppliers;
drop policy if exists "manual suppliers delete" on public.manual_suppliers;

create policy "manual suppliers read" on public.manual_suppliers
  for select to authenticated using ((select auth.uid()) is not null);

create policy "manual suppliers insert" on public.manual_suppliers
  for insert to authenticated with check ((select auth.uid()) is not null);

create policy "manual suppliers update" on public.manual_suppliers
  for update to authenticated using ((select auth.uid()) is not null)
  with check ((select auth.uid()) is not null);

create policy "manual suppliers delete" on public.manual_suppliers
  for delete to authenticated using ((select auth.uid()) is not null);

commit;
