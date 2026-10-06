//! Thin bearer-token REST clients for the personal connectors. Ported from
//! `connectors/clients.py` 1:1, using `reqwest` instead of `httpx`. Google was
//! removed from this codebase before the rewrite started -- no GoogleClient
//! here.
//!
//! Each client's pure response-shaping logic (`compute_*`) is split out from
//! its HTTP-fetching method so it can be unit tested without a live network
//! call.

use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use serde_json::{json, Value};

use crate::error::{AppError, AppResult};

fn str_field(credentials: &Value, key: &str) -> String {
    credentials.get(key).and_then(|v| v.as_str()).unwrap_or("").to_string()
}

fn bearer_headers(token: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    if !token.is_empty() {
        if let Ok(value) = HeaderValue::from_str(&format!("Bearer {token}")) {
            headers.insert(AUTHORIZATION, value);
        }
    }
    headers
}

fn round_to(x: f64, places: i32) -> f64 {
    let factor = 10f64.powi(places);
    (x * factor).round() / factor
}

async fn get_status(url: &str, headers: HeaderMap, query: &[(&str, String)], timeout_secs: u64) -> AppResult<bool> {
    let resp = reqwest::Client::new()
        .get(url)
        .headers(headers)
        .query(query)
        .timeout(Duration::from_secs(timeout_secs))
        .send()
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
    Ok(resp.status().is_success())
}

async fn get_json(url: &str, headers: HeaderMap, query: &[(&str, String)], timeout_secs: u64) -> AppResult<Value> {
    let resp = reqwest::Client::new()
        .get(url)
        .headers(headers)
        .query(query)
        .timeout(Duration::from_secs(timeout_secs))
        .send()
        .await
        .map_err(|e| AppError::internal(e.to_string()))?
        .error_for_status()
        .map_err(|e| AppError::internal(e.to_string()))?;
    resp.json::<Value>().await.map_err(|e| AppError::internal(e.to_string()))
}

async fn post_json(url: &str, headers: HeaderMap, body: &Value, timeout_secs: u64) -> AppResult<Value> {
    let resp = reqwest::Client::new()
        .post(url)
        .headers(headers)
        .json(body)
        .timeout(Duration::from_secs(timeout_secs))
        .send()
        .await
        .map_err(|e| AppError::internal(e.to_string()))?
        .error_for_status()
        .map_err(|e| AppError::internal(e.to_string()))?;
    resp.json::<Value>().await.map_err(|e| AppError::internal(e.to_string()))
}

// ---------------------------------------------------------------------------
// Up Bank
// ---------------------------------------------------------------------------

pub struct UpBankClient {
    token: String,
}

impl UpBankClient {
    const BASE_URL: &'static str = "https://api.up.com.au/api/v1";

    pub fn new(credentials: &Value) -> Self {
        Self { token: str_field(credentials, "personal_access_token") }
    }

    fn headers(&self) -> HeaderMap {
        bearer_headers(&self.token)
    }

    /// Exposes the bearer headers for callers (the `sources::up_bank` sync
    /// loop) that need to follow a JSON:API `links.next` pagination URL
    /// directly -- mirrors the Python source's raw `httpx.AsyncClient().get(nxt,
    /// headers=client._headers())` pagination walk.
    pub fn auth_headers(&self) -> HeaderMap {
        self.headers()
    }

    /// Fetches an absolute URL (e.g. a JSON:API `links.next` page) with this
    /// client's bearer token attached.
    pub async fn get_absolute(&self, url: &str) -> AppResult<Value> {
        get_json(url, self.headers(), &[], 15).await
    }

    pub async fn ping(&self) -> AppResult<bool> {
        get_status(&format!("{}/util/ping", Self::BASE_URL), self.headers(), &[], 10).await
    }

    pub async fn accounts(&self) -> AppResult<Value> {
        get_json(&format!("{}/accounts", Self::BASE_URL), self.headers(), &[], 10).await
    }

    pub async fn transactions(&self, params: &[(&str, String)]) -> AppResult<Value> {
        get_json(&format!("{}/transactions", Self::BASE_URL), self.headers(), params, 10).await
    }

    pub async fn categories(&self) -> AppResult<Value> {
        get_json(&format!("{}/categories", Self::BASE_URL), self.headers(), &[], 10).await
    }

    /// Balance across accounts + settled spend broken down by category since
    /// `since_iso`. Every field here comes straight off the
    /// transaction/account/category resources -- no invented metrics
    /// (personal bank account).
    pub async fn finance_summary(&self, since_iso: &str) -> AppResult<Value> {
        let accounts = self.accounts().await?;
        let categories = self.categories().await?;
        let transactions = self
            .transactions(&[("filter[since]", since_iso.to_string()), ("page[size]", "100".to_string())])
            .await?;
        Ok(compute_finance_summary(&accounts, &categories, &transactions))
    }

    /// Settled transaction count + total spend (negative amounts) since
    /// `since_iso`.
    pub async fn week_summary(&self, since_iso: &str) -> AppResult<Value> {
        let transactions = self
            .transactions(&[("filter[since]", since_iso.to_string()), ("page[size]", "100".to_string())])
            .await?;
        Ok(compute_week_summary(&transactions))
    }
}

pub fn compute_finance_summary(accounts: &Value, categories: &Value, transactions: &Value) -> Value {
    let accounts_data: Vec<Value> = accounts.get("data").and_then(|d| d.as_array()).cloned().unwrap_or_default();

    let balance_cents: i64 = accounts_data
        .iter()
        .filter_map(|a| a.pointer("/attributes/balance/valueInBaseUnits").and_then(|v| v.as_i64()))
        .sum();

    let cat_names: std::collections::HashMap<String, String> = categories
        .get("data")
        .and_then(|d| d.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|c| {
                    let id = c.get("id")?.as_str()?.to_string();
                    let name = c.pointer("/attributes/name")?.as_str()?.to_string();
                    Some((id, name))
                })
                .collect()
        })
        .unwrap_or_default();

    let data: Vec<Value> = transactions.get("data").and_then(|d| d.as_array()).cloned().unwrap_or_default();
    let settled: Vec<&Value> = data
        .iter()
        .filter(|t| t.pointer("/attributes/status").and_then(|s| s.as_str()) == Some("SETTLED"))
        .collect();

    let mut spend_by_category: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    let mut spend_by_day: std::collections::HashMap<String, i64> = std::collections::HashMap::new();

    for t in &settled {
        let cents = t.pointer("/attributes/amount/valueInBaseUnits").and_then(|v| v.as_i64()).unwrap_or(0);
        if cents >= 0 {
            continue;
        }
        let cat_id = t.pointer("/relationships/category/data/id").and_then(|v| v.as_str());
        let name = match cat_id {
            Some(id) => cat_names.get(id).cloned().unwrap_or_else(|| "Uncategorised".to_string()),
            None => "Uncategorised".to_string(),
        };
        *spend_by_category.entry(name).or_insert(0) -= cents;

        let created_at = t.pointer("/attributes/createdAt").and_then(|v| v.as_str()).unwrap_or("");
        let day: String = created_at.chars().take(10).collect();
        *spend_by_day.entry(day).or_insert(0) -= cents;
    }

    let mut spend_by_category_vec: Vec<Value> = spend_by_category
        .into_iter()
        .map(|(k, v)| json!({"category": k, "amount": round_to(v as f64 / 100.0, 2)}))
        .collect();
    spend_by_category_vec.sort_by(|a, b| {
        let av = a["amount"].as_f64().unwrap_or(0.0);
        let bv = b["amount"].as_f64().unwrap_or(0.0);
        bv.partial_cmp(&av).unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut spend_by_day_vec: Vec<Value> = spend_by_day
        .into_iter()
        .map(|(k, v)| json!({"day": k, "amount": round_to(v as f64 / 100.0, 2)}))
        .collect();
    spend_by_day_vec.sort_by(|a, b| a["day"].as_str().unwrap_or("").cmp(b["day"].as_str().unwrap_or("")));

    let mut sorted_data = data.clone();
    sorted_data.sort_by(|a, b| {
        let a_c = a.pointer("/attributes/createdAt").and_then(|v| v.as_str()).unwrap_or("");
        let b_c = b.pointer("/attributes/createdAt").and_then(|v| v.as_str()).unwrap_or("");
        b_c.cmp(a_c)
    });
    let recent_transactions: Vec<Value> = sorted_data
        .into_iter()
        .take(20)
        .map(|t| {
            json!({
                "description": t.pointer("/attributes/description").and_then(|v| v.as_str()).unwrap_or(""),
                "amount": t.pointer("/attributes/amount/value").and_then(|v| v.as_str()).unwrap_or(""),
                "created_at": t.pointer("/attributes/createdAt").and_then(|v| v.as_str()).unwrap_or(""),
            })
        })
        .collect();

    json!({
        "balance": round_to(balance_cents as f64 / 100.0, 2),
        "accounts": accounts_data.iter().map(|a| json!({
            "name": a.pointer("/attributes/displayName").and_then(|v| v.as_str()).unwrap_or(""),
            "balance": a.pointer("/attributes/balance/value").and_then(|v| v.as_str()).unwrap_or(""),
        })).collect::<Vec<_>>(),
        "spend_by_category": spend_by_category_vec,
        "spend_by_day": spend_by_day_vec,
        "recent_transactions": recent_transactions,
    })
}

pub fn compute_week_summary(transactions: &Value) -> Value {
    let data: Vec<Value> = transactions.get("data").and_then(|d| d.as_array()).cloned().unwrap_or_default();
    let rows: Vec<&Value> = data
        .iter()
        .filter(|t| t.pointer("/attributes/status").and_then(|s| s.as_str()) == Some("SETTLED"))
        .collect();
    let spend_cents: i64 = rows
        .iter()
        .filter_map(|t| {
            let v = t.pointer("/attributes/amount/valueInBaseUnits").and_then(|v| v.as_i64())?;
            if v < 0 {
                Some(-v)
            } else {
                None
            }
        })
        .sum();
    json!({
        "transaction_count": rows.len(),
        "spent": round_to(spend_cents as f64 / 100.0, 2),
    })
}

// ---------------------------------------------------------------------------
// PocketAI (heypocket)
// ---------------------------------------------------------------------------

pub struct PocketAIClient {
    api_key: String,
    base_url: String,
}

impl PocketAIClient {
    pub const DEFAULT_BASE_URL: &'static str = "https://public.heypocketai.com/api/v1";

    pub fn new(credentials: &Value, base_url: Option<&str>) -> Self {
        Self {
            api_key: str_field(credentials, "api_key"),
            base_url: base_url.unwrap_or(Self::DEFAULT_BASE_URL).trim_end_matches('/').to_string(),
        }
    }

    fn headers(&self) -> HeaderMap {
        bearer_headers(&self.api_key)
    }

    pub async fn ping(&self) -> AppResult<bool> {
        get_status(
            &format!("{}/public/recordings", self.base_url),
            self.headers(),
            &[("limit", "1".to_string())],
            10,
        )
        .await
    }

    pub async fn recordings(&self, params: &[(&str, String)]) -> AppResult<Value> {
        get_json(&format!("{}/public/recordings", self.base_url), self.headers(), params, 10).await
    }

    pub async fn search(&self, query: &str) -> AppResult<Value> {
        post_json(&format!("{}/public/search", self.base_url), self.headers(), &json!({"query": query}), 10).await
    }

    /// Full detail for a single recording -- transcript + summarizations.
    pub async fn recording(&self, recording_id: &str) -> AppResult<Value> {
        get_json(&format!("{}/public/recordings/{recording_id}", self.base_url), self.headers(), &[], 15).await
    }

    /// Recording count/duration/tags since a given date. Every field comes
    /// straight off the recording resource (`duration`, `tags`) -- heypocket's
    /// API has no dedicated action-items/todos field, so this doesn't invent
    /// one.
    pub async fn summary(&self, since_iso_date: &str) -> AppResult<Value> {
        let recordings = self
            .recordings(&[("start_date", since_iso_date.to_string()), ("limit", "100".to_string())])
            .await?;
        Ok(compute_pocketai_summary(&recordings))
    }
}

pub fn compute_pocketai_summary(recordings: &Value) -> Value {
    let data: Vec<Value> = recordings.get("data").and_then(|d| d.as_array()).cloned().unwrap_or_default();

    let mut tag_counts: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    for r in &data {
        if let Some(tags) = r.get("tags").and_then(|t| t.as_array()) {
            for tag in tags {
                let name = tag.get("name").and_then(|v| v.as_str()).unwrap_or("untagged").to_string();
                *tag_counts.entry(name).or_insert(0) += 1;
            }
        }
    }
    let mut tag_breakdown: Vec<Value> =
        tag_counts.into_iter().map(|(k, v)| json!({"tag": k, "count": v})).collect();
    tag_breakdown.sort_by(|a, b| b["count"].as_i64().unwrap_or(0).cmp(&a["count"].as_i64().unwrap_or(0)));

    let total_duration_secs: f64 = data.iter().map(|r| r.get("duration").and_then(|v| v.as_f64()).unwrap_or(0.0)).sum();

    let mut sorted_data = data.clone();
    sorted_data.sort_by(|a, b| {
        let a_at = a.get("recording_at").and_then(|v| v.as_str()).unwrap_or("");
        let b_at = b.get("recording_at").and_then(|v| v.as_str()).unwrap_or("");
        b_at.cmp(a_at)
    });
    let recent_recordings: Vec<Value> = sorted_data
        .into_iter()
        .take(10)
        .map(|r| {
            let duration = r.get("duration").and_then(|v| v.as_f64()).unwrap_or(0.0);
            json!({
                "title": r.get("title").and_then(|v| v.as_str()).unwrap_or(""),
                "duration_minutes": round_to(duration / 60.0, 1),
                "recorded_at": r.get("recording_at").and_then(|v| v.as_str())
                    .or_else(|| r.get("created_at").and_then(|v| v.as_str())),
                "tags": r.get("tags").and_then(|t| t.as_array()).map(|arr| {
                    arr.iter().filter_map(|tag| tag.get("name").and_then(|v| v.as_str()).map(String::from)).collect::<Vec<_>>()
                }).unwrap_or_default(),
            })
        })
        .collect();

    json!({
        "recordings_count": data.len(),
        "total_duration_minutes": round_to(total_duration_secs / 60.0, 1),
        "tag_breakdown": tag_breakdown,
        "recent_recordings": recent_recordings,
    })
}

// ---------------------------------------------------------------------------
// Open Connector gateway
// ---------------------------------------------------------------------------

/// Thin proxy client for an Open Connector gateway
/// (https://github.com/oomol-lab/open-connector) -- it holds OAuth/API-key
/// credentials for many third-party apps; this just forwards named actions to
/// it, so Eunomia doesn't need a hand-rolled client per app.
///
/// `action` is "{provider}.{action_name}", e.g. "github.get_current_user".
pub struct OpenConnectorClient {
    token: String,
    base_url: String,
}

impl OpenConnectorClient {
    pub const DEFAULT_BASE_URL: &'static str = "http://localhost:3000";

    pub fn new(credentials: &Value, base_url: Option<&str>) -> Self {
        Self {
            token: str_field(credentials, "api_key"),
            base_url: base_url.unwrap_or(Self::DEFAULT_BASE_URL).trim_end_matches('/').to_string(),
        }
    }

    fn headers(&self) -> HeaderMap {
        bearer_headers(&self.token)
    }

    pub async fn ping(&self) -> AppResult<bool> {
        get_status(&format!("{}/openapi.json", self.base_url), self.headers(), &[], 10).await
    }

    pub async fn list_connections(&self) -> AppResult<Value> {
        get_json(&format!("{}/api/connections", self.base_url), self.headers(), &[], 10).await
    }

    pub async fn call_action(&self, action: &str, params: Option<&Value>) -> AppResult<Value> {
        let body = json!({"input": params.cloned().unwrap_or_else(|| json!({}))});
        post_json(&format!("{}/v1/actions/{action}", self.base_url), self.headers(), &body, 30).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bearer_headers_includes_authorization_when_token_present() {
        let headers = bearer_headers("abc123");
        assert_eq!(headers.get(AUTHORIZATION).unwrap(), "Bearer abc123");
    }

    #[test]
    fn bearer_headers_empty_when_token_absent() {
        let headers = bearer_headers("");
        assert!(headers.get(AUTHORIZATION).is_none());
    }

    #[test]
    fn open_connector_sends_no_auth_header_without_api_key() {
        let client = OpenConnectorClient::new(&json!({}), None);
        assert!(client.headers().get(AUTHORIZATION).is_none());
    }

    #[test]
    fn up_bank_client_reads_personal_access_token_from_credentials() {
        let client = UpBankClient::new(&json!({"personal_access_token": "up:yeah:abc"}));
        assert_eq!(client.headers().get(AUTHORIZATION).unwrap(), "Bearer up:yeah:abc");
    }

    #[test]
    fn pocketai_client_defaults_and_overrides_base_url() {
        let default_client = PocketAIClient::new(&json!({"api_key": "k"}), None);
        assert_eq!(default_client.base_url, PocketAIClient::DEFAULT_BASE_URL);

        let custom_client = PocketAIClient::new(&json!({"api_key": "k"}), Some("https://example.com/api/"));
        assert_eq!(custom_client.base_url, "https://example.com/api");
    }

    #[test]
    fn compute_week_summary_counts_settled_spend_only() {
        let transactions = json!({
            "data": [
                {"attributes": {"status": "SETTLED", "amount": {"valueInBaseUnits": -500}}},
                {"attributes": {"status": "SETTLED", "amount": {"valueInBaseUnits": 1000}}},
                {"attributes": {"status": "PENDING", "amount": {"valueInBaseUnits": -999}}},
            ]
        });
        let summary = compute_week_summary(&transactions);
        assert_eq!(summary["transaction_count"], 2);
        assert_eq!(summary["spent"], 5.0);
    }

    #[test]
    fn compute_finance_summary_breaks_down_spend_by_category_and_day() {
        let accounts = json!({
            "data": [
                {"attributes": {"displayName": "Spending", "balance": {"value": "100.00", "valueInBaseUnits": 10000}}},
            ]
        });
        let categories = json!({"data": [{"id": "groceries", "attributes": {"name": "Groceries"}}]});
        let transactions = json!({
            "data": [
                {
                    "attributes": {
                        "status": "SETTLED",
                        "description": "Woolworths",
                        "amount": {"value": "-50.00", "valueInBaseUnits": -5000},
                        "createdAt": "2024-01-02T10:00:00Z"
                    },
                    "relationships": {"category": {"data": {"id": "groceries"}}}
                },
                {
                    "attributes": {
                        "status": "SETTLED",
                        "description": "Mystery shop",
                        "amount": {"value": "-10.00", "valueInBaseUnits": -1000},
                        "createdAt": "2024-01-03T10:00:00Z"
                    },
                    "relationships": {}
                },
            ]
        });

        let summary = compute_finance_summary(&accounts, &categories, &transactions);
        assert_eq!(summary["balance"], 100.0);
        assert_eq!(summary["spend_by_category"][0]["category"], "Groceries");
        assert_eq!(summary["spend_by_category"][0]["amount"], 50.0);
        assert_eq!(summary["spend_by_category"][1]["category"], "Uncategorised");
        assert_eq!(summary["spend_by_day"].as_array().unwrap().len(), 2);
        assert_eq!(summary["recent_transactions"].as_array().unwrap().len(), 2);
        // Most recent first.
        assert_eq!(summary["recent_transactions"][0]["description"], "Mystery shop");
    }

    #[test]
    fn compute_pocketai_summary_tallies_tags_and_duration() {
        let recordings = json!({
            "data": [
                {"title": "Standup", "duration": 600.0, "recording_at": "2024-01-02T00:00:00Z", "tags": [{"name": "work"}]},
                {"title": "1:1", "duration": 1800.0, "recording_at": "2024-01-03T00:00:00Z", "tags": [{"name": "work"}, {"name": "1:1"}]},
            ]
        });
        let summary = compute_pocketai_summary(&recordings);
        assert_eq!(summary["recordings_count"], 2);
        assert_eq!(summary["total_duration_minutes"], 40.0);
        assert_eq!(summary["tag_breakdown"][0]["tag"], "work");
        assert_eq!(summary["tag_breakdown"][0]["count"], 2);
        assert_eq!(summary["recent_recordings"][0]["title"], "1:1");
    }
}
