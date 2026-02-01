//! 本地文件 token 存储：不依赖系统钥匙串，与 device_id 一样仅用应用数据目录，避免 macOS 钥匙串权限弹窗。

use std::path::PathBuf;

use async_trait::async_trait;
use tracing::debug;

use crate::domain::entities::AuthToken;
use crate::domain::errors::AuthError;
use crate::domain::ports::TokenStoragePort;

const TOKEN_FILE: &str = "token";

/// 使用应用数据目录下的文件存储 token，不请求钥匙串权限。
pub struct FileTokenStorage {
    path: PathBuf,
}

impl FileTokenStorage {
    /// 使用 `data_dir` 下的 `token` 文件。
    pub fn new(data_dir: PathBuf) -> Self {
        Self {
            path: data_dir.join(TOKEN_FILE),
        }
    }

    fn ensure_dir(&self) -> Result<(), AuthError> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| AuthError::Unexpected {
                message: format!("创建数据目录失败: {}", e),
            })?;
        }
        Ok(())
    }
}

#[async_trait]
impl TokenStoragePort for FileTokenStorage {
    async fn get_token(&self) -> Result<Option<AuthToken>, AuthError> {
        let path = self.path.clone();
        tokio::task::spawn_blocking(move || {
            let s = match std::fs::read_to_string(&path) {
                Ok(s) => s,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(e) => return Err(AuthError::Unexpected { message: e.to_string() }),
            };
            let s = s.trim();
            if s.is_empty() {
                return Ok(None);
            }
            debug!("从本地文件读取 token");
            Ok(Some(AuthToken::new_unchecked(s.to_string())))
        })
        .await
        .map_err(|e| AuthError::Unexpected {
            message: e.to_string(),
        })?
    }

    async fn store_token(&self, token: &AuthToken) -> Result<(), AuthError> {
        self.ensure_dir()?;
        let path = self.path.clone();
        let value = token.as_str().to_string();
        tokio::task::spawn_blocking(move || {
            std::fs::write(&path, value).map_err(|e| AuthError::Unexpected {
                message: e.to_string(),
            })
        })
        .await
        .map_err(|e| AuthError::Unexpected {
            message: e.to_string(),
        })??;
        debug!("token 已写入本地文件");
        Ok(())
    }

    async fn delete_token(&self) -> Result<(), AuthError> {
        let path = self.path.clone();
        tokio::task::spawn_blocking(move || {
            match std::fs::remove_file(&path) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(AuthError::Unexpected {
                    message: e.to_string(),
                }),
            }
        })
        .await
        .map_err(|e| AuthError::Unexpected {
            message: e.to_string(),
        })?
    }
}
