//! Stripe source: recent charges, via `GET /v1/charges`. Real API shape;
//! not exercised against a live account in this environment -- see
//! `connectors::clients::StripeClient`.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::{json, Value};

use crate::connectors::clients::StripeClient;
use crate::error::AppResult;
use crate::sources::base::{Source, SourceCtx, SyncResult};
use crate::sources::registry::credentials_for;

/// Stripe timestamps are Unix seconds, not ISO strings like every other
/// source here -- convert once at the mapping boundary.
fn unix_to_rfc3339(secs: Option<i64>) -> Option<String> {
    let secs = secs?;
    DateTime::from_timestamp(secs, 0).map(|dt| dt.to_rfc3339())
}

pub struct StripeSource;

#[async_trait]
impl Source for StripeSource {
    fn key(&self) -> &'static str {
        "stripe"
    }

    fn label(&self) -> &'static str {
        "Stripe"
    }

    fn record_types(&self) -> &'static [&'static str] {
        &["stripe.charge"]
    }

    fn auth_kind(&self) -> &'static str {
        "api_key"
    }

    async fn sync(&self, ctx: &SourceCtx<'_>, _mode: &str, _cursor: Option<String>) -> AppResult<SyncResult> {
        let creds = credentials_for(ctx.db, ctx.encryption_key, ctx.owner, self).await?;
        let client = StripeClient::new(&creds);
        let resp = client.charges().await?;
        let records = resp.get("data").and_then(|v| v.as_array()).cloned().unwrap_or_default();
        Ok(SyncResult { records, cursor: Some(Utc::now().to_rfc3339()) })
    }

    fn map(&self, raw: &Value) -> Option<Value> {
        let id = raw.get("id").and_then(|v| v.as_str())?;
        let amount_cents = raw.get("amount").and_then(|v| v.as_i64()).unwrap_or(0);
        let currency = raw.get("currency").and_then(|v| v.as_str()).unwrap_or("usd");
        let description = raw.get("description").and_then(|v| v.as_str()).unwrap_or("");
        Some(json!({
            "id": format!("stripe:stripe.charge:{id}"),
            "source": "stripe",
            "type": "stripe.charge",
            "external_id": id,
            "title": if description.is_empty() { format!("Charge {id}") } else { description.to_string() },
            "body_text": format!("{:.2} {}", amount_cents as f64 / 100.0, currency.to_uppercase()),
            "occurred_at": unix_to_rfc3339(raw.get("created").and_then(|v| v.as_i64())),
            "url": raw.get("receipt_url").cloned().unwrap_or(Value::String(String::new())),
            "payload": {
                "amount_cents": amount_cents,
                "currency": currency,
                "status": raw.get("status"),
                "paid": raw.get("paid"),
            },
            "links": [],
            "deleted": false,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unix_to_rfc3339_converts_seconds() {
        assert_eq!(unix_to_rfc3339(Some(0)).unwrap(), "1970-01-01T00:00:00+00:00");
        assert!(unix_to_rfc3339(None).is_none());
    }

    #[test]
    fn map_falls_back_to_charge_id_when_no_description() {
        let src = StripeSource;
        let raw = json!({"id": "ch_1", "amount": 2599, "currency": "usd", "created": 1700000000, "status": "succeeded"});
        let env = src.map(&raw).unwrap();
        assert_eq!(env["title"], "Charge ch_1");
        assert_eq!(env["body_text"], "25.99 USD");
    }
}
