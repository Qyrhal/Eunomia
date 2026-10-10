//! Uploaded documents end to end against a real in-memory SurrealDB with a file bucket (`memory` by
//! default; `TEST_DOCUMENTS_BACKEND=s3+http://127.0.0.1:9000/bucket` with `AWS_*` set runs the same
//! suite against an S3-compatible store, see docs/documents.md): upload -> index job -> search, recall
//! and get find the passage with its document reference -> download is byte-exact -> export -> delete
//! removes it from retrieval and storage. Plus validation, isolation, revisions and failures.

mod common;

use std::time::{Duration, Instant};

use axum::body::Body;
use axum::http::{header, HeaderMap, Request, StatusCode};
use common::TestApp;
use eunomia_backend::config::Settings;
use eunomia_backend::jobs::{handlers, worker, WorkerConfig};
use eunomia_backend::models_user;
use eunomia_backend::tools::registry;
use http_body_util::BodyExt;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tower::ServiceExt;

const PDF: &[u8] = include_bytes!("fixtures/notes.pdf");

/// A test app with offline (stub) embeddings and a worker running the job queue.
async fn app_with(settings: Settings) -> (TestApp, tokio::sync::watch::Sender<bool>) {
    let app = TestApp::with_settings(settings).await;
    let (tx, rx) = tokio::sync::watch::channel(false);
    let cfg = WorkerConfig { id: "docs".into(), poll: Duration::from_millis(20), retry_base: Duration::from_millis(10), owner_cap: 1000, ..WorkerConfig::default() };
    tokio::spawn(worker::run(app.state.clone(), handlers::registry(), cfg, rx));
    (app, tx)
}

async fn app() -> (TestApp, tokio::sync::watch::Sender<bool>) {
    app_with(Settings { embeddings_backend: "stub".into(), ..common::test_settings() }).await
}

/// A raw request (any body) as `token`; returns status, headers and body bytes.
async fn raw(app: &TestApp, method: &str, path: &str, body: Vec<u8>, content_type: Option<&str>, token: &str) -> (StatusCode, HeaderMap, Vec<u8>) {
    let mut req = Request::builder().method(method).uri(path).header(header::AUTHORIZATION, format!("Bearer {token}"));
    if let Some(ct) = content_type {
        req = req.header(header::CONTENT_TYPE, ct);
    }
    let resp = app.router.clone().oneshot(req.body(Body::from(body)).unwrap()).await.unwrap();
    let (status, headers) = (resp.status(), resp.headers().clone());
    (status, headers, resp.into_body().collect().await.unwrap().to_bytes().to_vec())
}

fn json_of(bytes: &[u8]) -> Value {
    serde_json::from_slice(bytes).unwrap_or(Value::Null)
}

async fn upload_http(app: &TestApp, filename: &str, content_type: Option<&str>, bytes: &[u8]) -> (StatusCode, Value) {
    let path = format!("/api/documents?filename={}", filename.replace(' ', "%20"));
    let (s, _, b) = raw(app, "POST", &path, bytes.to_vec(), content_type, &app.token).await;
    (s, json_of(&b))
}

/// Polls `document_get` until the document leaves `indexing`.
async fn settled(app: &TestApp, id: &str) -> Value {
    let start = Instant::now();
    loop {
        let d = app.tool("document_get", json!({"id": id})).await;
        if d["document"]["status"] != "indexing" {
            return d;
        }
        assert!(start.elapsed() < Duration::from_secs(60), "document {id} still indexing: {d}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn sha(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

async fn object_exists(app: &TestApp, path: &str) -> bool {
    let db = app.db().await;
    let mut res = db.test_raw().query("file::exists(type::file('documents', $p))").bind(("p", path.to_string())).await.unwrap();
    res.take::<Option<bool>>(0).unwrap().unwrap_or(false)
}

async fn object_path(app: &TestApp, id: &str) -> String {
    let db = app.db().await;
    let mut res = db.test_raw().query("SELECT VALUE object_path FROM ONLY type::record($id)").bind(("id", id.to_string())).await.unwrap();
    res.take::<Option<String>>(0).unwrap().unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn upload_index_retrieve_download_export_delete() {
    let (app, _stop) = app().await;
    let text = "# Project notes\n\nThe quarterly zebra migration moves the herd to the northern pasture.\nOwner: Dana Okafor.\n";
    let (status, doc) = upload_http(&app, "project-notes.md", Some("text/markdown"), text.as_bytes()).await;
    assert_eq!(status, StatusCode::OK, "{doc}");
    assert_eq!(doc["status"], "indexing");
    assert!(doc.get("error").is_none() && doc.get("code").is_none(), "a success must not read as a failed tool call: {doc}");
    assert_eq!(doc["sha256"], sha(text.as_bytes()));
    assert_eq!(doc["size_bytes"], text.len());
    let id = doc["id"].as_str().unwrap().to_string();
    assert!(id.starts_with("document:"));

    let got = settled(&app, &id).await;
    assert_eq!(got["document"]["status"], "ready", "{got}");
    assert_eq!(got["document"]["chunk_count"], 1);
    assert_eq!(got["text"], text);
    let chunk_id = got["chunks"][0]["chunk_id"].as_str().unwrap().to_string();
    assert_eq!(chunk_id, format!("documents:document.chunk:{}:1:0", id.trim_start_matches("document:")));
    assert_eq!(got["chunks"][0]["char_end"], text.chars().count());
    assert_eq!(got["chunks"][0]["has_embedding"], true);

    // keyword and semantic search find the passage and name its document
    for mode in ["keyword", "semantic", "hybrid"] {
        let hits = app.tool("search", json!({"query": "zebra migration", "mode": mode})).await;
        let hit = hits["results"].as_array().unwrap().iter().find(|h| h["id"] == chunk_id).unwrap_or_else(|| panic!("{mode}: {hits}"));
        assert_eq!(hit["document"]["document_id"], id);
        assert_eq!(hit["document"]["chunk_id"], chunk_id);
        assert_eq!(hit["document"]["revision"], 1);
        assert_eq!(hit["document"]["download_url"], format!("/api/documents/{id}/download"));
    }
    // recall too
    let recalled = app.tool("recall", json!({"query": "zebra migration pasture"})).await;
    let hit = recalled["results"].as_array().unwrap().iter().find(|h| h["id"] == chunk_id).unwrap_or_else(|| panic!("{recalled}"));
    assert_eq!(hit["document"]["document_id"], id);
    assert_eq!(hit["document"]["char_start"], 0);
    // and get resolves the chunk to its document
    let rec = app.tool("get", json!({"id": chunk_id})).await;
    assert_eq!(rec["document"]["document_id"], id, "{rec}");
    assert_eq!(rec["document"]["status"], "ready");
    assert_eq!(rec["document"]["sha256"], sha(text.as_bytes()));

    // the original bytes, exactly, as an attachment; audited without content
    let (s, h, bytes) = raw(&app, "GET", &format!("/api/documents/{id}/download"), vec![], None, &app.token).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(bytes, text.as_bytes());
    assert_eq!(sha(&bytes), doc["sha256"].as_str().unwrap());
    assert_eq!(h[header::CONTENT_TYPE], "text/markdown; charset=utf-8");
    assert!(h[header::CONTENT_DISPOSITION].to_str().unwrap().starts_with("attachment; filename=\"project-notes.md\""));
    let dl = app.tool("document_download", json!({"id": id})).await;
    assert_eq!(dl["download"]["path"], format!("/api/documents/{id}/download"));
    assert_eq!(dl["sha256"], doc["sha256"]);

    // export: tool without vectors, HTTP with them
    let m = app.tool("document_export", json!({"id": id})).await;
    assert_eq!(m["format"], "eunomia.document-export");
    assert_eq!(m["embedding"]["dimension"], 1536);
    assert_eq!(m["embedding"]["model"], "stub");
    assert_eq!(m["chunks"][0]["embedding_state"], "present");
    assert!(m["chunks"][0].get("embedding").is_none());
    assert_eq!(m["vectors_url"]["path"], format!("/api/documents/{id}/export"));
    let (s, h, body) = raw(&app, "GET", &format!("/api/documents/{id}/export"), vec![], None, &app.token).await;
    assert_eq!(s, StatusCode::OK);
    assert!(h[header::CONTENT_DISPOSITION].to_str().unwrap().contains("project-notes.md.export.json"));
    let full = json_of(&body);
    assert_eq!(full["chunks"][0]["embedding"].as_array().unwrap().len(), 1536);
    assert_eq!(full["chunks"][0]["text"], text);

    let audit = app.http("GET", "/api/audit?limit=50", None, true).await.1;
    let rows = audit.to_string();
    for action in ["document_upload", "document_download", "document_export"] {
        assert!(rows.contains(action), "{action} not audited: {rows}");
    }
    assert!(!rows.contains("zebra"), "document content reached the audit log: {rows}");
    let events = app.control().test_raw().query("SELECT action, target, detail FROM audit_event").await.unwrap().take::<Vec<Value>>(0).unwrap();
    assert!(!serde_json::to_string(&events).unwrap().contains("zebra"));

    // delete: gone from retrieval, storage and the list at once
    let path = object_path(&app, &id).await;
    assert!(object_exists(&app, &path).await);
    let (s, _, body) = raw(&app, "DELETE", &format!("/api/documents/{id}"), vec![], None, &app.token).await;
    assert_eq!(s, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    assert_eq!(json_of(&body)["chunks"], 1);
    assert!(!object_exists(&app, &path).await);
    let hits = app.tool("search", json!({"query": "zebra migration", "mode": "keyword"})).await;
    assert_eq!(hits["results"], json!([]), "{hits}");
    let recalled = app.tool("recall", json!({"query": "zebra migration pasture"})).await;
    assert!(!recalled.to_string().contains(&chunk_id));
    assert_eq!(app.tool("get", json!({"id": chunk_id})).await["error"], "not found");
    let (s, _, body) = raw(&app, "GET", &format!("/api/documents/{id}/download"), vec![], None, &app.token).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    assert_eq!(json_of(&body)["code"], "document.not_found");
    assert_eq!(app.tool("document_list", json!({})).await["total"], 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn uploads_are_validated() {
    let (app, _stop) = app().await;
    let cases: Vec<(&str, Option<&str>, Vec<u8>, StatusCode, &str)> = vec![
        ("big.txt", Some("text/plain"), vec![b'a'; 300 * 1024], StatusCode::PAYLOAD_TOO_LARGE, "document.too_large"),
        ("tool.exe", Some("application/x-msdownload"), b"MZ".to_vec(), StatusCode::UNSUPPORTED_MEDIA_TYPE, "document.unsupported_type"),
        ("sheet.docx", None, b"PK".to_vec(), StatusCode::UNSUPPORTED_MEDIA_TYPE, "document.unsupported_type"),
        ("bin.txt", Some("text/plain"), vec![0xff, 0xfe, 0x00], StatusCode::BAD_REQUEST, "validation.invalid"),
        ("bad.json", Some("application/json"), b"{not json".to_vec(), StatusCode::BAD_REQUEST, "validation.invalid"),
        ("fake.pdf", Some("application/pdf"), b"hello".to_vec(), StatusCode::BAD_REQUEST, "validation.invalid"),
        ("empty.txt", Some("text/plain"), vec![], StatusCode::BAD_REQUEST, "validation.invalid"),
    ];
    for (name, ct, body, want, code) in cases {
        let (s, out) = upload_http(&app, name, ct, &body).await;
        assert_eq!((s, out["code"].as_str().unwrap_or_default()), (want, code), "{name}: {out}");
    }
    // a body past even the route's ceiling is refused before it is read
    let (s, _) = upload_http(&app, "huge.txt", Some("text/plain"), &vec![b'a'; 2 << 20]).await;
    assert_eq!(s, StatusCode::PAYLOAD_TOO_LARGE);
    let (s, _, out) = raw(&app, "POST", "/api/documents", b"x".to_vec(), Some("text/plain"), &app.token).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "no filename: {}", String::from_utf8_lossy(&out));

    // the tool: bad base64, both inputs, neither
    let bad = app.tool("document_upload", json!({"filename": "a.pdf", "content_base64": "%%%"})).await;
    assert!(bad["error"].as_str().unwrap().contains("base64"), "{bad}");
    let both = app.tool("document_upload", json!({"filename": "a.txt", "text": "x", "content_base64": "eA=="})).await;
    assert!(both["error"].as_str().unwrap().contains("exactly one"), "{both}");
    let tool_big = app.tool("document_upload", json!({"filename": "a.txt", "text": "a".repeat(300 * 1024)})).await;
    assert_eq!(tool_big["code"], "document.too_large");
    // nothing was stored
    assert_eq!(app.tool("document_list", json!({})).await["total"], 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn another_user_or_org_cannot_reach_a_document() {
    let (app, _stop) = app().await;
    let (_, doc) = upload_http(&app, "secret.txt", Some("text/plain"), b"the vault combination is 4-8-15").await;
    let id = doc["id"].as_str().unwrap().to_string();
    settled(&app, &id).await;
    let chunk = format!("documents:document.chunk:{}:1:0", id.trim_start_matches("document:"));

    // a second user in the same org, and a user in an org of their own
    let colleague = common::register(&app.state, "colleague@example.com").await;
    let stranger = common::register_personal(&app.state, "stranger@example.com").await;
    for other in [&colleague, &stranger] {
        let token = models_user::create_api_token(&app.state.control, &other.id, "t").await.unwrap().token;
        for (method, path) in [
            ("GET", format!("/api/documents/{id}")),
            ("GET", format!("/api/documents/{id}/download")),
            ("GET", format!("/api/documents/{id}/export")),
            ("POST", format!("/api/documents/{id}/reindex")),
            ("DELETE", format!("/api/documents/{id}")),
        ] {
            let (s, _, body) = raw(&app, method, &path, vec![], None, &token).await;
            assert_eq!(s, StatusCode::NOT_FOUND, "{} {method} {path}: {}", other.email, String::from_utf8_lossy(&body));
            assert!(!String::from_utf8_lossy(&body).contains("combination"));
        }
        let listed = raw(&app, "GET", "/api/documents", vec![], None, &token).await;
        assert_eq!(json_of(&listed.2)["total"], 0);
        for tool in ["document_get", "document_download", "document_export", "document_delete"] {
            let out = common::sys(registry::call(&app.state, other, tool, json!({"id": id}))).await.unwrap();
            assert_eq!(out["code"], "document.not_found", "{tool}: {out}");
        }
        let replaced = common::sys(registry::call(&app.state, other, "document_upload", json!({"filename": "x.txt", "text": "mine now", "document_id": id}))).await.unwrap();
        assert_eq!(replaced["code"], "document.not_found", "{replaced}");
        let hits = common::sys(registry::call(&app.state, other, "search", json!({"query": "vault combination"}))).await.unwrap();
        assert!(!hits.to_string().contains("combination"), "{hits}");
        let got = common::sys(registry::call(&app.state, other, "get", json!({"id": chunk}))).await.unwrap();
        assert_eq!(got["error"], "not found");
    }
    // guessed and malformed ids
    for guess in ["document:aaaaaaaaaaaaaaaaaaaa", "memory:abc", "document:../x", "x"] {
        assert_eq!(app.tool("document_get", json!({"id": guess})).await["code"], "document.not_found", "{guess}");
    }
    // a token restricted to an org vault cannot reach personal-vault documents
    let vault = app.tool("vault_create", json!({"name": "Team", "kind": "org"})).await;
    let vid = eunomia_backend::rid::parse(vault["id"].as_str().unwrap()).unwrap();
    let scopes: Vec<String> = ["memory:read", "memory:write"].iter().map(|s| s.to_string()).collect();
    let restricted = models_user::create_api_token_with(&app.state.control, &app.user.id, "r", &scopes, Some(&vid), None).await.unwrap().token;
    let (s, _, _) = raw(&app, "GET", &format!("/api/documents/{id}/download"), vec![], None, &restricted).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    // a read-only token cannot upload or delete
    let read_only = models_user::create_api_token_with(&app.state.control, &app.user.id, "ro", &["memory:read".to_string()], None, None).await.unwrap().token;
    let (s, _, _) = raw(&app, "DELETE", &format!("/api/documents/{id}"), vec![], None, &read_only).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _, _) = raw(&app, "GET", &format!("/api/documents/{id}/download"), vec![], None, &read_only).await;
    assert_eq!(s, StatusCode::OK);
    // the owner still has it, unchanged
    assert_eq!(settled(&app, &id).await["document"]["status"], "ready");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_new_revision_retires_the_old_chunks() {
    let (app, _stop) = app().await;
    let v1 = app.tool("document_upload", json!({"filename": "plan.md", "text": "Version one mentions the zebra crossing."})).await;
    let id = v1["id"].as_str().unwrap().to_string();
    settled(&app, &id).await;
    let old_path = object_path(&app, &id).await;

    let v2 = app.tool("document_upload", json!({"filename": "plan.md", "text": "Version two mentions the yak pasture instead.", "document_id": id})).await;
    assert_eq!(v2["id"], id, "{v2}");
    assert!(v2.get("code").is_none(), "{v2}");
    assert_eq!(v2["revision"], 2);
    let got = settled(&app, &id).await;
    assert_eq!(got["document"]["status"], "ready");
    assert!(got["chunks"][0]["chunk_id"].as_str().unwrap().ends_with(":2:0"));
    assert_eq!(app.tool("search", json!({"query": "zebra crossing", "mode": "keyword"})).await["results"], json!([]));
    let hits = app.tool("search", json!({"query": "yak pasture", "mode": "keyword"})).await;
    assert_eq!(hits["results"][0]["document"]["revision"], 2, "{hits}");
    assert!(!object_exists(&app, &old_path).await, "the replaced file is removed");
    let (_, _, bytes) = raw(&app, "GET", &format!("/api/documents/{id}/download"), vec![], None, &app.token).await;
    assert_eq!(bytes, b"Version two mentions the yak pasture instead.");

    // re-index: a new revision of the same bytes, the same text found under it, nothing stale left
    let (s, _, body) = raw(&app, "POST", &format!("/api/documents/{id}/reindex"), vec![], None, &app.token).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(json_of(&body)["revision"], 3);
    let got = settled(&app, &id).await;
    assert_eq!(got["document"]["revision"], 3);
    let hits = app.tool("search", json!({"query": "yak pasture", "mode": "keyword"})).await;
    assert_eq!(hits["results"].as_array().unwrap().len(), 1, "{hits}");
    assert_eq!(hits["results"][0]["document"]["revision"], 3);
    let db = app.db().await;
    let n: Option<i64> = db.test_raw().query("SELECT count() FROM cache_record WHERE source = 'documents' GROUP ALL").await.unwrap().take("count").unwrap();
    assert_eq!(n, Some(1), "stale chunks left");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pdfs_long_texts_and_failures() {
    use base64::Engine;
    let (app, _stop) = app().await;
    // a text-based PDF through the tool
    let b64 = base64::engine::general_purpose::STANDARD.encode(PDF);
    let pdf = app.tool("document_upload", json!({"filename": "notes.pdf", "content_base64": b64})).await;
    assert_eq!(pdf["media_type"], "application/pdf", "{pdf}");
    let got = settled(&app, pdf["id"].as_str().unwrap()).await;
    assert_eq!(got["document"]["status"], "ready", "{got}");
    assert!(got["text"].as_str().unwrap().contains("quarterly zebra migration plan"), "{got}");
    let hits = app.tool("search", json!({"query": "zebra migration plan", "mode": "keyword"})).await;
    assert_eq!(hits["results"][0]["document"]["filename"], "notes.pdf", "{hits}");
    let (_, _, bytes) = raw(&app, "GET", &format!("/api/documents/{}/download", pdf["id"].as_str().unwrap()), vec![], None, &app.token).await;
    assert_eq!(bytes, PDF);

    // a long text: bounded chunks that rebuild the text exactly
    let long: String = (0..2000).map(|i| format!("Line {i}: notes about the harbour project.\n")).collect();
    let doc = app.tool("document_upload", json!({"filename": "long.txt", "text": long})).await;
    let got = app.tool("document_get", json!({"id": doc["id"], "max_chars": 1_000_000})).await;
    let got = if got["document"]["status"] == "indexing" { settled(&app, doc["id"].as_str().unwrap()).await } else { got };
    let got = app.tool("document_get", json!({"id": got["document"]["id"], "max_chars": 1_000_000})).await;
    let n = got["document"]["chunk_count"].as_i64().unwrap();
    assert!(n > 10, "{n} chunks");
    assert_eq!(got["text"].as_str().unwrap(), long);
    let chunks = got["chunks"].as_array().unwrap();
    for (i, c) in chunks.iter().enumerate() {
        assert_eq!(c["part"], i);
        assert!(c["char_end"].as_i64().unwrap() - c["char_start"].as_i64().unwrap() <= 6000);
    }
    let short = app.tool("document_get", json!({"id": doc["id"], "max_chars": 100})).await;
    assert_eq!((short["text"].as_str().unwrap().chars().count(), short["text_truncated"].clone()), (100, json!(true)));

    // a PDF that cannot be parsed: failed with a reason, the original still downloadable
    let broken = b"%PDF-1.4\nthis is not really a pdf\n%%EOF\n".to_vec();
    let (s, doc) = upload_http(&app, "broken.pdf", Some("application/pdf"), &broken).await;
    assert_eq!(s, StatusCode::OK, "{doc}");
    let got = settled(&app, doc["id"].as_str().unwrap()).await;
    assert_eq!(got["document"]["status"], "failed", "{got}");
    assert!(got["document"]["error"].as_str().unwrap().contains("PDF"), "{got}");
    let (s, _, bytes) = raw(&app, "GET", &format!("/api/documents/{}/download", doc["id"].as_str().unwrap()), vec![], None, &app.token).await;
    assert_eq!((s, bytes), (StatusCode::OK, broken));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn without_embeddings_keyword_search_works_and_vectors_are_explicitly_missing() {
    // the default test settings: an OpenAI backend with no key, so nothing is embedded
    let (app, _stop) = app_with(common::test_settings()).await;
    let doc = app.tool("document_upload", json!({"filename": "n.txt", "text": "The lighthouse keeper logs every storm."})).await;
    let id = doc["id"].as_str().unwrap().to_string();
    assert_eq!(settled(&app, &id).await["chunks"][0]["has_embedding"], false);
    let hits = app.tool("search", json!({"query": "lighthouse storm", "mode": "keyword"})).await;
    assert_eq!(hits["results"][0]["document"]["document_id"], id, "{hits}");
    let (_, _, body) = raw(&app, "GET", &format!("/api/documents/{id}/export"), vec![], None, &app.token).await;
    let m = json_of(&body);
    assert_eq!(m["chunks"][0]["embedding_state"], "missing");
    assert_eq!(m["chunks"][0]["embedding"], Value::Null);
    assert_eq!(m["embedding"]["chunks_missing"], 1);
}

#[tokio::test]
async fn storage_off_is_a_clear_error_and_the_rest_works() {
    let app = TestApp::with_settings(Settings { documents_backend: String::new(), ..common::test_settings() }).await;
    let (s, out) = upload_http(&app, "a.txt", Some("text/plain"), b"hello").await;
    assert_eq!((s, out["code"].as_str().unwrap()), (StatusCode::SERVICE_UNAVAILABLE, "document.storage_unavailable"), "{out}");
    assert!(out["detail"].as_str().unwrap().contains("EUNOMIA_DOCUMENTS_BACKEND"));
    assert_eq!(app.tool("document_list", json!({})).await["total"], 0);
    // and the browser session works the same way as tokens
    let (s, body) = app.http_session("GET", "/api/documents", None).await;
    assert_eq!((s, body["total"].clone()), (StatusCode::OK, json!(0)));
}

/// Against a real S3-compatible store. Runs only with `TEST_DOCUMENTS_S3` set to a bucket URL such as
/// `s3+http://127.0.0.1:9000/eunomia-docs?prefix=test` and `AWS_ACCESS_KEY_ID`/`AWS_SECRET_ACCESS_KEY`
/// in the environment (the database engine reads them, never the URL). See docs/documents.md.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn s3_backend_round_trip() {
    let Ok(url) = std::env::var("TEST_DOCUMENTS_S3") else {
        eprintln!("skipped: TEST_DOCUMENTS_S3 is not set");
        return;
    };
    let (app, _stop) = app_with(Settings { embeddings_backend: "stub".into(), documents_backend: url, ..common::test_settings() }).await;
    let text = "S3 round trip: the walrus ledger balances on Tuesdays.\n".repeat(200);
    let (s, doc) = upload_http(&app, "ledger.txt", Some("text/plain"), text.as_bytes()).await;
    assert_eq!(s, StatusCode::OK, "{doc}");
    let id = doc["id"].as_str().unwrap().to_string();
    assert_eq!(settled(&app, &id).await["document"]["status"], "ready");
    let path = object_path(&app, &id).await;
    assert!(object_exists(&app, &path).await);
    let (_, _, bytes) = raw(&app, "GET", &format!("/api/documents/{id}/download"), vec![], None, &app.token).await;
    assert_eq!(sha(&bytes), sha(text.as_bytes()));
    let (s, pdf) = upload_http(&app, "notes.pdf", Some("application/pdf"), PDF).await;
    assert_eq!(s, StatusCode::OK);
    let (_, _, bytes) = raw(&app, "GET", &format!("/api/documents/{}/download", pdf["id"].as_str().unwrap()), vec![], None, &app.token).await;
    assert_eq!(bytes, PDF);
    let hits = app.tool("search", json!({"query": "walrus ledger", "mode": "keyword"})).await;
    assert_eq!(hits["results"][0]["document"]["document_id"], id);
    let (s, _, _) = raw(&app, "DELETE", &format!("/api/documents/{id}"), vec![], None, &app.token).await;
    assert_eq!(s, StatusCode::OK);
    assert!(!object_exists(&app, &path).await);
}
