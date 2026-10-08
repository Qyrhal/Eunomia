//! The store seam's two gates: every named statement parses and runs against
//! a freshly migrated database, and no code outside `src/store/` builds
//! queries (files still being moved are listed in store_gate_allow.txt).

mod common;

use std::collections::BTreeSet;

use regex::Regex;

// Bound by SurrealDB itself, never by us.
const BUILTIN_PARAMS: &[&str] = &["value", "this", "parent", "before", "after", "auth", "session", "token", "input", "event"];

#[tokio::test]
async fn every_query_executes() {
    let state = common::bare_state().await;
    let param = Regex::new(r"\$([A-Za-z_][A-Za-z0-9_]*)").unwrap();
    let mut broken = Vec::new();
    for stmt in eunomia_backend::store::all() {
        let names: BTreeSet<&str> = param
            .captures_iter(stmt.sql)
            .map(|c| c.get(1).unwrap().as_str())
            .filter(|n| !BUILTIN_PARAMS.contains(n))
            .collect();
        let mut q = stmt.on(&state.db);
        for n in names {
            q = q.bind((n.to_string(), surrealdb::types::Value::None));
        }
        // NONE params make many statements fail at runtime (type checks);
        // that is fine. What must never happen is a statement that does not
        // parse or names a function SurrealDB does not have.
        let text = match q.await {
            Err(e) => e.to_string(),
            Ok(mut res) => res.take_errors().into_values().map(|e| e.to_string()).collect::<Vec<_>>().join("; "),
        };
        let lower = text.to_lowercase();
        // SurrealDB 3.x also reports these at run time: a missing table (every table exists after
        // migrating), a field the schema lacks, a function the server's --allow-funcs denies.
        if ["parse error", "invalid function", "unknown function", "failed to parse", "does not exist", "no such field", "not allowed"]
            .iter()
            .any(|bad| lower.contains(bad))
        {
            broken.push(format!("{}: {}", stmt.name, text));
        }
    }
    assert!(broken.is_empty(), "statements that do not parse:\n{}", broken.join("\n"));
}

/// The words `every_query_executes` greps for must match what this SurrealDB version really says.
#[tokio::test]
async fn gate_vocabulary_matches_the_server() {
    let state = common::bare_state().await;
    let say = |sql: &'static str| {
        let db = state.db.clone();
        async move {
            match db.query(sql).await {
                Err(e) => e.to_string().to_lowercase(),
                Ok(mut r) => r.take_errors().into_values().map(|e| e.to_string()).collect::<Vec<_>>().join("; ").to_lowercase(),
            }
        }
    };
    assert!(say("SELECT * FROM no_such_table").await.contains("does not exist"));
    assert!(say("SELECT FROM WHERE").await.contains("parse error"));
    assert!(say("RETURN no::such_fn()").await.contains("invalid function"));
    let extra = say("CREATE user SET email = 'a', password_hash = 'x', bogus = 1").await;
    assert!(extra.contains("no such field"), "{extra}");
}

#[test]
fn store_names_are_unique() {
    let mut seen = BTreeSet::new();
    for s in eunomia_backend::store::all() {
        assert!(seen.insert(s.name), "duplicate statement name {}", s.name);
    }
}

#[test]
fn no_queries_outside_store() {
    let allow: BTreeSet<String> = include_str!("store_gate_allow.txt").lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect();
    let mut offenders = Vec::new();
    for entry in walk("src") {
        if entry.starts_with("src/store/") || allow.contains(&entry) {
            continue;
        }
        let text = std::fs::read_to_string(&entry).unwrap();
        if text.contains(".query(") {
            offenders.push(entry);
        }
    }
    assert!(offenders.is_empty(), "build queries through src/store/ (Stmt::on or store::dynamic): {offenders:?}");
}

fn walk(dir: &str) -> Vec<String> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        let s = p.to_string_lossy().replace('\\', "/");
        if p.is_dir() {
            out.extend(walk(&s));
        } else if s.ends_with(".rs") {
            out.push(s);
        }
    }
    out
}

/// A real optimistic-commit conflict must be recognised by `tx::is_conflict` (the retry loop and
/// the DbConflict error code hang on it); 3.x changed the error from an enum variant to a kind.
#[tokio::test]
async fn real_commit_conflict_is_detected() {
    let state = common::bare_state().await;
    let db = &state.db;
    db.query("DEFINE TABLE ctr SCHEMALESS; CREATE ctr:one SET n = 0;").await.unwrap().check().unwrap();
    let a = db.clone().begin().await.unwrap();
    let b = db.clone().begin().await.unwrap();
    a.query("UPDATE ctr:one SET n = n + 1").await.unwrap().check().unwrap();
    b.query("UPDATE ctr:one SET n = n + 1").await.unwrap().check().unwrap();
    a.commit().await.unwrap();
    let err = b.commit().await.expect_err("second commit must conflict");
    eprintln!("conflict error: {err:?}");
    assert!(eunomia_backend::tx::is_conflict(&err), "not recognised: {err:?}");
}
