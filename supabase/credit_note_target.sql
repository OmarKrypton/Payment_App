-- Run this in the Supabase SQL Editor once.
--
-- A credit note whose ETA XML has no `references` can be ambiguous when a
-- supplier issued two invoices with the same seller, currency and total. The
-- app lets the user pick which invoice the credit note cancels. That choice is
-- stored here so every device sees the same cancellation.

alter table public.pool_invoices
  add column if not exists credit_target_uuid text not null default '';

comment on column public.pool_invoices.credit_target_uuid is
  'ETA uuid of the invoice a reference-less credit note was manually assigned to (empty = automatic matching)';
