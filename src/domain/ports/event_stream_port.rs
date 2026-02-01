//! 事件流端口：连接后返回 SDKEvent 流，透传。

use async_trait::async_trait;
use tokio::sync::broadcast;

use crate::domain::entities::AuthToken;
use crate::domain::errors::AuthError;
use crate::domain::SDKEvent;

/// 事件流端口：connect 后返回 `Receiver<SDKEvent>`，上层直接处理 SDK 事件。
#[async_trait]
pub trait EventStreamPort: Send + Sync {
    /// 使用 token 连接并认证，返回事件接收器。若尚未初始化则先初始化 SDK 并 authenticate。
    async fn connect(
        &self,
        token: &AuthToken,
    ) -> Result<broadcast::Receiver<SDKEvent>, AuthError>;

    fn disconnect(&self);
    fn is_connected(&self) -> bool;
}
