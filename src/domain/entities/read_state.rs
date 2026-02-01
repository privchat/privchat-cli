//! Read state entity.

use serde::{Deserialize, Serialize};

use super::{ChannelId, MessageId};

/// Read state for a channel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadState {
    /// Channel ID.
    pub channel_id: ChannelId,
    /// ID of the last read message.
    pub last_read_message_id: Option<MessageId>,
    /// Number of mentions.
    #[serde(default)]
    pub mention_count: u32,
}

impl ReadState {
    /// Creates a new read state.
    #[must_use]
    pub fn new(channel_id: ChannelId, last_read_message_id: Option<MessageId>) -> Self {
        Self {
            channel_id,
            last_read_message_id,
            mention_count: 0,
        }
    }

    /// Sets the mention count.
    #[must_use]
    pub const fn with_mention_count(mut self, count: u32) -> Self {
        self.mention_count = count;
        self
    }
}
