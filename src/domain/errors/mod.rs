//! Domain error types.

mod auth_error;
mod secret_error;

pub use auth_error::AuthError;
pub use secret_error::SecretError;
