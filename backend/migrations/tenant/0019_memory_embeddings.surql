-- Semantic recall over remembered facts (issue #75, part 1). Each fact keeps its vector, the
-- provider and model that made it (`embedding_spec`, see `EmbedSpec::key`) and when it was made.
-- Recall compares a fact only under the caller's current spec, and only when it was not edited after
-- `embedded_at`. All optional: existing facts hold NONE until the embed job backfills them.
DEFINE FIELD OVERWRITE embedding ON memory TYPE option<array<float>>;
DEFINE FIELD OVERWRITE embedding_spec ON memory TYPE option<string>;
DEFINE FIELD OVERWRITE embedded_at ON memory TYPE option<datetime>;
