-- Accounts, credentials, OAuth, the job queue and failure capsules moved to the control database
-- (migrations/control). An org database holds only org data. Dropping them here (they are empty in a
-- database provisioned for an org) makes a stray query against one fail loudly under STRICT.
-- Never apply this to the legacy single database: that is the data the self-host move reads.
REMOVE TABLE IF EXISTS user;
REMOVE TABLE IF EXISTS api_token;
REMOVE TABLE IF EXISTS session;
REMOVE TABLE IF EXISTS audit_event;
REMOVE TABLE IF EXISTS oauth_client;
REMOVE TABLE IF EXISTS oauth_code;
REMOVE TABLE IF EXISTS oauth_grant;
REMOVE TABLE IF EXISTS oauth_token;
REMOVE TABLE IF EXISTS job;
REMOVE TABLE IF EXISTS job_leader;
REMOVE TABLE IF EXISTS failure_capsule;
