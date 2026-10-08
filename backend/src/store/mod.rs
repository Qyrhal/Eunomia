//! Every SurrealQL statement the app runs lives here as a named constant.
//! Request code calls `STMT.on(&db).bind(..).await`; the name tags the
//! query span and a `-- op:<name> trace:<id>` comment, so SurrealDB's slow
//! query log joins the request trace. `all()` feeds the
//! `every_query_executes` test, which runs each statement against a freshly
//! migrated database.

use std::future::{Future, IntoFuture};
use std::marker::PhantomData;
use std::pin::Pin;
use std::time::Instant;

use surrealdb::engine::any::Any;
use surrealdb::IndexedResults;
use surrealdb::method::{IntoVariables, Query};
use surrealdb::types::{RecordId, SurrealValue};
use tracing::Instrument;

use crate::db::Db;
use crate::pool::{ControlDb, OrgDb};

pub mod app;
pub mod capsules;
pub mod cache;
pub mod control;
pub mod entities;
pub mod jobs;
pub mod tenant;
pub mod vaults;

/// Proof of being inside `store/`: its field is private to this module and its children, so only
/// they can build one, and `ControlDb::raw` / `OrgDb::raw` take it.
pub(crate) struct RawKey(());

/// Marker: the statement runs in an org's database (`Stmt::on` takes an `&OrgDb`).
pub struct Tenant;
/// Marker: the statement runs in the control database (`Stmt::on` takes a `&ControlDb`).
pub struct Control;

pub struct Stmt<S = Tenant> {
    pub name: &'static str,
    pub sql: &'static str,
    /// Index of the statement whose rows the caller wants (`res.take(stmt.slot)`). SurrealDB 3.x
    /// gives every statement a result slot, BEGIN, LET and COMMIT included, so a transaction's
    /// `RETURN` is not slot 0.
    pub slot: usize,
    scope: PhantomData<fn() -> S>,
}

/// A statement for the control database.
pub type ControlStmt = Stmt<Control>;

impl<S> Stmt<S> {
    pub const fn new(name: &'static str, sql: &'static str) -> Self {
        Stmt { name, sql, slot: 0, scope: PhantomData }
    }

    pub const fn at(name: &'static str, sql: &'static str, slot: usize) -> Self {
        Stmt { name, sql, slot, scope: PhantomData }
    }
}

impl Stmt<Tenant> {
    pub fn on<'a>(&self, db: &'a OrgDb) -> Q<'a> {
        Q::new(db.raw(RawKey(())), Scope::Org(db.org()), self.name, self.sql)
    }
}

impl Stmt<Control> {
    pub fn on<'a>(&self, db: &'a ControlDb) -> Q<'a> {
        Q::new(db.raw(RawKey(())), Scope::Control, self.name, self.sql)
    }
}

/// For the few statements whose text is built at runtime (a table name or
/// an optional clause). Still named, so they still trace.
// ponytail: not covered by every_query_executes; keep these rare.
pub fn dynamic<'a>(db: &'a OrgDb, name: &'static str, sql: impl AsRef<str>) -> Q<'a> {
    Q::new(db.raw(RawKey(())), Scope::Org(db.org()), name, sql.as_ref())
}

/// [`dynamic`] for the control database.
pub fn dynamic_control<'a>(db: &'a ControlDb, name: &'static str, sql: impl AsRef<str>) -> Q<'a> {
    Q::new(db.raw(RawKey(())), Scope::Control, name, sql.as_ref())
}

/// A statement on a root session (provisioning) or the migration runner: either table set, no
/// scope check, no isolation transforms.
pub(crate) fn root<'a>(db: &'a Db, name: &'static str, sql: impl AsRef<str>) -> Q<'a> {
    Q::new(db, Scope::Root, name, sql.as_ref())
}

/// One record by id, if it exists (the replacement for the SDK's `select(id)`).
pub async fn get<T: SurrealValue>(db: &OrgDb, id: &RecordId) -> surrealdb::Result<Option<T>> {
    GET.on(db).bind(("id", id.clone())).await?.take(0)
}

/// [`get`] in the control database.
pub async fn get_control<T: SurrealValue>(db: &ControlDb, id: &RecordId) -> surrealdb::Result<Option<T>> {
    CONTROL_GET.on(db).bind(("id", id.clone())).await?.take(0)
}

const GET: Stmt = Stmt::new("store.get", "SELECT * FROM ONLY $id");
const CONTROL_GET: ControlStmt = Stmt::new("store.control_get", "SELECT * FROM ONLY $id");

#[derive(Clone, Copy)]
pub(crate) enum Scope {
    Org(crate::pool::OrgId),
    Control,
    Root,
}

pub struct Q<'a> {
    name: &'static str,
    inner: Query<'a, Any>,
    scope: Scope,
}

impl<'a> Q<'a> {
    fn new(db: &'a Db, scope: Scope, name: &'static str, sql: &str) -> Self {
        let trace = crate::telemetry::current_trace_id();
        #[cfg(feature = "test-support")]
        let transformed = crate::isolation::transform(name, sql, matches!(scope, Scope::Org(_)));
        #[cfg(feature = "test-support")]
        let sql = &*transformed;
        let crossing = match scope {
            Scope::Org(_) => crossing_table(sql, true),
            Scope::Control => crossing_table(sql, false),
            Scope::Root => None,
        };
        if let Some(table) = crossing {
            crate::pool::no_org_context(&format!("{name} reaches for {table} in the wrong database"));
        }
        Q { name, inner: db.query(format!("-- op:{name} trace:{trace}\n{sql}")), scope }
    }

    pub fn bind(mut self, bindings: impl IntoVariables) -> Self {
        self.inner = self.inner.bind(bindings);
        self
    }
}

impl<'a> IntoFuture for Q<'a> {
    type Output = surrealdb::Result<IndexedResults>;
    type IntoFuture = Pin<Box<dyn Future<Output = Self::Output> + Send + 'a>>;

    fn into_future(self) -> Self::IntoFuture {
        let org = match self.scope {
            Scope::Org(o) => o.key(),
            _ => String::new(),
        };
        let span = tracing::info_span!("store.query", op = self.name, org = %org, duration_ms = tracing::field::Empty, ok = tracing::field::Empty);
        Box::pin(
            async move {
                let started = Instant::now();
                let res = self.inner.await.and_then(surface_root_cause);
                let span = tracing::Span::current();
                span.record("duration_ms", started.elapsed().as_millis() as u64);
                span.record("ok", res.is_ok());
                res
            }
            .instrument(span),
        )
    }
}

/// When a transaction fails to commit, SurrealDB marks every statement in it "not executed due to a
/// failed transaction" and puts the real error (a commit conflict, a unique violation) on a later
/// one. `IndexedResults::check` returns the first, so `tx::is_conflict` never saw the conflict and a
/// lost race surfaced as a 500 instead of a retry. Return the real error from the await itself.
/// Any statement error becomes the `Err` of the await (the same error `.check()` would have given,
/// minus the shadowing), so a batch with a failed statement is never half-read.
fn surface_root_cause(mut res: surrealdb::IndexedResults) -> surrealdb::Result<surrealdb::IndexedResults> {
    let mut errors: Vec<(usize, surrealdb::Error)> = res.take_errors().into_iter().collect();
    if errors.is_empty() {
        return Ok(res);
    }
    errors.sort_by_key(|(i, _)| *i);
    let shadow = |e: &surrealdb::Error| e.to_string().contains("not executed due to a failed transaction");
    let pick = errors
        .iter()
        .position(|(_, e)| crate::tx::is_conflict(e))
        .or_else(|| errors.iter().position(|(_, e)| !shadow(e)))
        .unwrap_or(0);
    Err(errors.swap_remove(pick).1)
}

/// Tables that live in the control database. Everything else a statement names is an org table.
pub const CONTROL_TABLES: &[&str] = &[
    "user", "org", "membership", "tenant", "api_token", "session", "oauth_client", "oauth_code", "oauth_grant", "oauth_token", "audit_event", "job",
    "job_leader", "failure_capsule",
];

/// Tables that live in an org database.
pub const TENANT_TABLES: &[&str] = &[
    "app_settings", "vault", "vault_member", "connector", "sync_status", "cache_record", "linked_to", "person", "organisation", "location", "repository",
    "file", "symbol", "memory", "relates_to", "chat_thread", "chat_message", "audit_log", "embed_cache",
];

/// The first table `sql` reads or writes that lives in the other database. Looks only at the word
/// after FROM, INTO, UPDATE, DELETE, CREATE, UPSERT, TABLE and RELATE's edge, so field names that
/// happen to equal a table name (`user`, `vault`) do not count.
// ponytail: a word scanner, not a parser; a table named only inside a subquery after other keywords is missed.
pub fn crossing_table(sql: &str, in_org: bool) -> Option<String> {
    let foreign = if in_org { CONTROL_TABLES } else { TENANT_TABLES };
    let words: Vec<&str> = sql.split(|c: char| c.is_whitespace() || c == ';' || c == '(' || c == ')').filter(|w| !w.is_empty()).collect();
    let mut hits = words.windows(2).filter_map(|w| {
        let kw = w[0].to_ascii_uppercase();
        if !matches!(kw.as_str(), "FROM" | "INTO" | "UPDATE" | "DELETE" | "CREATE" | "UPSERT" | "TABLE" | "ONLY") {
            return None;
        }
        let table = w[1].split(':').next().unwrap_or("").trim_matches('`');
        foreign.contains(&table).then(|| table.to_string())
    });
    let edge = words.iter().filter_map(|w| w.split("->").nth(1)).find(|t| foreign.contains(t)).map(String::from);
    hits.next().or(edge)
}

/// Every static tenant statement, for the `every_query_executes` test.
pub fn all() -> Vec<&'static Stmt> {
    [app::ALL, cache::ALL, entities::ALL, jobs::ALL, vaults::ALL].concat()
}

/// Every static control statement.
pub fn all_control() -> Vec<&'static ControlStmt> {
    [control::ALL, tenant::ALL, capsules::ALL, jobs::CONTROL_ALL].concat()
}
