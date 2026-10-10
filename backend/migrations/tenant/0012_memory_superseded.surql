-- A raw fact a later fact shows is no longer true is marked "superseded" (entities::supersede):
-- recall leaves it out, entities_get still shows it. Observations keep "fresh"/"stale".
DEFINE FIELD OVERWRITE status ON memory TYPE option<string> ASSERT $value IN ["fresh","stale","superseded"];
