//! Secret storage error types.

use thiserror::Error;

/// Secret storage error variants.
#[derive(Debug, Error)]
pub enum SecretError {
    #[error("failed to access secure storage: {0}")]
    AccessFailed(String),

    #[error("failed to retrieve secret: {0}")]
    RetrievalFailed(String),

    #[error("failed to store secret: {0}")]
    StorageFailed(String),

    #[error("failed to delete secret: {0}")]
    DeletionFailed(String),

    #[error("secure storage not available: {0}")]
    NotAvailable(String),
}
