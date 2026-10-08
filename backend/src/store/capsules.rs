//! Statements for failure capsules (docs/debugging.md). See the module docs in store/mod.rs.

use super::ControlStmt;

pub const ALL: &[&ControlStmt] = &[&INSERT, &GET, &PRUNE_OLD, &PRUNE_EXCESS, &FIRST_USER];

/// One row per failure: the id is the trace id plus a counter, so a batch with several failures keeps all of them.
pub const INSERT: ControlStmt = ControlStmt::new(
    "capsules.insert",
    "INSERT IGNORE INTO failure_capsule { id: $id, org: $org, trace_id: $trace_id, kind: $kind, name: $name, user: $user, \
     args: $args, code: $code, status: $status, source: $source, version: $version, truncated: $truncated } RETURN VALUE id",
);

pub const GET: ControlStmt = ControlStmt::new(
    "capsules.get",
    "SELECT org, trace_id, kind, name, user, args, code, status, source, version, truncated, <string> created_at AS created_at \
     FROM failure_capsule WHERE trace_id = $trace_id ORDER BY created_at ASC",
);

pub const PRUNE_OLD: ControlStmt =
    ControlStmt::new("capsules.prune_old", "DELETE failure_capsule WHERE created_at < time::now() - <duration>$age RETURN BEFORE");

/// Keeps the `$max` newest rows.
pub const PRUNE_EXCESS: ControlStmt = ControlStmt::new(
    "capsules.prune_excess",
    "LET $keep = (SELECT id, created_at FROM failure_capsule ORDER BY created_at DESC LIMIT $max); \
     DELETE failure_capsule WHERE id NOT IN $keep.id RETURN BEFORE;",
);

/// The instance admin: whoever signed up first.
pub const FIRST_USER: ControlStmt = ControlStmt::new("capsules.first_user", "SELECT id, created_at FROM user ORDER BY created_at ASC LIMIT 1");
