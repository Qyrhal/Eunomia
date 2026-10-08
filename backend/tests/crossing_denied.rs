//! A statement whose text names the other database's tables is refused with `tenant.denied`, never
//! sent. Its own test binary: it bumps the process-global `NO_ORG_CONTEXT` counter on purpose, which
//! the other suites assert stays 0.

mod common;

use std::sync::atomic::Ordering;

use eunomia_backend::error::{AppError, ErrorCode};
use eunomia_backend::pool::NO_ORG_CONTEXT;
use eunomia_backend::store;

#[tokio::test]
async fn a_statement_naming_the_other_databases_tables_fails_with_tenant_denied() {
    let app = common::TestApp::new().await;
    let org = app.db().await;
    assert_eq!(NO_ORG_CONTEXT.load(Ordering::Relaxed), 0);

    // tenant handle, control table
    let err = store::dynamic(&org, "test.crossing", "SELECT * FROM session").await.expect_err("must be refused");
    assert_eq!(AppError::from(err).code, ErrorCode::TenantDenied);
    assert_eq!(NO_ORG_CONTEXT.load(Ordering::Relaxed), 1, "still counted");

    // control handle, tenant table
    let err = store::dynamic_control(app.control(), "test.crossing", "SELECT * FROM memory").await.expect_err("must be refused");
    assert_eq!(AppError::from(err).code, ErrorCode::TenantDenied);
    assert_eq!(NO_ORG_CONTEXT.load(Ordering::Relaxed), 2);

    // the refusal is up front: a write naming a control table changes nothing
    let err = store::dynamic(&org, "test.crossing", "CREATE user SET email = 'x'").await.expect_err("must be refused");
    assert_eq!(AppError::from(err).code, ErrorCode::TenantDenied);

    // own tables, and a field that happens to be named like a table, still run
    store::dynamic(&org, "test.fine", "SELECT user, vault FROM vault_member").await.unwrap().check().unwrap();
    store::dynamic_control(app.control(), "test.fine", "SELECT user FROM session").await.unwrap().check().unwrap();
    assert_eq!(NO_ORG_CONTEXT.load(Ordering::Relaxed), 3, "legitimate statements add nothing");
}
