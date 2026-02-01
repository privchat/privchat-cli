//! Login use case implementation.

use std::sync::Arc;

use tracing::{debug, info, warn};

use crate::application::dto::{LoginRequest, LoginResponse};
use crate::domain::entities::AuthToken;
use crate::domain::errors::AuthError;
use crate::domain::ports::{AuthPort, TokenStoragePort};

/// Handles user authentication workflow.
#[derive(Clone)]
pub struct LoginUseCase {
    auth_port: Arc<dyn AuthPort>,
    storage_port: Arc<dyn TokenStoragePort>,
}

impl LoginUseCase {
    /// Creates new login use case.
    #[must_use]
    pub const fn new(
        auth_port: Arc<dyn AuthPort>,
        storage_port: Arc<dyn TokenStoragePort>,
    ) -> Self {
        Self {
            auth_port,
            storage_port,
        }
    }

    /// Executes login with provided request（支持 token 校验或用户名+密码登录/注册）。
    ///
    /// # Errors
    /// Returns error if token 无效或登录/注册失败。
    pub async fn execute(&self, request: LoginRequest) -> Result<LoginResponse, AuthError> {
        let (user, token) = if request.is_credentials() {
            let username = request.username.as_deref().unwrap_or("");
            let password = request.password.as_deref().unwrap_or("");
            let intent = request.intent;
            debug!(username = %username, ?intent, "Attempting auth with credentials");
            match intent {
                Some(crate::application::dto::AuthIntent::Register) => {
                    info!(username = %username, "Registering");
                    self.auth_port.register(username, password).await.map_err(|e| {
                        warn!(error = %e, "Register failed");
                        e
                    })?
                }
                Some(crate::application::dto::AuthIntent::Login) | None => {
                    info!(username = %username, "Logging in");
                    self.auth_port.login(username, password).await.map_err(|e| {
                        warn!(error = %e, "Login failed");
                        e
                    })?
                }
            }
        } else {
            let token_str = request.token.as_deref().ok_or_else(|| {
                warn!("No token and no credentials provided");
                AuthError::invalid_format("provide token or username and password")
            })?;
            let token = AuthToken::new(token_str).ok_or_else(|| {
                warn!("Token format invalid");
                AuthError::invalid_format("invalid token format")
            })?;
            debug!(source = %request.source, "Validating token");
            let user = self.auth_port.validate_token(&token).await.map_err(|e| {
                warn!(error = %e, "Token validation failed");
                e
            })?;
            (user, token)
        };

        info!(
            user_id = %user.id(),
            username = %user.username(),
            "Successfully authenticated"
        );

        let token_persisted = if request.persist_token {
            match self.storage_port.store_token(&token).await {
                Ok(()) => {
                    info!("Token persisted to secure storage");
                    true
                }
                Err(e) => {
                    tracing::error!(error = %e, "Failed to persist token to secure storage");
                    false
                }
            }
        } else {
            debug!("Token persistence disabled, skipping storage");
            false
        };

        Ok(LoginResponse::new(
            user,
            request.source,
            token_persisted,
            token.as_str().to_string(),
        ))
    }

    /// Deletes the stored token.
    ///
    /// # Errors
    /// Returns error if deletion fails.
    pub async fn delete_token(&self) -> Result<(), AuthError> {
        debug!("Deleting token from secure storage");
        match self.storage_port.delete_token().await {
            Ok(()) => {
                info!("Token deleted from secure storage");
                Ok(())
            }
            Err(e) => {
                tracing::error!(error = %e, "Failed to delete token from secure storage");
                Err(e)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::dto::TokenSource;
    use crate::domain::ports::mocks::{MockAuthPort, MockTokenStorage};

    fn make_valid_token() -> String {
        // Placeholder for unit test only: length >= 50, 3 segments; not a real token.
        "test-token-placeholder-part-one.part-two.part-three-extra-padding".to_string()
    }

    #[tokio::test]
    async fn test_successful_login() {
        let auth_port = Arc::new(MockAuthPort::new(true));
        let storage_port = Arc::new(MockTokenStorage::new());

        let use_case = LoginUseCase::new(auth_port, storage_port.clone());
        let request = LoginRequest::new(make_valid_token(), TokenSource::Environment);

        let result = use_case.execute(request).await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.user.username(), "testuser");
        assert!(response.token_persisted);

        assert!(storage_port.has_token().await.unwrap());
    }

    #[tokio::test]
    async fn test_invalid_token_format() {
        let auth_port = Arc::new(MockAuthPort::new(true));
        let storage_port = Arc::new(MockTokenStorage::new());

        let use_case = LoginUseCase::new(auth_port, storage_port);
        let request = LoginRequest::new("invalid".to_string(), TokenSource::UserInput);

        let result = use_case.execute(request).await;

        assert!(matches!(result, Err(AuthError::InvalidTokenFormat { .. })));
    }

    #[tokio::test]
    async fn test_rejected_token() {
        let auth_port = Arc::new(MockAuthPort::new(false));
        let storage_port = Arc::new(MockTokenStorage::new());

        let use_case = LoginUseCase::new(auth_port, storage_port);
        let request = LoginRequest::new(make_valid_token(), TokenSource::CommandLine);

        let result = use_case.execute(request).await;

        assert!(matches!(result, Err(AuthError::TokenRejected { .. })));
    }

    #[tokio::test]
    async fn test_login_without_persistence() {
        let auth_port = Arc::new(MockAuthPort::new(true));
        let storage_port = Arc::new(MockTokenStorage::new());

        let use_case = LoginUseCase::new(auth_port, storage_port.clone());
        let request =
            LoginRequest::new(make_valid_token(), TokenSource::CommandLine).without_persistence();

        let result = use_case.execute(request).await;

        assert!(result.is_ok());
        assert!(!result.unwrap().token_persisted);
        assert!(!storage_port.has_token().await.unwrap());
    }
}
