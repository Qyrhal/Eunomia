-- Redacted failure capsules (foundation-plan 3.7): what an agent needs to replay a failed
-- request or tool call. Pruned by the `prune_capsules` job.
DEFINE TABLE OVERWRITE failure_capsule SCHEMAFULL;
DEFINE FIELD OVERWRITE trace_id ON failure_capsule TYPE string;
DEFINE FIELD OVERWRITE kind ON failure_capsule TYPE string ASSERT $value IN ["tool", "route"];
DEFINE FIELD OVERWRITE name ON failure_capsule TYPE string;
DEFINE FIELD OVERWRITE user ON failure_capsule TYPE option<string>;
DEFINE FIELD OVERWRITE args ON failure_capsule TYPE string;
DEFINE FIELD OVERWRITE code ON failure_capsule TYPE string;
DEFINE FIELD OVERWRITE status ON failure_capsule TYPE int;
DEFINE FIELD OVERWRITE source ON failure_capsule TYPE string;
DEFINE FIELD OVERWRITE version ON failure_capsule TYPE string;
DEFINE FIELD OVERWRITE truncated ON failure_capsule TYPE bool DEFAULT false;
DEFINE FIELD OVERWRITE created_at ON failure_capsule TYPE datetime DEFAULT time::now();
DEFINE INDEX OVERWRITE failure_capsule_created_idx ON failure_capsule FIELDS created_at;
