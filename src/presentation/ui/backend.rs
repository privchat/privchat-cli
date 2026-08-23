//! Backend worker：使用 ChatDataPort 拉取会话/消息并派发 Action。

use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use crate::domain::entities::User;
use crate::domain::ports::ChatDataPort;
use crate::domain::{ChannelListEntry, TimelineMessage};
use crate::infrastructure::image::ImageLoader;

#[derive(Debug)]
pub enum Action {
    HistoryLoaded(Vec<TimelineMessage>),
    LoadError(String),
    DataLoaded {
        user: User,
        channels: Vec<ChannelListEntry>,
        initial_channel_id: Option<u64>,
        initial_messages: Vec<TimelineMessage>,
    },
    ChannelMessagesLoaded {
        channel_id: u64,
        messages: Vec<TimelineMessage>,
    },
    ChannelMessagesLoadError {
        channel_id: u64,
        error: String,
    },
    MessageSent {
        channel_id: u64,
        local_message_id: u64,
    },
    MessageSendError(String),
    TypingIndicatorSent(u64),
    LoginSuccess {
        user: User,
        token: String,
        source: crate::application::dto::TokenSource,
    },
    LoginFailure(crate::domain::errors::AuthError),
    ImageLoaderReady(Arc<ImageLoader>),
    PasteImageLoaded(std::path::PathBuf),
    PasteTextLoaded(String),
    /// 事件流已就绪（用于 Privchat SDK 事件接收器）
    EventStreamReady(tokio::sync::broadcast::Receiver<crate::domain::SDKEvent>),
    /// 会话列表已刷新（如收到 ChannelListUpdate 后重新拉取）
    ChannelListRefreshed(Vec<ChannelListEntry>),
}

#[derive(Debug)]
pub enum BackendCommand {
    LoadChannelMessages { channel_id: u64 },
    LoadHistory {
        channel_id: u64,
        before_message_id: u64,
    },
    SendMessage {
        channel_id: u64,
        content: String,
    },
    SendTypingIndicator { channel_id: u64 },
    AcknowledgeMessage {
        channel_id: u64,
        message_id: u64,
    },
    LoadInitialData {
        user: User,
        initial_channel_id: Option<u64>,
    },
    /// 刷新会话列表（收到 SDK ChannelListUpdate 时由 UI 下发）
    RefreshChannelList,
}

pub struct Backend {
    chat_data: Arc<dyn ChatDataPort>,
    command_rx: mpsc::UnboundedReceiver<BackendCommand>,
    action_tx: mpsc::UnboundedSender<Action>,
}

impl Backend {
    pub fn new(
        chat_data: Arc<dyn ChatDataPort>,
        command_rx: mpsc::UnboundedReceiver<BackendCommand>,
        action_tx: mpsc::UnboundedSender<Action>,
    ) -> Self {
        Self {
            chat_data,
            command_rx,
            action_tx,
        }
    }

    pub async fn run(mut self) {
        info!("Backend worker started");
        while let Some(command) = self.command_rx.recv().await {
            self.handle_command(command).await;
        }
        info!("Backend worker stopped");
    }

    #[allow(clippy::too_many_lines)]
    async fn handle_command(&self, command: BackendCommand) {
        match command {
            BackendCommand::LoadChannelMessages { channel_id } => {
                match self
                    .chat_data
                    .get_messages(channel_id, 50, None)
                    .await
                {
                    Ok(messages) => {
                        debug!(channel_id, count = messages.len(), "Loaded messages for channel");
                        let _ = self.action_tx.send(Action::ChannelMessagesLoaded {
                            channel_id,
                            messages,
                        });
                    }
                    Err(e) => {
                        warn!(channel_id, error = %e, "Failed to load messages for channel");
                        let _ = self.action_tx.send(Action::ChannelMessagesLoadError {
                            channel_id,
                            error: e.to_string(),
                        });
                    }
                }
            }
            BackendCommand::LoadHistory {
                channel_id,
                before_message_id,
            } => {
                match self
                    .chat_data
                    .get_messages(channel_id, 50, Some(before_message_id))
                    .await
                {
                    Ok(messages) => {
                        debug!(count = messages.len(), "Loaded historical messages");
                        let _ = self.action_tx.send(Action::HistoryLoaded(messages));
                    }
                    Err(e) => {
                        warn!(error = %e, "Failed to load history");
                        let _ = self.action_tx.send(Action::LoadError(e.to_string()));
                    }
                }
            }
            BackendCommand::SendMessage {
                channel_id,
                content,
            } => {
                match self.chat_data.send_message(channel_id, &content).await {
                    Ok(local_message_id) => {
                        info!(channel_id, local_message_id, "Message sent");
                        let _ = self.action_tx.send(Action::MessageSent {
                            channel_id,
                            local_message_id,
                        });
                    }
                    Err(e) => {
                        error!(error = %e, "Failed to send message");
                        let _ = self.action_tx.send(Action::MessageSendError(e.to_string()));
                    }
                }
            }
            BackendCommand::SendTypingIndicator { channel_id } => {
                if let Err(e) = self.chat_data.send_typing(channel_id).await {
                    debug!(error = %e, "Failed to send typing indicator");
                } else {
                    let _ = self.action_tx.send(Action::TypingIndicatorSent(channel_id));
                }
            }
            BackendCommand::AcknowledgeMessage {
                channel_id,
                message_id,
            } => {
                if let Err(e) = self.chat_data.mark_as_read(channel_id, message_id).await {
                    warn!(error = %e, "Failed to mark as read");
                }
            }
            BackendCommand::LoadInitialData {
                user,
                initial_channel_id,
            } => {
                let channels_result = self.chat_data.get_channels(None, None).await;
                let channels = match channels_result {
                    Ok(c) => c,
                    Err(e) => {
                        error!(error = %e, "Failed to load channels");
                        Vec::new()
                    }
                };

                info!(count = channels.len(), "Backend LoadInitialData 会话数量");

                let initial_messages = if let Some(cid) = initial_channel_id {
                    self.chat_data
                        .get_messages(cid, 50, None)
                        .await
                        .unwrap_or_default()
                } else {
                    Vec::new()
                };

                let _ = self.action_tx.send(Action::DataLoaded {
                    user,
                    channels,
                    initial_channel_id,
                    initial_messages,
                });
            }
            BackendCommand::RefreshChannelList => {
                let channels_result = self.chat_data.get_channels(None, None).await;
                let channels = match channels_result {
                    Ok(c) => c,
                    Err(e) => {
                        warn!(error = %e, "Failed to refresh channel list");
                        Vec::new()
                    }
                };
                info!(count = channels.len(), "Backend 刷新会话列表完成");
                let _ = self.action_tx.send(Action::ChannelListRefreshed(channels));
            }
        }
    }
}
