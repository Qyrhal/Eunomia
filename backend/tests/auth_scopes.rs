//! Token scopes, vault restriction, expiry, audit events and rate limits,
//! end to end through the real router and an in-memory SurrealDB.

mod common;

use axum::body::Body;
use axum::http::{header, HeaderMap, Request, StatusCode};
use common::{TestApp, PASSWORD};
use eunomia_backend::models_user::create_api_token_with;
use eunomia_backend::ratelimit::RateConfig;
use eunomia_backend::scopes;
use eunomia_backend::vaults::service as vaults;
use http_body_util::BodyExt;
use serde_json::{json, Value};
use eunomia_backend::rid::{self, RecordIdExt};
use surrealdb::types::{Datetime, RecordId};
use tower::ServiceExt;

async fn send(
    router: &axum::Router,
    method: &str,
    path: &str,
    body: Option<Value>,
    headers: &[(&str, String)],
) -> (StatusCode, HeaderMap, Value) {
    let mut req = Request::builder().method(method).uri(path);
    for (k, v) in headers {
        req = req.header(*k, v);
    }
    let req = match body {
        Some(b) => req.header(header::CONTENT_TYPE, "application/json").body(Body::from(b.to_string())),
        None => req.body(Body::empty()),
    }
    .unwrap();
    // the socket peer is the frontend proxy on the compose network
    let mut req = req;
    req.extensions_mut().insert(axum::extract::ConnectInfo(std::net::SocketAddr::from(([172, 18, 0, 2], 4000))));
    let resp = router.clone().oneshot(req).await.unwrap();
    let (status, headers) = (resp.status(), resp.headers().clone());
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (status, headers, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

fn bearer(token: &str) -> Vec<(&'static str, String)> {
    vec![("authorization", format!("Bearer {token}"))]
}

async fn token(app: &TestApp, granted: &[&str], vault: Option<&RecordId>) -> String {
    let granted: Vec<String> = granted.iter().map(|s| s.to_string()).collect();
    create_api_token_with(&app.state.db, &app.user.id, "t", &granted, vault, None).await.unwrap().token
}

async fn org_vault(app: &TestApp) -> RecordId {
    rid::parse(&vaults::create_vault(&app.state.db, &app.user.id, "Team", "org").await.unwrap().id).unwrap()
}

/// A JSON-RPC `tools/call` over `/mcp`; returns (isError, tool value).
async fn mcp_call(app: &TestApp, token: &str, name: &str, args: Value) -> (bool, Value) {
    let body = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": name, "arguments": args}});
    let (status, _, v) = send(&app.router, "POST", "/mcp", Some(body), &bearer(token)).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    let text = v["result"]["content"][0]["text"].as_str().expect("tool text");
    (v["result"]["isError"].as_bool().unwrap(), serde_json::from_str(text).unwrap())
}

async fn events(app: &TestApp, filter: &str) -> Vec<Value> {
    let mut res = app.state.db.query(format!(
        "SELECT action, actor_kind, actor_id, target, outcome, trace_id, detail, (IF user != NONE THEN <string> user ELSE '' END) AS user, <string> created_at AS created_at \
         FROM audit_event WHERE {filter} ORDER BY created_at"
    )).await.unwrap();
    res.take::<Vec<Value>>(0).unwrap()
}

fn all() -> Vec<String> {
    scopes::ALL.iter().map(|s| s.to_string()).collect()
}

// ---------------------------------------------------------------- scopes

#[tokio::test]
async fn read_only_token_reads_but_cannot_write_or_administer() {
    let app = TestApp::new().await;
    let t = token(&app, &[scopes::MEMORY_READ], None).await;
    let h = bearer(&t);

    let (status, _, _) = send(&app.router, "GET", "/api/entities", None, &h).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _, body) = send(&app.router, "POST", "/api/entities", Some(json!({"kind": "person", "name": "Ann"})), &h).await;
    assert_eq!((status, body["code"].as_str()), (StatusCode::FORBIDDEN, Some("auth.scope")));
    let (status, _, body) = send(&app.router, "POST", "/api/vaults", Some(json!({"name": "X"})), &h).await;
    assert_eq!((status, body["code"].as_str()), (StatusCode::FORBIDDEN, Some("auth.scope")));
    for path in ["/api/settings", "/api/auth/tokens", "/api/connectors", "/api/audit", "/api/export"] {
        let (status, _, body) = send(&app.router, "GET", path, None, &h).await;
        assert_eq!((status, body["code"].as_str()), (StatusCode::FORBIDDEN, Some("auth.scope")), "{path}");
    }
    // the same call with a fully scoped token passes the gate
    let (status, _, _) = send(&app.router, "GET", "/api/settings", None, &bearer(&app.token)).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn connectors_scope_gates_connector_routes() {
    let app = TestApp::new().await;
    let only = token(&app, &[scopes::CONNECTORS], None).await;
    let (status, _, _) = send(&app.router, "GET", "/api/connectors", None, &bearer(&only)).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _, _) = send(&app.router, "GET", "/api/entities", None, &bearer(&only)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn tool_scopes_hold_over_rest_and_mcp() {
    let app = TestApp::new().await;
    let read = token(&app, &[scopes::MEMORY_READ], None).await;
    let write = token(&app, &[scopes::MEMORY_WRITE], None).await;
    let write_args = json!({"subject_name": "Ann", "subject_kind": "person", "text": "likes tea"});

    // REST
    let (status, _, body) = send(&app.router, "POST", "/api/tools/memory_write", Some(write_args.clone()), &bearer(&read)).await;
    assert_eq!((status, body["code"].as_str()), (StatusCode::FORBIDDEN, Some("auth.scope")));
    // memory:write implies memory:read, as for OAuth tokens; connectors alone grants neither
    let (status, _, _) = send(&app.router, "POST", "/api/tools/recall", Some(json!({"query": "tea"})), &bearer(&write)).await;
    assert_eq!(status, StatusCode::OK);
    let conn = token(&app, &[scopes::CONNECTORS], None).await;
    let (status, _, body) = send(&app.router, "POST", "/api/tools/recall", Some(json!({"query": "tea"})), &bearer(&conn)).await;
    assert_eq!((status, body["code"].as_str()), (StatusCode::FORBIDDEN, Some("auth.scope")));
    let (status, _, _) = send(&app.router, "POST", "/api/tools/memory_write", Some(write_args.clone()), &bearer(&write)).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _, _) = send(&app.router, "POST", "/api/tools/entities_search", Some(json!({"query": "Ann"})), &bearer(&read)).await;
    assert_eq!(status, StatusCode::OK);

    // MCP: a refusal is a tool result with isError and the same code
    let (is_error, v) = mcp_call(&app, &read, "memory_write", write_args).await;
    assert!(is_error);
    assert_eq!(v["code"], "auth.scope");
    let (is_error, v) = mcp_call(&app, &read, "vault_create", json!({"name": "X"})).await;
    assert!((is_error, v["code"].as_str()) == (true, Some("auth.scope")), "{v}");
    let (is_error, v) = mcp_call(&app, &read, "entities_search", json!({"query": "Ann"})).await;
    assert!(!is_error, "{v}");
}

#[tokio::test]
async fn token_creation_validates_and_cannot_escalate() {
    let app = TestApp::new().await;
    let org = org_vault(&app).await;
    let (status, _, made) = send(
        &app.router,
        "POST",
        "/api/auth/tokens",
        Some(json!({"name": "ci", "scopes": ["memory:read"], "vault_id": org.to_string(), "expires_at": "2999-01-01T00:00:00Z"})),
        &bearer(&app.token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{made}");
    assert_eq!(made["scopes"], json!(["memory:read"]));
    assert_eq!(made["vault_id"], org.to_string());
    assert!(made["expires_at"].is_string());

    let (_, _, list) = send(&app.router, "GET", "/api/auth/tokens", None, &bearer(&app.token)).await;
    let row = list.as_array().unwrap().iter().find(|t| t["name"] == "ci").unwrap();
    assert_eq!((&row["scopes"], &row["vault_id"]), (&json!(["memory:read"]), &json!(org.to_string())));
    let legacy = list.as_array().unwrap().iter().find(|t| t["name"] == "test").unwrap();
    assert_eq!(legacy["scopes"].as_array().unwrap().len(), 4, "tokens made without scopes are full-scope");
    assert!(legacy["expires_at"].is_null());

    for bad in [
        json!({"name": "x", "scopes": []}),
        json!({"name": "x", "scopes": ["root"]}),
        json!({"name": "x", "expires_at": "yesterday"}),
        json!({"name": "x", "expires_at": "2001-01-01T00:00:00Z"}),
        json!({"name": "x", "vault_id": "person:nope"}),
    ] {
        let (status, _, _) = send(&app.router, "POST", "/api/auth/tokens", Some(bad.clone()), &bearer(&app.token)).await;
        assert!(status.is_client_error(), "{bad}");
    }
    // a vault the creator is not in
    let other = eunomia_backend::models_user::register_user(&app.state.db, "other@example.com", PASSWORD).await.unwrap();
    let theirs = vaults::default_vault_id(&app.state.db, &other.id).await.unwrap();
    let (status, _, body) = send(
        &app.router,
        "POST",
        "/api/auth/tokens",
        Some(json!({"name": "x", "vault_id": theirs.to_string()})),
        &bearer(&app.token),
    )
    .await;
    assert_eq!((status, body["code"].as_str()), (StatusCode::FORBIDDEN, Some("vault.forbidden")));

    // vaults:admin alone cannot mint a memory:read token
    let admin = token(&app, &[scopes::VAULTS_ADMIN], None).await;
    let (status, _, body) =
        send(&app.router, "POST", "/api/auth/tokens", Some(json!({"name": "x", "scopes": ["memory:read"]})), &bearer(&admin)).await;
    assert_eq!((status, body["code"].as_str()), (StatusCode::FORBIDDEN, Some("auth.scope")));
}

// ------------------------------------------------------- vault restriction

#[tokio::test]
async fn vault_restricted_token_is_confined_to_its_vault() {
    let app = TestApp::new().await;
    let org = org_vault(&app).await;
    let personal = vaults::default_vault_id(&app.state.db, &app.user.id).await.unwrap();
    // personal data the restricted token must never see
    app.tool("memory_write", json!({"subject_name": "Secret", "subject_kind": "person", "text": "in personal"})).await;

    let t = token(&app, scopes::ALL, Some(&org)).await;
    let h = bearer(&t);

    // no vault_id means its own vault, not the personal one
    let (status, _, body) = send(
        &app.router,
        "POST",
        "/api/tools/memory_write",
        Some(json!({"subject_name": "Team Fact", "subject_kind": "person", "text": "in org"})),
        &h,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (_, _, listing) = send(&app.router, "GET", &format!("/api/entities?vault_id={}", org.to_string()), None, &h).await;
    let names: Vec<&str> = listing["results"].as_array().unwrap().iter().map(|e| e["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["Team Fact"]);
    let (_, _, default_listing) = send(&app.router, "GET", "/api/entities", None, &h).await;
    assert_eq!(default_listing["results"].as_array().unwrap().len(), 1, "the default scope is the restricted vault");

    // the personal vault is a 403 auth.scope, over REST and MCP, read and write
    let (status, _, body) = send(&app.router, "GET", &format!("/api/entities?vault_id={}", personal.to_string()), None, &h).await;
    assert_eq!((status, body["code"].as_str()), (StatusCode::FORBIDDEN, Some("auth.scope")), "{body}");
    let (status, _, body) = send(&app.router, "GET", &format!("/api/vaults/{}/members", personal.to_string()), None, &h).await;
    assert_eq!((status, body["code"].as_str()), (StatusCode::FORBIDDEN, Some("auth.scope")), "{body}");
    let (is_error, v) = mcp_call(&app, &t, "recall", json!({"query": "personal", "vault_id": personal.to_string()})).await;
    assert!((is_error, v["code"].as_str()) == (true, Some("auth.scope")), "{v}");
    let (is_error, v) = mcp_call(
        &app,
        &t,
        "memory_write",
        json!({"subject_name": "X", "subject_kind": "person", "text": "t", "vault_id": personal.to_string()}),
    )
    .await;
    assert!((is_error, v["code"].as_str()) == (true, Some("auth.scope")), "{v}");

    // nothing from the personal vault leaks through the default scope
    let (is_error, v) = mcp_call(&app, &t, "entities_search", json!({"query": "Secret"})).await;
    assert!(!is_error);
    assert!(!v.to_string().contains("Secret"), "{v}");
    let (_, v) = mcp_call(&app, &t, "recall", json!({"query": "personal"})).await;
    assert!(!v.to_string().contains("in personal"), "{v}");

    // it sees only its vault, cannot make new ones, cannot read raw source records
    let (_, _, vs) = send(&app.router, "GET", "/api/vaults", None, &h).await;
    let ids: Vec<&str> = vs.as_array().or(vs["results"].as_array()).unwrap().iter().map(|v| v["id"].as_str().unwrap()).collect();
    assert_eq!(ids, [org.to_string()]);
    for (m, p, b) in [
        ("POST", "/api/vaults", Some(json!({"name": "new"}))),
        ("POST", &*format!("/api/vaults/{}/clone", org.to_string()), Some(json!({}))),
        ("GET", "/api/settings", None),
        ("GET", "/api/auth/tokens", None),
        ("POST", "/api/chat/threads", Some(json!({}))),
    ] {
        let (status, _, body) = send(&app.router, m, p, b, &h).await;
        assert_eq!((status, body["code"].as_str()), (StatusCode::FORBIDDEN, Some("auth.scope")), "{m} {p}");
    }
    let (is_error, v) = mcp_call(&app, &t, "search", json!({"query": "x"})).await;
    assert!((is_error, v["code"].as_str()) == (true, Some("auth.scope")), "{v}");

    // and the unrestricted token still reaches both
    let (status, _, _) = send(&app.router, "GET", &format!("/api/entities?vault_id={}", personal.to_string()), None, &bearer(&app.token)).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn restricted_token_loses_access_when_removed_from_its_vault() {
    let app = TestApp::new().await;
    let other = eunomia_backend::models_user::register_user(&app.state.db, "o@example.com", PASSWORD).await.unwrap();
    let theirs: RecordId = rid::parse(&vaults::create_vault(&app.state.db, &other.id, "Theirs", "org").await.unwrap().id).unwrap();
    vaults::invite_member(&app.state.db, &other.id, &theirs, &app.user.email, "member").await.unwrap();
    vaults::accept_invitation(&app.state.db, &app.user.id, &theirs).await.unwrap();
    let t = token(&app, scopes::ALL, Some(&theirs)).await;

    let (status, _, _) = send(&app.router, "GET", "/api/entities", None, &bearer(&t)).await;
    assert_eq!(status, StatusCode::OK);
    vaults::remove_member(&app.state.db, &other.id, &theirs, &app.user.email).await.unwrap();
    let (status, _, body) = send(&app.router, "GET", "/api/entities", None, &bearer(&t)).await;
    assert_eq!((status, body["code"].as_str()), (StatusCode::FORBIDDEN, Some("vault.forbidden")));
}

// ----------------------------------------------------------------- expiry

#[tokio::test]
async fn expired_token_is_401_token_expired() {
    let app = TestApp::new().await;
    let past = Datetime::from(chrono::Utc::now() - chrono::Duration::minutes(1));
    let expired = create_api_token_with(&app.state.db, &app.user.id, "old", &all(), None, Some(past)).await.unwrap().token;
    let future = Datetime::from(chrono::Utc::now() + chrono::Duration::days(1));
    let live = create_api_token_with(&app.state.db, &app.user.id, "new", &all(), None, Some(future)).await.unwrap().token;

    let (status, _, body) = send(&app.router, "GET", "/api/auth/me", None, &bearer(&expired)).await;
    assert_eq!((status, body["code"].as_str()), (StatusCode::UNAUTHORIZED, Some("auth.token_expired")));
    let (status, _, _) = send(&app.router, "GET", "/api/auth/me", None, &bearer(&live)).await;
    assert_eq!(status, StatusCode::OK);

    let body = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"});
    let (status, headers, v) = send(&app.router, "POST", "/mcp", Some(body), &bearer(&expired)).await;
    assert_eq!((status, v["code"].as_str()), (StatusCode::UNAUTHORIZED, Some("auth.token_expired")));
    let challenge = headers[header::WWW_AUTHENTICATE].to_str().unwrap();
    assert!(challenge.starts_with("Bearer error=\"invalid_token\"") && challenge.contains("resource_metadata="), "{challenge}");
}

async fn login_cookie(app: &TestApp) -> String {
    let creds = json!({"email": "tester@example.com", "password": PASSWORD});
    let (status, headers, _) = send(&app.router, "POST", "/api/auth/login", Some(creds), &[]).await;
    assert_eq!(status, StatusCode::OK);
    headers[header::SET_COOKIE].to_str().unwrap().split(';').next().unwrap().to_string()
}

async fn session_expiry_secs_from_now(app: &TestApp) -> i64 {
    let mut res = app.state.db.query("SELECT VALUE duration::secs(expires_at - time::now()) FROM session").await.unwrap();
    res.take::<Vec<i64>>(0).unwrap()[0]
}

#[tokio::test]
async fn expired_session_is_401_and_live_one_slides_at_most_hourly() {
    let app = TestApp::new().await;
    let cookie = login_cookie(&app).await;
    let c = vec![("cookie", cookie.clone())];

    let (status, _, _) = send(&app.router, "GET", "/api/auth/me", None, &c).await;
    assert_eq!(status, StatusCode::OK);
    let day = 24 * 3600;
    let secs = session_expiry_secs_from_now(&app).await;
    assert!((29 * day..=30 * day).contains(&secs), "default 30 day window, got {secs}s");

    // used within the hour: not extended
    app.state.db.query("UPDATE session SET expires_at = time::now() + 29d + 23h + 30m").await.unwrap().check().unwrap();
    send(&app.router, "GET", "/api/auth/me", None, &c).await;
    let secs = session_expiry_secs_from_now(&app).await;
    assert!(secs < 29 * day + 23 * 3600 + 31 * 60, "not extended, got {secs}s");

    // idle for a while: extended back to the full window
    app.state.db.query("UPDATE session SET expires_at = time::now() + 10d").await.unwrap().check().unwrap();
    send(&app.router, "GET", "/api/auth/me", None, &c).await;
    let secs = session_expiry_secs_from_now(&app).await;
    assert!(secs > 29 * day, "slid forward, got {secs}s");

    // past expiry: rejected
    app.state.db.query("UPDATE session SET expires_at = time::now() - 1m").await.unwrap().check().unwrap();
    let (status, _, body) = send(&app.router, "GET", "/api/auth/me", None, &c).await;
    assert_eq!((status, body["code"].as_str()), (StatusCode::UNAUTHORIZED, Some("auth.session_expired")));
}

#[tokio::test]
async fn jwt_exp_is_enforced_and_legacy_cookies_without_exp_still_work() {
    let app = TestApp::new().await;
    let sid = eunomia_backend::models_user::generate_token();
    app.state
        .db
        .query("CREATE session SET owner = $o, sid = $sid, expires_at = time::now() + 1d")
        .bind(("o", app.user.id.clone()))
        .bind(("sid", sid.clone()))
        .await
        .unwrap()
        .check()
        .unwrap();
    let jwt = |claims: Value| {
        jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &claims,
            &jsonwebtoken::EncodingKey::from_secret(app.state.settings.jwt_secret.as_bytes()),
        )
        .unwrap()
    };
    let base = json!({"sub": app.user.id.to_string(), "email": app.user.email, "sid": sid});

    let legacy = jwt(base.clone());
    let (status, _, _) = send(&app.router, "GET", "/api/auth/me", None, &[("cookie", format!("eunomia_session={legacy}"))]).await;
    assert_eq!(status, StatusCode::OK, "cookies issued before exp existed keep working");

    let mut expired = base;
    expired["exp"] = json!(chrono::Utc::now().timestamp() - 3600);
    let (status, _, body) =
        send(&app.router, "GET", "/api/auth/me", None, &[("cookie", format!("eunomia_session={}", jwt(expired)))]).await;
    assert_eq!((status, body["code"].as_str()), (StatusCode::UNAUTHORIZED, Some("auth.session_expired")));
}

// ------------------------------------------------------------------ audit

/// An OAuth access token for `scope`, minted straight into the database.
async fn oauth_token(app: &TestApp, scope: &[&str]) -> String {
    let token = format!("eoa_{}", eunomia_backend::models_user::generate_token());
    let grant: Option<RecordId> = app
        .state
        .db
        .query(
            "CREATE oauth_grant SET owner = $o, client_id = 'client-x', client_name = 'X', scope = $scope, \
             resource = 'http://localhost:8001/mcp' RETURN VALUE id",
        )
        .bind(("o", app.user.id.clone()))
        .bind(("scope", scope.iter().map(|s| s.to_string()).collect::<Vec<_>>()))
        .await
        .unwrap()
        .take(0)
        .unwrap();
    app.state
        .db
        .query("CREATE oauth_token SET kind = 'access', token_hash = $h, family = $g, expires_at = time::now() + 15m")
        .bind(("h", eunomia_backend::models_user::hash_token(&token)))
        .bind(("g", grant.unwrap()))
        .await
        .unwrap()
        .check()
        .unwrap();
    token
}

#[tokio::test]
async fn oauth_tokens_work_on_mcp_only_and_are_audited_as_the_client() {
    let app = TestApp::new().await;
    let t = oauth_token(&app, &["memory:write"]).await;
    let (is_error, v) =
        mcp_call(&app, &t, "memory_write", json!({"subject_name": "Ann", "subject_kind": "person", "text": "x"})).await;
    assert!(!is_error, "{v}");
    let rows = events(&app, "action = 'tool.memory_write'").await;
    assert_eq!((rows[0]["actor_kind"].as_str(), rows[0]["actor_id"].as_str()), (Some("oauth"), Some("client-x")));
    // never accepted on the REST API
    let (status, _, _) = send(&app.router, "GET", "/api/entities", None, &bearer(&t)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn audit_events_cover_auth_and_tool_calls_with_trace_ids() {
    let app = TestApp::new().await;

    // login (failed, then ok)
    let bad = json!({"email": "tester@example.com", "password": "wrong-password"});
    let (status, headers, _) = send(&app.router, "POST", "/api/auth/login", Some(bad), &[]).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let failed_trace = headers["x-trace-id"].to_str().unwrap().to_string();
    login_cookie(&app).await;

    // token create + revoke
    let (_, _, made) = send(&app.router, "POST", "/api/auth/tokens", Some(json!({"name": "k"})), &bearer(&app.token)).await;
    let id = made["id"].as_str().unwrap().to_string();
    send(&app.router, "DELETE", &format!("/api/auth/tokens/{id}"), None, &bearer(&app.token)).await;

    // a mutating tool call over REST, and over MCP
    let (_, headers, _) = send(
        &app.router,
        "POST",
        "/api/tools/memory_write",
        Some(json!({"subject_name": "Ann", "subject_kind": "person", "text": "x"})),
        &bearer(&app.token),
    )
    .await;
    let tool_trace = headers["x-trace-id"].to_str().unwrap().to_string();
    mcp_call(&app, &app.token, "memory_write", json!({"subject_name": "Bob", "subject_kind": "person", "text": "y"})).await;

    // failed auth, and a scope denial
    let (_, headers, _) = send(&app.router, "GET", "/api/auth/me", None, &bearer("not-a-token")).await;
    let auth_trace = headers["x-trace-id"].to_str().unwrap().to_string();
    let read = token(&app, &[scopes::MEMORY_READ], None).await;
    send(&app.router, "POST", "/api/tools/memory_write", Some(json!({"subject_name": "Z", "subject_kind": "person", "text": "z"})), &bearer(&read)).await;
    send(&app.router, "GET", "/api/settings", None, &bearer(&read)).await;

    let login = events(&app, "action = 'auth.login'").await;
    assert_eq!(login.len(), 2);
    assert_eq!((login[0]["outcome"].as_str(), login[0]["actor_kind"].as_str()), (Some("auth.unauthorized"), Some("anonymous")));
    assert_eq!(login[0]["trace_id"], failed_trace);
    assert_eq!((login[1]["outcome"].as_str(), login[1]["actor_kind"].as_str()), (Some("ok"), Some("user")));

    let created = events(&app, "action = 'auth.token_create'").await;
    assert_eq!((created.len(), created[0]["actor_kind"].as_str(), created[0]["target"].as_str()), (1, Some("token"), Some(id.as_str())));
    assert_eq!(events(&app, "action = 'auth.token_revoke'").await.len(), 1);

    let tools = events(&app, "action = 'tool.memory_write' AND outcome = 'ok'").await;
    assert_eq!(tools.len(), 2);
    assert_eq!(tools[0]["trace_id"], tool_trace);
    assert!(tools.iter().all(|e| e["actor_kind"] == "token" && e["user"] == app.user.id.to_string()));
    assert!(tools[0]["detail"].as_str().unwrap().contains("Ann"));

    let failed = events(&app, "action = 'auth.failed'").await;
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0]["trace_id"], auth_trace);
    assert_eq!(failed[0]["actor_kind"], "anonymous");

    let denied = events(&app, "outcome = 'auth.scope'").await;
    assert!(denied.iter().any(|e| e["action"] == "authz.denied" && e["actor_kind"] == "token"), "route denial: {denied:?}");
    assert!(denied.iter().any(|e| e["action"] == "tool.memory_write" && e["actor_kind"] == "token"), "tool denial: {denied:?}");

    for e in events(&app, "true").await {
        let t = e["trace_id"].as_str().unwrap();
        assert!(t.len() == 32 && t.bytes().all(|b| b.is_ascii_hexdigit()), "{e}");
    }
    // the owner-facing history still lists the mutating tool calls
    let (_, _, log) = send(&app.router, "GET", "/api/audit", None, &bearer(&app.token)).await;
    assert!(log["total"].as_i64().unwrap() >= 2, "{log}");
}

#[test]
fn audit_event_is_append_only_in_the_store() {
    for stmt in eunomia_backend::store::all() {
        let sql = stmt.sql.to_lowercase();
        if sql.contains("audit_event") {
            assert!(sql.trim_start().starts_with("create audit_event"), "{} touches audit_event: {}", stmt.name, stmt.sql);
        }
    }
    // and no other code path writes to it
    for entry in std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/src")).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "rs") {
            let text = std::fs::read_to_string(&path).unwrap().to_lowercase();
            for verb in ["update audit_event", "delete audit_event", "delete from audit_event", "upsert audit_event"] {
                assert!(!text.contains(verb), "{} contains {verb:?}", path.display());
            }
        }
    }
}

// ------------------------------------------------------------ rate limits

fn app_with_limits(app: &TestApp, limits: RateConfig) -> axum::Router {
    eunomia_backend::app_with(app.state.clone(), limits)
}

#[tokio::test]
async fn login_is_limited_per_client_address_with_retry_after() {
    let app = TestApp::new().await;
    let router = app_with_limits(&app, RateConfig { auth_per_min: 3, ..RateConfig::default() });
    let creds = json!({"email": "tester@example.com", "password": "wrong-password"});
    let from = |ip: &str| vec![("x-forwarded-for", ip.to_string())];

    for _ in 0..3 {
        let (status, _, _) = send(&router, "POST", "/api/auth/login", Some(creds.clone()), &from("203.0.113.1")).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }
    let (status, headers, body) = send(&router, "POST", "/api/auth/login", Some(creds.clone()), &from("203.0.113.1")).await;
    assert_eq!((status, body["code"].as_str()), (StatusCode::TOO_MANY_REQUESTS, Some("rate.limited")));
    assert_eq!(headers[header::CONTENT_TYPE], "application/problem+json");
    assert!(headers[header::RETRY_AFTER].to_str().unwrap().parse::<u64>().unwrap() >= 1);
    assert!(body["trace_id"].is_string());
    // registration shares the auth limit; another address is unaffected
    let (status, _, _) = send(&router, "POST", "/api/auth/register", Some(creds.clone()), &from("203.0.113.1")).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    let (status, _, _) = send(&router, "POST", "/api/auth/login", Some(creds), &from("203.0.113.2")).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn forwarded_for_from_an_untrusted_peer_cannot_dodge_the_limit() {
    let app = TestApp::new().await;
    let router = app_with_limits(&app, RateConfig { auth_per_min: 2, ..RateConfig::default() });
    let creds = json!({"email": "tester@example.com", "password": "wrong-password"});
    let mut last = StatusCode::OK;
    for i in 0..3 {
        let mut req = Request::builder()
            .method("POST")
            .uri("/api/auth/login")
            .header(header::CONTENT_TYPE, "application/json")
            .header("x-forwarded-for", format!("10.1.1.{i}"))
            .body(Body::from(creds.to_string()))
            .unwrap();
        req.extensions_mut().insert(axum::extract::ConnectInfo(std::net::SocketAddr::from(([203, 0, 113, 7], 4000))));
        last = router.clone().oneshot(req).await.unwrap().status();
    }
    assert_eq!(last, StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn guessing_tokens_is_throttled_by_address() {
    let app = TestApp::new().await;
    let router = app_with_limits(&app, RateConfig { auth_per_min: 2, ..RateConfig::default() });
    let h = |t: &str| vec![("authorization", format!("Bearer {t}")), ("x-forwarded-for", "203.0.113.9".to_string())];
    for _ in 0..2 {
        let (status, _, _) = send(&router, "GET", "/api/auth/me", None, &h("guess")).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }
    let (status, _, body) = send(&router, "GET", "/api/auth/me", None, &h("guess")).await;
    assert_eq!((status, body["code"].as_str()), (StatusCode::TOO_MANY_REQUESTS, Some("rate.limited")));
    // a valid token from that address is not affected by the failed-credential bucket
    let (status, _, _) = send(&router, "GET", "/api/auth/me", None, &h(&app.token)).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn requests_are_limited_per_token_and_per_user() {
    let app = TestApp::new().await;
    let router = app_with_limits(&app, RateConfig { token_per_min: 3, user_per_min: 5, auth_per_min: 100, ..RateConfig::default() });
    let other = token(&app, scopes::ALL, None).await;

    for _ in 0..3 {
        let (status, _, _) = send(&router, "GET", "/api/auth/me", None, &bearer(&app.token)).await;
        assert_eq!(status, StatusCode::OK);
    }
    let (status, headers, body) = send(&router, "GET", "/api/auth/me", None, &bearer(&app.token)).await;
    assert_eq!((status, body["code"].as_str()), (StatusCode::TOO_MANY_REQUESTS, Some("rate.limited")));
    assert!(headers.contains_key(header::RETRY_AFTER));

    // a second token has its own bucket, but both draw on the user's
    let mut ok = 0;
    for _ in 0..4 {
        let (status, _, _) = send(&router, "GET", "/api/auth/me", None, &bearer(&other)).await;
        if status == StatusCode::OK {
            ok += 1;
        }
    }
    assert!((1..=2).contains(&ok), "user bucket (5) minus the 3+1 already spent, got {ok} ok");
}

#[tokio::test]
async fn limits_come_from_the_environment_with_self_host_defaults() {
    let d = RateConfig::default();
    assert!(d.auth_per_min >= 5 && d.auth_per_min <= d.token_per_min && d.token_per_min <= d.user_per_min);
}

// ------------------------------------------------------ route classification

#[test]
fn every_documented_route_is_classified_and_tools_have_a_scope() {
    use axum::http::Method;
    let doc = serde_json::to_value(eunomia_backend::openapi::spec()).unwrap();
    let mut unclassified = Vec::new();
    for (path, item) in doc["paths"].as_object().unwrap() {
        let concrete = path.replace(['{', '}'], "").replace("kind", "up_bank");
        for m in ["get", "post", "put", "patch", "delete"] {
            if item.get(m).is_some() {
                let method = Method::from_bytes(m.to_uppercase().as_bytes()).unwrap();
                if eunomia_backend::gate::explicit_class(&method, &concrete).is_none() {
                    unclassified.push(format!("{m} {path}"));
                }
            }
        }
    }
    assert!(unclassified.is_empty(), "add these routes to gate::explicit_class: {unclassified:?}");
}
