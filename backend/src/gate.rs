//! The request gate: one middleware in front of every route that
//!
//! 1. authenticates the credential once (Bearer token or session cookie) and
//!    hands the result to the `User` extractor through a request extension,
//! 2. rate limits (per user, per token, and per client address for login,
//!    signup and failed credentials),
//! 3. checks the token's scope for the route and refuses vault-restricted
//!    tokens on account-level routes,
//! 4. runs the handler with the credential as the task-local `authz::Caller`, so
//!    `authorize()` and the vault lookups enforce the token's vault restriction,
//! 5. writes `audit_event` rows for failed authentication and scope denials.
//!
//! [`classify`] is the single table saying what each route needs. A new `/api`
//! route that is not in it defaults to the strictest class, and a test fails
//! until it is added.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::{
    extract::{ConnectInfo, Request, State},
    http::{header, HeaderValue, Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};

use crate::audit::{self, Event};
use crate::auth::{self, Authn};
use crate::authz;
use crate::error::{AppError, ErrorCode};
use crate::ratelimit::{RateConfig, RateLimiter};
use crate::scopes;
use crate::state::AppState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    /// No credential needed.
    Public,
    /// Login and signup: public, but limited per client address.
    AuthAttempt,
    /// `/mcp`: Bearer only, and the handler does its own protocol-level errors.
    Mcp,
    /// Any authenticated caller (scope is checked per tool, or not at all).
    Any,
    /// Touches vault content; needs the scope, and the vault restriction is enforced by `authorize()`.
    Vault(&'static str),
    /// Account-level (settings, tokens, connectors, chat, export): needs the
    /// scope and is closed to vault-restricted tokens.
    Account(&'static str),
    /// Not an `/api` route (for example OAuth endpoints): authenticate if a
    /// credential is present, never require one.
    Optional,
}

pub fn classify(method: &Method, path: &str) -> Class {
    explicit_class(method, path).unwrap_or(Class::Account(scopes::VAULTS_ADMIN))
}

/// The class a route is listed under. `None` is an `/api` route nobody has
/// classified yet ([`classify`] gives it the strictest class; a test fails on it).
pub fn explicit_class(method: &Method, path: &str) -> Option<Class> {
    use Class::*;
    let read = method == Method::GET || method == Method::HEAD;
    let segs: Vec<&str> = path.trim_matches('/').split('/').collect();
    Some(match segs.as_slice() {
        ["healthz"] => Public,
        ["mcp"] => Mcp,
        ["api", "openapi.json"] => Public,
        ["api", "auth", "register" | "login"] => AuthAttempt,
        ["api", "auth", "logout" | "bootstrap"] => Public,
        ["api", "auth", "me"] => Any,
        ["api", "auth", "tokens" | "sessions", ..] => Account(scopes::VAULTS_ADMIN),
        ["api", "tools"] if read => Public,
        ["api", "tools", _] => Any,
        ["api", "entities", ..] => Vault(if read { scopes::MEMORY_READ } else { scopes::MEMORY_WRITE }),
        ["api", "vaults", ..] => Vault(if read { scopes::MEMORY_READ } else { scopes::VAULTS_ADMIN }),
        ["api", "connectors", ..] | ["api", "snapshot"] => Account(scopes::CONNECTORS),
        ["api", "sources", _, "webhook", _] => Public,
        ["api", "sources", ..] => Account(scopes::CONNECTORS),
        ["api", "settings" | "debug" | "export" | "update" | "audit" | "chat" | "oauth", ..] => Account(scopes::VAULTS_ADMIN),
        // OAuth endpoints for MCP clients: public, so limited per client address like login
        ["oauth", "token" | "register" | "revoke"] => AuthAttempt,
        ["api", ..] => return None,
        _ => Optional,
    })
}

#[derive(Clone)]
pub struct Gate {
    state: AppState,
    limits: Arc<RateConfig>,
    limiter: Arc<RateLimiter>,
}

impl Gate {
    pub fn new(state: AppState, limits: RateConfig) -> Self {
        Gate { state, limits: Arc::new(limits), limiter: Arc::new(RateLimiter::default()) }
    }
}

fn client_ip(req: &Request) -> String {
    // ponytail: X-Forwarded-For is client-controlled when nothing trusted sits in
    // front; behind the bundled frontend proxy it is the real address. Pin it to a
    // trusted-proxy list if the backend is ever exposed directly.
    req.headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| req.extensions().get::<ConnectInfo<SocketAddr>>().map(|c| c.0.ip().to_string()))
        .unwrap_or_else(|| "unknown".to_string())
}

fn rate_limited(wait_secs: u64) -> Response {
    let mut resp = AppError::coded(ErrorCode::RateLimited, "Too many requests. Slow down and retry.").into_response();
    if let Ok(v) = HeaderValue::from_str(&wait_secs.to_string()) {
        resp.headers_mut().insert(header::RETRY_AFTER, v);
    }
    resp
}

fn reject(g: &Gate, class: Class, err: AppError) -> Response {
    let mut resp = err.into_response();
    if class == Class::Mcp
        && resp.status() == StatusCode::UNAUTHORIZED
        && let Ok(v) = HeaderValue::from_str(&crate::oauth::www_authenticate(&g.state.settings, true))
    {
        resp.headers_mut().insert(header::WWW_AUTHENTICATE, v);
    }
    resp
}

pub async fn gate(State(g): State<Gate>, mut req: Request, next: Next) -> Response {
    let class = classify(req.method(), req.uri().path());
    match class {
        Class::Public => return next.run(req).await,
        Class::AuthAttempt => {
            if let Err(wait) = g.limiter.check(&format!("auth:{}", client_ip(&req)), g.limits.auth_per_min) {
                return rate_limited(wait);
            }
            return next.run(req).await;
        }
        _ => {}
    }

    let ip = client_ip(&req);
    let mcp = class == Class::Mcp;
    let presented = auth::bearer_token(req.headers()).is_some() || (!mcp && auth::session_token(req.headers()).is_some());
    let target = format!("{} {}", req.method(), req.uri().path());

    let authn = match auth::authenticate(&g.state, req.headers(), mcp).await {
        Ok(a) => a,
        Err(e) => {
            audit::record(
                &g.state.db,
                Event { user: None, actor: &audit::anonymous(), action: "auth.failed", target: &target, outcome: e.code.as_str(), detail: "" },
            )
            .await;
            return reject(&g, class, e);
        }
    };

    let Some(authn) = authn else {
        if presented {
            // a credential that does not check out: throttle guessing, and leave a trail
            if let Err(wait) = g.limiter.check(&format!("auth:{ip}"), g.limits.auth_per_min) {
                return rate_limited(wait);
            }
            audit::record(
                &g.state.db,
                Event { user: None, actor: &audit::anonymous(), action: "auth.failed", target: &target, outcome: ErrorCode::AuthUnauthorized.as_str(), detail: "" },
            )
            .await;
        }
        return match class {
            Class::Optional | Class::Mcp => next.run(req).await,
            _ => reject(&g, class, AppError::unauthorized("Not authenticated.")),
        };
    };

    if let Err(wait) = rate_check(&g, &authn) {
        return rate_limited(wait);
    }

    let denied = match class {
        Class::Vault(s) if !authn.caller.allows(s) => Some(format!("this token does not have the {s} scope")),
        Class::Account(s) if !authn.caller.allows(s) => Some(format!("this token does not have the {s} scope")),
        Class::Account(_) if authn.caller.vault.is_some() => {
            Some("this route is not available to a vault-restricted token".to_string())
        }
        _ => None,
    };
    if let Some(message) = denied {
        audit::record(
            &g.state.db,
            Event { user: Some(&authn.user.id), actor: &authn.caller.actor, action: "authz.denied", target: &target, outcome: ErrorCode::AuthScope.as_str(), detail: &message },
        )
        .await;
        return AppError::coded(ErrorCode::AuthScope, message).into_response();
    }

    let caller = authn.caller.clone();
    req.extensions_mut().insert(authn);
    authz::with_caller(caller, next.run(req)).await
}

fn rate_check(g: &Gate, authn: &Authn) -> Result<(), u64> {
    if authn.caller.actor.kind == "token" {
        g.limiter.check(&format!("token:{}", authn.caller.actor.id), g.limits.token_per_min)?;
    }
    g.limiter.check(&format!("user:{}", authn.user.id), g.limits.user_per_min)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn class(method: Method, path: &str) -> Class {
        classify(&method, path)
    }

    #[test]
    fn route_classes() {
        use Class::*;
        assert_eq!(class(Method::GET, "/healthz"), Public);
        assert_eq!(class(Method::POST, "/api/auth/login"), AuthAttempt);
        assert_eq!(class(Method::POST, "/api/auth/tokens"), Account(scopes::VAULTS_ADMIN));
        assert_eq!(class(Method::GET, "/api/entities/graph"), Vault(scopes::MEMORY_READ));
        assert_eq!(class(Method::PATCH, "/api/entities/memory/memory:x"), Vault(scopes::MEMORY_WRITE));
        assert_eq!(class(Method::GET, "/api/vaults/vault:x/members"), Vault(scopes::MEMORY_READ));
        assert_eq!(class(Method::DELETE, "/api/vaults/vault:x"), Vault(scopes::VAULTS_ADMIN));
        assert_eq!(class(Method::POST, "/api/sources/github/webhook/user:x"), Public);
        assert_eq!(class(Method::POST, "/api/sources/github/sync"), Account(scopes::CONNECTORS));
        assert_eq!(class(Method::GET, "/api/tools"), Public);
        assert_eq!(class(Method::POST, "/api/tools/recall"), Any);
        assert_eq!(class(Method::GET, "/api/something-new"), Account(scopes::VAULTS_ADMIN));
        assert_eq!(class(Method::GET, "/.well-known/oauth-authorization-server"), Optional);
        assert_eq!(class(Method::POST, "/mcp"), Mcp);
    }
}
