//! PrivchatService：实现 AuthPort / ChatDataPort / 事件流，透传 privchat-sdk-ffi。

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use tokio::sync::RwLock;
use tokio::time::{Duration, sleep};

use crate::domain::entities::{AuthToken, User};
use crate::domain::errors::AuthError;
use crate::domain::ports::{AuthPort, ChatDataPort, EventStreamPort};
use crate::domain::{ChannelListEntry, SDKEvent, TimelineMessage};

use privchat_sdk_ffi::{
    ConnectionState, PrivchatClient, PrivchatConfig, ServerEndpoint, TransportProtocol,
    TypingActionType,
};

/// 本机持久化 device_id 文件名（与 login/register/authenticate 共用，服务端要求同机同 device_id）。
const DEVICE_ID_FILE: &str = "device_id";

/// Device-id contract: a random UUID v4 (RFC 4122 variant), lowercase hyphenated.
/// The servers reject anything else (uppercase, unhyphenated, v1/v5, free-form strings).
fn is_canonical_device_id(value: &str) -> bool {
    match uuid::Uuid::parse_str(value) {
        Ok(id) => {
            id.get_version_num() == 4
                && id.get_variant() == uuid::Variant::RFC4122
                && id.hyphenated().to_string() == value
        }
        Err(_) => false,
    }
}

/// Reuse the persisted device id when it is a canonical v4; otherwise generate a fresh
/// random v4 once and persist it so every later login/register/connect reuses it.
fn load_or_create_device_id(data_dir: &Path) -> Result<String, AuthError> {
    let path = data_dir.join(DEVICE_ID_FILE);
    if path.exists() {
        let s = std::fs::read_to_string(&path)
            .map_err(|e| AuthError::rejected(format!("读取 device_id 失败: {}", e)))?;
        let s = s.trim();
        if is_canonical_device_id(s) {
            return Ok(s.to_string());
        }
    }
    std::fs::create_dir_all(data_dir)
        .map_err(|e| AuthError::rejected(format!("创建数据目录失败: {}", e)))?;
    let device_id = uuid::Uuid::new_v4().to_string();
    std::fs::write(&path, &device_id)
        .map_err(|e| AuthError::rejected(format!("写入 device_id 失败: {}", e)))?;
    Ok(device_id)
}

/// Privchat 协议适配：透传 SDK，不做 DTO 转换。
pub struct PrivchatService {
    /// 数据目录（用于初始化 SDK）
    data_dir: PathBuf,
    /// 服务器 URL（默认 quic://127.0.0.1:8082；也可用 wss://127.0.0.1:8081）
    server_url: String,
    /// 已初始化的 SDK（首次 validate_token 或 connect 时创建）
    sdk: Arc<RwLock<Option<Arc<PrivchatClient>>>>,
    /// 当前登录用户 ID（发送消息等需要）
    current_user_id: Arc<RwLock<Option<u64>>>,
    /// 事件桥接通道（next_event 轮询 -> broadcast）
    event_tx: tokio::sync::broadcast::Sender<SDKEvent>,
    /// 保证只启动一次事件轮询任务
    event_pump_started: AtomicBool,
}

impl PrivchatService {
    /// 创建服务。`data_dir` 为 SDK 数据目录，`server_url` 为连接地址。
    pub fn new(data_dir: PathBuf, server_url: String) -> Self {
        let (event_tx, _) = tokio::sync::broadcast::channel(256);
        Self {
            data_dir,
            server_url,
            sdk: Arc::new(RwLock::new(None)),
            current_user_id: Arc::new(RwLock::new(None)),
            event_tx,
            event_pump_started: AtomicBool::new(false),
        }
    }

    /// 获取或初始化 SDK。若尚未初始化则根据当前 config 创建。
    async fn get_or_init_sdk(&self) -> Result<Arc<PrivchatClient>, AuthError> {
        let mut guard = self.sdk.write().await;
        if let Some(ref sdk) = *guard {
            return Ok(Arc::clone(sdk));
        }
        let config = Self::build_config(&self.data_dir, &self.server_url)?;
        let sdk =
            Arc::new(PrivchatClient::new(config).map_err(|e| AuthError::rejected(e.to_string()))?);
        self.ensure_event_pump(Arc::clone(&sdk));
        *guard = Some(Arc::clone(&sdk));
        Ok(sdk)
    }

    fn ensure_event_pump(&self, sdk: Arc<PrivchatClient>) {
        if self
            .event_pump_started
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }
        let tx = self.event_tx.clone();
        tokio::spawn(async move {
            loop {
                match sdk.next_event(1000).await {
                    Ok(Some(event)) => {
                        let _ = tx.send(event);
                    }
                    Ok(None) => {}
                    Err(_) => sleep(Duration::from_millis(300)).await,
                }
            }
        });
    }

    fn build_config(data_dir: &Path, server_url: &str) -> Result<PrivchatConfig, AuthError> {
        let endpoints = parse_server_url(server_url)?;
        Ok(PrivchatConfig {
            endpoints,
            connection_timeout_secs: 10,
            data_dir: data_dir.to_string_lossy().to_string(),
            spki_pins: std::env::var("PRIVCHAT_SPKI_PINS")
                .unwrap_or_default()
                .split(',')
                .map(str::trim)
                .filter(|pin| !pin.is_empty())
                .map(str::to_owned)
                .collect(),
        })
    }

    /// 本机持久化 device_id：首次运行生成并写入 data_dir/device_id，之后复用，保证与服务端校验一致。
    fn get_or_create_device_id(&self) -> Result<String, AuthError> {
        load_or_create_device_id(&self.data_dir)
    }

    async fn resolve_channel_type(sdk: &PrivchatClient, channel_id: u64) -> Result<i32, AuthError> {
        let channel = sdk
            .get_channel_by_id(channel_id)
            .await
            .map_err(|e| AuthError::rejected(e.to_string()))?;
        Ok(channel.map_or(1, |c| c.channel_type))
    }

    fn parse_token(token: &AuthToken) -> Result<(u64, String), AuthError> {
        let token_str = token.as_str();
        if let Some(colon) = token_str.find(':') {
            let uid = token_str[..colon]
                .parse::<u64>()
                .map_err(|_| AuthError::rejected("invalid userId in token"))?;
            Ok((uid, token_str[colon + 1..].to_string()))
        } else {
            Err(AuthError::rejected(
                "token 格式应为 userId:token，例如 12345:your_token",
            ))
        }
    }
}

#[async_trait]
impl AuthPort for PrivchatService {
    async fn validate_token(&self, token: &AuthToken) -> Result<User, AuthError> {
        let sdk = self.get_or_init_sdk().await?;
        let (user_id, token_only) = Self::parse_token(token)?;
        let device_id = self.get_or_create_device_id()?;
        sdk.connect()
            .await
            .map_err(|e| AuthError::rejected(e.to_string()))?;
        sdk.authenticate(user_id, token_only, device_id)
            .await
            .map_err(|e| AuthError::rejected(e.to_string()))?;
        *self.current_user_id.write().await = Some(user_id);
        let uid_str = user_id.to_string();
        Ok(User::new(
            crate::domain::entities::UserId::from(user_id),
            uid_str.clone(),
            "0",
            None,
            false,
            None,
        ))
    }

    async fn login(&self, username: &str, password: &str) -> Result<(User, AuthToken), AuthError> {
        let sdk = self.get_or_init_sdk().await?;
        sdk.connect()
            .await
            .map_err(|e| AuthError::rejected(e.to_string()))?;
        let device_id = self.get_or_create_device_id()?;
        let result = sdk
            .login(
                username.to_string(),
                password.to_string(),
                device_id.clone(),
            )
            .await
            .map_err(|e| AuthError::rejected(e.to_string()))?;
        sdk.authenticate(result.user_id, result.token.clone(), device_id)
            .await
            .map_err(|e| AuthError::rejected(e.to_string()))?;
        *self.current_user_id.write().await = Some(result.user_id);

        let user = User::new(
            crate::domain::entities::UserId::from(result.user_id),
            result.user_id.to_string(),
            "0",
            Some(username.to_string()),
            false,
            None,
        );
        let token = AuthToken::new_unchecked(format!("{}:{}", result.user_id, result.token));
        Ok((user, token))
    }

    async fn register(
        &self,
        username: &str,
        password: &str,
    ) -> Result<(User, AuthToken), AuthError> {
        let sdk = self.get_or_init_sdk().await?;
        sdk.connect()
            .await
            .map_err(|e| AuthError::rejected(e.to_string()))?;
        let device_id = self.get_or_create_device_id()?;
        let result = sdk
            .register(
                username.to_string(),
                password.to_string(),
                device_id.clone(),
            )
            .await
            .map_err(|e| AuthError::rejected(e.to_string()))?;
        sdk.authenticate(result.user_id, result.token.clone(), device_id)
            .await
            .map_err(|e| AuthError::rejected(e.to_string()))?;
        *self.current_user_id.write().await = Some(result.user_id);

        let user = User::new(
            crate::domain::entities::UserId::from(result.user_id),
            result.user_id.to_string(),
            "0",
            Some(username.to_string()),
            false,
            None,
        );
        let token = AuthToken::new_unchecked(format!("{}:{}", result.user_id, result.token));
        Ok((user, token))
    }

    async fn health_check(&self) -> Result<(), AuthError> {
        let sdk = self.get_or_init_sdk().await?;
        let _ = sdk
            .get_connection_state()
            .await
            .map_err(|e| AuthError::rejected(e.to_string()))?;
        Ok(())
    }
}

#[async_trait]
impl ChatDataPort for PrivchatService {
    async fn get_channels(
        &self,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<Vec<ChannelListEntry>, AuthError> {
        let sdk = self.get_or_init_sdk().await?;
        let channels = sdk
            .list_channels(limit.unwrap_or(200) as u64, offset.unwrap_or(0) as u64)
            .await
            .map_err(|e| AuthError::rejected(e.to_string()))?;

        Ok(channels
            .into_iter()
            .map(|c| ChannelListEntry {
                channel_id: c.channel_id,
                channel_type: c.channel_type,
                name: if c.channel_name.is_empty() {
                    format!("channel-{}", c.channel_id)
                } else {
                    c.channel_name
                },
                mentions: c.unread_count.max(0) as u32,
            })
            .collect())
    }

    async fn get_messages(
        &self,
        channel_id: u64,
        limit: u32,
        before_id: Option<u64>,
    ) -> Result<Vec<TimelineMessage>, AuthError> {
        let sdk = self.get_or_init_sdk().await?;
        let channel_type = Self::resolve_channel_type(&sdk, channel_id).await?;
        let mut messages = sdk
            .list_messages(channel_id, channel_type, limit as u64, 0)
            .await
            .map_err(|e| AuthError::rejected(e.to_string()))?
            .into_iter()
            .map(storage_message_to_timeline)
            .collect::<Vec<_>>();

        if let Some(before) = before_id {
            messages.retain(|m| m.id < before);
            messages.truncate(limit as usize);
        }

        Ok(messages)
    }

    async fn send_message(&self, channel_id: u64, content: &str) -> Result<u64, AuthError> {
        let sdk = self.get_or_init_sdk().await?;
        let channel_type = Self::resolve_channel_type(&sdk, channel_id).await?;
        let user_id = self
            .current_user_id
            .read()
            .await
            .ok_or_else(|| AuthError::rejected("not authenticated"))?;

        sdk.send_message(channel_id, channel_type, user_id, content.to_string())
            .await
            .map_err(|e| AuthError::rejected(e.to_string()))
    }

    async fn mark_as_read(&self, channel_id: u64, message_id: u64) -> Result<(), AuthError> {
        let sdk = self.get_or_init_sdk().await?;
        let message = sdk
            .get_message_by_id(message_id)
            .await
            .map_err(|e| AuthError::rejected(e.to_string()))?
            .ok_or_else(|| AuthError::rejected("read cursor message not found"))?;
        if message.channel_id != channel_id {
            return Err(AuthError::rejected(
                "read cursor belongs to another channel",
            ));
        }
        let Some(pts) = message.pts.filter(|pts| *pts > 0) else {
            return Ok(());
        };
        sdk.mark_read_to_pts(channel_id, pts)
            .await
            .map(|_| ())
            .map_err(|e| AuthError::rejected(e.to_string()))
    }

    async fn send_typing(&self, channel_id: u64) -> Result<(), AuthError> {
        let sdk = self.get_or_init_sdk().await?;
        let channel_type = Self::resolve_channel_type(&sdk, channel_id).await?;
        sdk.send_typing(channel_id, channel_type, true, TypingActionType::Typing)
            .await
            .map_err(|e| AuthError::rejected(e.to_string()))
    }
}

#[async_trait]
impl EventStreamPort for PrivchatService {
    async fn connect(
        &self,
        token: &AuthToken,
    ) -> Result<tokio::sync::broadcast::Receiver<SDKEvent>, AuthError> {
        let sdk = self.get_or_init_sdk().await?;
        let (user_id, token_only) = Self::parse_token(token)?;
        let device_id = self.get_or_create_device_id()?;

        let state = sdk
            .get_connection_state()
            .await
            .map_err(|e| AuthError::rejected(e.to_string()))?;

        if matches!(state, ConnectionState::New | ConnectionState::Shutdown) {
            sdk.connect()
                .await
                .map_err(|e| AuthError::rejected(e.to_string()))?;
        }

        if matches!(
            state,
            ConnectionState::Connected | ConnectionState::LoggedIn | ConnectionState::New
        ) {
            sdk.authenticate(user_id, token_only, device_id)
                .await
                .map_err(|e| AuthError::rejected(e.to_string()))?;
        }

        *self.current_user_id.write().await = Some(user_id);
        Ok(self.event_tx.subscribe())
    }

    fn disconnect(&self) {
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            let sdk_holder = Arc::clone(&self.sdk);
            handle.spawn(async move {
                if let Some(sdk) = sdk_holder.read().await.as_ref() {
                    let _ = sdk.disconnect().await;
                }
            });
        }
    }

    fn is_connected(&self) -> bool {
        false
    }
}

fn storage_message_to_timeline(m: privchat_sdk_ffi::StoredMessage) -> TimelineMessage {
    TimelineMessage {
        id: m.message_id,
        server_message_id: m.server_message_id,
        channel_id: m.channel_id,
        channel_type: m.channel_type,
        from_uid: m.from_uid,
        content: m.content,
        message_type: m.message_type,
        timestamp: m.created_at.max(0) as u64,
        pts: m.pts.unwrap_or(0),
    }
}

/// 简单解析 server_url 为单端点。支持 wss://, ws://, quic://, tcp://。
fn parse_server_url(url: &str) -> Result<Vec<ServerEndpoint>, AuthError> {
    let url = url.trim();
    let (protocol, rest) = url
        .split_once("://")
        .ok_or_else(|| AuthError::rejected("server_url 需包含 ://"))?;
    let (host, path) = rest
        .split_once('/')
        .map(|(h, p)| (h, Some(format!("/{}", p))))
        .unwrap_or((rest, None));
    let (host, port) = if let Some(colon) = host.rfind(':') {
        let p: u16 = host[colon + 1..]
            .parse()
            .map_err(|_| AuthError::rejected("invalid port"))?;
        (host[..colon].to_string(), p)
    } else {
        let (default_host, default_port) = match protocol.to_lowercase().as_str() {
            "wss" | "ws" => (
                host.to_string(),
                if protocol.eq_ignore_ascii_case("wss") {
                    443
                } else {
                    80
                },
            ),
            "quic" => (host.to_string(), 8082),
            "tcp" => (host.to_string(), 8080),
            _ => return Err(AuthError::rejected("未知协议，支持 wss/ws/quic/tcp")),
        };
        (default_host, default_port)
    };
    let (protocol_enum, use_tls) = match protocol.to_lowercase().as_str() {
        "wss" => (TransportProtocol::WebSocket, true),
        "ws" => (TransportProtocol::WebSocket, false),
        "quic" => (TransportProtocol::Quic, false),
        "tcp" => (TransportProtocol::Tcp, false),
        _ => return Err(AuthError::rejected("未知协议")),
    };
    let endpoint = ServerEndpoint {
        protocol: protocol_enum,
        host,
        port,
        path,
        use_tls,
    };
    Ok(vec![endpoint])
}

#[cfg(test)]
mod device_id_tests {
    use super::*;

    #[test]
    fn canonical_device_id_accepts_only_lowercase_v4() {
        let v4 = uuid::Uuid::new_v4().to_string();
        assert!(is_canonical_device_id(&v4));
        assert!(!is_canonical_device_id(&v4.to_uppercase()));
        assert!(!is_canonical_device_id(&v4.replace('-', "")));
        assert!(!is_canonical_device_id(""));
        assert!(!is_canonical_device_id("dev-1"));
        assert!(!is_canonical_device_id("6ba7b810-9dad-11d1-80b4-00c04fd430c8"));
    }

    #[test]
    fn persisted_device_id_is_reused_and_invalid_one_replaced() {
        let dir = std::env::temp_dir().join(format!("privchat-cli-devid-{}", uuid::Uuid::new_v4()));
        let first = load_or_create_device_id(&dir).unwrap();
        assert!(is_canonical_device_id(&first));
        assert_eq!(load_or_create_device_id(&dir).unwrap(), first);

        let upper = first.to_uppercase();
        std::fs::write(dir.join(DEVICE_ID_FILE), &upper).unwrap();
        let replaced = load_or_create_device_id(&dir).unwrap();
        assert!(is_canonical_device_id(&replaced));
        assert_ne!(replaced, upper);
        assert_eq!(load_or_create_device_id(&dir).unwrap(), replaced);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
