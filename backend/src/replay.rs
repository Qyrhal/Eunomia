//! `eunomia replay <trace_id>`: re-run a failure capsule against a scratch in-memory database
//! seeded with the caller's own data, so an agent can reproduce a failure and turn it into a test
//! (docs/debugging.md). The real database is only read.

use surrealdb::types::SurrealValue;
use axum::body::Body;
use axum::http::{header, Request};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use surrealdb::types::RecordId;
use tower::ServiceExt;

use crate::capsules::{self, Capsule};
use crate::config::Settings;
use crate::error::{AppError, AppResult, ErrorCode};
use crate::models_user::{self, User};
use crate::pool::OrgDb;
use crate::state::AppState;
use crate::store;
use crate::tools::registry;

#[derive(Debug)]
pub struct Outcome {
    /// `ok`, or the error code.
    pub code: String,
    pub status: Option<u16>,
    pub body: Value,
}

#[derive(Debug)]
pub struct Report {
    pub capsule: Capsule,
    pub replayed: Outcome,
    pub reproduced: bool,
}

/// Loads the capsule from `src`, rebuilds the caller's data in a scratch database and re-runs the call.
pub async fn replay(src: &AppState, settings: &Settings, trace_id: &str) -> AppResult<Report> {
    let capsule = capsules::get(&src.control, trace_id).await?.ok_or_else(|| AppError::not_found(format!("No failure capsule for trace {trace_id}.")))?;
    if capsule.truncated {
        return Err(AppError::bad_request("The capsule was cut to fit the size cap, so it cannot be replayed."));
    }

    let scratch_settings = Settings { surreal_url: "mem://".into(), surreal_ns: "replay".into(), surreal_db: "replay".into(), ..settings.clone() };
    let state = AppState::build(&scratch_settings, surrealdb::opt::Config::new()).await?;

    let user = match &capsule.user {
        Some(id) => Some(seed(src, &state, id).await?),
        None => None,
    };
    let replayed = crate::telemetry::with_trace_id(capsule.trace_id.clone(), run(&state, &capsule, user.as_ref())).await?;
    let reproduced = replayed.code == capsule.code;
    Ok(Report { capsule, replayed, reproduced })
}

/// Copies the capsule user's personal-vault data into `scratch` under a fresh user with the same email and the same record ids.
async fn seed(src: &AppState, scratch: &AppState, user_id: &str) -> AppResult<User> {
    #[derive(serde::Deserialize, SurrealValue)]
    struct Email {
        email: String,
    }
    let id: RecordId = crate::rid::parse(user_id).map_err(|_| AppError::bad_request("The capsule has a malformed user id."))?;
    let mut res = store::entities::EMAILS_FOR.on(&src.control).bind(("ids", vec![id.clone()])).await?;
    let email = res.take::<Vec<Email>>(0)?.into_iter().next().ok_or_else(|| AppError::not_found("The capsule's user no longer exists."))?.email;

    let original = models_user::load_user(&src.control, id, email.clone()).await?;
    let from = src.pool.for_org(&original.org).await?;
    let doc = crate::routers::export::build_export(&from, &src.control, &original).await?;
    let user = models_user::register_user(scratch, &email, &uuid::Uuid::new_v4().simple().to_string()).await?;
    import(&scratch.pool.for_org(&user.org).await?, &user, &doc).await?;
    Ok(user)
}

fn rid(v: &Value) -> AppResult<RecordId> {
    v.as_str().and_then(|s| crate::rid::parse(s).ok()).ok_or_else(|| AppError::internal(format!("export holds a bad record id: {v}")))
}

/// Loads an export document into `user`'s personal vault, keeping record ids.
async fn import(db: &OrgDb, user: &User, doc: &Value) -> AppResult<()> {
    let vault = crate::routers::export::resolve_personal_vault(db, &user.id).await?;
    let mut seen_relations = std::collections::HashSet::new();
    for e in doc["entities"].as_array().into_iter().flatten() {
        crate::entities::service::kind_table(e["kind"].as_str().unwrap_or_default())?;
        let id = rid(&e["id"])?;
        let aliases: Vec<String> = e["aliases"].as_array().into_iter().flatten().filter_map(|a| a.as_str().map(String::from)).collect();
        store::dynamic(db, "replay.import_entity", "CREATE $id SET owner = $owner, vault = $vault, name = $name, aliases = $aliases, summary = $summary")
            .bind(("id", id.clone()))
            .bind(("owner", user.id.clone()))
            .bind(("vault", vault.clone()))
            .bind(("name", e["name"].as_str().unwrap_or_default().to_string()))
            .bind(("aliases", aliases))
            .bind(("summary", e["summary"].as_str().unwrap_or_default().to_string()))
            .await?
            .check()?;
        for m in e["memory"].as_array().into_iter().flatten() {
            store::dynamic(
                db,
                "replay.import_memory",
                "CREATE $id SET owner = $owner, vault = $vault, subject = $subject, text = $text, type = $type, proof_count = $proof_count, status = $status",
            )
            .bind(("id", rid(&m["id"])?))
            .bind(("owner", user.id.clone()))
            .bind(("vault", vault.clone()))
            .bind(("subject", id.clone()))
            .bind(("text", m["text"].as_str().unwrap_or_default().to_string()))
            .bind(("type", m["type"].as_str().unwrap_or("world").to_string()))
            .bind(("proof_count", m["proof_count"].as_i64().unwrap_or(1)))
            .bind(("status", m["status"].as_str().map(String::from)))
            .await?
            .check()?;
        }
        for r in e["relations"].as_array().into_iter().flatten() {
            if !seen_relations.insert(r["id"].to_string()) {
                continue;
            }
            store::dynamic(
                db,
                "replay.import_relation",
                "INSERT RELATION INTO relates_to { id: $id, in: $in, out: $out, label: $label, owner: $owner }",
            )
            .bind(("id", rid(&r["id"])?))
            .bind(("in", rid(&r["in"])?))
            .bind(("out", rid(&r["out"])?))
            .bind(("label", r["label"].as_str().unwrap_or_default().to_string()))
            .bind(("owner", user.id.clone()))
            .await?
            .check()?;
        }
    }
    Ok(())
}

async fn run(state: &AppState, capsule: &Capsule, user: Option<&User>) -> AppResult<Outcome> {
    if capsule.kind == "tool" {
        let user = user.ok_or_else(|| AppError::bad_request("A tool capsule has no user."))?;
        let outcome = match registry::call(state, user, &capsule.name, capsule.args.clone()).await {
            Ok(v) if v.get("error").is_some() => Outcome { code: v["code"].as_str().unwrap_or("error").to_string(), status: None, body: v },
            Ok(v) => Outcome { code: "ok".into(), status: None, body: v },
            Err(e) => Outcome { code: e.code.as_str().into(), status: Some(e.status.as_u16()), body: json!({ "error": e.message }) },
        };
        return Ok(outcome);
    }

    let a = &capsule.args;
    let mut req = Request::builder().method(a["method"].as_str().unwrap_or("GET")).uri(a["uri"].as_str().unwrap_or("/"));
    if let Some(u) = user {
        let token = models_user::create_api_token(&state.control, &u.id, "replay").await?.token;
        req = req.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    let body = match &a["body"] {
        Value::Null => Body::empty(),
        Value::String(s) => Body::from(s.clone()),
        v => {
            req = req.header(header::CONTENT_TYPE, "application/json");
            Body::from(v.to_string())
        }
    };
    let req = req.body(body).map_err(|e| AppError::internal(e.to_string()))?;
    let resp = crate::app(state.clone()).oneshot(req).await.map_err(|e| AppError::internal(e.to_string()))?;
    let status = resp.status();
    let bytes = resp.into_body().collect().await.map_err(|e| AppError::internal(e.to_string()))?.to_bytes();
    let body: Value = serde_json::from_slice(&bytes).unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned()));
    let code = body["code"].as_str().map(String::from).unwrap_or_else(|| if status.is_success() { "ok".into() } else { ErrorCode::Internal.as_str().into() });
    Ok(Outcome { code, status: Some(status.as_u16()), body })
}

/// A Rust integration-test skeleton for `backend/tests/` that fails while the bug is still there.
pub fn emit_test(c: &Capsule) -> String {
    let short: String = c.trace_id.chars().take(8).collect();
    let args = serde_json::to_string_pretty(&c.args).unwrap_or_default();
    let (call, check) = if c.kind == "tool" {
        (
            format!("    let out = registry::call(&app.state, &app.user, {:?}, json!({args})).await;\n    let code = match &out {{\n        Ok(v) => v.get(\"code\").and_then(|c| c.as_str()).unwrap_or(\"ok\").to_string(),\n        Err(e) => e.code.as_str().to_string(),\n    }};", c.name),
            "code",
        )
    } else {
        let method = c.args["method"].as_str().unwrap_or("GET");
        let uri = c.args["uri"].as_str().unwrap_or("/");
        let body = if c.args["body"].is_null() { "None".to_string() } else { format!("Some(json!({}))", serde_json::to_string_pretty(&c.args["body"]).unwrap_or_default()) };
        (format!("    let (status, out) = app.http({method:?}, {uri:?}, {body}, true).await;\n    let code = out[\"code\"].as_str().unwrap_or(\"ok\").to_string();\n    let _ = status;"), "code")
    };
    format!(
        "//! Reproduces trace {trace} ({code}) captured on {version}. Generated by `eunomia replay --emit-test`.\n\
         mod common;\n\n\
         use common::TestApp;\n\
         use eunomia_backend::tools::registry;\n\
         use serde_json::json;\n\n\
         #[tokio::test]\n\
         async fn replay_{short}() {{\n\
         \x20   let app = TestApp::new().await;\n\
         \x20   // TODO: seed what the failure needs (run `eunomia replay {trace}` to see the caller's data shape).\n\
         {call}\n\
         \x20   // Fails while the bug exists; it passes once the call stops failing with {code}.\n\
         \x20   assert_ne!({check}, {code:?}, \"still failing: {name}\");\n\
         }}\n",
        trace = c.trace_id,
        code = c.code,
        version = c.version,
        name = c.name,
    )
}

/// The `replay` subcommand. Returns the process exit code: 0 reproduced, 1 not reproduced, 2 usage or load error.
pub async fn cli(args: &[String], settings: &Settings) -> i32 {
    let emit = args.iter().any(|a| a == "--emit-test");
    let Some(trace_id) = args.iter().find(|a| !a.starts_with("--")) else {
        eprintln!("usage: eunomia replay <trace_id> [--emit-test]");
        return 2;
    };
    let src = match AppState::attach(settings, surrealdb::opt::Config::new()).await {
        Ok(d) => d,
        Err(e) => return fail(format!("cannot connect to the database: {}", e.source.unwrap_or(e.message))),
    };
    match replay(&src, settings, trace_id).await {
        Err(e) => fail(format!("{}: {}", e.code.as_str(), e.source.unwrap_or(e.message))),
        Ok(r) if emit => {
            print!("{}", emit_test(&r.capsule));
            0
        }
        Ok(r) => {
            let c = &r.capsule;
            println!("trace     {}", c.trace_id);
            println!("what      {} {} (version {}, {})", c.kind, c.name, c.version, c.created_at);
            println!("original  {} ({})\n          {}", c.code, c.status, c.source);
            println!("replayed  {}{}\n          {}", r.replayed.code, r.replayed.status.map_or(String::new(), |s| format!(" ({s})")), r.replayed.body);
            println!("reproduced: {}", if r.reproduced { "yes" } else { "no (the data or the code differs from when it failed)" });
            i32::from(!r.reproduced)
        }
    }
}

fn fail(msg: String) -> i32 {
    eprintln!("{msg}");
    2
}
