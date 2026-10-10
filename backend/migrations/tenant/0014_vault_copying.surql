-- A vault being built by a clone or merge (vaults::service) is "copying" until the copy is done; then
-- the field is removed in the same transaction that adds its owner. Until then it has no members, so
-- no read path sees it. A failed copy deletes it; one a crash left behind is deleted by the scheduler.
DEFINE FIELD OVERWRITE status ON vault TYPE option<string> ASSERT $value IN ["copying"];
