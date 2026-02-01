//! 输入状态（typing）展示逻辑，与协议无关。

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::domain::entities::ChannelId;

const TYPING_INDICATOR_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone)]
pub struct TypingUser {
    pub user_id: String,
    pub username: String,
    pub channel_id: ChannelId,
    pub started_at: Instant,
}

impl TypingUser {
    #[must_use]
    pub fn new(user_id: String, username: String, channel_id: ChannelId) -> Self {
        Self {
            user_id,
            username,
            channel_id,
            started_at: Instant::now(),
        }
    }

    #[must_use]
    pub fn is_expired(&self, timeout: Duration) -> bool {
        self.started_at.elapsed() > timeout
    }

    pub fn refresh(&mut self) {
        self.started_at = Instant::now();
    }
}

pub struct TypingIndicatorManager {
    typing_users: HashMap<ChannelId, Vec<TypingUser>>,
    timeout: Duration,
}

impl TypingIndicatorManager {
    #[must_use]
    pub fn new() -> Self {
        Self {
            typing_users: HashMap::new(),
            timeout: TYPING_INDICATOR_TIMEOUT,
        }
    }

    #[must_use]
    pub fn with_timeout(timeout: Duration) -> Self {
        Self {
            typing_users: HashMap::new(),
            timeout,
        }
    }

    pub fn add_typing(&mut self, channel_id: ChannelId, user_id: String, username: String) {
        let users = self.typing_users.entry(channel_id).or_default();

        if let Some(existing) = users.iter_mut().find(|u| u.user_id == user_id) {
            existing.refresh();
        } else {
            users.push(TypingUser::new(user_id, username, channel_id));
        }
    }

    pub fn remove_typing(&mut self, channel_id: ChannelId, user_id: &str) {
        if let Some(users) = self.typing_users.get_mut(&channel_id) {
            users.retain(|u| u.user_id != user_id);
            if users.is_empty() {
                self.typing_users.remove(&channel_id);
            }
        }
    }

    pub fn clear_channel(&mut self, channel_id: ChannelId) {
        self.typing_users.remove(&channel_id);
    }

    pub fn cleanup_expired(&mut self) {
        let timeout = self.timeout;
        for users in self.typing_users.values_mut() {
            users.retain(|u| !u.is_expired(timeout));
        }
        self.typing_users.retain(|_, users| !users.is_empty());
    }

    #[must_use]
    pub fn get_typing_users(&self, channel_id: ChannelId) -> Vec<&TypingUser> {
        self.typing_users
            .get(&channel_id)
            .map(|users| {
                users
                    .iter()
                    .filter(|u| !u.is_expired(self.timeout))
                    .collect()
            })
            .unwrap_or_default()
    }

    #[must_use]
    pub fn format_typing_indicator(&self, channel_id: ChannelId) -> Option<String> {
        let users = self.get_typing_users(channel_id);

        match users.len() {
            0 => None,
            1 => Some(format!("{} is typing...", users[0].username)),
            2 => Some(format!(
                "{} and {} are typing...",
                users[0].username, users[1].username
            )),
            3 => Some(format!(
                "{}, {} and {} are typing...",
                users[0].username, users[1].username, users[2].username
            )),
            _ => Some("Several people are typing...".to_string()),
        }
    }

    #[must_use]
    pub fn has_typing_users(&self, channel_id: ChannelId) -> bool {
        !self.get_typing_users(channel_id).is_empty()
    }

    #[must_use]
    pub fn typing_count(&self, channel_id: ChannelId) -> usize {
        self.get_typing_users(channel_id).len()
    }
}

impl Default for TypingIndicatorManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_typing() {
        let mut manager = TypingIndicatorManager::new();
        let channel = ChannelId(123);

        manager.add_typing(channel, "user1".into(), "Alice".into());
        assert!(manager.has_typing_users(channel));
        assert_eq!(manager.typing_count(channel), 1);
    }

    #[test]
    fn test_remove_typing() {
        let mut manager = TypingIndicatorManager::new();
        let channel = ChannelId(123);

        manager.add_typing(channel, "user1".into(), "Alice".into());
        manager.remove_typing(channel, "user1");
        assert!(!manager.has_typing_users(channel));
    }

    #[test]
    fn test_format_single_user() {
        let mut manager = TypingIndicatorManager::new();
        let channel = ChannelId(123);

        manager.add_typing(channel, "user1".into(), "Alice".into());
        assert_eq!(
            manager.format_typing_indicator(channel),
            Some("Alice is typing...".to_string())
        );
    }

    #[test]
    fn test_clear_channel() {
        let mut manager = TypingIndicatorManager::new();
        let channel = ChannelId(123);

        manager.add_typing(channel, "user1".into(), "Alice".into());
        manager.add_typing(channel, "user2".into(), "Bob".into());
        manager.clear_channel(channel);

        assert!(!manager.has_typing_users(channel));
    }
}
