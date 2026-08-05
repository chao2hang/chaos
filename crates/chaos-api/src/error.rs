//! API error type with consistent JSON envelope and localized messages.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use chaos_i18n::{error_message, Locale};
use serde::Serialize;

#[derive(Debug, Clone)]
pub struct ApiError {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
    pub draft_saved: bool,
}

impl ApiError {
    pub fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
            draft_saved: false,
        }
    }

    /// Coded error: `message` resolved from shared locale catalog.
    pub fn coded(status: StatusCode, code: &'static str, locale: Locale) -> Self {
        Self {
            status,
            code,
            message: error_message(locale, code),
            draft_saved: false,
        }
    }

    pub fn with_draft_saved(mut self) -> Self {
        self.draft_saved = true;
        self
    }

    pub fn bad_request(code: &'static str, locale: Locale) -> Self {
        Self::coded(StatusCode::BAD_REQUEST, code, locale)
    }

    pub fn unauthorized(code: &'static str, locale: Locale) -> Self {
        Self::coded(StatusCode::UNAUTHORIZED, code, locale)
    }

    pub fn conflict(code: &'static str, locale: Locale) -> Self {
        Self::coded(StatusCode::CONFLICT, code, locale)
    }

    pub fn not_found(code: &'static str, locale: Locale) -> Self {
        Self::coded(StatusCode::NOT_FOUND, code, locale)
    }

    pub fn forbidden(code: &'static str, locale: Locale) -> Self {
        Self::coded(StatusCode::FORBIDDEN, code, locale)
    }

    pub fn not_implemented(code: &'static str, locale: Locale) -> Self {
        Self::coded(StatusCode::NOT_IMPLEMENTED, code, locale)
    }

    /// Client-facing internal error (generic localized message). Details go to logs.
    pub fn internal(locale: Locale) -> Self {
        Self::coded(StatusCode::INTERNAL_SERVER_ERROR, "internal_error", locale)
    }

    /// Like `internal` but logs an English diagnostic first.
    pub fn internal_logged(locale: Locale, log: impl std::fmt::Display) -> Self {
        tracing::error!(error = %log, "internal error");
        Self::internal(locale)
    }
}

#[derive(Serialize)]
struct ErrorBody<'a> {
    error: ErrorDetail<'a>,
}

#[derive(Serialize)]
struct ErrorDetail<'a> {
    code: &'a str,
    message: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    draft_saved: Option<bool>,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = ErrorBody {
            error: ErrorDetail {
                code: self.code,
                message: &self.message,
                draft_saved: self.draft_saved.then_some(true),
            },
        };
        (self.status, Json(body)).into_response()
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(err: sqlx::Error) -> Self {
        tracing::error!(?err, "database error");
        // Default English when locale is unavailable (From impl has no request context).
        ApiError::internal(Locale::En)
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(err: anyhow::Error) -> Self {
        tracing::error!(error = %err, "internal error");
        ApiError::internal(Locale::En)
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for ApiError {}
