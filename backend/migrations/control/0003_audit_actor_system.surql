-- Work with no request behind it (jobs, replay) acts as `system`.
DEFINE FIELD OVERWRITE actor_kind ON audit_event TYPE string ASSERT $value IN ["user", "token", "oauth", "anonymous", "system"];
