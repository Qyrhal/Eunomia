//! `store::dynamic` builders (SQL text assembled at runtime: optional filters, SET lists, table
//! names) are invisible to `every_query_executes`. This test drives each one through every
//! combination of its inputs against a migrated database and fails on any parse error, unknown
//! function, unknown field or missing table. Then it checks that every dynamic name found in the
//! source was built at least once, so a new builder cannot go untested.

mod common;

use std::collections::{BTreeSet, HashMap};

use common::TestApp;
use eunomia_backend::cache::search::{self as cs, ListParams, SearchParams};
use eunomia_backend::capsules::{self, Failure};
use eunomia_backend::error::ErrorCode;
use eunomia_backend::replay;
use eunomia_backend::rid::RecordIdExt;
use eunomia_backend::telemetry::with_trace_id;
use regex::Regex;
use serde_json::{json, Value};
use surrealdb::types::Datetime;

const KINDS: [&str; 6] = ["person", "organisation", "location", "repository", "file", "symbol"];

/// Every subset of `items`, as index lists.
fn subsets(n: usize) -> impl Iterator<Item = Vec<usize>> {
    (0..1usize << n).map(move |mask| (0..n).filter(|i| mask >> i & 1 == 1).collect())
}

async fn plant_records(app: &TestApp) {
    let db = app.db().await;
    for (i, (source, ty)) in [("alpha", "note"), ("alpha", "event"), ("beta", "note"), ("beta", "event")].into_iter().enumerate() {
        let mut v = vec![0.0f32; 1536];
        v[i] = 1.0;
        db.test_raw()
            .query(
                "CREATE $k SET owner = $o, source = $s, type = $t, external_id = $x, title = 'netflix invoice', body_text = 'netflix monthly charge', \
                 occurred_at = time::now() - 1d * $n, url = '', payload = { a: { b: 'v' }, n: $n }, content_hash = 'h', \
                 ingested_at = time::now(), updated_at = time::now(), deleted = false, embedding = $e",
            )
            .bind(("k", surrealdb::types::RecordId::new("cache_record", format!("{}:r{i}", eunomia_backend::rid::key_string(app.user.id.key()).unwrap()))))
            .bind(("o", app.user.id.clone()))
            .bind(("s", source))
            .bind(("t", ty))
            .bind(("x", format!("x{i}")))
            .bind(("n", i as i64))
            .bind(("e", v))
            .await
            .unwrap()
            .check()
            .unwrap();
    }
}

async fn cache_searches(app: &TestApp) {
    let db = app.db().await;
    let when = |d: i64| Datetime::from(chrono::Utc::now() + chrono::Duration::days(d));
    // cache::search::search: sources, types, since, until, each present or not
    for on in subsets(4) {
        let mut p = SearchParams::new();
        if on.contains(&0) {
            p.sources = Some(vec!["alpha".into()]);
        }
        if on.contains(&1) {
            p.types = Some(vec!["note".into(), "event".into()]);
        }
        if on.contains(&2) {
            p.since = Some(when(-30));
        }
        if on.contains(&3) {
            p.until = Some(when(1));
        }
        for mode in ["keyword", "semantic", "hybrid"] {
            p.mode = mode.into();
            let hits = cs::search(&db, &app.state.settings, &app.user.id, "netflix", &p).await.unwrap_or_else(|e| panic!("search {on:?} {mode}: {e:?}"));
            assert!(!hits.is_empty() || on.contains(&0) || on.contains(&1) || on.contains(&2) || on.contains(&3), "search found nothing for no filters");
        }
    }
    // list_records / count_records: type, filters on a column, every sort direction
    for ty in [None, Some("note".to_string())] {
        for filters in [HashMap::new(), HashMap::from([("source".to_string(), json!("alpha"))]), HashMap::from([("source".to_string(), json!("alpha")), ("external_id__exact".to_string(), json!("x0"))])] {
            for sort in ["-occurred_at", "title", "-source", "bad sort; drop"] {
                let p = ListParams { type_: ty.clone(), filters: filters.clone(), sort: sort.into(), limit: 10, offset: 0 };
                cs::list_records(&db, &app.user.id, &p).await.unwrap_or_else(|e| panic!("list_records {ty:?} {filters:?} {sort}: {e:?}"));
            }
            cs::count_records(&db, &app.user.id, ty.as_deref(), &filters).await.unwrap_or_else(|e| panic!("count_records: {e:?}"));
        }
    }
    // nearest_ids: the KNN text is built per limit
    for limit in [1, 3, 10, 50] {
        let mut v = vec![0.0f32; 1536];
        v[0] = 1.0;
        cs::nearest_ids(&db, &app.user.id, v, limit).await.unwrap_or_else(|e| panic!("nearest_ids {limit}: {e:?}"));
    }
}

async fn generic_tools(app: &TestApp) {
    // `search` tool: the four optional filters x modes
    for on in subsets(4) {
        let mut args = json!({"query": "netflix", "mode": "keyword"});
        if on.contains(&0) {
            args["sources"] = json!(["alpha"]);
        }
        if on.contains(&1) {
            args["types"] = json!(["note"]);
        }
        if on.contains(&2) {
            args["since"] = json!("2000-01-01T00:00:00Z");
        }
        if on.contains(&3) {
            args["until"] = json!("2999-01-01T00:00:00Z");
        }
        for mode in ["keyword", "semantic", "hybrid"] {
            args["mode"] = json!(mode);
            let out = app.tool("search", args.clone()).await;
            assert!(out.get("error").is_none(), "search {args}: {out}");
        }
    }
    // `list` tool: type, field filters, a payload path filter, every sortable column both ways
    for ty in [None, Some("note")] {
        for filters in [json!({}), json!({"source": "alpha"}), json!({"payload__n": 0}), json!({"source": "alpha", "payload__a__b": "v"})] {
            for field in ["occurred_at", "ingested_at", "updated_at", "title", "type", "source", "id", "external_id"] {
                for dir in ["", "-"] {
                    let mut args = json!({"filters": filters, "sort": format!("{dir}{field}")});
                    if let Some(t) = ty {
                        args["type"] = json!(t);
                    }
                    let out = app.tool("list", args.clone()).await;
                    assert!(out.get("error").is_none(), "list {args}: {out}");
                }
            }
        }
    }
}

async fn entities_and_vaults(app: &TestApp) -> Value {
    // one entity of every kind (create_entity), and a fact on each
    for kind in KINDS {
        let out = app.tool("memory_write", json!({"subject_name": format!("Thing {kind}"), "subject_kind": kind, "text": format!("a {kind} fact")})).await;
        assert!(out.get("error").is_none(), "{kind}: {out}");
    }
    let found = app.tool("entities_search", json!({"query": "Thing person"})).await;
    let person = found["results"][0]["id"].as_str().expect("entity id").to_string();
    let other = app.tool("entities_search", json!({"query": "Thing organisation"})).await["results"][0]["id"].as_str().unwrap().to_string();
    app.tool("code_relate", json!({"from_id": person, "to_id": other, "label": "knows"})).await;

    // update_entity: every non-empty combination of name, aliases, summary (and the empty one)
    for (n, on) in subsets(3).enumerate() {
        let mut args = json!({"entity_id": person});
        if on.contains(&0) {
            args["name"] = json!(format!("Thing person {n}"));
        }
        if on.contains(&1) {
            args["aliases"] = json!([format!("alias {n}")]);
        }
        if on.contains(&2) {
            args["summary"] = json!(format!("summary {n}"));
        }
        let out = app.tool("entity_update", args.clone()).await;
        assert!(out.get("error").is_none(), "entity_update {args}: {out}");
    }

    // list (ordered select_by_vault) and graph (unordered), with and without a kind
    let listed = app.http("GET", "/api/entities", None, true).await;
    assert_eq!(listed.0, 200, "{:?}", listed.1);
    for kind in KINDS {
        let (status, body) = app.http("GET", &format!("/api/entities?kind={kind}"), None, true).await;
        assert_eq!(status, 200, "{kind}: {body}");
    }
    let graph = app.tool("entities_graph", json!({})).await;
    assert!(graph.get("error").is_none(), "{graph}");
    let graph = app.tool("entities_graph", json!({"kinds": ["person", "organisation"]})).await;
    assert!(graph.get("error").is_none(), "{graph}");

    // vault clone and merge (find_by_name, entities_in_vault, copy_entity, count_entities)
    let vaults = app.tool("vault_list", json!({})).await;
    let a = vaults["results"][0]["id"].as_str().unwrap().to_string();
    let b = app.tool("vault_create", json!({"name": "Second"})).await;
    let b = b["id"].as_str().unwrap_or_else(|| panic!("vault_create: {b}")).to_string();
    for kind in KINDS {
        app.tool("memory_write", json!({"subject_name": format!("Thing {kind}"), "subject_kind": kind, "text": "same name, other vault", "vault_id": b})).await;
    }
    for kind in ["org", "personal"] {
        let out = app.tool("vault_clone", json!({"vault_id": a, "kind": kind})).await;
        assert!(out.get("error").is_none(), "vault_clone: {out}");
        let out = app.tool("vault_merge", json!({"vault_id_a": a, "vault_id_b": b, "kind": kind})).await;
        assert!(out.get("error").is_none(), "vault_merge: {out}");
    }
    vaults
}

async fn settings_and_connectors(app: &TestApp) {
    // app.settings_update: every subset of the seven patchable fields
    for on in subsets(7) {
        let mut body = serde_json::Map::new();
        for i in on {
            let (k, v) = [
                ("embedding_model", json!("text-embedding-3-small")),
                ("sync_intervals", json!({"heypocket": 3600})),
                ("theme", json!({"mode": "dark"})),
                ("openai_api_key", json!("sk-test")),
                ("openai_base_url", json!("http://127.0.0.1:9/v1")),
                ("observations_mission", json!("keep it short")),
                ("memory_skill", json!("my skill")),
            ][i]
                .clone();
            body.insert(k.into(), v);
        }
        let (status, out) = app.http("PATCH", "/api/settings", Some(Value::Object(body.clone())), true).await;
        assert_eq!(status, 200, "PATCH settings {body:?}: {out}");
    }
    // app.connector_update: every subset of enabled, config, credentials
    for on in subsets(3) {
        let mut body = serde_json::Map::new();
        if on.contains(&0) {
            body.insert("enabled".into(), json!(true));
        }
        if on.contains(&1) {
            body.insert("config".into(), json!({"k": "v"}));
        }
        if on.contains(&2) {
            body.insert("credentials".into(), json!({"token": "t"}));
        }
        let (status, out) = app.http("PUT", "/api/connectors/up_bank", Some(Value::Object(body.clone())), true).await;
        assert_eq!(status, 200, "PUT connector {body:?}: {out}");
    }
}

async fn export_and_replay(app: &TestApp) {
    let (status, out) = app.http("GET", "/api/export", None, true).await;
    assert_eq!(status, 200, "{out}");
    // replay rebuilds this user's data in a scratch database through the replay.import_* statements
    let trace_id = format!("{:032x}", 424242);
    with_trace_id(
        trace_id.clone(),
        capsules::record(
            &app.state.control,
            Failure {
                org: None,
                kind: "tool",
                name: "entities_search".into(),
                user: Some(app.user.id.to_string()),
                args: json!({"query": "Thing"}),
                code: ErrorCode::Internal,
                status: 500,
                source: "boom".into(),
            },
        ),
    )
    .await;
    let r = replay::replay(&app.state, &app.state.settings, &trace_id).await.expect("replay seeds a scratch database");
    assert_eq!(r.replayed.code, "ok");
}

/// Every name passed to `store::dynamic` / `dynamic_control` in `src/`.
fn dynamic_names_in_source() -> BTreeSet<String> {
    let call = Regex::new(r#"dynamic(?:_control)?\(\s*\w+,\s*"([^"]+)""#).unwrap();
    let mut names = BTreeSet::new();
    let mut stack = vec![std::path::PathBuf::from("src")];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let text = std::fs::read_to_string(&path).unwrap();
                names.extend(call.captures_iter(&text).map(|c| c[1].to_string()));
            }
        }
    }
    names
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn every_dynamic_builder_runs_in_every_combination() {
    let app = TestApp::new().await;
    plant_records(&app).await;
    cache_searches(&app).await;
    generic_tools(&app).await;
    entities_and_vaults(&app).await;
    settings_and_connectors(&app).await;
    export_and_replay(&app).await;

    // the control-database dynamic statement (the legacy move) runs on every boot
    eunomia_backend::provisioning::legacy::move_if_needed(app.state.provisioner.as_ref().unwrap(), &app.state.control, &app.state.settings).await.unwrap();

    let seen: BTreeSet<String> = eunomia_backend::store::DYNAMIC_SEEN.lock().unwrap().iter().map(|s| s.to_string()).collect();
    let in_source = dynamic_names_in_source();
    assert!(in_source.len() >= 20, "source scan found only {in_source:?}");
    let untested: Vec<_> = in_source.difference(&seen).collect();
    assert!(untested.is_empty(), "dynamic builders no case here reached: {untested:?}");
    assert_eq!(eunomia_backend::pool::NO_ORG_CONTEXT.load(std::sync::atomic::Ordering::Relaxed), 0);
}
