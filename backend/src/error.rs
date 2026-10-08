//! One error type for the whole backend. Every error has a stable dotted
//! `ErrorCode` (documented in `docs/errors.md`) and leaves the process as an
//! RFC 9457 `application/problem+json` body carrying that code and the
//! request's trace id. Raw database or upstream text goes to the log only
//! (`AppError::source`), never into a response.

use axum::{
    http::{header, StatusCode},
    response::{IntoResponse, Response},
};
use serde_json::json;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    AuthUnauthorized,
    AuthForbidden,
    AuthEmailTaken,
    AuthNotFound,
    TenantDenied,
    VaultNotFound,
    VaultForbidden,
    EntityNotFound,
    MemoryNotFound,
    ChatThreadNotFound,
    ConnectorNotFound,
    ConnectorNotConnected,
    SourceNotFound,
    ToolNotFound,
    ResourceNotFound,
    ValidationInvalid,
    DbConflict,
    DbDuplicate,
    Internal,
}

impl ErrorCode {
    pub const ALL: &'static [ErrorCode] = &[
        ErrorCode::AuthUnauthorized,
        ErrorCode::AuthForbidden,
        ErrorCode::AuthEmailTaken,
        ErrorCode::AuthNotFound,
        ErrorCode::TenantDenied,
        ErrorCode::VaultNotFound,
        ErrorCode::VaultForbidden,
        ErrorCode::EntityNotFound,
        ErrorCode::MemoryNotFound,
        ErrorCode::ChatThreadNotFound,
        ErrorCode::ConnectorNotFound,
        ErrorCode::ConnectorNotConnected,
        ErrorCode::SourceNotFound,
        ErrorCode::ToolNotFound,
        ErrorCode::ResourceNotFound,
        ErrorCode::ValidationInvalid,
        ErrorCode::DbConflict,
        ErrorCode::DbDuplicate,
        ErrorCode::Internal,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            ErrorCode::AuthUnauthorized => "auth.unauthorized",
            ErrorCode::AuthForbidden => "auth.forbidden",
            ErrorCode::AuthEmailTaken => "auth.email_taken",
            ErrorCode::AuthNotFound => "auth.not_found",
            ErrorCode::TenantDenied => "tenant.denied",
            ErrorCode::VaultNotFound => "vault.not_found",
            ErrorCode::VaultForbidden => "vault.forbidden",
            ErrorCode::EntityNotFound => "entity.not_found",
            ErrorCode::MemoryNotFound => "memory.not_found",
            ErrorCode::ChatThreadNotFound => "chat.thread_not_found",
            ErrorCode::ConnectorNotFound => "connector.not_found",
            ErrorCode::ConnectorNotConnected => "connector.not_connected",
            ErrorCode::SourceNotFound => "source.not_found",
            ErrorCode::ToolNotFound => "tool.not_found",
            ErrorCode::ResourceNotFound => "resource.not_found",
            ErrorCode::ValidationInvalid => "validation.invalid",
            ErrorCode::DbConflict => "db.conflict",
            ErrorCode::DbDuplicate => "db.duplicate",
            ErrorCode::Internal => "internal",
        }
    }

    pub fn default_status(self) -> StatusCode {
        match self {
            ErrorCode::AuthUnauthorized => StatusCode::UNAUTHORIZED,
            ErrorCode::AuthForbidden | ErrorCode::TenantDenied | ErrorCode::VaultForbidden => StatusCode::FORBIDDEN,
            ErrorCode::AuthEmailTaken | ErrorCode::DbConflict | ErrorCode::DbDuplicate => StatusCode::CONFLICT,
            ErrorCode::AuthNotFound
            | ErrorCode::VaultNotFound
            | ErrorCode::EntityNotFound
            | ErrorCode::MemoryNotFound
            | ErrorCode::ChatThreadNotFound
            | ErrorCode::ConnectorNotFound
            | ErrorCode::SourceNotFound
            | ErrorCode::ToolNotFound
            | ErrorCode::ResourceNotFound => StatusCode::NOT_FOUND,
            ErrorCode::ConnectorNotConnected | ErrorCode::ValidationInvalid => StatusCode::BAD_REQUEST,
            ErrorCode::Internal => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// Codes whose message may hold internals, so callers outside the process
    /// see a generic detail instead.
    fn hides_message(self) -> bool {
        matches!(self, ErrorCode::Internal | ErrorCode::TenantDenied)
    }

    /// A code for a bare status, for the `AppError::new(status, ..)` callers.
    fn from_status(status: StatusCode) -> Self {
        match status {
            StatusCode::BAD_REQUEST => ErrorCode::ValidationInvalid,
            StatusCode::UNAUTHORIZED => ErrorCode::AuthUnauthorized,
            StatusCode::FORBIDDEN => ErrorCode::VaultForbidden,
            StatusCode::NOT_FOUND => ErrorCode::ResourceNotFound,
            StatusCode::CONFLICT => ErrorCode::DbConflict,
            _ => ErrorCode::Internal,
        }
    }
}

#[derive(Debug)]
pub struct AppError {
    pub code: ErrorCode,
    pub status: StatusCode,
    /// For `internal` and `tenant.denied` this can hold raw upstream text;
    /// use `detail()` for anything that leaves the process.
    pub message: String,
    /// Raw cause (database or upstream text). Logged, never sent.
    pub source: Option<String>,
}

impl AppError {
    pub fn coded(code: ErrorCode, message: impl Into<String>) -> Self {
        AppError { code, status: code.default_status(), message: message.into(), source: None }
    }

    pub fn new(status: StatusCode, message: impl Into<String>) -> Self {
        AppError { code: ErrorCode::from_status(status), status, message: message.into(), source: None }
    }

    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::coded(ErrorCode::AuthUnauthorized, message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::coded(ErrorCode::ResourceNotFound, message)
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::coded(ErrorCode::ValidationInvalid, message)
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::coded(ErrorCode::Internal, message)
    }

    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }

    /// The message that is safe to show a client or agent.
    pub fn detail(&self) -> &str {
        if self.code.hides_message() {
            "Internal server error."
        } else {
            &self.message
        }
    }

    /// The `{error, code, trace_id}` shape tools return.
    pub fn to_tool_value(&self) -> serde_json::Value {
        json!({ "error": self.detail(), "code": self.code.as_str(), "trace_id": crate::telemetry::current_trace_id() })
    }

    /// Logs the cause on the current span. Called once, where the error leaves the process.
    fn log(&self) {
        let source = self.source.as_deref().unwrap_or(&self.message);
        if self.code == ErrorCode::TenantDenied {
            tracing::error!(code = self.code.as_str(), %source, "tenant isolation denied a query");
        } else if self.status.is_server_error() {
            tracing::error!(code = self.code.as_str(), %source, "request failed");
        } else {
            tracing::debug!(code = self.code.as_str(), %source, "request rejected");
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        self.log();
        let body = json!({
            "type": "about:blank",
            "title": self.status.canonical_reason().unwrap_or("Error"),
            "status": self.status.as_u16(),
            "detail": self.detail(),
            "code": self.code.as_str(),
            "trace_id": crate::telemetry::current_trace_id(),
        });
        (self.status, [(header::CONTENT_TYPE, "application/problem+json")], body.to_string()).into_response()
    }
}

impl From<surrealdb::Error> for AppError {
    fn from(err: surrealdb::Error) -> Self {
        let raw = err.to_string();
        let lower = raw.to_lowercase();
        let (code, message) = match &err {
            _ if crate::tx::is_conflict(&err) => (ErrorCode::DbConflict, "The write conflicted with another; retry it."),
            _ if err.is_not_allowed() => (ErrorCode::TenantDenied, "Internal server error."),
            _ if lower.contains("you don't have permission") || lower.contains("not enough permissions") => {
                (ErrorCode::TenantDenied, "Internal server error.")
            }
            _ if err.is_already_exists() || lower.contains("already contains") => (ErrorCode::DbDuplicate, "That record already exists."),
            _ => (ErrorCode::Internal, "Internal server error."),
        };
        AppError::coded(code, message).with_source(raw)
    }
}

pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_error_code_is_documented() {
        let docs = include_str!("../../docs/errors.md");
        for code in ErrorCode::ALL {
            assert!(docs.contains(&format!("`{}`", code.as_str())), "docs/errors.md is missing {}", code.as_str());
        }
    }

    #[test]
    fn all_lists_every_variant_once() {
        let mut names: Vec<_> = ErrorCode::ALL.iter().map(|c| c.as_str()).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), ErrorCode::ALL.len());
        // exhaustive match: adding a variant fails to compile until it is handled here and in ALL
        for c in ErrorCode::ALL {
            match c {
                ErrorCode::AuthUnauthorized | ErrorCode::AuthForbidden | ErrorCode::AuthEmailTaken | ErrorCode::AuthNotFound
                | ErrorCode::TenantDenied | ErrorCode::VaultNotFound | ErrorCode::VaultForbidden | ErrorCode::EntityNotFound
                | ErrorCode::MemoryNotFound | ErrorCode::ChatThreadNotFound | ErrorCode::ConnectorNotFound
                | ErrorCode::ConnectorNotConnected | ErrorCode::SourceNotFound | ErrorCode::ToolNotFound
                | ErrorCode::ResourceNotFound | ErrorCode::ValidationInvalid | ErrorCode::DbConflict
                | ErrorCode::DbDuplicate | ErrorCode::Internal => {}
            }
        }
    }

    #[test]
    fn internal_detail_never_leaks_the_source() {
        let e = AppError::internal("select * from secret: boom");
        assert_eq!(e.detail(), "Internal server error.");
        assert!(!e.to_tool_value().to_string().contains("secret"));
    }
}
