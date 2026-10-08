//! Where a model request goes and which key it carries -- the one resolver
//! behind settings' model list, embeddings, chat, reflect, entity
//! extraction and observation consolidation.
//!
//! Credentials are bound to a destination:
//!   - the server-wide `OPENAI_API_KEY` is only ever sent to the
//!     server-configured `OPENAI_BASE_URL`;
//!   - a user's own key goes to the base URL they configured;
//!   - a user-chosen base URL without a key of their own gets no key.
//!
//! [`client`] never follows redirects, so a 3xx can't forward the
//! `Authorization` header to a host this policy didn't pick.

use reqwest::redirect::Policy;
use reqwest::Url;
use serde::Deserialize;
use surrealdb::RecordId;

use crate::config::Settings;
use crate::connectors::crypto;
use crate::db::Db;
use crate::error::AppResult;

pub const OPENAI_BASE_URL: &str = "https://api.openai.com/v1";

#[derive(Debug, Clone, PartialEq)]
pub struct Provider {
    pub base_url: String,
    pub api_key: String,
    /// The user's `embedding_model` setting ("" = default).
    pub embedding_model: String,
}

impl Provider {
    /// Enough to make a real call: a key, or a non-OpenAI base URL (a
    /// self-hosted server may not need one).
    pub fn configured(&self) -> bool {
        !self.api_key.is_empty() || !same_url(&self.base_url, OPENAI_BASE_URL)
    }

    pub fn url(&self, path: &str) -> String {
        format!("{}/{path}", self.base_url.trim_end_matches('/'))
    }

    pub fn bearer(&self) -> &str {
        if self.api_key.is_empty() { "not-needed" } else { &self.api_key }
    }
}

/// HTTP client for model providers: no redirects (see module docs).
pub fn client() -> reqwest::Client {
    reqwest::Client::builder().redirect(Policy::none()).build().expect("static reqwest config")
}

/// Same endpoint, ignoring a trailing slash and host case.
pub fn same_url(a: &str, b: &str) -> bool {
    match (Url::parse(a.trim_end_matches('/')), Url::parse(b.trim_end_matches('/'))) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// The base URL a user's settings point at ("" = the server's).
pub fn effective_base_url<'a>(server_base: &'a str, user_base: &'a str) -> &'a str {
    if user_base.trim().is_empty() { server_base } else { user_base.trim() }
}

/// `(base_url, api_key)` -- the credential-binding rule from the module docs.
fn pick(server_base: &str, server_key: Option<&str>, user_base: &str, user_key: &str) -> (String, String) {
    let base = effective_base_url(server_base, user_base).to_string();
    if !user_key.is_empty() {
        (base, user_key.to_string())
    } else if same_url(&base, server_base) {
        (server_base.to_string(), server_key.unwrap_or_default().to_string())
    } else {
        (base, String::new())
    }
}

/// The server's own provider (no per-user settings).
pub fn server(settings: &Settings) -> Provider {
    let (base_url, api_key) = pick(&settings.openai_base_url, settings.openai_api_key.as_deref(), "", "");
    Provider { base_url, api_key, embedding_model: String::new() }
}

#[derive(Debug, Default, Deserialize)]
struct Row {
    #[serde(default)]
    openai_base_url: String,
    #[serde(default)]
    openai_api_key_encrypted: String,
    #[serde(default)]
    embedding_model: String,
}

/// `owner`'s provider. A stored key that can't be decrypted is an error --
/// never sent anywhere.
pub async fn resolve(db: &Db, settings: &Settings, owner: &RecordId) -> AppResult<Provider> {
    let row: Option<Row> = db.select(RecordId::from_table_key("app_settings", owner.key().clone())).await?;
    let row = row.unwrap_or_default();
    let user_key = crypto::decrypt_or_plaintext(&settings.encryption_key, &row.openai_api_key_encrypted)?;
    let (base_url, api_key) =
        pick(&settings.openai_base_url, settings.openai_api_key.as_deref(), &row.openai_base_url, &user_key);
    Ok(Provider { base_url, api_key, embedding_model: row.embedding_model })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SERVER: &str = "https://api.openai.com/v1";
    const SERVER_KEY: Option<&str> = Some("sk-server");

    #[test]
    fn a_custom_url_without_a_user_key_never_gets_the_server_key() {
        let (base, key) = pick(SERVER, SERVER_KEY, "https://evil.example/v1", "");
        assert_eq!(base, "https://evil.example/v1");
        assert_eq!(key, "");
    }

    #[test]
    fn the_server_key_goes_only_to_the_server_url() {
        assert_eq!(pick(SERVER, SERVER_KEY, "", ""), (SERVER.to_string(), "sk-server".to_string()));
        assert_eq!(pick(SERVER, SERVER_KEY, "https://API.openai.com/v1/", "").1, "sk-server");
        // a lookalike path or host is a different destination
        assert_eq!(pick(SERVER, SERVER_KEY, "https://api.openai.com/v1/../evil", "").1, "");
        assert_eq!(pick(SERVER, SERVER_KEY, "https://api.openai.com.evil.example/v1", "").1, "");
        assert_eq!(pick(SERVER, SERVER_KEY, "http://api.openai.com/v1", "").1, "");
    }

    #[test]
    fn a_custom_server_url_is_honoured() {
        let (base, key) = pick("http://llm.internal:8080/v1", SERVER_KEY, "", "");
        assert_eq!((base.as_str(), key.as_str()), ("http://llm.internal:8080/v1", "sk-server"));
    }

    #[test]
    fn a_user_key_goes_to_the_users_own_url() {
        assert_eq!(
            pick(SERVER, SERVER_KEY, "http://localhost:11434/v1", "sk-user"),
            ("http://localhost:11434/v1".to_string(), "sk-user".to_string())
        );
        assert_eq!(pick(SERVER, SERVER_KEY, "", "sk-user"), (SERVER.to_string(), "sk-user".to_string()));
    }

    #[test]
    fn configured_needs_a_key_only_for_openai() {
        let p = |base: &str, key: &str| Provider { base_url: base.into(), api_key: key.into(), embedding_model: String::new() };
        assert!(!p(OPENAI_BASE_URL, "").configured());
        assert!(p(OPENAI_BASE_URL, "sk-abc").configured());
        assert!(p("http://localhost:11434/v1", "").configured());
    }

    #[tokio::test]
    async fn the_client_does_not_follow_redirects() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            // answer every connection with a redirect to the same server; a
            // client that followed it would make a second request
            let mut hits = 0;
            loop {
                let (mut sock, _) = listener.accept().await.unwrap();
                hits += 1;
                let mut buf = [0u8; 4096];
                let _ = sock.read(&mut buf).await;
                let resp = format!(
                    "HTTP/1.1 307 Temporary Redirect\r\nLocation: http://{addr}/elsewhere\r\nX-Hits: {hits}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                );
                let _ = sock.write_all(resp.as_bytes()).await;
            }
        });
        let resp = client().get(format!("http://{addr}/v1/models")).bearer_auth("sk-user").send().await.unwrap();
        assert_eq!(resp.status(), 307);
        assert_eq!(resp.headers()["x-hits"], "1");
    }
}
