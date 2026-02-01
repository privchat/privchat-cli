//! Port definition for image caching.

use std::sync::Arc;

use crate::domain::entities::{ImageId, LoadedImage};

/// Result type for cache operations.
pub type CacheResult<T> = std::result::Result<T, CacheError>;

/// Errors that can occur during cache operations.
#[derive(Debug, Clone, thiserror::Error)]
pub enum CacheError {
    /// Image not found in cache.
    #[error("Image not found: {0}")]
    NotFound(String),
    /// Failed to decode image.
    #[error("Decode error: {0}")]
    DecodeError(String),
    /// I/O error during cache operation.
    #[error("IO error: {0}")]
    IoError(String),
    /// Network error during download.
    #[error("Network error: {0}")]
    NetworkError(String),
}

/// Port for image caching operations.
/// Implementations must be thread-safe.
#[async_trait::async_trait]
pub trait ImageCachePort: Send + Sync {
    /// Attempts to get an image from the cache.
    /// Returns None if not cached.
    async fn get(&self, id: &ImageId) -> Option<Arc<image::DynamicImage>>;

    /// Stores an image in the cache.
    async fn put(&self, id: ImageId, image: Arc<image::DynamicImage>);

    /// Removes an image from the cache.
    async fn evict(&self, id: &ImageId);

    /// Returns the current number of cached images.
    fn len(&self) -> usize;

    /// Returns true if the cache is empty.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Clears all images from the cache.
    async fn clear(&self);
}

/// Port for loading images from various sources.
#[async_trait::async_trait]
pub trait ImageLoaderPort: Send + Sync {
    /// Loads an image, checking caches first then network.
    /// Returns the loaded image with source information.
    async fn load(&self, id: &ImageId, url: &str) -> CacheResult<LoadedImage>;

    /// Prefetches images into cache without blocking.
    fn prefetch(&self, id: ImageId, url: String);

    /// Cancels any pending load for the given ID.
    fn cancel(&self, id: &ImageId);
}
