//! Client ID Metadata Documents (draft-ietf-oauth-client-id-metadata-document):
//! an `https://` URL used as `client_id` that serves the client's metadata.
//! The URL is attacker-chosen, so the fetch is an SSRF surface: https only, no
//! IP literals, every resolved address must be public, the connection is
//! pinned to the address we checked, no redirects, small body, short timeout.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::Duration;

use reqwest::Url;
use serde_json::Value;

const MAX_BODY: usize = 64 * 1024;
const TIMEOUT: Duration = Duration::from_secs(5);
const DEFAULT_TTL: Duration = Duration::from_secs(3600);
const MIN_TTL: Duration = Duration::from_secs(300);
const MAX_TTL: Duration = Duration::from_secs(86400);

/// What we keep from a client's metadata (also the shape of a DCR registration).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientMeta {
    pub name: String,
    pub logo_uri: Option<String>,
    pub client_uri: Option<String>,
    pub redirect_uris: Vec<String>,
}

fn v4_blocked(ip: Ipv4Addr) -> bool {
    let o = ip.octets();
    ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.is_broadcast()
        || ip.is_multicast()
        || ip.is_documentation()
        || o[0] == 0
        || (o[0] == 100 && (64..128).contains(&o[1])) // CGNAT 100.64/10
        || (o[0] == 192 && o[1] == 0 && o[2] == 0) // 192.0.0.0/24
        || (o[0] == 198 && (o[1] == 18 || o[1] == 19)) // benchmarking
        || o[0] >= 240 // reserved
}

fn v6_blocked(ip: Ipv6Addr) -> bool {
    if let Some(v4) = ip.to_ipv4_mapped() {
        return v4_blocked(v4);
    }
    let s = ip.segments();
    ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        || (s[0] & 0xfe00) == 0xfc00 // unique local
        || (s[0] & 0xffc0) == 0xfe80 // link local
        || (s[0] == 0x2001 && s[1] == 0x0db8) // documentation
        || s[0] == 0x2002 // 6to4 embeds an arbitrary v4
        || (s[0] == 0x0064 && s[1] == 0xff9b) // NAT64 embeds an arbitrary v4
}

/// True for any address a server-side fetch must never reach.
pub fn ip_blocked(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => v4_blocked(v),
        IpAddr::V6(v) => v6_blocked(v),
    }
}

/// A `client_id` that is a CIMD URL: https, a host name (not an IP literal), a real path, no fragment or credentials.
pub fn is_cimd_client_id(s: &str) -> bool {
    s.starts_with("https://")
}

pub fn validate_client_id_url(s: &str) -> Result<Url, String> {
    let url = Url::parse(s).map_err(|_| "client_id is not a valid URL".to_string())?;
    if url.scheme() != "https" {
        return Err("client_id URL must use https".into());
    }
    if !url.username().is_empty() || url.password().is_some() || url.fragment().is_some() {
        return Err("client_id URL must not contain credentials or a fragment".into());
    }
    match url.host() {
        Some(url::Host::Domain(d)) if d.contains('.') => {}
        _ => return Err("client_id URL needs a public host name".into()),
    }
    if url.path().len() <= 1 {
        return Err("client_id URL must contain a path".into());
    }
    if url.path_segments().is_some_and(|mut p| p.any(|seg| seg == "." || seg == "..")) {
        return Err("client_id URL must not contain dot segments".into());
    }
    if url.as_str() != s {
        return Err("client_id URL must be in normalised form".into());
    }
    Ok(url)
}

/// Validate a redirect URI for registration (CIMD or DCR): https, http on a
/// loopback host, or a private-use scheme for native apps (RFC 8252 7.1).
pub fn validate_redirect_uri(s: &str) -> Result<(), String> {
    let url = Url::parse(s).map_err(|_| format!("redirect_uri {s:?} is not a valid URI"))?;
    if url.fragment().is_some() {
        return Err("redirect_uri must not contain a fragment".into());
    }
    match url.scheme() {
        "https" if url.host_str().is_none() || !url.username().is_empty() || url.password().is_some() => {
            return Err("https redirect_uri needs a host and no credentials".into());
        }
        "http" if !is_loopback_host(&url) => {
            return Err("http redirect_uri is only allowed for localhost, 127.0.0.1 or [::1]".into());
        }
        "javascript" | "data" | "file" | "vbscript" | "about" | "blob" | "ftp" | "ws" | "wss" => {
            return Err("redirect_uri scheme is not allowed".into());
        }
        _ => {} // private-use scheme (cursor://, vscode://, ...)
    }
    Ok(())
}

pub fn is_loopback_host(url: &Url) -> bool {
    matches!(url.scheme(), "http")
        && match url.host() {
            Some(url::Host::Domain(d)) => d == "localhost",
            Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
            Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
            None => false,
        }
}

/// Parse and validate a metadata document fetched from `client_id`.
pub fn parse_metadata(client_id: &str, doc: &Value) -> Result<ClientMeta, String> {
    if doc.get("client_id").and_then(Value::as_str) != Some(client_id) {
        return Err("metadata client_id does not match the document URL".into());
    }
    if let Some(m) = doc.get("token_endpoint_auth_method").and_then(Value::as_str)
        && m != "none"
    {
        return Err("only token_endpoint_auth_method none is supported".into());
    }
    let redirect_uris = redirect_uris_from(doc)?;
    let host = Url::parse(client_id).ok().and_then(|u| u.host_str().map(String::from)).unwrap_or_default();
    Ok(ClientMeta {
        name: clean_name(doc.get("client_name").and_then(Value::as_str)).unwrap_or(host),
        logo_uri: https_only(doc.get("logo_uri")),
        client_uri: https_only(doc.get("client_uri")),
        redirect_uris,
    })
}

pub fn redirect_uris_from(doc: &Value) -> Result<Vec<String>, String> {
    let uris: Vec<String> = doc
        .get("redirect_uris")
        .and_then(Value::as_array)
        .ok_or("redirect_uris is required")?
        .iter()
        .map(|v| v.as_str().map(String::from).ok_or("redirect_uris must be strings"))
        .collect::<Result<_, _>>()?;
    if uris.is_empty() || uris.len() > 10 {
        return Err("redirect_uris must hold 1 to 10 entries".into());
    }
    for u in &uris {
        validate_redirect_uri(u)?;
    }
    Ok(uris)
}

/// Display names are shown on the consent page: trim, drop control characters, cap the length.
pub fn clean_name(s: Option<&str>) -> Option<String> {
    let name: String = s?.chars().filter(|c| !c.is_control()).take(100).collect();
    let name = name.trim().to_string();
    (!name.is_empty()).then_some(name)
}

pub fn https_only(v: Option<&Value>) -> Option<String> {
    let s = v?.as_str()?;
    (s.len() <= 2048 && Url::parse(s).is_ok_and(|u| u.scheme() == "https" && u.host_str().is_some())).then(|| s.to_string())
}

fn ttl_from(cache_control: Option<&str>) -> Duration {
    let secs = cache_control
        .and_then(|cc| cc.split(',').find_map(|d| d.trim().strip_prefix("max-age=")?.parse::<u64>().ok()))
        .map(Duration::from_secs);
    secs.unwrap_or(DEFAULT_TTL).clamp(MIN_TTL, MAX_TTL)
}

/// Fetch and validate the metadata document for a CIMD `client_id`. Returns the metadata and how long it may be cached.
pub async fn fetch(client_id: &str) -> Result<(ClientMeta, Duration), String> {
    let url = validate_client_id_url(client_id)?;
    let host = url.host_str().ok_or("client_id URL has no host")?.to_string();
    let port = url.port_or_known_default().unwrap_or(443);

    let addrs: Vec<SocketAddr> = tokio::time::timeout(TIMEOUT, tokio::net::lookup_host((host.as_str(), port)))
        .await
        .map_err(|_| "client metadata host lookup timed out")?
        .map_err(|_| "client metadata host did not resolve")?
        .collect();
    let first = addrs.first().ok_or("client metadata host did not resolve")?;
    if addrs.iter().any(|a| ip_blocked(a.ip())) {
        return Err("client metadata host resolves to a non-public address".into());
    }

    // Pin the connection to an address we just checked, so DNS cannot change between check and connect.
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(TIMEOUT)
        .user_agent("Eunomia-OAuth/1")
        .resolve(&host, *first)
        .build()
        .map_err(|_| "could not build the metadata client")?;
    let mut resp = http
        .get(url)
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(|_| "could not fetch client metadata")?;
    if !resp.status().is_success() {
        return Err(format!("client metadata fetch returned {}", resp.status().as_u16()));
    }
    if resp.content_length().is_some_and(|n| n as usize > MAX_BODY) {
        return Err("client metadata document is too large".into());
    }
    let ttl = ttl_from(resp.headers().get(reqwest::header::CACHE_CONTROL).and_then(|v| v.to_str().ok()));
    let mut body = Vec::new();
    while let Some(chunk) = resp.chunk().await.map_err(|_| "could not read client metadata")? {
        body.extend_from_slice(&chunk);
        if body.len() > MAX_BODY {
            return Err("client metadata document is too large".into());
        }
    }
    let doc: Value = serde_json::from_slice(&body).map_err(|_| "client metadata is not valid JSON")?;
    Ok((parse_metadata(client_id, &doc)?, ttl))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn blocked(s: &str) -> bool {
        ip_blocked(s.parse().unwrap())
    }

    #[test]
    fn private_and_special_addresses_are_blocked() {
        for ip in [
            "127.0.0.1", "10.1.2.3", "172.16.0.1", "172.31.255.255", "192.168.1.1", "169.254.169.254", "0.0.0.0",
            "100.64.0.1", "224.0.0.1", "255.255.255.255", "198.18.0.1", "240.0.0.1", "::1", "::", "fe80::1", "fc00::1",
            "fd12:3456::1", "::ffff:127.0.0.1", "::ffff:10.0.0.1", "ff02::1", "2002:7f00:1::", "64:ff9b::7f00:1", "2001:db8::1",
        ] {
            assert!(blocked(ip), "{ip} should be blocked");
        }
    }

    #[test]
    fn public_addresses_pass() {
        for ip in ["1.1.1.1", "8.8.8.8", "93.184.216.34", "172.32.0.1", "2606:4700:4700::1111", "::ffff:8.8.8.8"] {
            assert!(!blocked(ip), "{ip} should be allowed");
        }
    }

    #[test]
    fn client_id_urls_are_checked() {
        assert!(validate_client_id_url("https://app.example.com/oauth/client.json").is_ok());
        for bad in [
            "http://app.example.com/client.json",
            "https://app.example.com",
            "https://app.example.com/",
            "https://127.0.0.1/client.json",
            "https://[::1]/client.json",
            "https://localhost/client.json",
            "https://user:pw@app.example.com/client.json",
            "https://app.example.com/client.json#frag",
            "https://app.example.com/a/../client.json",
            "https://app.example.com/%2e%2e/client.json",
            "ftp://app.example.com/client.json",
            "not a url",
        ] {
            assert!(validate_client_id_url(bad).is_err(), "{bad} should be rejected");
        }
    }

    #[test]
    fn redirect_uris_follow_the_spec() {
        for ok in ["https://claude.ai/api/mcp/auth_callback", "http://127.0.0.1:3000/callback", "http://localhost/cb", "http://[::1]:8080/cb", "cursor://anysphere.cursor-retrieval/oauth/x/callback"] {
            assert!(validate_redirect_uri(ok).is_ok(), "{ok}");
        }
        for bad in ["http://example.com/cb", "javascript:alert(1)", "https://x.example/cb#f", "data:text/html,hi", "file:///etc/passwd", "/relative", "https://u:p@x.example/cb"] {
            assert!(validate_redirect_uri(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn metadata_must_match_its_url_and_be_public_client() {
        let id = "https://app.example.com/client.json";
        let good = json!({"client_id": id, "client_name": "App", "redirect_uris": ["http://127.0.0.1:3000/cb"], "logo_uri": "https://app.example.com/l.png"});
        let m = parse_metadata(id, &good).unwrap();
        assert_eq!(m.name, "App");
        assert_eq!(m.logo_uri.as_deref(), Some("https://app.example.com/l.png"));
        let wrong_id = json!({"client_id": "https://evil.example/c.json", "redirect_uris": ["https://a.example/cb"]});
        assert!(parse_metadata(id, &wrong_id).is_err());
        let secret = json!({"client_id": id, "redirect_uris": ["https://a.example/cb"], "token_endpoint_auth_method": "client_secret_basic"});
        assert!(parse_metadata(id, &secret).is_err());
        let none = json!({"client_id": id});
        assert!(parse_metadata(id, &none).is_err());
        // an http logo is dropped, a missing name falls back to the host
        let m = parse_metadata(id, &json!({"client_id": id, "redirect_uris": ["https://a.example/cb"], "logo_uri": "http://x.example/l.png"})).unwrap();
        assert_eq!((m.logo_uri, m.name.as_str()), (None, "app.example.com"));
    }

    #[test]
    fn cache_ttl_is_clamped() {
        assert_eq!(ttl_from(None), DEFAULT_TTL);
        assert_eq!(ttl_from(Some("max-age=5")), MIN_TTL);
        assert_eq!(ttl_from(Some("public, max-age=600")), Duration::from_secs(600));
        assert_eq!(ttl_from(Some("max-age=999999999")), MAX_TTL);
    }
}
