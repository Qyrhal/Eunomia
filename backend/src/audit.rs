//! Append-only security ledger (`audit_event`). One row per auth event and per
//! mutating tool call: who (user, token or OAuth client), what, on which target,
//! the outcome code and the request's trace id. Nothing in the app updates or
//! deletes these rows, and the store only has a CREATE statement for them.
//! Writes are best effort: a failure is logged, never surfaced to the caller.

use surrealdb::types::RecordId;
use crate::rid::RecordIdExt;

use crate::authz::Actor;
use crate::db::Db;
use crate::store;

pub struct Event<'a> {
    pub user: Option<&'a RecordId>,
    pub actor: &'a Actor,
    pub action: &'a str,
    pub target: &'a str,
    /// `ok` or the error code that ended the action.
    pub outcome: &'a str,
    pub detail: &'a str,
}

pub fn anonymous() -> Actor {
    Actor { kind: "anonymous", id: String::new() }
}

pub async fn record(db: &Db, e: Event<'_>) {
    let detail: String = e.detail.chars().take(500).collect();
    let res = store::app::AUDIT_EVENT_CREATE
        .on(db)
        .bind(("user", e.user.cloned()))
        .bind(("actor_kind", e.actor.kind.to_string()))
        .bind(("actor_id", e.actor.id.clone()))
        .bind(("action", e.action.to_string()))
        .bind(("target", e.target.to_string()))
        .bind(("outcome", e.outcome.to_string()))
        .bind(("trace_id", crate::telemetry::current_trace_id()))
        .bind(("detail", detail))
        .await;
    let failure = match res {
        Ok(r) => r.check().err(),
        Err(err) => Some(err),
    };
    if let Some(err) = failure {
        tracing::warn!(action = e.action, error = %err, "audit_event write failed");
    }
}

/// Records an event for the current request's credential (the gate's caller), or
/// a plain user actor outside a request.
pub async fn record_as_caller(db: &Db, user: &RecordId, action: &str, target: &str, outcome: &str) {
    let actor = crate::authz::current().map(|c| c.actor).unwrap_or_else(|| Actor { kind: "user", id: user.to_string() });
    record(db, Event { user: Some(user), actor: &actor, action, target, outcome, detail: "" }).await;
}
