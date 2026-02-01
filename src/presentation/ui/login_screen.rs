//! Login screen.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget},
};

use crate::presentation::theme::Theme;
use crate::presentation::widgets::TextInput;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginState {
    Input,
    Validating,
    Error,
    Success,
}

/// Which field has focus on the login form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginFocus {
    Username,
    Password,
    PersistCheckbox,
}

/// Login screen UI.
pub struct LoginScreen {
    username_input: TextInput,
    password_input: TextInput,
    focus: LoginFocus,
    state: LoginState,
    error_message: Option<String>,
    persist_token: bool,
    theme: Theme,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginAction {
    None,
    Submit,
    DeleteToken,
    Paste,
}

impl LoginScreen {
    /// Creates new login screen.
    #[must_use]
    pub fn new() -> Self {
        let mut username_input = TextInput::new("Username")
            .placeholder("Your username");
        username_input.set_focused(true);

        let password_input = TextInput::new("Password")
            .password()
            .placeholder("Your password");

        Self {
            username_input,
            password_input,
            focus: LoginFocus::Username,
            state: LoginState::Input,
            error_message: None,
            persist_token: true,
            theme: Theme::default(),
        }
    }

    #[must_use]
    pub fn with_theme(mut self, theme: Theme) -> Self {
        self.theme = theme;
        self
    }

    /// Returns current state.
    #[must_use]
    pub const fn state(&self) -> LoginState {
        self.state
    }

    /// Returns entered username (empty string if none).
    #[must_use]
    pub fn username(&self) -> &str {
        self.username_input.value()
    }

    /// Returns entered password (empty string if none).
    #[must_use]
    pub fn password(&self) -> &str {
        self.password_input.value()
    }

    /// Returns whether both username and password are non-empty.
    #[must_use]
    pub fn has_credentials(&self) -> bool {
        !self.username_input.value().is_empty() && !self.password_input.value().is_empty()
    }

    /// Returns persistence preference.
    #[must_use]
    pub const fn should_persist(&self) -> bool {
        self.persist_token
    }

    /// Sets validating state.
    pub fn set_validating(&mut self) {
        self.state = LoginState::Validating;
        self.error_message = None;
    }

    /// Sets success state.
    pub fn set_success(&mut self) {
        self.state = LoginState::Success;
        self.error_message = None;
    }

    /// Sets error state.
    pub fn set_error(&mut self, message: impl Into<String>) {
        self.state = LoginState::Error;
        self.error_message = Some(message.into());
    }

    /// Resets to input state.
    pub fn reset(&mut self) {
        self.state = LoginState::Input;
        self.error_message = None;
    }

    /// Inserts text at the focused field's cursor.
    pub fn paste_into_focused(&mut self, text: &str) {
        match self.focus {
            LoginFocus::Username => self.username_input.insert_str(text),
            LoginFocus::Password => self.password_input.insert_str(text),
            LoginFocus::PersistCheckbox => {}
        }
    }

    fn focused_input_mut(&mut self) -> &mut TextInput {
        match self.focus {
            LoginFocus::Username => &mut self.username_input,
            LoginFocus::Password => &mut self.password_input,
            LoginFocus::PersistCheckbox => &mut self.username_input, // unused
        }
    }

    fn set_focus(&mut self, focus: LoginFocus) {
        self.focus = focus;
        self.username_input.set_focused(focus == LoginFocus::Username);
        self.password_input.set_focused(focus == LoginFocus::Password);
    }

    /// Handles key event, returns action.
    pub fn handle_key(&mut self, key: KeyEvent) -> LoginAction {
        if self.state == LoginState::Validating {
            return LoginAction::None;
        }

        if self.state == LoginState::Error {
            self.reset();
            return LoginAction::None;
        }

        match key.code {
            KeyCode::Enter => {
                match self.focus {
                    LoginFocus::Username => {
                        if !self.username_input.value().is_empty() {
                            self.set_focus(LoginFocus::Password);
                        }
                    }
                    LoginFocus::Password => {
                        if self.has_credentials() {
                            return LoginAction::Submit;
                        }
                    }
                    LoginFocus::PersistCheckbox => {
                        if self.has_credentials() {
                            return LoginAction::Submit;
                        }
                    }
                }
            }
            KeyCode::Char('d') if key.modifiers.contains(crossterm::event::KeyModifiers::ALT) => {
                return LoginAction::DeleteToken;
            }
            KeyCode::Char('u')
                if key
                    .modifiers
                    .contains(crossterm::event::KeyModifiers::CONTROL) =>
            {
                self.focused_input_mut().clear();
            }
            KeyCode::Char('v')
                if key
                    .modifiers
                    .contains(crossterm::event::KeyModifiers::CONTROL) =>
            {
                return LoginAction::Paste;
            }
            KeyCode::Char(c) => {
                self.focused_input_mut().input_char(c);
            }
            KeyCode::Backspace => {
                self.focused_input_mut().backspace();
            }
            KeyCode::Delete => {
                self.focused_input_mut().delete();
            }
            KeyCode::Left => {
                self.focused_input_mut().move_left();
            }
            KeyCode::Right => {
                self.focused_input_mut().move_right();
            }
            KeyCode::Home => {
                self.focused_input_mut().move_start();
            }
            KeyCode::End => {
                self.focused_input_mut().move_end();
            }
            KeyCode::Tab => {
                if key.modifiers.contains(crossterm::event::KeyModifiers::SHIFT) {
                    match self.focus {
                        LoginFocus::Password => self.set_focus(LoginFocus::Username),
                        LoginFocus::Username | LoginFocus::PersistCheckbox => {}
                    }
                } else {
                    match self.focus {
                        LoginFocus::Username => self.set_focus(LoginFocus::Password),
                        LoginFocus::Password => {
                            self.persist_token = !self.persist_token;
                            self.set_focus(LoginFocus::Username);
                        }
                        LoginFocus::PersistCheckbox => self.set_focus(LoginFocus::Username),
                    }
                }
            }
            _ => {}
        }

        LoginAction::None
    }

    fn render_inner(&self, area: Rect, buf: &mut Buffer) {
        let vertical = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Min(15),
            Constraint::Fill(1),
        ]);
        let [_, center, _] = vertical.areas(area);

        let horizontal = Layout::horizontal([
            Constraint::Fill(1),
            Constraint::Min(60),
            Constraint::Fill(1),
        ]);
        let [_, content_area, _] = horizontal.areas(center);

        Clear.render(content_area, buf);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.theme.accent))
            .title(" Privchat Login ");

        let inner = block.inner(content_area);
        block.render(content_area, buf);

        let constraints = vec![
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(2),
        ];

        let inner_layout = Layout::vertical(constraints);
        let areas = inner_layout.split(inner);

        let title = Paragraph::new("Enter username and password to login or register")
            .style(Style::default().fg(Color::White));
        title.render(areas[0], buf);

        (&self.username_input).render(areas[2], buf);
        (&self.password_input).render(areas[4], buf);

        let checkbox = if self.persist_token { "[x]" } else { "[ ]" };
        let persist_line = Line::from(vec![
            Span::styled(checkbox, Style::default().fg(self.theme.accent)),
            Span::raw(" Remember token (Tab to toggle)"),
        ]);
        let persist_para = Paragraph::new(persist_line);
        persist_para.render(areas[5], buf);

        let status = match self.state {
            LoginState::Input => Line::from(vec![
                Span::styled("Enter: Login / Register", Style::default().fg(Color::DarkGray)),
                Span::raw(" | "),
                Span::styled("Esc: Quit", Style::default().fg(Color::DarkGray)),
                Span::raw(" | "),
                Span::styled("Alt+D: Clear Saved Token", Style::default().fg(Color::DarkGray)),
            ]),
            LoginState::Validating => Line::from(Span::styled(
                "Logging in...",
                Style::default()
                    .fg(self.theme.accent)
                    .add_modifier(Modifier::ITALIC),
            )),
            LoginState::Error => {
                let msg = self.error_message.as_deref().unwrap_or("Unknown error");
                Line::from(Span::styled(
                    format!("Error: {msg}"),
                    Style::default().fg(Color::Red),
                ))
            }
            LoginState::Success => Line::from(Span::styled(
                "Login successful!",
                Style::default().fg(Color::Green),
            )),
        };

        let status_para = Paragraph::new(status).wrap(ratatui::widgets::Wrap { trim: true });
        status_para.render(areas[7], buf);
    }
}

impl Default for LoginScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for &LoginScreen {
    fn render(self, area: Rect, buf: &mut Buffer) {
        self.render_inner(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn test_initial_state() {
        let screen = LoginScreen::new();
        assert_eq!(screen.state(), LoginState::Input);
        assert!(screen.username().is_empty());
        assert!(screen.password().is_empty());
        assert!(!screen.has_credentials());
        assert!(screen.should_persist());
    }

    #[test]
    fn test_has_credentials() {
        let mut screen = LoginScreen::new();
        assert!(!screen.has_credentials());
        screen.handle_key(key(KeyCode::Char('u')));
        screen.handle_key(key(KeyCode::Char('1')));
        assert!(!screen.has_credentials());
        screen.handle_key(key(KeyCode::Enter)); // move to password
        screen.handle_key(key(KeyCode::Char('p')));
        screen.handle_key(key(KeyCode::Char('1')));
        assert!(screen.has_credentials());
    }

    #[test]
    fn test_toggle_persist() {
        let mut screen = LoginScreen::new();
        assert!(screen.should_persist());
        screen.handle_key(key(KeyCode::Tab));
        screen.handle_key(key(KeyCode::Tab));
        // Tab from Username -> Password, then Tab from Password toggles persist and cycles
        assert!(!screen.should_persist());
    }

    #[test]
    fn test_submit_empty_returns_none() {
        let mut screen = LoginScreen::new();
        assert_eq!(screen.handle_key(key(KeyCode::Enter)), LoginAction::None);
    }

    #[test]
    fn test_submit_with_credentials_returns_submit() {
        let mut screen = LoginScreen::new();
        screen.handle_key(key(KeyCode::Char('u')));
        screen.handle_key(key(KeyCode::Char('1')));
        screen.handle_key(key(KeyCode::Enter));
        screen.handle_key(key(KeyCode::Char('p')));
        screen.handle_key(key(KeyCode::Char('1')));
        assert_eq!(screen.handle_key(key(KeyCode::Enter)), LoginAction::Submit);
    }

    #[test]
    fn test_delete_token_action() {
        let mut screen = LoginScreen::new();
        let event = KeyEvent::new(KeyCode::Char('d'), KeyModifiers::ALT);
        assert_eq!(screen.handle_key(event), LoginAction::DeleteToken);
    }

    #[test]
    fn test_ctrl_v_returns_paste_action() {
        let mut screen = LoginScreen::new();
        let action = screen.handle_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::CONTROL));
        assert_eq!(action, LoginAction::Paste);
    }
}
