//! 会话与消息数据端口（以 privchat 类型为准，透传）。

use async_trait::async_trait;

use crate::domain::errors::AuthError;
use crate::domain::{ChannelListEntry, TimelineMessage};

/// 会话/消息数据端口：get_channels、get_messages、send_message 等，直接返回 SDK 类型，透传。
#[async_trait]
pub trait ChatDataPort: Send + Sync {
    /// 获取会话列表。`limit`/`offset` 分页。
    async fn get_channels(
        &self,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<Vec<ChannelListEntry>, AuthError>;

    /// 获取某会话的消息。`before_id` 为 None 表示最新一批。
    async fn get_messages(
        &self,
        channel_id: u64,
        limit: u32,
        before_id: Option<u64>,
    ) -> Result<Vec<TimelineMessage>, AuthError>;

    /// 发送文本消息，返回 local_message_id。
    async fn send_message(&self, channel_id: u64, content: &str) -> Result<u64, AuthError>;

    /// 标记已读。
    async fn mark_as_read(
        &self,
        channel_id: u64,
        message_id: u64,
    ) -> Result<(), AuthError>;

    /// 发送输入状态（typing）。
    async fn send_typing(&self, channel_id: u64) -> Result<(), AuthError>;
}
