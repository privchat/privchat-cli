//! Authentication error types.

use thiserror::Error;

/// Authentication error variants.
#[derive(Debug, Error)]
#[allow(missing_docs)]
pub enum AuthError {
    #[error("invalid token format: {reason}")]
    InvalidTokenFormat { reason: String },

    #[error("{message}")]
    TokenRejected { message: String },

    #[error("failed to retrieve stored token: {message}")]
    TokenRetrievalFailed { message: String },

    #[error("failed to store token: {message}")]
    TokenStorageFailed { message: String },

    #[error("no authentication token available")]
    NoTokenAvailable,

    #[error("network error during authentication: {message}")]
    NetworkError { message: String },

    #[error("rate limited by Discord, retry after {retry_after_ms}ms")]
    RateLimited { retry_after_ms: u64 },

    #[error("secure storage error: {0}")]
    Secret(#[from] super::SecretError),

    #[error("unexpected authentication error: {message}")]
    Unexpected { message: String },
}

impl AuthError {
    /// Creates invalid format error.
    #[must_use]
    pub fn invalid_format(reason: impl Into<String>) -> Self {
        Self::InvalidTokenFormat {
            reason: reason.into(),
        }
    }

    /// Creates token rejected error.
    #[must_use]
    pub fn rejected(message: impl Into<String>) -> Self {
        Self::TokenRejected {
            message: message.into(),
        }
    }

    /// Creates network error.
    #[must_use]
    pub fn network(message: impl Into<String>) -> Self {
        Self::NetworkError {
            message: message.into(),
        }
    }

    /// Creates retrieval failed error.
    #[must_use]
    pub fn retrieval_failed(message: impl Into<String>) -> Self {
        Self::TokenRetrievalFailed {
            message: message.into(),
        }
    }

    /// Creates storage failed error.
    #[must_use]
    pub fn storage_failed(message: impl Into<String>) -> Self {
        Self::TokenStorageFailed {
            message: message.into(),
        }
    }

    /// Creates unexpected error.
    #[must_use]
    pub fn unexpected(message: impl Into<String>) -> Self {
        Self::Unexpected {
            message: message.into(),
        }
    }

    /// Returns whether error is recoverable.
    #[must_use]
    pub const fn is_recoverable(&self) -> bool {
        matches!(
            self,
            Self::NetworkError { .. }
                | Self::RateLimited { .. }
                | Self::TokenRejected { .. }
                | Self::NoTokenAvailable
        )
    }

    /// Returns whether error is network related.
    #[must_use]
    pub const fn is_network_error(&self) -> bool {
        matches!(self, Self::NetworkError { .. } | Self::RateLimited { .. })
    }

    /// 返回给用户展示的文案（优先使用服务端/SDK 的 error_code、error_message，不替换为固定文案）。
    #[must_use]
    pub fn user_message(&self) -> String {
        match self {
            Self::InvalidTokenFormat { reason } => reason.clone(),
            Self::TokenRejected { message } => message.clone(),
            Self::TokenRetrievalFailed { message } => message.clone(),
            Self::TokenStorageFailed { message } => message.clone(),
            Self::NoTokenAvailable => Self::NoTokenAvailable.to_string(),
            Self::NetworkError { message } => message.clone(),
            Self::RateLimited { retry_after_ms } => {
                format!("Rate limited. Try again in {}s", retry_after_ms / 1000)
            }
            Self::Secret(e) => e.to_string(),
            Self::Unexpected { message } => message.clone(),
        }
    }
}
