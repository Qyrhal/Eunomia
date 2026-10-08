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
            q = q.bind((n.to_string(), surrealdb::sql::Value::None));
        }
        // NONE params make many statements fail at runtime (type checks);
        // that is fine. What must never happen is a statement that does not
        // parse or names a function SurrealDB does not have.
        let text = match q.await {
            Err(e) => e.to_string(),
            Ok(mut res) => res.take_errors().into_values().map(|e| e.to_string()).collect::<Vec<_>>().join("; "),
        };
        let lower = text.to_lowercase();
        if lower.contains("parse error") || lower.contains("invalid function") || lower.contains("unknown function") || lower.contains("failed to parse") {
            broken.push(format!("{}: {}", stmt.name, text));
        }
    }
    assert!(broken.is_empty(), "statements that do not parse:\n{}", broken.join("\n"));
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
