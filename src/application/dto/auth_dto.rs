//! Authentication DTOs.

use crate::domain::entities::User;

/// Source of the authentication token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenSource {
    /// Token from CLI argument.
    CommandLine,
    /// Token from environment variable.
    Environment,
    /// Token from system keyring.
    Keyring,
    /// Token entered by user.
    UserInput,
}

impl TokenSource {
    /// Returns human-readable description.
    #[must_use]
    pub const fn description(&self) -> &'static str {
        match self {
            Self::CommandLine => "command-line argument",
            Self::Environment => "environment variable",
            Self::Keyring => "system keyring",
            Self::UserInput => "user input",
        }
    }
}

impl std::fmt::Display for TokenSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.description())
    }
}

/// Login request data（支持「已有 token」或「用户名+密码」两种方式）。
#[derive(Debug, Clone)]
pub struct LoginRequest {
    /// 已有 token（与 username/password 二选一）。
    pub token: Option<String>,
    /// 用户名（与 password 一起时走登录/注册流程）。
    pub username: Option<String>,
    /// 密码。
    pub password: Option<String>,
    /// Token 来源（仅当使用 token 时有效）。
    pub source: TokenSource,
    /// 是否持久化 token（登录/注册得到的 token 或用户输入的 token）。
    pub persist_token: bool,
}

impl LoginRequest {
    /// 使用已有 token 的登录请求（如 keyring/env/CLI）。
    #[must_use]
    pub fn with_token(token: String, source: TokenSource) -> Self {
        Self {
            token: Some(token),
            username: None,
            password: None,
            source,
            persist_token: true,
        }
    }

    /// 使用用户名+密码的登录请求（先尝试 login，失败则 register）。
    #[must_use]
    pub fn with_credentials(username: String, password: String, persist_token: bool) -> Self {
        Self {
            token: None,
            username: Some(username),
            password: Some(password),
            source: TokenSource::UserInput,
            persist_token,
        }
    }

    /// 兼容旧调用：new(token, source) 等价 with_token。
    #[must_use]
    pub fn new(token: String, source: TokenSource) -> Self {
        Self::with_token(token, source)
    }

    /// 禁用 token 持久化。
    #[must_use]
    pub const fn without_persistence(mut self) -> Self {
        self.persist_token = false;
        self
    }

    /// 是否为「用户名+密码」流程。
    #[must_use]
    pub fn is_credentials(&self) -> bool {
        self.username.is_some() && self.password.is_some()
    }
}

/// Login response data.
#[derive(Debug, Clone)]
pub struct LoginResponse {
    /// Authenticated user.
    pub user: User,
    /// Token source used.
    pub token_source: TokenSource,
    /// Whether token was persisted.
    pub token_persisted: bool,
    /// Session token（用于 App 设置 current_token；登录/注册或校验 token 后均有值）。
    pub session_token: String,
}

impl LoginResponse {
    /// Creates new login response.
    #[must_use]
    pub fn new(
        user: User,
        token_source: TokenSource,
        token_persisted: bool,
        session_token: String,
    ) -> Self {
        Self {
            user,
            token_source,
            token_persisted,
            session_token,
        }
    }
}
