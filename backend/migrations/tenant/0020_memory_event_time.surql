-- Event time for remembered facts (issue #75, part 2). `occurred_at` is when the fact happened, set by
-- the writer (`memory_write`), distinct from `created_at` (when it was stored). Recall's time range and
-- recency use `occurred_at ?? created_at`. Optional: existing facts hold NONE and read as before.
DEFINE FIELD OVERWRITE occurred_at ON memory TYPE option<datetime>;
-- Recall's graph arm follows `relates_to` one hop in both directions from the matched entities.
DEFINE INDEX OVERWRITE relates_to_in_idx ON relates_to FIELDS in CONCURRENTLY;
DEFINE INDEX OVERWRITE relates_to_out_idx ON relates_to FIELDS out CONCURRENTLY;
