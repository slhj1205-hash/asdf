use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};

use lyre_core::player::PlaybackState;

use crate::app::App;
use crate::app_name::APP_NAME;
use crate::theme;

pub fn render(app: &App, area: Rect, buf: &mut Buffer) {
    let (state_label, state_style) = match app.player.state() {
        PlaybackState::Idle => ("idle", Style::new().fg(theme::text_muted())),
        PlaybackState::Playing => (
            "playing",
            Style::new().fg(theme::success()).add_modifier(Modifier::BOLD),
        ),
        PlaybackState::Paused => (
            "paused",
            Style::new().fg(theme::warning()).add_modifier(Modifier::BOLD),
        ),
    };

    let line = Line::from(vec![
        Span::raw(format!(
            " {APP_NAME} — {} song{} — ",
            app.library.len(),
            crate::strings::plural(app.library.len(), "s")
        )),
        Span::styled(state_label, state_style),
        Span::raw(format!(" — volume {:.0}% ", app.player.volume() * 100.0)),
    ]);

    Paragraph::new(line)
        .style(Style::new().fg(theme::text_primary()))
        .render(area, buf);
}
