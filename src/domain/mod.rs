//! Domain layer with core business entities and port definitions.

/// SDK event type from privchat-rust FFI client.
pub type SDKEvent = privchat_sdk_ffi::SdkEvent;

/// Simplified channel list entry used by TUI.
#[derive(Debug, Clone, Default)]
pub struct ChannelListEntry {
    pub channel_id: u64,
    pub channel_type: i32,
    pub name: String,
    pub mentions: u32,
}

/// Simplified timeline message used by TUI.
#[derive(Debug, Clone, Default)]
pub struct TimelineMessage {
    pub id: u64,
    pub server_message_id: Option<u64>,
    pub channel_id: u64,
    pub channel_type: i32,
    pub from_uid: u64,
    pub content: String,
    pub message_type: i32,
    pub timestamp: u64,
    pub pts: u64,
}

/// Connection status definitions.
pub mod connection;
/// Entity definitions.
pub mod entities;
/// Error types.
pub mod errors;
/// Keybinding definitions.
pub mod keybinding;
/// Port definitions.
pub mod ports;

pub use connection::ConnectionStatus;
pub use entities::{AuthToken, User};
pub use errors::AuthError;
pub use ports::{AuthPort, ChatDataPort, EventStreamPort, TokenStoragePort};
