use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::domain::entities::{AuthToken, ChannelId, GuildId, Message, MessageId};
use crate::domain::errors::AuthError;

#[derive(Debug, Clone)]
pub enum GatewayEvent {
    Ready {
        session_id: String,
    },
    Resumed,
    Reconnecting {
        attempt: u32,
    },
    Disconnected {
        reason: String,
        can_resume: bool,
    },
    HeartbeatAck {
        latency_ms: u64,
    },
    MessageCreate {
        message: Message,
    },
    MessageUpdate {
        message: Message,
    },
    MessageDelete {
        message_id: MessageId,
        channel_id: ChannelId,
        guild_id: Option<GuildId>,
    },
    MessageDeleteBulk {
        message_ids: Vec<MessageId>,
        channel_id: ChannelId,
        guild_id: Option<GuildId>,
    },
    TypingStart {
        channel_id: ChannelId,
        guild_id: Option<GuildId>,
        user_id: String,
        username: Option<String>,
    },
    PresenceUpdate {
        user_id: String,
        guild_id: Option<GuildId>,
        status: String,
    },
    ReactionAdd {
        user_id: String,
        channel_id: ChannelId,
        message_id: MessageId,
        emoji: String,
    },
    ReactionRemove {
        user_id: String,
        channel_id: ChannelId,
        message_id: MessageId,
        emoji: String,
    },
    ChannelCreate {
        channel_id: ChannelId,
        guild_id: Option<GuildId>,
        name: String,
    },
    ChannelUpdate {
        channel_id: ChannelId,
        guild_id: Option<GuildId>,
        name: String,
    },
    ChannelDelete {
        channel_id: ChannelId,
        guild_id: Option<GuildId>,
    },
    GuildCreate {
        guild_id: GuildId,
        name: String,
    },
    GuildUpdate {
        guild_id: GuildId,
        name: String,
    },
    GuildDelete {
        guild_id: GuildId,
    },
    Error {
        message: String,
        recoverable: bool,
    },
}

impl GatewayEvent {
    #[must_use]
    pub const fn is_message_event(&self) -> bool {
        matches!(
            self,
            Self::MessageCreate { .. }
                | Self::MessageUpdate { .. }
                | Self::MessageDelete { .. }
                | Self::MessageDeleteBulk { .. }
        )
    }

    #[must_use]
    pub const fn is_connection_event(&self) -> bool {
        matches!(
            self,
            Self::Ready { .. }
                | Self::Resumed
                | Self::Reconnecting { .. }
                | Self::Disconnected { .. }
        )
    }

    #[must_use]
    pub const fn channel_id(&self) -> Option<ChannelId> {
        match self {
            Self::MessageCreate { message } | Self::MessageUpdate { message } => {
                Some(message.channel_id())
            }
            Self::MessageDelete { channel_id, .. }
            | Self::MessageDeleteBulk { channel_id, .. }
            | Self::TypingStart { channel_id, .. }
            | Self::ReactionAdd { channel_id, .. }
            | Self::ReactionRemove { channel_id, .. }
            | Self::ChannelCreate { channel_id, .. }
            | Self::ChannelUpdate { channel_id, .. }
            | Self::ChannelDelete { channel_id, .. } => Some(*channel_id),
            _ => None,
        }
    }

    #[must_use]
    pub const fn guild_id(&self) -> Option<GuildId> {
        match self {
            Self::MessageDelete { guild_id, .. }
            | Self::MessageDeleteBulk { guild_id, .. }
            | Self::TypingStart { guild_id, .. }
            | Self::PresenceUpdate { guild_id, .. }
            | Self::ChannelCreate { guild_id, .. }
            | Self::ChannelUpdate { guild_id, .. }
            | Self::ChannelDelete { guild_id, .. } => *guild_id,
            Self::GuildCreate { guild_id, .. }
            | Self::GuildUpdate { guild_id, .. }
            | Self::GuildDelete { guild_id } => Some(*guild_id),
            _ => None,
        }
    }
}

#[async_trait]
pub trait GatewayPort: Send + Sync {
    /// Connects to the Discord Gateway.
    ///
    /// # Errors
    ///
    /// Returns `AuthError` if connection fails.
    fn connect(
        &mut self,
        token: &AuthToken,
    ) -> Result<mpsc::UnboundedReceiver<GatewayEvent>, AuthError>;

    fn disconnect(&self);

    fn is_connected(&self) -> bool;
}
