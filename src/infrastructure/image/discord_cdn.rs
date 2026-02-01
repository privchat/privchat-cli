//! Discord CDN URL optimization.

/// Default target width for optimized images.
pub const DEFAULT_WIDTH: u32 = 800;

/// Default target height for optimized images.
pub const DEFAULT_HEIGHT: u32 = 600;

/// Optimizes a Discord CDN URL by adding format and size parameters.
/// This significantly reduces bandwidth usage and RAM consumption.
///
/// # Arguments
/// * `url` - The original image URL
/// * `width` - Target width (default: 800)
/// * `height` - Target height (default: 600)
///
/// # Returns
/// The optimized URL with query parameters, or the original URL if not a Discord CDN URL.
#[must_use]
pub fn optimize_cdn_url(url: &str, width: u32, height: u32) -> String {
    if !url.contains("cdn.discordapp.com") && !url.contains("media.discordapp.net") {
        return url.to_string();
    }

    let (base_url, existing_params) = if let Some(idx) = url.find('?') {
        (&url[..idx], Some(&url[idx + 1..]))
    } else {
        (url, None)
    };

    let mut params = vec![
        format!("format=webp"),
        format!("width={width}"),
        format!("height={height}"),
        format!("quality=low"),
    ];

    if let Some(existing) = existing_params {
        for param in existing.split('&') {
            let key = param.split('=').next().unwrap_or("");
            if !["format", "width", "height", "size", "quality"].contains(&key) {
                params.push(param.to_string());
            }
        }
    }

    format!("{}?{}", base_url, params.join("&"))
}

/// Optimizes a URL with default dimensions.
#[must_use]
pub fn optimize_cdn_url_default(url: &str) -> String {
    optimize_cdn_url(url, DEFAULT_WIDTH, DEFAULT_HEIGHT)
}

/// Extracts the attachment ID from a Discord CDN URL.
#[must_use]
pub fn extract_attachment_id(url: &str) -> Option<String> {
    if !url.contains("discordapp.com") && !url.contains("discordapp.net") {
        return None;
    }

    let path = url.split("attachments/").nth(1)?;

    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() >= 2 {
        Some(parts[1].split('?').next()?.to_string())
    } else {
        None
    }
}

/// Checks if a URL is a Discord CDN URL.
#[must_use]
pub fn is_discord_cdn_url(url: &str) -> bool {
    url.contains("cdn.discordapp.com") || url.contains("media.discordapp.net")
}

/// Generates an avatar URL for a user.
#[must_use]
pub fn avatar_url(user_id: &str, avatar_hash: Option<&str>, discriminator: &str) -> String {
    if let Some(hash) = avatar_hash {
        let ext = if hash.starts_with("a_") { "gif" } else { "png" };
        format!("https://cdn.discordapp.com/avatars/{user_id}/{hash}.{ext}")
    } else {
        default_avatar_url(user_id, discriminator)
    }
}

/// Generates a default avatar URL.
#[must_use]
pub fn default_avatar_url(user_id: &str, discriminator: &str) -> String {
    let index = if discriminator == "0" {
        // New username system: (user_id >> 22) % 6
        let id = user_id.parse::<u64>().unwrap_or(0);
        (id >> 22) % 6
    } else {
        // Legacy system: discriminator % 5
        let disc = discriminator.parse::<u64>().unwrap_or(0);
        disc % 5
    };
    format!("https://cdn.discordapp.com/embed/avatars/{index}.png")
}

/// Generates a guild icon URL.
#[must_use]
pub fn guild_icon_url(guild_id: &str, icon_hash: Option<&str>) -> Option<String> {
    icon_hash.map(|hash| {
        let ext = if hash.starts_with("a_") { "gif" } else { "png" };
        format!("https://cdn.discordapp.com/icons/{guild_id}/{hash}.{ext}")
    })
}

/// Generates a guild banner URL.
#[must_use]
pub fn guild_banner_url(guild_id: &str, banner_hash: Option<&str>) -> Option<String> {
    banner_hash.map(|hash| {
        let ext = if hash.starts_with("a_") { "gif" } else { "png" };
        format!("https://cdn.discordapp.com/banners/{guild_id}/{hash}.{ext}")
    })
}

/// Generates a guild splash URL.
#[must_use]
pub fn guild_splash_url(guild_id: &str, splash_hash: Option<&str>) -> Option<String> {
    splash_hash.map(|hash| format!("https://cdn.discordapp.com/splashes/{guild_id}/{hash}.png"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_optimize_basic_url() {
        let url = "https://cdn.discordapp.com/attachments/123/456/image.png";
        let optimized = optimize_cdn_url_default(url);

        assert!(optimized.contains("format=webp"));
        assert!(optimized.contains("width=800"));
        assert!(optimized.contains("height=600"));
        assert!(optimized.contains("quality=low"));
    }

    #[test]
    fn test_optimize_with_existing_params() {
        let url = "https://cdn.discordapp.com/attachments/123/456/image.png?ex=abc123";
        let optimized = optimize_cdn_url_default(url);

        assert!(optimized.contains("format=webp"));
        assert!(optimized.contains("ex=abc123"));
    }

    #[test]
    fn test_non_discord_url_unchanged() {
        let url = "https://example.com/image.png";
        let optimized = optimize_cdn_url_default(url);

        assert_eq!(optimized, url);
    }

    #[test]
    fn test_extract_attachment_id() {
        let url = "https://cdn.discordapp.com/attachments/123/456789/image.png";
        let id = extract_attachment_id(url);

        assert_eq!(id, Some("456789".to_string()));
    }

    #[test]
    fn test_extract_attachment_id_with_params() {
        let url = "https://cdn.discordapp.com/attachments/123/456789/image.png?format=webp";
        let id = extract_attachment_id(url);

        assert_eq!(id, Some("456789".to_string()));
    }

    #[test]
    fn test_is_discord_cdn_url() {
        assert!(is_discord_cdn_url(
            "https://cdn.discordapp.com/attachments/1/2/img.png"
        ));
        assert!(is_discord_cdn_url(
            "https://media.discordapp.net/attachments/1/2/img.png"
        ));
        assert!(!is_discord_cdn_url("https://example.com/image.png"));
    }

    #[test]
    fn test_custom_dimensions() {
        let url = "https://cdn.discordapp.com/attachments/123/456/image.png";
        let optimized = optimize_cdn_url(url, 400, 300);

        assert!(optimized.contains("width=400"));
        assert!(optimized.contains("height=300"));
    }

    #[test]
    fn test_avatar_url_generation() {
        let user_id = "123456";
        let hash_static = "abcdef";
        let hash_animated = "a_abcdef";
        let discriminator = "1234";

        assert_eq!(
            avatar_url(user_id, Some(hash_static), discriminator),
            "https://cdn.discordapp.com/avatars/123456/abcdef.png"
        );
        assert_eq!(
            avatar_url(user_id, Some(hash_animated), discriminator),
            "https://cdn.discordapp.com/avatars/123456/a_abcdef.gif"
        );
    }

    #[test]
    fn test_default_avatar_url() {
        // Legacy: 1234 % 5 = 4
        assert_eq!(
            default_avatar_url("123456", "1234"),
            "https://cdn.discordapp.com/embed/avatars/4.png"
        );
        // New: (123456 >> 22) % 6. 123456 is small, shift 22 makes it 0. 0 % 6 = 0.
        assert_eq!(
            default_avatar_url("123456", "0"),
            "https://cdn.discordapp.com/embed/avatars/0.png"
        );
    }

    #[test]
    fn test_guild_asset_urls() {
        let guild_id = "123";
        let hash = "abc";
        let hash_anim = "a_abc";

        assert_eq!(
            guild_icon_url(guild_id, Some(hash)),
            Some("https://cdn.discordapp.com/icons/123/abc.png".to_string())
        );
        assert_eq!(
            guild_icon_url(guild_id, Some(hash_anim)),
            Some("https://cdn.discordapp.com/icons/123/a_abc.gif".to_string())
        );
        assert_eq!(guild_icon_url(guild_id, None), None);

        assert_eq!(
            guild_splash_url(guild_id, Some(hash)),
            Some("https://cdn.discordapp.com/splashes/123/abc.png".to_string())
        );
    }
}
