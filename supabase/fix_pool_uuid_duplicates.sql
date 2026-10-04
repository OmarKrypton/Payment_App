-- Guarantee every pool_invoices row has a real, non-empty uuid, and collapse
-- the synthetic duplicates that accumulated when a file WITHOUT an embedded
-- uuid was re-imported (each import used to insert a new row, while the app now
-- derives a deterministic uuid).
--
-- Run this in the Supabase SQL Editor. It is safe to run more than once.
--
-- A "synthetic" uuid is one the app did not read from the ETA document:
--   ''            – never assigned
--   'legacy-<id>' – backfilled by the old v0.3.30 migration
-- The app now derives 'GEN:' || invoice_id || ':' || seller_tax_id for these, so
-- this script produces exactly the same value — local and cloud then converge.

-- ── 0) Look before you leap (optional): what would change? ──────────────────
-- select
--   count(*) filter (where uuid is null or uuid = '' or uuid like 'legacy-%') as synthetic_rows,
--   count(*) filter (where uuid is null or uuid = '')                         as empty_rows,
--   count(*)                                                                  as total_rows
-- from public.pool_invoices;

begin;

-- 1) Same document already has a real ETA uuid -> the synthetic copy is a
--    duplicate. Delete it. (Revisions keep their own real uuids and are left
--    untouched.)
delete from public.pool_invoices s
where (s.uuid is null or s.uuid = '' or s.uuid like 'legacy-%')
  and s.invoice_id <> ''
  and exists (
    select 1 from public.pool_invoices r
    where r.id <> s.id
      and r.invoice_id = s.invoice_id
      and r.seller_tax_id = s.seller_tax_id
      and r.uuid <> '' and r.uuid not like 'legacy-%' and r.uuid not like 'GEN:%'
  );

-- 2) Remaining synthetic rows with the same document: keep one (prefer a claimed
--    row, then one with raw_xml, then the lowest id), delete the rest.
with synth as (
  select id,
         row_number() over (
           partition by invoice_id, seller_tax_id
           order by (status = 'used') desc, (raw_xml <> '') desc, id asc
         ) as rn
  from public.pool_invoices
  where (uuid is null or uuid = '' or uuid like 'legacy-%')
    and invoice_id <> ''
)
delete from public.pool_invoices p
using synth s
where p.id = s.id and s.rn > 1;

-- 3) Give the surviving synthetic rows the same deterministic uuid the app uses.
update public.pool_invoices p
set uuid = 'GEN:' || p.invoice_id || ':' || p.seller_tax_id
where (p.uuid is null or p.uuid = '' or p.uuid like 'legacy-%')
  and p.invoice_id <> ''
  and not exists (
    select 1 from public.pool_invoices x
    where x.id <> p.id
      and x.uuid = 'GEN:' || p.invoice_id || ':' || p.seller_tax_id
  );

-- 4) Any synthetic row left is a duplicate of an already-derived GEN row.
delete from public.pool_invoices p
where (p.uuid is null or p.uuid = '' or p.uuid like 'legacy-%')
  and exists (
    select 1 from public.pool_invoices x
    where x.id <> p.id
      and x.uuid = 'GEN:' || p.invoice_id || ':' || p.seller_tax_id
  );

-- 5) Last resort for rows with no invoice_id at all: still never empty.
update public.pool_invoices
set uuid = case when file_name <> '' then 'GEN:file:' || file_name else 'GEN:row:' || id end
where (uuid is null or uuid = '' or uuid like 'legacy-%')
  and invoice_id = '';

-- 6) The app uploads with ON CONFLICT (uuid), which PostgREST can only resolve
--    against a NON-partial unique index. If an earlier setup skipped the v0.3.30
--    migration (or created a partial index), every upload is rejected with
--    "there is no unique or exclusion constraint matching the ON CONFLICT
--    specification" and the cloud never grows — which is exactly how the portal
--    ends up far behind the app. Collapse any rows that still share a uuid, then
--    create the index the app needs.
with dups as (
  select id,
         row_number() over (
           partition by uuid
           order by (status = 'used') desc, (raw_xml <> '') desc, id asc
         ) as rn
  from public.pool_invoices
  where uuid <> ''
)
delete from public.pool_invoices p
using dups d
where p.id = d.id and d.rn > 1;

create unique index if not exists idx_pool_invoices_uuid on public.pool_invoices (uuid);

commit;

-- Verify: this must return 0.
-- select count(*) as still_empty_or_legacy
-- from public.pool_invoices
-- where uuid is null or uuid = '' or uuid like 'legacy-%';
