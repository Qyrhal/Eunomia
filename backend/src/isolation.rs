//! Test-only switches for the isolation proof (`tests/isolation.rs`, docs/architecture/tenancy.md).
//! Compiled only with the `test-support` feature, so a release binary cannot turn a filter off.
//!
//! * [`set_no_app_filters`] (or `ISOLATION_TEST_NO_APP_FILTERS=1`) rewrites every org-database
//!   statement so its owner and vault filters match everything. With them gone, only the database
//!   wall (one database per org, one database user per org) is left standing.
//! * [`mutate`] edits one named statement's text, to prove the suite goes red when a filter really
//!   is missing.

use std::borrow::Cow;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

static NO_APP_FILTERS: AtomicBool = AtomicBool::new(false);
static MUTATIONS: Mutex<Vec<(String, String, String)>> = Mutex::new(Vec::new());

/// The filter predicates the app puts in org-database statements.
const FILTERS: &[&str] = &["owner = $owner", "vault = $vault", "user = $user"];

pub fn set_no_app_filters(on: bool) {
    NO_APP_FILTERS.store(on, Ordering::SeqCst);
}

fn no_app_filters() -> bool {
    NO_APP_FILTERS.load(Ordering::Relaxed) || std::env::var("ISOLATION_TEST_NO_APP_FILTERS").is_ok_and(|v| v == "1")
}

/// In statement `stmt`, replace `from` with `to`. Cleared by [`clear_mutations`].
pub fn mutate(stmt: &str, from: &str, to: &str) {
    MUTATIONS.lock().unwrap().push((stmt.into(), from.into(), to.into()));
}

pub fn clear_mutations() {
    MUTATIONS.lock().unwrap().clear();
}

pub(crate) fn transform<'a>(name: &str, sql: &'a str, org_scope: bool) -> Cow<'a, str> {
    let mut out = Cow::Borrowed(sql);
    if org_scope && no_app_filters() {
        for f in FILTERS {
            if out.contains(f) {
                out = Cow::Owned(out.replace(f, "true"));
            }
        }
    }
    let muts = MUTATIONS.lock().unwrap();
    for (stmt, from, to) in muts.iter() {
        if stmt == name && out.contains(from.as_str()) {
            out = Cow::Owned(out.replace(from.as_str(), to));
        }
    }
    out
}
