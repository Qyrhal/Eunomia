//! Every SurrealQL statement the app runs lives here as a named constant.
//! Request code calls `STMT.on(&db).bind(..).await`; the name tags the
//! query span and a `-- op:<name> trace:<id>` comment, so SurrealDB's slow
//! query log joins the request trace. `all()` feeds the
//! `every_query_executes` test, which runs each statement against a freshly
//! migrated database.

use std::future::{Future, IntoFuture};
use std::pin::Pin;
use std::time::Instant;

use serde::Serialize;
use surrealdb::engine::any::Any;
use surrealdb::method::Query;
use tracing::Instrument;

use crate::db::Db;

pub mod app;
pub mod cache;
pub mod entities;
pub mod vaults;

pub struct Stmt {
    pub name: &'static str,
    pub sql: &'static str,
}

impl Stmt {
    pub const fn new(name: &'static str, sql: &'static str) -> Self {
        Stmt { name, sql }
    }

    pub fn on<'a>(&self, db: &'a Db) -> Q<'a> {
        Q::new(db, self.name, self.sql)
    }
}

/// For the few statements whose text is built at runtime (a table name or
/// an optional clause). Still named, so they still trace.
// ponytail: not covered by every_query_executes; keep these rare.
pub fn dynamic<'a>(db: &'a Db, name: &'static str, sql: impl AsRef<str>) -> Q<'a> {
    Q::new(db, name, sql.as_ref())
}

pub struct Q<'a> {
    name: &'static str,
    inner: Query<'a, Any>,
}

impl<'a> Q<'a> {
    fn new(db: &'a Db, name: &'static str, sql: &str) -> Self {
        let trace = crate::telemetry::current_trace_id();
        Q { name, inner: db.query(format!("-- op:{name} trace:{trace}\n{sql}")) }
    }

    pub fn bind(mut self, bindings: impl Serialize + 'static) -> Self {
        self.inner = self.inner.bind(bindings);
        self
    }
}

impl<'a> IntoFuture for Q<'a> {
    type Output = surrealdb::Result<surrealdb::Response>;
    type IntoFuture = Pin<Box<dyn Future<Output = Self::Output> + Send + 'a>>;

    fn into_future(self) -> Self::IntoFuture {
        let span = tracing::info_span!("store.query", op = self.name, duration_ms = tracing::field::Empty, ok = tracing::field::Empty);
        Box::pin(
            async move {
                let started = Instant::now();
                let res = self.inner.await;
                let span = tracing::Span::current();
                span.record("duration_ms", started.elapsed().as_millis() as u64);
                span.record("ok", res.is_ok());
                res
            }
            .instrument(span),
        )
    }
}

/// Every static statement, for the `every_query_executes` test.
pub fn all() -> Vec<&'static Stmt> {
    [app::ALL, cache::ALL, entities::ALL, vaults::ALL].concat()
}
