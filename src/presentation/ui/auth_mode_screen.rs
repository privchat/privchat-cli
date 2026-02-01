//! 登录/注册选择屏：无 token 时先显示两个按钮（登录 / 注册），选择后再进入用户名密码输入框。

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget},
};

use crate::presentation::theme::Theme;

/// 当前焦点：第一个按钮 = 登录，第二个 = 注册。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AuthModeFocus {
    #[default]
    Login,
    Register,
}

/// 用户选择结果：进入登录表单 或 进入注册表单。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthModeAction {
    None,
    SelectLogin,
    SelectRegister,
}

/// 登录/注册选择屏：上下两个按钮。
pub struct AuthModeScreen {
    focus: AuthModeFocus,
    theme: Theme,
}

impl AuthModeScreen {
    #[must_use]
    pub fn new() -> Self {
        Self {
            focus: AuthModeFocus::Login,
            theme: Theme::default(),
        }
    }

    #[must_use]
    pub fn with_theme(mut self, theme: Theme) -> Self {
        self.theme = theme;
        self
    }

    /// 处理按键：j/k 或上下移动焦点，Enter 确认。
    pub fn handle_key(&mut self, key: KeyEvent) -> AuthModeAction {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.focus = AuthModeFocus::Login;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.focus = AuthModeFocus::Register;
            }
            KeyCode::Enter => {
                return match self.focus {
                    AuthModeFocus::Login => AuthModeAction::SelectLogin,
                    AuthModeFocus::Register => AuthModeAction::SelectRegister,
                };
            }
            _ => {}
        }
        AuthModeAction::None
    }

    fn render_inner(&self, area: Rect, buf: &mut Buffer) {
        let vertical = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Min(14),
            Constraint::Fill(1),
        ]);
        let [_, center, _] = vertical.areas(area);

        let horizontal = Layout::horizontal([
            Constraint::Fill(1),
            Constraint::Min(40),
            Constraint::Fill(1),
        ]);
        let [_, content_area, _] = horizontal.areas(center);

        Clear.render(content_area, buf);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.theme.accent))
            .title(" Privchat ");

        let inner = block.inner(content_area);
        block.render(content_area, buf);

        let constraints = vec![
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Min(2),
        ];
        let inner_layout = Layout::vertical(constraints);
        let areas = inner_layout.split(inner);

        let title = Paragraph::new("选择登录或注册")
            .style(Style::default().fg(Color::White));
        title.render(areas[0], buf);

        let login_selected = self.focus == AuthModeFocus::Login;
        let login_style = if login_selected {
            Style::default().fg(self.theme.accent).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };
        let login_line = Line::from(vec![
            Span::styled("  [ ", Style::default().fg(Color::DarkGray)),
            Span::styled(if login_selected { "●" } else { " " }, login_style),
            Span::styled(" ] 登录 ", login_style),
        ]);
        Paragraph::new(login_line).render(areas[1], buf);

        let register_selected = self.focus == AuthModeFocus::Register;
        let register_style = if register_selected {
            Style::default().fg(self.theme.accent).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };
        let register_line = Line::from(vec![
            Span::styled("  [ ", Style::default().fg(Color::DarkGray)),
            Span::styled(if register_selected { "●" } else { " " }, register_style),
            Span::styled(" ] 注册 ", register_style),
        ]);
        Paragraph::new(register_line).render(areas[3], buf);

        let hint = Line::from(Span::styled(
            "j/k 或 ↑/↓ 切换，Enter 确认 | Esc 退出",
            Style::default().fg(Color::DarkGray),
        ));
        Paragraph::new(hint).render(areas[5], buf);
    }
}

impl Default for AuthModeScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for &AuthModeScreen {
    fn render(self, area: Rect, buf: &mut Buffer) {
        self.render_inner(area, buf);
    }
}
