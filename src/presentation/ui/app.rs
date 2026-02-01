//! Main application orchestrator.

use std::sync::Arc;
use std::time::{Duration, Instant};

use crossterm::event::{Event, EventStream, KeyCode, KeyEvent};
use futures_util::StreamExt;
use ratatui::{DefaultTerminal, Frame};
use tokio::sync::mpsc;
use tokio::time::interval;
use tracing::{debug, error, info, warn};
use zeroize::Zeroize;

use crate::application::dto::{LoginRequest, TokenSource};
use crate::application::services::markdown_service::MarkdownService;
use crate::application::services::notification_service::NotificationService;
use crate::application::use_cases::{LoginUseCase, ResolveTokenUseCase};
use crate::domain::ConnectionStatus;
use crate::domain::entities::{AuthToken, Channel, ChannelId, GuildId, MessageId, UserCache};
use crate::domain::errors::AuthError;
use crate::domain::ports::{AuthPort, ChatDataPort, EventStreamPort, TokenStoragePort};
use crate::domain::{ChannelListEntry, SDKEvent, TimelineMessage};
use crate::presentation::typing::TypingIndicatorManager;
use crate::infrastructure::image::{ImageLoadedEvent, ImageLoader};
use crate::infrastructure::{ClipboardService, StateStore};
use crate::presentation::events::EventResult;
use crate::presentation::theme::Theme;
use crate::presentation::ui::{
    ChatKeyResult, ChatScreen, ChatScreenState, LoginAction, LoginScreen, SplashScreen,
    backend::{Action, Backend, BackendCommand},
};
use crate::presentation::widgets::TreeNodeId;

const TYPING_CLEANUP_INTERVAL: Duration = Duration::from_secs(2);
const TYPING_THROTTLE_DURATION: Duration = Duration::from_secs(8);
const ANIMATION_TICK_RATE: Duration = Duration::from_millis(33);
const IMAGE_CHECK_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AppState {
    Login,
    Initializing,
    Chat,
    Exiting,
}

enum CurrentScreen {
    Login(LoginScreen),
    Splash(SplashScreen),
    Chat(Box<ChatScreenState>),
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Copy)]
pub struct AppConfig {
    pub disable_user_colors: bool,
    pub group_guilds: bool,
    pub enable_desktop_notifications: bool,
    pub use_display_name: bool,
    pub theme: Theme,
}

#[allow(clippy::struct_excessive_bools)]
pub struct App {
    state: AppState,
    screen: CurrentScreen,
    login_use_case: LoginUseCase,
    resolve_token_use_case: ResolveTokenUseCase,
    command_tx: mpsc::UnboundedSender<BackendCommand>,
    pending_token: Option<(String, TokenSource)>,
    current_token: Option<AuthToken>,
    token_source: Option<TokenSource>,
    event_stream: Arc<dyn EventStreamPort>,
    event_rx: Option<tokio::sync::broadcast::Receiver<SDKEvent>>,
    action_tx: mpsc::UnboundedSender<Action>,
    action_rx: mpsc::UnboundedReceiver<Action>,
    typing_manager: TypingIndicatorManager,
    last_typing_cleanup: Instant,
    last_typing_sent: Option<(u64, Instant)>,
    markdown_service: Arc<MarkdownService>,
    user_cache: UserCache,
    current_user_id: Option<String>,
    pending_chat_state: Option<Box<ChatScreenState>>,
    event_stream_ready: bool,
    connection_status: ConnectionStatus,
    should_render: bool,
    /// Image loader for async image loading.
    image_loader: Option<Arc<ImageLoader>>,
    /// Receiver for image load completion events.
    image_load_rx: Option<mpsc::UnboundedReceiver<ImageLoadedEvent>>,
    /// Last time we checked for images to load.
    last_image_check: Instant,
    disable_user_colors: bool,
    group_guilds: bool,
    use_display_name: bool,
    theme: Theme,
    state_store: StateStore,
    state_save_tx: mpsc::UnboundedSender<(Option<u64>, Option<u64>)>,
    clipboard_service: ClipboardService,
    notification: Option<(String, std::time::Instant)>,
    notification_service: NotificationService,
    last_desktop_notification: Option<Instant>,
}

impl App {
    /// # Panics
    ///
    /// Panics if the application state persistence cannot be initialized.
    #[must_use]
    pub fn new(
        auth_port: Arc<dyn AuthPort>,
        chat_data: Arc<dyn ChatDataPort>,
        event_stream: Arc<dyn EventStreamPort>,
        storage_port: Arc<dyn TokenStoragePort>,
        config: AppConfig,
    ) -> Self {
        let login_use_case = LoginUseCase::new(auth_port, storage_port.clone());
        let resolve_token_use_case = ResolveTokenUseCase::new(storage_port);
        let (action_tx, action_rx) = mpsc::unbounded_channel();
        let (command_tx, command_rx) = mpsc::unbounded_channel();
        let markdown_service = Arc::new(MarkdownService::new());
        let notification_service = NotificationService::new(config.enable_desktop_notifications);

        let backend = Backend::new(chat_data, command_rx, action_tx.clone());
        tokio::spawn(backend.run());

        let state_store = StateStore::new();
        let (state_save_tx, mut state_save_rx) =
            mpsc::unbounded_channel::<(Option<u64>, Option<u64>)>();
        let store = state_store.clone();

        tokio::spawn(async move {
            const DEBOUNCE_DURATION: Duration = Duration::from_secs(1);
            let mut pending_state: Option<(Option<u64>, Option<u64>)> = None;
            let mut timer = Box::pin(tokio::time::sleep(Duration::MAX));

            loop {
                tokio::select! {
                    Some(state) = state_save_rx.recv() => {
                        pending_state = Some(state);
                        timer = Box::pin(tokio::time::sleep(DEBOUNCE_DURATION));
                    }
                    () = &mut timer, if pending_state.is_some() => {
                        if let Some((guild_id, channel_id)) = pending_state.take() {
                            let gid = guild_id.map(|g| g.to_string());
                            let cid = channel_id.map(|c| c.to_string());
                            if let Err(e) = store.save(gid, cid).await {
                                tracing::warn!("Failed to save state: {e}");
                            }
                        }
                        timer = Box::pin(tokio::time::sleep(Duration::MAX));
                    }
                    else => break,
                }
            }
        });

        Self {
            state: AppState::Login,
            screen: CurrentScreen::Login(LoginScreen::new().with_theme(config.theme)),
            login_use_case,
            resolve_token_use_case,
            command_tx,
            pending_token: None,
            current_token: None,
            token_source: None,
            event_stream,
            event_rx: None,
            action_tx,
            action_rx,
            typing_manager: TypingIndicatorManager::new(),
            last_typing_cleanup: Instant::now(),
            last_typing_sent: None,
            markdown_service,
            user_cache: UserCache::new(),
            current_user_id: None,
            pending_chat_state: None,
            event_stream_ready: false,
            connection_status: ConnectionStatus::Disconnected,
            should_render: true,
            image_loader: None,
            image_load_rx: None,
            last_image_check: Instant::now(),
            disable_user_colors: config.disable_user_colors,
            group_guilds: config.group_guilds,
            use_display_name: config.use_display_name,
            theme: config.theme,
            state_store,
            state_save_tx,
            clipboard_service: ClipboardService::new(),
            notification: None,
            notification_service,
            last_desktop_notification: None,
        }
    }

    /// # Errors
    /// Returns error if terminal or token resolution fails.
    pub async fn run(
        mut self,
        terminal: &mut DefaultTerminal,
        external_token: Option<(String, TokenSource)>,
    ) -> color_eyre::Result<()> {
        if let Some(resolved) = self.resolve_token_use_case.execute(external_token).await? {
            info!(source = %resolved.source, "Found existing token");
            self.pending_token = Some((resolved.token.as_str().to_string(), resolved.source));
        }

        if let Some((token, source)) = self.pending_token.take() {
            self.attempt_auto_login(token, source).await;
        }

        self.run_event_loop(terminal).await?;

        self.disconnect_event_stream();
        info!("Application exiting normally");
        Ok(())
    }

    async fn validate_auth_status(&self) -> bool {
        match self.token_source {
            Some(TokenSource::Keyring) => {
                matches!(self.resolve_token_use_case.execute(None).await, Ok(Some(_)))
            }
            Some(TokenSource::CommandLine | TokenSource::Environment | TokenSource::UserInput) => {
                true
            }
            None => false,
        }
    }

    async fn run_event_loop(&mut self, terminal: &mut DefaultTerminal) -> color_eyre::Result<()> {
        let mut terminal_events = EventStream::new();
        let mut typing_cleanup_interval = interval(TYPING_CLEANUP_INTERVAL);
        let mut animation_interval = interval(ANIMATION_TICK_RATE);

        terminal.draw(|frame| self.render(frame))?;

        while self.state != AppState::Exiting {
            self.should_render = false;

            // Splash 时优先排空 action 通道，避免被 event_future 抢占导致 DataLoaded 迟迟不处理而卡在启动屏
            if let CurrentScreen::Splash(_) = &self.screen {
                while let Ok(action) = self.action_rx.try_recv() {
                    self.handle_action(action);
                    self.should_render = true;
                }
            }

            let event_future = match &mut self.event_rx {
                Some(rx) => futures_util::future::Either::Left(rx.recv()),
                None => futures_util::future::Either::Right(std::future::pending()),
            };
            let terminal_event = terminal_events.next();

            let image_load_future = match &mut self.image_load_rx {
                Some(rx) => futures_util::future::Either::Left(rx.recv()),
                None => futures_util::future::Either::Right(std::future::pending()),
            };

            tokio::select! {
                biased;

                result = event_future => {
                    if let Ok(event) = result {
                        self.handle_sdk_event(event);
                        self.should_render = true;
                    }
                }

                Some(action) = self.action_rx.recv() => {
                    self.handle_action(action);
                    self.should_render = true;
                }

                Some(event) = image_load_future => {
                    self.handle_image_loaded(event);
                    self.should_render = true;
                }

                _ = animation_interval.tick() => {
                     if let CurrentScreen::Splash(splash) = &mut self.screen {
                        splash.tick(ANIMATION_TICK_RATE);

                        if splash.state.animation_complete && self.pending_chat_state.is_some() {
                            if self.validate_auth_status().await {
                                self.state = AppState::Chat;
                                self.screen = CurrentScreen::Chat(self.pending_chat_state.take().unwrap());
                                self.should_render = true;
                            } else {
                                warn!("Token verification failed after splash");
                                self.transition_to_login();
                                self.should_render = true;
                            }
                        } else if !splash.state.intro_finished || (splash.state.data_ready && !splash.state.animation_complete) {
                             self.should_render = true;
                        }
                    } else if let CurrentScreen::Chat(state) = &mut self.screen {
                        state.tick(ANIMATION_TICK_RATE);
                        if !state.has_entered() {
                            self.should_render = true;
                        }
                    }

                    if self.last_image_check.elapsed() > IMAGE_CHECK_INTERVAL {
                        self.trigger_image_loads();
                        self.last_image_check = Instant::now();
                    }
                }

                Some(Ok(event)) = terminal_event => {
                    let result = self.handle_terminal_event(&event);
                    match result {
                        EventResult::Exit => {
                            self.state = AppState::Exiting;
                        }
                        EventResult::OpenEditor {
                            initial_content,
                            message_id,
                        } => {
                            self.handle_open_editor(terminal, &initial_content, message_id)?;
                            self.should_render = true;
                        }
                        _ => {
                            self.should_render = true;
                        }
                    }
                }

                _ = typing_cleanup_interval.tick() => {
                    self.cleanup_typing_indicators();
                    self.should_render = true;
                }
            }

            if self.should_render {
                terminal.draw(|frame| self.render(frame))?;
            }
        }

        Ok(())
    }

    fn handle_terminal_event(&mut self, event: &Event) -> EventResult {
        match event {
            Event::Key(key) => self.handle_key(*key),
            Event::Paste(text) => self.handle_paste(text),
            _ => EventResult::Continue,
        }
    }

    fn handle_paste(&mut self, text: &str) -> EventResult {
        match &mut self.screen {
            CurrentScreen::Login(screen) => {
                screen.paste_into_focused(text);
                EventResult::Continue
            }
            CurrentScreen::Chat(state) => {
                state.insert_text(text);
                EventResult::Continue
            }
            CurrentScreen::Splash(_) => EventResult::Continue,
        }
    }

    fn save_state(&self, guild_id: Option<GuildId>, channel_id: Option<ChannelId>) {
        let _ = self.state_save_tx.send((
            guild_id.map(|g| g.as_u64()),
            channel_id.map(|c| c.as_u64()),
        ));
    }

    async fn attempt_auto_login(&mut self, token: String, source: TokenSource) {
        debug!("Attempting automatic login");

        if let CurrentScreen::Login(ref mut login_screen) = self.screen {
            login_screen.set_validating();
        }

        let mut request = LoginRequest::new(token.clone(), source);
        if source == TokenSource::Environment {
            request = request.without_persistence();
        }

        match self.login_use_case.execute(request).await {
            Ok(response) => {
                info!(user = %response.user.display_name(), "Auto-login successful");
                if let Some(auth_token) = AuthToken::new(&token) {
                    self.current_token = Some(auth_token);
                }
                self.token_source = Some(source);
                self.start_app_loading(response.user);
            }
            Err(e) => {
                error!(error = %e, "Auto-login failed");
                if let CurrentScreen::Login(ref mut login_screen) = self.screen {
                    login_screen.set_error(e.to_string());
                }
            }
        }
    }

    pub fn show_notification(&mut self, message: String) {
        self.notification = Some((message, std::time::Instant::now()));
    }

    fn render(&mut self, frame: &mut Frame) {
        match &mut self.screen {
            CurrentScreen::Login(screen) => {
                frame.render_widget(&*screen, frame.area());
            }
            CurrentScreen::Splash(screen) => {
                frame.render_widget(screen, frame.area());
            }
            CurrentScreen::Chat(state) => {
                let width = frame.area().width;
                state.update_visible_image_protocols(width);
                frame.render_stateful_widget(ChatScreen::new(), frame.area(), state);
            }
        }

        if let Some((message, time)) = &self.notification
            && time.elapsed() < std::time::Duration::from_secs(3)
        {
            use ratatui::layout::Rect;
            use ratatui::style::{Color, Modifier, Style};
            use ratatui::widgets::{Block, Borders, Paragraph};

            let area = frame.area();
            let max_width = area.width.saturating_sub(2);
            let width = u16::try_from(message.len())
                .unwrap_or(u16::MAX)
                .saturating_add(4)
                .min(max_width);
            let height = 3;
            let x = area.width.saturating_sub(width).saturating_sub(2);
            let y = 2;

            let rect = Rect::new(x, y, width, height);
            let block = Block::default()
                .borders(Borders::ALL)
                .style(Style::default().fg(Color::Green));
            let para = Paragraph::new(message.as_str())
                .block(block)
                .style(Style::default().add_modifier(Modifier::BOLD));

            frame.render_widget(ratatui::widgets::Clear, rect);
            frame.render_widget(para, rect);
        }
    }

    #[allow(clippy::too_many_lines)]
    fn handle_key(&mut self, key: KeyEvent) -> EventResult {
        if self.state == AppState::Login {
            let is_force_quit = matches!(
                key,
                KeyEvent {
                    code: KeyCode::Esc,
                    ..
                } | KeyEvent {
                    code: KeyCode::Char('c'),
                    modifiers: crossterm::event::KeyModifiers::CONTROL,
                    ..
                }
            );

            if is_force_quit {
                return EventResult::Exit;
            }
        } else if let KeyEvent {
            code: KeyCode::Char('c'),
            modifiers: crossterm::event::KeyModifiers::CONTROL,
            ..
        } = key
        {
            return EventResult::Exit;
        }

        let result = match &mut self.screen {
            CurrentScreen::Login(screen) => {
                match screen.handle_key(key) {
                    LoginAction::Submit => self.handle_login_submit(),
                    LoginAction::DeleteToken => self.handle_delete_token(),
                    LoginAction::Paste => self.handle_login_paste(),
                    LoginAction::None => {}
                }
                return EventResult::Continue;
            }
            CurrentScreen::Splash(_) => return EventResult::Continue,
            CurrentScreen::Chat(state) => state.handle_key(key),
        };

        self.process_chat_key_result(result)
    }

    #[allow(clippy::too_many_lines)]
    fn process_chat_key_result(&mut self, result: ChatKeyResult) -> EventResult {
        match result {
            ChatKeyResult::Quit => return EventResult::Exit,
            ChatKeyResult::Logout => {
                self.transition_to_login();
            }
            ChatKeyResult::SecureLogout => {
                self.handle_delete_token();
                self.transition_to_login();
            }
            ChatKeyResult::CopyToClipboard(text) => {
                debug!(text = %text, "Copy to clipboard requested");
                self.clipboard_service.set_text(text);
                self.show_notification("Copied to clipboard".to_string());
            }
            ChatKeyResult::LoadGuildChannels(guild_id) => {
                self.load_guild_channels(guild_id);
            }
            ChatKeyResult::LoadChannelMessages {
                channel_id,
                guild_id,
            } => {
                self.save_state(guild_id, Some(channel_id));
                if let Some(guild_id) = guild_id {
                    self.subscribe_to_channel(guild_id, channel_id);
                }
                self.load_channel_messages(channel_id);
            }
            ChatKeyResult::LoadForumThreads {
                channel_id,
                guild_id,
                offset,
            } => {
                self.save_state(guild_id, Some(channel_id));
                if let Some(guild_id) = guild_id {
                    self.subscribe_to_channel(guild_id, channel_id);
                }
                self.load_forum_threads(channel_id, guild_id, offset);
            }
            ChatKeyResult::LoadDmMessages {
                channel_id,
                recipient_name,
            } => {
                self.save_state(None, Some(channel_id));
                debug!(channel_id = %channel_id, recipient = %recipient_name, "Loading DM messages");
                self.load_channel_messages(channel_id);
            }
            ChatKeyResult::LoadHistory {
                channel_id,
                before_message_id,
            } => {
                debug!(channel_id = %channel_id, before = %before_message_id, "Loading history");
                self.load_history(channel_id, before_message_id);
            }
            ChatKeyResult::ReplyToMessage {
                message_id,
                mention,
            } => {
                debug!(message_id = %message_id, mention = mention, "Reply to message requested");
                self.handle_reply_to_message(message_id, mention);
            }
            ChatKeyResult::SubmitEdit {
                message_id,
                content,
            } => {
                self.handle_edit_message(message_id, content);
            }
            ChatKeyResult::EditMessage(message_id) => {
                debug!(message_id = %message_id, "Edit message requested");
            }
            ChatKeyResult::DeleteMessage(message_id) => {
                debug!(message_id = %message_id, "Delete message requested");
                self.handle_delete_message(message_id);
            }
            ChatKeyResult::OpenAttachments(message_id) => {
                debug!(message_id = %message_id, "Open attachments requested");
            }
            ChatKeyResult::JumpToMessage(message_id) => {
                debug!(message_id = %message_id, "Jump to message requested");
                if let CurrentScreen::Chat(state) = &mut self.screen {
                    state.jump_to_message(message_id);
                }
            }
            ChatKeyResult::SendMessage {
                content,
                reply_to,
                attachments,
            } => {
                self.handle_send_message(content, reply_to, attachments);
            }
            ChatKeyResult::StartTyping => {
                self.handle_start_typing();
            }
            ChatKeyResult::OpenEditor {
                initial_content,
                message_id,
            } => {
                return EventResult::OpenEditor {
                    initial_content,
                    message_id,
                };
            }
            ChatKeyResult::Paste => {
                let clipboard = self.clipboard_service.clone();
                let tx = self.action_tx.clone();

                tokio::task::spawn_blocking(move || {
                    if let Some(image) = clipboard.get_image() {
                        let temp_dir = std::env::temp_dir();
                        let filename = format!("paste_{}.png", uuid::Uuid::new_v4());
                        let path = temp_dir.join(filename);

                        if let Some(img) = image::ImageBuffer::<image::Rgba<u8>, _>::from_raw(
                            u32::try_from(image.width).unwrap_or_default(),
                            u32::try_from(image.height).unwrap_or_default(),
                            image.bytes.into_owned(),
                        ) && img.save(&path).is_ok()
                        {
                            let _ = tx.send(Action::PasteImageLoaded(path));
                            return;
                        }
                    }

                    if let Some(text) = clipboard.get_text() {
                        let _ = tx.send(Action::PasteTextLoaded(text));
                    }
                });
            }
            ChatKeyResult::ToggleHelp | ChatKeyResult::Consumed | ChatKeyResult::Ignored => {}
            ChatKeyResult::ToggleDisplayName => {
                self.use_display_name = !self.use_display_name;
                info!(
                    enabled = self.use_display_name,
                    "Toggled display name preference"
                );
                if let CurrentScreen::Chat(ref mut state) = self.screen {
                    state.set_use_display_name(self.use_display_name);

                    if let Some(channel_id) = state.message_pane_data().channel_id() {
                        let indicator = self.typing_manager.format_typing_indicator(channel_id);
                        state.set_typing_indicator(indicator);
                    }
                } else if let Some(ref mut state) = self.pending_chat_state {
                    state.set_use_display_name(self.use_display_name);
                }

                let status = if self.use_display_name {
                    "enabled"
                } else {
                    "disabled"
                };
                self.show_notification(format!("Display names {status}"));
            }
        }

        EventResult::Continue
    }

    fn handle_login_submit(&mut self) {
        let (username, password, persist) = if let CurrentScreen::Login(ref screen) = self.screen {
            if !screen.has_credentials() {
                return;
            }
            (
                screen.username().to_string(),
                screen.password().to_string(),
                screen.should_persist(),
            )
        } else {
            return;
        };

        if let CurrentScreen::Login(ref mut screen) = self.screen {
            screen.set_validating();
        }

        let request = LoginRequest::with_credentials(username, password, persist);
        let use_case = self.login_use_case.clone();
        let tx = self.action_tx.clone();

        tokio::spawn(async move {
            match use_case.execute(request).await {
                Ok(response) => {
                    let _ = tx.send(Action::LoginSuccess {
                        user: response.user,
                        token: response.session_token,
                        source: response.token_source,
                    });
                }
                Err(e) => {
                    let _ = tx.send(Action::LoginFailure(e));
                }
            }
        });
    }

    fn handle_login_paste(&mut self) {
        let clipboard = self.clipboard_service.clone();
        let tx = self.action_tx.clone();

        tokio::task::spawn_blocking(move || {
            if let Some(text) = clipboard.get_text() {
                let _ = tx.send(Action::PasteTextLoaded(text));
            }
        });
    }

    fn handle_delete_token(&self) {
        let use_case = self.login_use_case.clone();
        let tx = self.action_tx.clone();

        tokio::spawn(async move {
            match use_case.delete_token().await {
                Ok(()) => {
                    let _ = tx.send(Action::LoginFailure(AuthError::unexpected(
                        "Token deleted from secure storage",
                    )));
                }
                Err(e) => {
                    let _ = tx.send(Action::LoginFailure(e));
                }
            }
        });
    }

    fn start_app_loading(&mut self, user: crate::domain::entities::User) {
        self.state = AppState::Initializing;
        self.screen = CurrentScreen::Splash(SplashScreen::new());
        self.event_stream_ready = false;

        let (img_tx, img_rx) = mpsc::unbounded_channel();
        let action_tx = self.action_tx.clone();
        tokio::spawn(async move {
            match ImageLoader::with_defaults(img_tx).await {
                Ok(loader) => {
                    let _ = action_tx.send(Action::ImageLoaderReady(Arc::new(loader)));
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to initialize image loader");
                }
            }
        });
        self.image_load_rx = Some(img_rx);

        let Some(ref token) = self.current_token else {
            return;
        };
        let token = token.clone();
        let event_stream = self.event_stream.clone();
        let action_tx_ev = self.action_tx.clone();
        tokio::spawn(async move {
            if let Ok(rx) = event_stream.connect(&token).await {
                let _ = action_tx_ev.send(Action::EventStreamReady(rx));
            }
        });

        let state_store = self.state_store.clone();
        let command_tx = self.command_tx.clone();

        tokio::spawn(async move {
            let state: crate::infrastructure::state_store::AppState =
                state_store.load().await.unwrap_or_default();
            debug!(
                guild_id = ?state.last_guild_id,
                channel_id = ?state.last_channel_id,
                "Loaded persisted state"
            );

            let initial_channel_id = state
                .last_channel_id
                .and_then(|id| id.parse::<u64>().ok());

            let _ = command_tx.send(BackendCommand::LoadInitialData {
                user,
                initial_channel_id,
            });
        });
    }

    fn set_connection_status(&mut self, status: ConnectionStatus) {
        self.connection_status = status;
        if let CurrentScreen::Chat(ref mut state) = self.screen {
            state.set_connection_status(status);
        }
        if let Some(ref mut state) = self.pending_chat_state {
            state.set_connection_status(status);
        }
    }

    fn disconnect_event_stream(&mut self) {
        self.event_stream.disconnect();
        self.event_rx = None;
    }

    fn handle_sdk_event(&mut self, event: SDKEvent) {
        use privchat_sdk::events::{ConnectionState as ConnState, TimelineDiffKind, SDKEvent as PSE};
        match &event {
            PSE::ConnectionStateChanged { new_state, .. } => {
                let status = match new_state {
                    ConnState::Connected => ConnectionStatus::Connected,
                    ConnState::Connecting | ConnState::Reconnecting => ConnectionStatus::Connecting,
                    _ => ConnectionStatus::Disconnected,
                };
                self.set_connection_status(status);
            }
            PSE::MessageReceived { .. } => {
                // 不构造消息、不赋值 id；只通过 TimelineDiff::Append 的 TimelineMessage（id 由 SDK 提供）追加
            }
            PSE::TimelineDiff { diff_kind, .. } => {
                if let TimelineDiffKind::Append { messages } = diff_kind {
                    for msg in messages {
                        self.handle_privchat_message_received(msg.clone());
                    }
                }
            }
            PSE::TypingIndicator(typing_event) => {
                self.handle_typing_start(
                    ChannelId(typing_event.channel_id),
                    typing_event.user_id.to_string(),
                    None,
                );
            }
            PSE::ChannelListUpdate { .. } => {
                // 服务端推送新会话或 SDK 同步会话列表后需刷新 UI 会话列表
                info!("SDK ChannelListUpdate 收到，触发会话列表刷新");
                let _ = self.command_tx.send(BackendCommand::RefreshChannelList);
                self.should_render = true;
            }
            _ => {}
        }
    }

    fn handle_privchat_message_received(&mut self, msg: TimelineMessage) {
        let channel_id_domain = ChannelId(msg.channel_id);
        let user_id_str = msg.from_uid.to_string();
        if let CurrentScreen::Chat(ref mut state) = self.screen {
            state.add_timeline_message(msg);
        } else if let Some(ref mut state) = self.pending_chat_state {
            state.add_timeline_message(msg);
        }
        self.typing_manager.remove_typing(channel_id_domain, &user_id_str);
        self.update_typing_indicator(channel_id_domain);
    }

    fn handle_message_create(&mut self, message: crate::domain::entities::Message) {
        let channel_id = message.channel_id();
        let user_id = message.author().id().to_string();
        debug!(message_id = %message.id(), channel_id = %channel_id, "New message received");

        self.cache_users_from_message(&message);

        let mut is_mentioned = false;
        if let Some(current_user_id) = &self.current_user_id
            && message
                .mentions()
                .iter()
                .any(|u| u.id().to_string() == *current_user_id)
        {
            is_mentioned = true;
        }

        let mut is_dm = false;
        if let CurrentScreen::Chat(ref state) = self.screen
            && state
                .guilds_tree_data()
                .dm_users()
                .iter()
                .any(|dm| dm.channel_id == channel_id.to_string())
        {
            is_dm = true;
        }

        if is_dm && Some(&user_id) != self.current_user_id.as_ref() {
            is_mentioned = true;
        }

        if is_mentioned {
            if let CurrentScreen::Chat(ref mut state) = self.screen {
                state.increment_mention_count(channel_id);
            } else if let Some(ref mut state) = self.pending_chat_state {
                state.increment_mention_count(channel_id);
            }
        }

        let is_focused = if let CurrentScreen::Chat(ref state) = self.screen {
            state.message_pane_data().channel_id() == Some(channel_id)
        } else {
            false
        };

        if is_mentioned && !is_focused {
            let now = Instant::now();
            let should_notify = match self.last_desktop_notification {
                Some(last) => now.duration_since(last) > Duration::from_secs(2),
                None => true,
            };

            if should_notify {
                let title = format!("Privchat - @{}", message.author().username());
                let content = message.content();
                let body = if content.is_empty() {
                    if !message.attachments().is_empty() {
                        "[Attachment]".to_string()
                    } else if !message.embeds().is_empty() {
                        "[Embed]".to_string()
                    } else {
                        "Sent a message".to_string()
                    }
                } else {
                    content.chars().take(120).collect()
                };
                self.notification_service.send(title, body);
                self.last_desktop_notification = Some(now);
            }
        }

        if let CurrentScreen::Chat(ref mut state) = self.screen {
            state.on_message_received(&message);
            state.add_message(message);
        } else if let Some(ref mut state) = self.pending_chat_state {
            state.on_message_received(&message);
            state.add_message(message);
        }

        self.typing_manager.remove_typing(channel_id, &user_id);
        self.update_typing_indicator(channel_id);
    }

    fn cache_users_from_message(&self, message: &crate::domain::entities::Message) {
        let author = message.author();
        self.user_cache
            .insert(crate::domain::entities::CachedUser::new(
                author.id(),
                author.username(),
                author.discriminator(),
                author.avatar().map(String::from),
                author.is_bot(),
            ));

        for mention in message.mentions() {
            self.user_cache.insert_from_user(mention);
        }
    }

    fn handle_message_update(&mut self, message: crate::domain::entities::Message) {
        debug!(message_id = %message.id(), "Message updated");
        if let CurrentScreen::Chat(ref mut state) = self.screen {
            state.update_message(message);
        }
    }

    fn handle_typing_start(
        &mut self,
        channel_id: ChannelId,
        user_id: String,
        username: Option<String>,
    ) {
        if self.current_user_id.as_deref() == Some(user_id.as_str()) {
            return;
        }

        if let Some(ref name) = username {
            self.user_cache.insert_basic(&user_id, name);
        }

        let display_name = username
            .or_else(|| self.user_cache.get_display_name(&user_id))
            .or_else(|| {
                if let CurrentScreen::Chat(ref state) = self.screen {
                    state
                        .message_pane_data()
                        .get_author_name(&user_id)
                        .map(String::from)
                } else {
                    None
                }
            })
            .unwrap_or_else(|| user_id.clone());

        debug!(user = %display_name, channel_id = %channel_id, "User started typing");

        self.typing_manager
            .add_typing(channel_id, user_id, display_name);
        self.update_typing_indicator(channel_id);
    }

    fn update_typing_indicator(&mut self, channel_id: ChannelId) {
        if let CurrentScreen::Chat(ref mut state) = self.screen {
            let current_channel = state.message_pane_data().channel_id();
            debug!(
                typing_channel = %channel_id,
                current_channel = ?current_channel,
                "Updating typing indicator"
            );

            if current_channel == Some(channel_id) {
                let indicator = self.typing_manager.format_typing_indicator(channel_id);
                debug!(indicator = ?indicator, "Setting typing indicator");
                state.set_typing_indicator(indicator);
            } else {
                debug!("Channel mismatch, not updating indicator");
            }
        } else {
            debug!("Not in chat screen, skipping typing indicator update");
        }
    }

    fn cleanup_typing_indicators(&mut self) {
        self.last_typing_cleanup = Instant::now();
        self.typing_manager.cleanup_expired();

        if let CurrentScreen::Chat(ref mut state) = self.screen
            && let Some(channel_id) = state.message_pane_data().channel_id()
        {
            let indicator = self.typing_manager.format_typing_indicator(channel_id);
            state.set_typing_indicator(indicator);
        }
    }

    fn load_guild_channels(&mut self, _guild_id: GuildId) {
        // Privchat: 无 guild/channels 分页，会话列表由 get_channels 一次拉取
    }

    fn load_channel_messages(&mut self, channel_id: ChannelId) {
        self.typing_manager.clear_channel(channel_id);
        let _ = self.command_tx.send(BackendCommand::LoadChannelMessages {
            channel_id: channel_id.as_u64(),
        });
    }

    fn load_forum_threads(
        &mut self,
        _channel_id: ChannelId,
        _guild_id: Option<GuildId>,
        _offset: u32,
    ) {
        // Privchat: 暂无 forum threads
    }

    fn subscribe_to_channel(&mut self, _guild_id: GuildId, _channel_id: ChannelId) {
        // Privchat: 事件通过 EventStreamPort 统一推送，无需按 channel 订阅
    }

    fn load_history(&mut self, channel_id: ChannelId, before_message_id: MessageId) {
        let _ = self.command_tx.send(BackendCommand::LoadHistory {
            channel_id: channel_id.as_u64(),
            before_message_id: before_message_id.as_u64(),
        });
    }

    #[allow(clippy::too_many_lines)]
    fn handle_action(&mut self, action: Action) {
        match action {
            Action::HistoryLoaded(messages) => {
                if let CurrentScreen::Chat(ref mut state) = self.screen {
                    state.prepend_timeline_messages(messages);
                }
            }
            Action::LoadError(e) => {
                warn!(error = %e, "Failed to load history");
            }
            Action::DataLoaded {
                user,
                channels,
                initial_channel_id,
                initial_messages,
            } => {
                info!("Data loaded, preparing chat state");
                self.current_user_id = Some(user.id().to_string());
                let mut chat_state = ChatScreenState::new(
                    user,
                    self.markdown_service.clone(),
                    self.user_cache.clone(),
                    self.disable_user_colors,
                    self.use_display_name,
                    self.theme,
                );

                chat_state.set_connection_status(self.connection_status);
                chat_state.set_group_guilds(self.group_guilds);
                chat_state.set_channel_list_entries(channels.clone());
                if !channels.is_empty() {
                    chat_state.guilds_tree_state_mut().expand(TreeNodeId::DirectMessages);
                }

                let initial_cid = initial_channel_id.map(ChannelId::from);
                let restore_result = chat_state.restore_state_privchat(
                    initial_cid,
                    initial_messages,
                );

                self.pending_chat_state = Some(Box::new(chat_state));

                if let Some(result) = restore_result {
                    self.process_chat_key_result(result);
                }

                // 只要在 Splash 且数据已加载就标记 data_ready，不依赖 event_stream 先后顺序，否则会一直卡在启动屏
                if let CurrentScreen::Splash(splash) = &mut self.screen {
                    splash.set_data_ready();
                }
            }
            Action::EventStreamReady(rx) => {
                self.event_rx = Some(rx);
                self.event_stream_ready = true;
                // connect() 成功返回才发 EventStreamReady，此时已连上且认证完成，应显示 CONNECTED 而非 CONNECTING
                self.set_connection_status(ConnectionStatus::Connected);
                // 若 DataLoaded 先到、此时才连上事件流，也要标记 data_ready 以便 Splash 能进入 outro
                if let CurrentScreen::Splash(splash) = &mut self.screen {
                    splash.set_data_ready();
                }
                // 服务端在连接后会推送离线消息，SDK 可能稍后才写入会话列表；延迟刷新一次以拉取推送后的会话
                let command_tx = self.command_tx.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
                    let _ = command_tx.send(BackendCommand::RefreshChannelList);
                });
            }
            Action::LoginSuccess {
                user,
                token,
                source,
            } => {
                info!(
                    user = %user.display_name(),
                    source = %source,
                    "Login successful"
                );
                if let Some(auth_token) = AuthToken::new(&token) {
                    self.current_token = Some(auth_token);
                }
                self.token_source = Some(source);
                self.start_app_loading(user);
            }
            Action::LoginFailure(error) => {
                error!(error = %error, "Login failed");
                self.handle_login_error(&error);
            }
            Action::ChannelMessagesLoaded {
                channel_id,
                messages,
            } => {
                let cid = ChannelId(channel_id);
                if let CurrentScreen::Chat(state) = &mut self.screen
                    && state.message_pane_data().channel_id() == Some(cid)
                {
                    state.set_timeline_messages(messages.clone());
                    state.set_typing_indicator(None);

                    if let Some(last_msg) = messages.last() {
                        let _ = self.command_tx.send(BackendCommand::AcknowledgeMessage {
                            channel_id,
                            message_id: last_msg.id,
                        });
                        state.mark_channel_read(cid, MessageId(last_msg.id));
                    }
                }
            }
            Action::ChannelMessagesLoadError { channel_id, error } => {
                let cid = ChannelId(channel_id);
                if let CurrentScreen::Chat(state) = &mut self.screen
                    && state.message_pane_data().channel_id() == Some(cid)
                {
                    state.set_message_error(error);
                    state.focus_guilds_tree();
                }
            }
            Action::MessageSent {
                channel_id,
                local_message_id: _,
            } => {
                info!(channel_id, "Message sent");
                // 实际新消息会通过 SDK 事件 TimelineDiff/MessageReceived 推送，此处仅作日志
            }
            Action::MessageSendError(error) => {
                error!(error = %error, "Failed to send message");
                if let CurrentScreen::Chat(ref mut state) = self.screen {
                    state.set_message_error(format!("Failed to send: {error}"));
                }
            }
            Action::TypingIndicatorSent(_) => {}
            Action::ChannelListRefreshed(entries) => {
                info!(count = entries.len(), "会话列表已刷新");
                if let CurrentScreen::Chat(state) = &mut self.screen {
                    state.set_channel_list_entries(entries.clone());
                }
                if let Some(state) = &mut self.pending_chat_state {
                    state.set_channel_list_entries(entries);
                }
                self.should_render = true;
            }
            Action::ImageLoaderReady(loader) => {
                self.image_loader = Some(loader);
            }
            Action::PasteImageLoaded(path) => {
                if let CurrentScreen::Chat(state) = &mut self.screen {
                    state.add_attachment(path);
                    self.show_notification("Image pasted as attachment".to_string());
                }
            }
            Action::PasteTextLoaded(text) => {
                if let CurrentScreen::Chat(state) = &mut self.screen {
                    state.insert_text(&text);
                } else if let CurrentScreen::Login(screen) = &mut self.screen {
                    screen.paste_into_focused(&text);
                }
            }
        }
    }

    fn transition_to_login(&mut self) {
        self.disconnect_event_stream();

        if let Some((mut token, _)) = self.pending_token.take() {
            token.zeroize();
        }

        self.state = AppState::Login;
        self.current_token = None;
        self.token_source = None;
        self.current_user_id = None;
        self.pending_chat_state = None;
        self.typing_manager = TypingIndicatorManager::new();
        self.user_cache.clear();
        self.screen = CurrentScreen::Login(LoginScreen::new().with_theme(self.theme));
    }

    fn handle_login_error(&mut self, error: &AuthError) {
        if let CurrentScreen::Login(ref mut screen) = self.screen {
            screen.set_error(error.user_message());
        }
    }

    fn handle_reply_to_message(
        &mut self,
        message_id: crate::domain::entities::MessageId,
        mention: bool,
    ) {
        if let CurrentScreen::Chat(ref mut state) = self.screen
            && let Some(author_name) = state.get_reply_author(message_id)
        {
            state.start_reply(message_id, author_name, mention);
        }
    }

    fn handle_send_message(
        &mut self,
        content: String,
        reply_to: Option<crate::domain::entities::MessageId>,
        attachments: Vec<std::path::PathBuf>,
    ) {
        let channel_id = if let CurrentScreen::Chat(ref state) = self.screen {
            state
                .selected_channel()
                .map(crate::domain::entities::Channel::id)
        } else {
            None
        };

        let Some(channel_id) = channel_id else {
            warn!("Cannot send message: no channel selected");
            return;
        };

        // Privchat: 暂不支持 reply/attachments，仅发送纯文本
        debug!(channel_id = %channel_id, "Sending message");

        let _ = self.command_tx.send(BackendCommand::SendMessage {
            channel_id: channel_id.as_u64(),
            content,
        });

        self.last_typing_sent = None;
    }

    fn handle_edit_message(
        &mut self,
        _message_id: crate::domain::entities::MessageId,
        _content: String,
    ) {
        // Privchat ChatDataPort 暂不支持编辑消息
        warn!("Edit message not supported with Privchat backend");
    }

    fn handle_delete_message(&mut self, _message_id: crate::domain::entities::MessageId) {
        // Privchat ChatDataPort 暂不支持删除消息
        warn!("Delete message not supported with Privchat backend");
    }

    fn handle_start_typing(&mut self) {
        let channel_id = if let CurrentScreen::Chat(ref state) = self.screen {
            state
                .selected_channel()
                .map(crate::domain::entities::Channel::id)
        } else {
            None
        };

        let Some(channel_id) = channel_id else {
            return;
        };

        let cid_u64 = channel_id.as_u64();
        let should_send = match self.last_typing_sent {
            Some((last_channel, last_time)) => {
                last_channel != cid_u64 || last_time.elapsed() >= TYPING_THROTTLE_DURATION
            }
            None => true,
        };

        if !should_send {
            return;
        }

        let _ = self.command_tx.send(BackendCommand::SendTypingIndicator {
            channel_id: cid_u64,
        });

        self.last_typing_sent = Some((cid_u64, Instant::now()));
    }

    /// Handle an image load completion event.
    fn handle_image_loaded(&mut self, event: ImageLoadedEvent) {
        if let CurrentScreen::Chat(ref mut state) = self.screen {
            match event.result {
                Ok(loaded) => {
                    debug!(id = %loaded.id, source = ?loaded.source, "Image loaded successfully");
                    state.on_image_loaded(&loaded.id, &loaded.image);
                }
                Err(e) => {
                    warn!(id = %event.id, error = %e, "Failed to load image");
                    state.mark_image_failed(&event.id, &e);
                }
            }
        }
    }

    /// Trigger loading of images in the visible viewport.
    fn trigger_image_loads(&mut self) {
        let CurrentScreen::Chat(ref mut state) = self.screen else {
            return;
        };

        let Some(ref loader) = self.image_loader else {
            return;
        };

        let needed = state.collect_needed_image_loads();
        for (id, url) in needed {
            state.mark_image_downloading(&id);
            loader.load_async(id, url);
        }
    }

    fn handle_open_editor(
        &mut self,
        terminal: &mut DefaultTerminal,
        initial_content: &str,
        target_message_id: Option<MessageId>,
    ) -> color_eyre::Result<()> {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut temp_file = NamedTempFile::new()?;
        write!(temp_file, "{initial_content}")?;
        let temp_path = temp_file.path().to_owned();

        let editor = std::env::var("EDITOR")
            .or_else(|_| std::env::var("VISUAL"))
            .unwrap_or_else(|_| {
                for editor in &["nvim", "vim", "nano", "vi"] {
                    if std::process::Command::new("which")
                        .arg(editor)
                        .output()
                        .map(|o| o.status.success())
                        .unwrap_or(false)
                    {
                        return (*editor).to_string();
                    }
                }
                "vi".to_string()
            });

        debug!(editor = %editor, path = %temp_path.display(), "Opening external editor");

        crossterm::terminal::disable_raw_mode()?;
        crossterm::execute!(
            std::io::stdout(),
            crossterm::terminal::LeaveAlternateScreen,
            crossterm::cursor::Show
        )?;

        let status = std::process::Command::new(&editor).arg(&temp_path).status();

        crossterm::terminal::enable_raw_mode()?;
        crossterm::execute!(
            std::io::stdout(),
            crossterm::terminal::EnterAlternateScreen,
            crossterm::cursor::Hide
        )?;

        terminal.clear()?;

        match status {
            Ok(exit_status) if exit_status.success() => {
                let new_content = match std::fs::read_to_string(&temp_path) {
                    Ok(content) => content,
                    Err(e) => {
                        error!(error = %e, "Failed to read from temp file after edit");
                        if let CurrentScreen::Chat(ref mut state) = self.screen {
                            state.set_message_error(format!("Failed to read editor output: {e}"));
                        }
                        if let Err(e) = std::fs::remove_file(&temp_path) {
                            debug!(error = %e, "Failed to remove temp file");
                        }
                        return Ok(());
                    }
                };

                let new_content = if new_content.ends_with('\n') && !initial_content.ends_with('\n')
                {
                    new_content[..new_content.len() - 1].to_string()
                } else {
                    new_content
                };

                if let CurrentScreen::Chat(ref mut state) = self.screen {
                    if let Some(id) = target_message_id {
                        state.message_input_parts_mut().start_edit(id, &new_content);
                        state.focus_message_input();
                    } else {
                        state.message_input_parts_mut().set_content(&new_content);
                    }
                }

                info!("Editor closed successfully");
            }

            Ok(exit_status) => {
                warn!(
                    exit_code = ?exit_status.code(),
                    "Editor exited with non-zero status"
                );
            }
            Err(e) => {
                error!(error = %e, editor = %editor, "Failed to spawn editor");
                if let CurrentScreen::Chat(ref mut state) = self.screen {
                    state.set_message_error(format!("Failed to open editor: {e}"));
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ports::{
        ChatDataPort, EventStreamPort,
        mocks::{MockAuthPort, MockTokenStorage},
    };
    use crate::domain::{ChannelListEntry, TimelineMessage};

    struct MockChatData;

    #[async_trait::async_trait]
    impl ChatDataPort for MockChatData {
        async fn get_channels(
            &self,
            _limit: Option<u32>,
            _offset: Option<u32>,
        ) -> Result<Vec<ChannelListEntry>, AuthError> {
            Ok(vec![])
        }

        async fn get_messages(
            &self,
            _channel_id: u64,
            _limit: u32,
            _before_id: Option<u64>,
        ) -> Result<Vec<TimelineMessage>, AuthError> {
            Ok(vec![])
        }

        async fn send_message(&self, _channel_id: u64, _content: &str) -> Result<u64, AuthError> {
            Ok(0)
        }

        async fn mark_as_read(
            &self,
            _channel_id: u64,
            _message_id: u64,
        ) -> Result<(), AuthError> {
            Ok(())
        }

        async fn send_typing(&self, _channel_id: u64) -> Result<(), AuthError> {
            Ok(())
        }
    }

    struct MockEventStream;

    #[async_trait::async_trait]
    impl EventStreamPort for MockEventStream {
        async fn connect(
            &self,
            _token: &AuthToken,
        ) -> Result<tokio::sync::broadcast::Receiver<SDKEvent>, AuthError> {
            let (_tx, rx) = tokio::sync::broadcast::channel(16);
            Ok(rx)
        }

        fn disconnect(&self) {}

        fn is_connected(&self) -> bool {
            false
        }
    }

    #[tokio::test]
    async fn test_app_creation() {
        let auth = Arc::new(MockAuthPort::new(true));
        let chat_data = Arc::new(MockChatData);
        let event_stream = Arc::new(MockEventStream);
        let storage = Arc::new(MockTokenStorage::new());
        let theme = Theme::new("Orange");
        let config = AppConfig {
            disable_user_colors: false,
            group_guilds: false,
            enable_desktop_notifications: false,
            use_display_name: true,
            theme,
        };
        let app = App::new(auth, chat_data.clone(), event_stream, storage, config);

        assert_eq!(app.state, AppState::Login);
    }
}
