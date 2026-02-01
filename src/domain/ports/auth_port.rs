//! Authentication port definition.

use async_trait::async_trait;

use crate::domain::entities::{AuthToken, User};
use crate::domain::errors::AuthError;

/// Port for authentication operations（支持 token 校验与用户名/密码登录、注册）。
#[async_trait]
pub trait AuthPort: Send + Sync {
    /// 使用已有 token 校验并返回用户信息（如从 keyring/env 恢复）。
    async fn validate_token(&self, token: &AuthToken) -> Result<User, AuthError>;

    /// 用户名+密码登录，返回 (User, AuthToken)。失败时可由上层尝试 register。
    async fn login(&self, username: &str, password: &str) -> Result<(User, AuthToken), AuthError>;

    /// 用户名+密码注册，返回 (User, AuthToken)。
    async fn register(&self, username: &str, password: &str) -> Result<(User, AuthToken), AuthError>;

    /// 健康检查（如 API 可用性）。
    async fn health_check(&self) -> Result<(), AuthError>;
}

#[cfg(test)]
pub mod mock {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    /// Mock authentication port for testing.
    pub struct MockAuthPort {
        should_succeed: Arc<AtomicBool>,
        user: User,
    }

    impl MockAuthPort {
        /// Creates new mock.
        #[must_use]
        pub fn new(should_succeed: bool) -> Self {
            Self {
                should_succeed: Arc::new(AtomicBool::new(should_succeed)),
                user: User::new("123", "testuser", "0", None, false, None),
            }
        }

        /// Sets success behavior.
        pub fn set_should_succeed(&self, value: bool) {
            self.should_succeed.store(value, Ordering::SeqCst);
        }
    }

    #[async_trait]
    impl AuthPort for MockAuthPort {
        async fn validate_token(&self, _token: &AuthToken) -> Result<User, AuthError> {
            if self.should_succeed.load(Ordering::SeqCst) {
                Ok(self.user.clone())
            } else {
                Err(AuthError::rejected("mock rejection"))
            }
        }

        async fn login(&self, _username: &str, _password: &str) -> Result<(User, AuthToken), AuthError> {
            if self.should_succeed.load(Ordering::SeqCst) {
                let token = AuthToken::new_unchecked("123:mock_token_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx");
                Ok((self.user.clone(), token))
            } else {
                Err(AuthError::rejected("mock login rejection"))
            }
        }

        async fn register(&self, _username: &str, _password: &str) -> Result<(User, AuthToken), AuthError> {
            if self.should_succeed.load(Ordering::SeqCst) {
                let token = AuthToken::new_unchecked("123:mock_token_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx");
                Ok((self.user.clone(), token))
            } else {
                Err(AuthError::rejected("mock register rejection"))
            }
        }

        async fn health_check(&self) -> Result<(), AuthError> {
            Ok(())
        }
    }
}
