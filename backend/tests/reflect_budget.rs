//! Reflect, issue #75 part 4: the memories it sends fit the model's context budget, metadata
//! included, and an over-budget set is cut; a citation maps only to a memory that was in the
//! prompt, with its evidence id, vault and event time; recall-only mode obeys the same budget and
//! returns the same evidence fields.

mod common;

use common::TestApp;
use eunomia_backend::cache::reflect::{ANSWER_TOKENS, CONTEXT_TOKENS};
use eunomia_backend::embeddings::service as embeddings;
use eunomia_backend::sources::mock::{route, serve, Mock};
use serde_json::{json, Value};

const MEMORIES: usize = 10; // reflect's default limit

fn tokens(s: &str) -> usize {
    s.chars().count().div_ceil(4)
}

/// Ten memories of ~1,000 tokens each, all matching "zephyr": together far over the budget.
async fn write_long_memories(app: &TestApp) {
    for n in 0..MEMORIES {
        let text = format!("zephyr note {n} {}", "lorem ipsum ".repeat(330));
        let r = app
            .tool("memory_write", json!({"subject_name": format!("Person {n}"), "subject_kind": "person", "text": text, "occurred_at": "2025-03-01"}))
            .await;
        assert!(r.get("error").is_none(), "{r}");
    }
}

/// An embeddings and chat endpoint whose model answers citing `cited`.
async fn model(cited: &[i64]) -> Mock {
    let answer = json!({"answer": "From the notes [1].", "cited": cited}).to_string();
    serve(vec![
        route("POST", "/embeddings", json!({"data": [{"index": 0, "embedding": vec![0.01_f32; embeddings::DIM]}]})),
        route("POST", "/chat/completions", json!({"choices": [{"message": {"content": answer}}]})).body("ONLY the numbered memories"),
    ])
    .await
}

async fn with_model(cited: &[i64]) -> (TestApp, Mock) {
    let mock = model(cited).await;
    let app = TestApp::new().await;
    let (status, body) = app.http("PATCH", "/api/settings", Some(json!({"openai_base_url": mock.base})), true).await;
    assert_eq!(status, 200, "{body}");
    (app, mock)
}

/// The system and user messages reflect sent to the model.
fn prompt(mock: &Mock) -> String {
    let req = mock.requests().into_iter().rfind(|r| r.path == "/chat/completions").expect("a chat call");
    let body: Value = serde_json::from_str(&req.body).unwrap();
    body["messages"].as_array().unwrap().iter().map(|m| m["content"].as_str().unwrap()).collect::<Vec<_>>().join("\n")
}

fn memory_lines(prompt: &str) -> usize {
    prompt.lines().filter(|l| l.starts_with('[') && l.contains("zephyr note")).count()
}

#[tokio::test]
async fn the_reflect_prompt_fits_the_context_budget_and_cuts_what_does_not_fit() {
    let (app, mock) = with_model(&[1]).await;
    write_long_memories(&app).await;

    let r = app.tool("reflect", json!({"query": "zephyr"})).await;
    assert_eq!(r["answer"], "From the notes [1].", "{r}");
    let p = prompt(&mock);
    assert!(tokens(&p) + ANSWER_TOKENS <= CONTEXT_TOKENS, "prompt of {} tokens leaves no room for the answer", tokens(&p));
    let sent = memory_lines(&p);
    assert!(sent > 0 && sent < MEMORIES, "an over-budget set is cut, not dropped whole: {sent} of {MEMORIES} sent");
}

#[tokio::test]
async fn citations_map_only_to_memories_in_the_prompt_with_their_evidence() {
    // [1] is in the prompt; [10] was recalled but cut by the budget; [99] never existed
    let (app, mock) = with_model(&[1, 10, 99]).await;
    write_long_memories(&app).await;

    let r = app.tool("reflect", json!({"query": "zephyr"})).await;
    let citations = r["citations"].as_array().unwrap();
    assert_eq!(citations.len(), 1, "only [1] maps to a memory in the prompt: {r}");
    let c = &citations[0];
    assert_eq!(c["index"], 1);
    let id = c["id"].as_str().unwrap();
    assert!(id.starts_with("memory:"), "{c}");
    assert!(c["vault"].as_str().is_some_and(|v| v.starts_with("vault:")), "{c}");
    assert_eq!(c["vault_name"], "Personal");
    assert!(c["occurred_at"].as_str().unwrap().starts_with("2025-03-01"), "freshness is the event time: {c}");
    let p = prompt(&mock);
    let first = p.lines().find(|l| l.starts_with("[1] ")).expect("memory [1] in the prompt");
    assert!(first.contains(&c["text"].as_str().unwrap()[..40]), "citation [1] is the memory numbered [1]: {first}");
}

#[tokio::test]
async fn recall_only_mode_respects_the_budget_and_returns_the_same_evidence_fields() {
    let app = TestApp::new().await; // no model configured
    write_long_memories(&app).await;

    let r = app.tool("reflect", json!({"query": "zephyr"})).await;
    assert_eq!(r["mode"], "recall_only", "{r}");
    let memories = r["memories"].as_array().unwrap();
    assert!(!memories.is_empty() && memories.len() < MEMORIES, "an over-budget set is cut: {} of {MEMORIES}", memories.len());
    let payload = tokens(&r.to_string());
    assert!(payload + ANSWER_TOKENS <= CONTEXT_TOKENS, "recall-only payload of {payload} tokens is over budget");
    for (i, m) in memories.iter().enumerate() {
        assert_eq!(m["index"], i + 1);
        assert!(m["id"].as_str().unwrap().starts_with("memory:"), "{m}");
        assert!(m["vault"].as_str().unwrap().starts_with("vault:"), "{m}");
        assert!(m["occurred_at"].as_str().unwrap().starts_with("2025-03-01"), "{m}");
    }
}
