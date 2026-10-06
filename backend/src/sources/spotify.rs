//! Spotify source: recently played tracks, via
//! `GET /me/player/recently-played`. Real API shape; not exercised against a
//! live account in this environment -- see `connectors::clients::SpotifyClient`.

use async_trait::async_trait;
use chrono::Utc;
use serde_json::{json, Value};

use crate::connectors::clients::SpotifyClient;
use crate::error::AppResult;
use crate::sources::base::{Source, SourceCtx, SyncResult};
use crate::sources::registry::credentials_for;

pub struct SpotifySource;

#[async_trait]
impl Source for SpotifySource {
    fn key(&self) -> &'static str {
        "spotify"
    }

    fn label(&self) -> &'static str {
        "Spotify"
    }

    fn record_types(&self) -> &'static [&'static str] {
        &["spotify.track"]
    }

    fn auth_kind(&self) -> &'static str {
        "oauth"
    }

    async fn sync(&self, ctx: &SourceCtx<'_>, _mode: &str, _cursor: Option<String>) -> AppResult<SyncResult> {
        let creds = credentials_for(ctx.db, ctx.encryption_key, ctx.owner, self).await?;
        let client = SpotifyClient::new(&creds);
        let resp = client.recently_played().await?;
        let records = resp.get("items").and_then(|v| v.as_array()).cloned().unwrap_or_default();
        Ok(SyncResult { records, cursor: Some(Utc::now().to_rfc3339()) })
    }

    fn map(&self, raw: &Value) -> Option<Value> {
        let track = &raw["track"];
        let id = track.get("id").and_then(|v| v.as_str())?;
        let name = track.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let artists: Vec<&str> =
            track.get("artists").and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|x| x.get("name").and_then(|v| v.as_str())).collect()).unwrap_or_default();
        let artist_names = artists.join(", ");
        Some(json!({
            "id": format!("spotify:spotify.track:{id}"),
            "source": "spotify",
            "type": "spotify.track",
            "external_id": id,
            "title": name,
            "body_text": format!("{name} — {artist_names}"),
            "occurred_at": raw.get("played_at"),
            "url": track.pointer("/external_urls/spotify").cloned().unwrap_or(Value::String(String::new())),
            "payload": {
                "artists": artist_names,
                "album": track.pointer("/album/name"),
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
    fn map_joins_multiple_artists() {
        let src = SpotifySource;
        let raw = json!({
            "played_at": "2024-01-01T00:00:00Z",
            "track": {
                "id": "t1", "name": "Song", "artists": [{"name": "A"}, {"name": "B"}],
                "album": {"name": "Album"}, "external_urls": {"spotify": "https://open.spotify.com/track/t1"},
            },
        });
        let env = src.map(&raw).unwrap();
        assert_eq!(env["body_text"], "Song — A, B");
        assert_eq!(env["payload"]["album"], "Album");
    }

    #[test]
    fn map_returns_none_without_track_id() {
        let src = SpotifySource;
        assert!(src.map(&json!({"track": {}})).is_none());
    }
}
