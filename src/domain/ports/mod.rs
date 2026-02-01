mod auth_port;
mod chat_data_port;
mod discord_data_port;
mod event_stream_port;
mod gateway_port;
mod image_cache_port;
mod token_storage_port;

pub use auth_port::AuthPort;
pub use chat_data_port::ChatDataPort;
pub use discord_data_port::{
    DirectMessageChannel, DiscordDataPort, EditMessageRequest, FetchMessagesOptions,
    SendMessageRequest,
};
pub use event_stream_port::EventStreamPort;
pub use gateway_port::{GatewayEvent, GatewayPort};
pub use image_cache_port::{CacheError, CacheResult, ImageCachePort, ImageLoaderPort};
pub use token_storage_port::TokenStoragePort;

#[cfg(test)]
pub mod mocks {
    pub use super::auth_port::mock::MockAuthPort;
    pub use super::token_storage_port::mock::MockTokenStorage;
}
