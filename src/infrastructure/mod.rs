//! Infrastructure layer with external service adapters.

pub mod clipboard;
/// Application configuration.
pub mod config;
/// Privchat 协议适配（透传 privchat-sdk）。
pub mod privchat;
/// Image handling (caching, loading, CDN optimization).
pub mod image;
/// Application state persistence.
pub mod state_store;
/// Token storage adapters.
pub mod storage;

pub use clipboard::ClipboardService;
pub use config::{AppConfig, LogLevel};
pub use image::{
    CacheStats, DiskImageCache, ImageLoadedEvent, ImageLoader, ImageLoaderConfig, MemoryImageCache,
    extract_attachment_id, is_discord_cdn_url, optimize_cdn_url, optimize_cdn_url_default,
};
pub use state_store::StateStore;
pub use storage::{FileTokenStorage, KeyringTokenStorage};
pub use privchat::PrivchatService;
