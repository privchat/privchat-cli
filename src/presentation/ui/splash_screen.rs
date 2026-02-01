use std::time::Duration;

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    text::Text,
    widgets::{Paragraph, Widget},
};
use tachyonfx::{Effect, Interpolation, fx};

const LOGO_TEXT: &str = "
  ░██████              ░██                                      ░██
 ░██   ░██                                                      ░██
░██     ░██ ░██    ░██ ░██ ░███████   ░███████  ░██░████  ░████████
░██     ░██  ░██  ░██  ░██░██    ░██ ░██    ░██ ░███     ░██    ░██
░██     ░██   ░█████   ░██░██        ░██    ░██ ░██      ░██    ░██
 ░██   ░██   ░██  ░██  ░██░██    ░██ ░██    ░██ ░██      ░██   ░███
  ░██████   ░██    ░██ ░██ ░███████   ░███████  ░██       ░█████░██";

#[derive(Default)]
pub struct LoadingState {
    pub data_ready: bool,
    pub animation_complete: bool,
    pub intro_finished: bool,
}

pub struct SplashScreen {
    intro_effect: Effect,
    outro_effect: Effect,
    pub state: LoadingState,
    pending_duration: Duration,
}

impl Default for SplashScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl SplashScreen {
    #[must_use]
    pub fn new() -> Self {
        let intro_effect = fx::coalesce((800, Interpolation::CircOut));
        let outro_effect = fx::dissolve((600, Interpolation::CircIn));

        Self {
            intro_effect,
            outro_effect,
            state: LoadingState::default(),
            pending_duration: Duration::ZERO,
        }
    }

    pub fn tick(&mut self, duration: Duration) {
        self.pending_duration = self.pending_duration.saturating_add(duration);
    }

    pub fn set_data_ready(&mut self) {
        self.state.data_ready = true;
    }
}

impl Widget for &mut SplashScreen {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let text_content = LOGO_TEXT.trim_matches('\n');
        let text = Text::from(text_content).centered();

        let text_width = u16::try_from(
            text.lines
                .iter()
                .map(ratatui::prelude::Line::width)
                .max()
                .unwrap_or(0),
        )
        .unwrap_or(0);
        let text_height = u16::try_from(text.lines.len()).unwrap_or(0);

        let x = area.x + (area.width.saturating_sub(text_width)) / 2;
        let y = area.y + (area.height.saturating_sub(text_height)) / 2;
        let center_area = Rect::new(
            x,
            y,
            text_width.min(area.width),
            text_height.min(area.height),
        );

        Paragraph::new(text).render(center_area, buf);

        let duration = self.pending_duration;
        self.pending_duration = Duration::ZERO;

        if !self.state.intro_finished {
            let overflow = self.intro_effect.process(duration.into(), buf, center_area);
            if overflow.is_some() {
                self.state.intro_finished = true;
            }
        } else if self.state.data_ready {
            let overflow = self.outro_effect.process(duration.into(), buf, center_area);
            if overflow.is_some() {
                self.state.animation_complete = true;
            }
        }
    }
}
