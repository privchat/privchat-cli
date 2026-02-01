//! Image handling infrastructure.
//!
//! This module provides:
//! - Memory caching with LRU eviction
//! - Disk caching for persistence
//! - Discord CDN URL optimization
//! - Async image loading pipeline

pub mod discord_cdn;
pub mod disk_cache;
pub mod loader;
pub mod memory_cache;

pub use discord_cdn::{
    extract_attachment_id, is_discord_cdn_url, optimize_cdn_url, optimize_cdn_url_default,
};
pub use disk_cache::DiskImageCache;
pub use loader::{ImageLoadedEvent, ImageLoader, ImageLoaderConfig};
pub use memory_cache::{CacheStats, MemoryImageCache};
