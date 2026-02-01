//! Domain layer with core business entities and port definitions.

// 单一真相源：直接使用 privchat-sdk 类型，禁止镜像 DTO
pub use privchat_sdk::events::{ChannelListEntry, SDKEvent, TimelineMessage};

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
