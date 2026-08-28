use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    widgets::{Paragraph, Widget},
};

use crate::app::{App, StatusKind};
use crate::keymap;
use crate::theme;

pub fn render(app: &App, area: Rect, buf: &mut Buffer) {
    let layout = Layout::vertical([Constraint::Length(1), Constraint::Length(1)]);
    let [status_area, help_area] = area.layout(&layout);

    let status_style = match app.status.kind {
        StatusKind::Info => Style::new().fg(theme::status_info()),
        StatusKind::Success => Style::new().fg(theme::success()),
        StatusKind::Error => Style::new().fg(theme::error()).add_modifier(Modifier::BOLD),
    };
    let status_text = if app.status.kind == StatusKind::Error && !app.status.text.is_empty() {
        format!("⚠ {}", app.status.text)
    } else {
        app.status.text.clone()
    };
    Paragraph::new(status_text)
        .style(status_style)
        .render(status_area, buf);

    Paragraph::new(keymap::footer_hint())
        .style(Style::new().fg(theme::text_dim()))
        .render(help_area, buf);
}
