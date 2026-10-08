//! `reflect`: synthesize an answer from recalled memories, with citations.
//!
//! One `recall` call (same vault scoping -- see `cache::recall`) feeding one
//! LLM call that must answer only from what's already there and cite it by
//! index -- no follow-up retrieval, matching `cache/reflect.py`'s deliberate
//! scope cut from Hindsight's agentic multi-round loop.
//!
//! Ported from `cache/reflect.py`.

use serde::Deserialize;
use serde_json::{json, Value};
use surrealdb::types::RecordId;

use crate::cache::recall::{self, RecallItem};
use crate::config::Settings;
use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::embeddings::service::{endpoint_configured, resolve_openai_for_owner};

fn build_prompt(query: &str, memories: &str) -> String {
    format!(
        "Answer the question using ONLY the numbered memories below. Cite the memories you used by index, \
         e.g. \"[1]\". If the memories don't contain enough to answer, say so plainly rather than guessing. \
         Return strict JSON, no prose, with this exact shape:\n\n\
         {{\"answer\": str, \"cited\": [int]}}\n\n\
         Question: {query}\n\n\
         Memories:\n{memories}\n"
    )
}

fn render_memories(items: &[RecallItem]) -> String {
    items.iter().enumerate().map(|(i, item)| format!("[{}] {}", i + 1, item.text)).collect::<Vec<_>>().join("\n")
}

/// Keeps only in-range integer indices (1-based, matching the numbered
/// memories list), same filter as `cache/reflect.py`'s `cited_indices`
/// list comprehension.
fn filter_cited_indices(cited: &[i64], items_len: usize) -> Vec<usize> {
    cited.iter().filter(|&&i| i >= 1 && (i as usize) <= items_len).map(|&i| i as usize).collect()
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}

#[derive(Debug, Deserialize)]
struct ChatMessage {
    content: String,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ReflectModel {
    #[serde(default)]
    answer: String,
    #[serde(default)]
    cited: Vec<i64>,
}

/// No server-side model: hand the recalled memories to the caller (the MCP
/// agent is the model) with the same answer-from-memories-only contract the
/// server-side prompt enforces.
fn recall_only(query: &str, items: &[RecallItem], note: Option<&str>) -> Value {
    let memories: Vec<Value> = items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let mut v = serde_json::to_value(item).unwrap_or(Value::Null);
            if let Value::Object(obj) = &mut v {
                obj.insert("index".to_string(), json!(i + 1));
            }
            v
        })
        .collect();
    json!({
        "answer": Value::Null,
        "mode": "recall_only",
        "note": note,
        "instructions": "Eunomia has no model configured, so you answer. Use ONLY the numbered memories below, \
cite them by index like [1], and say so plainly if they don't contain enough to answer.",
        "question": query,
        "memories": memories,
    })
}

pub async fn reflect(
    db: &Db,
    settings: &Settings,
    owner: &RecordId,
    query: &str,
    vault_id: Option<&RecordId>,
    limit: usize,
) -> AppResult<Value> {
    let items = recall::recall(db, settings, owner, query, None, limit, None, None, vault_id).await?;
    if items.is_empty() {
        return Ok(json!({ "answer": "No relevant memories found.", "citations": [] }));
    }

    let (base_url, api_key) = resolve_openai_for_owner(db, owner, &settings.openai_api_key, &settings.encryption_key).await?;
    if settings.embeddings_backend != "openai" || !endpoint_configured(&base_url, &api_key) {
        return Ok(recall_only(query, &items, None));
    }

    match synthesize(query, &items, &base_url, &api_key).await {
        Ok(v) => Ok(v),
        Err(e) => {
            tracing::warn!("reflect: model call failed, returning recalled memories: {}", e.message);
            Ok(recall_only(query, &items, Some(&format!("server model call failed: {}", e.message))))
        }
    }
}

async fn synthesize(query: &str, items: &[RecallItem], base_url: &str, api_key: &str) -> AppResult<Value> {
    let prompt = build_prompt(query, &render_memories(items));
    let auth_key = if api_key.is_empty() { "not-needed" } else { api_key };
    let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));

    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .bearer_auth(auth_key)
        .json(&json!({
            "model": "gpt-4o-mini",
            "response_format": {"type": "json_object"},
            "messages": [{"role": "user", "content": prompt}],
        }))
        .send()
        .await
        .map_err(|e| AppError::internal(format!("chat completion request failed: {e}")))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(AppError::internal(format!("chat completion HTTP {status}: {body}")));
    }

    let parsed: ChatResponse =
        resp.json().await.map_err(|e| AppError::internal(format!("invalid chat completion response: {e}")))?;
    let content = parsed
        .choices
        .into_iter()
        .next()
        .ok_or_else(|| AppError::internal("chat completion returned no choices"))?
        .message
        .content;
    let data: ReflectModel =
        serde_json::from_str(&content).map_err(|e| AppError::internal(format!("model did not return valid JSON: {e}")))?;

    let citations: Vec<Value> = filter_cited_indices(&data.cited, items.len())
        .into_iter()
        .map(|i| {
            let mut v = serde_json::to_value(&items[i - 1]).unwrap_or(Value::Null);
            if let Value::Object(obj) = &mut v {
                obj.insert("index".to_string(), json!(i));
            }
            v
        })
        .collect();

    Ok(json!({ "answer": data.answer, "citations": citations }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(text: &str) -> RecallItem {
        RecallItem {
            id: "x".to_string(),
            kind: "cache_record",
            text: text.to_string(),
            source: None,
            occurred_at: None,
            score: 1.0,
            arms_hit: 1,
        }
    }

    #[test]
    fn build_prompt_includes_query_and_memories() {
        let p = build_prompt("what happened?", "[1] foo");
        assert!(p.contains("what happened?"));
        assert!(p.contains("[1] foo"));
        assert!(p.contains("\"answer\""));
    }

    #[test]
    fn render_memories_numbers_from_one() {
        let items = vec![item("a"), item("b")];
        assert_eq!(render_memories(&items), "[1] a\n[2] b");
    }

    #[test]
    fn filter_cited_indices_drops_out_of_range_and_keeps_valid() {
        let cited = vec![0, 1, 2, 3, -5];
        assert_eq!(filter_cited_indices(&cited, 2), vec![1, 2]);
    }

    #[test]
    fn recall_only_numbers_memories_and_tells_the_caller_to_answer() {
        let r = recall_only("who is ada?", &[item("a"), item("b")], None);
        assert_eq!(r["mode"], "recall_only");
        assert!(r["answer"].is_null());
        assert_eq!(r["memories"][0]["index"], 1);
        assert_eq!(r["memories"][1]["text"], "b");
        assert!(r["instructions"].as_str().unwrap().contains("you answer"));
    }

    #[test]
    fn filter_cited_indices_empty_when_no_items() {
        assert_eq!(filter_cited_indices(&[1, 2], 0), Vec::<usize>::new());
    }
}
