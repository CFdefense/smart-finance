//! Unified API error type and result alias.

#[cfg(not(tarpaulin_include))]
use axum::{http::StatusCode, response::{IntoResponse, Response}};
#[cfg(not(tarpaulin_include))]
use std::fmt;
#[cfg(not(tarpaulin_include))]
use tracing::error;

/// Convenience alias — controllers return `ApiResult<T>` instead of `Result<T, AppError>`.
#[cfg(not(tarpaulin_include))]
pub type ApiResult<T> = std::result::Result<T, AppError>;

/// All error conditions the API can surface to a caller.
#[derive(Debug)]
#[cfg(not(tarpaulin_include))]
pub enum AppError {
    /// A request field failed validation.
    Validation(String),
    /// The request was structurally invalid.
    BadRequest(String),
    /// The caller is not authenticated.
    Unauthorized,
    /// The requested resource does not exist.
    NotFound,
    /// The operation conflicts with existing state.
    Conflict(String),
    /// An unexpected internal failure occurred.
    Internal(String),
}

#[cfg(not(tarpaulin_include))]
impl AppError {
    /// Maps this error to its HTTP status code.
    pub fn status_code(&self) -> StatusCode {
        match self {
            AppError::Validation(_) => StatusCode::BAD_REQUEST,
            AppError::BadRequest(_) => StatusCode::BAD_REQUEST,
            AppError::Unauthorized => StatusCode::UNAUTHORIZED,
            AppError::NotFound => StatusCode::NOT_FOUND,
            AppError::Conflict(_) => StatusCode::CONFLICT,
            AppError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// Logs the error via tracing with structured fields.
    pub fn log(&self) {
        match self {
            AppError::Validation(m) => {
                error!(target: "api_error", kind = "validation", message = %m)
            }
            AppError::BadRequest(m) => {
                error!(target: "api_error", kind = "bad_request", message = %m)
            }
            AppError::Unauthorized => {
                error!(target: "api_error", kind = "unauthorized")
            }
            AppError::NotFound => {
                error!(target: "api_error", kind = "not_found")
            }
            AppError::Conflict(m) => {
                error!(target: "api_error", kind = "conflict", message = %m)
            }
            AppError::Internal(m) => {
                error!(target: "api_error", kind = "internal", message = %m)
            }
        }
    }
}

#[cfg(not(tarpaulin_include))]
impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::Validation(m) => write!(f, "validation error: {m}"),
            AppError::BadRequest(m) => write!(f, "bad request: {m}"),
            AppError::Unauthorized => write!(f, "unauthorized"),
            AppError::NotFound => write!(f, "not found"),
            AppError::Conflict(m) => write!(f, "conflict: {m}"),
            AppError::Internal(m) => write!(f, "internal error: {m}"),
        }
    }
}

#[cfg(not(tarpaulin_include))]
impl std::error::Error for AppError {}

#[cfg(not(tarpaulin_include))]
impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        self.log();
        self.status_code().into_response()
    }
}

#[cfg(not(tarpaulin_include))]
impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        AppError::Internal(format!("db error: {e:?}"))
    }
}

#[cfg(not(tarpaulin_include))]
impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::Internal(format!("json error: {e:?}"))
    }
}

#[cfg(not(tarpaulin_include))]
impl From<std::env::VarError> for AppError {
    fn from(e: std::env::VarError) -> Self {
        AppError::Internal(format!("env error: {e:?}"))
    }
}
