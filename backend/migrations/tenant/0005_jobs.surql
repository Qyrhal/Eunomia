-- Background jobs (foundation-plan 3.6). Lives in the tenant database for now;
-- `org` is NONE until the control database exists, so the move is mechanical.
DEFINE TABLE OVERWRITE job SCHEMAFULL;
DEFINE FIELD OVERWRITE org ON job TYPE option<string>;
DEFINE FIELD OVERWRITE kind ON job TYPE string;
DEFINE FIELD OVERWRITE owner ON job TYPE record<user>;
DEFINE FIELD OVERWRITE payload ON job TYPE object FLEXIBLE DEFAULT {};
DEFINE FIELD OVERWRITE idempotency_key ON job TYPE string;
DEFINE FIELD OVERWRITE run_at ON job TYPE datetime DEFAULT time::now();
DEFINE FIELD OVERWRITE attempts ON job TYPE int DEFAULT 0;
DEFINE FIELD OVERWRITE max_attempts ON job TYPE int DEFAULT 5;
DEFINE FIELD OVERWRITE locked_by ON job TYPE option<string>;
DEFINE FIELD OVERWRITE locked_until ON job TYPE option<datetime>;
DEFINE FIELD OVERWRITE status ON job TYPE string DEFAULT "ready" ASSERT $value IN ["ready", "running", "done", "dead"];
DEFINE FIELD OVERWRITE last_error_code ON job TYPE option<string>;
DEFINE FIELD OVERWRITE traceparent ON job TYPE option<string>;
DEFINE FIELD OVERWRITE created_at ON job TYPE datetime DEFAULT time::now();
DEFINE FIELD OVERWRITE updated_at ON job TYPE datetime DEFAULT time::now();
DEFINE INDEX OVERWRITE job_idempotency_unique ON job FIELDS idempotency_key UNIQUE;
DEFINE INDEX OVERWRITE job_ready_idx ON job FIELDS status, run_at;
DEFINE INDEX OVERWRITE job_lease_idx ON job FIELDS status, locked_until;

-- One row, id `scheduler`: whoever holds an unexpired lease runs the periodic reconcilers.
DEFINE TABLE OVERWRITE job_leader SCHEMAFULL;
DEFINE FIELD OVERWRITE holder ON job_leader TYPE option<string>;
DEFINE FIELD OVERWRITE until ON job_leader TYPE datetime;
UPSERT job_leader:scheduler SET holder = NONE, until = d"1970-01-01T00:00:00Z";

-- Serves the stale-observation reconciler.
DEFINE INDEX OVERWRITE memory_type_status_idx ON memory FIELDS type, status;
