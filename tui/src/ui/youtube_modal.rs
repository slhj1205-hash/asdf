use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Clear, Paragraph, Widget},
};

use crate::app::FormFields;
use crate::app::{App, FetchStatus, Mode, YoutubeField, YoutubeModal};
use crate::keymap::{self, ModalKey};
use crate::theme;

use super::{
    centered_rect, dim_area, modal_block, modal_body_style, modal_error_style, modal_hint_style,
    modal_label_style, modal_value_style,
};

const WIDTH: u16 = 62;

fn url_hint() -> String {
    format!(
        "{} fetch · {} cancel",
        keymap::display_for(crate::keymap::Action::Activate),
        keymap::modal_display_for(ModalKey::Cancel),
    )
}

fn fields_hint() -> String {
    format!(
        "{}/{} field · {} download · {} cancel",
        keymap::modal_display_for(ModalKey::NextField),
        keymap::modal_display_for(ModalKey::PrevField),
        keymap::display_for(crate::keymap::Action::Activate),
        keymap::modal_display_for(ModalKey::Cancel),
    )
}

fn render_lines(title: &str, lines: Vec<Line<'static>>, full_area: Rect, buf: &mut Buffer) {
    dim_area(full_area, buf);

    let height = lines.len() as u16 + 2;
    let popup = centered_rect(WIDTH, height, full_area);
    Widget::render(Clear, popup, buf);

    Paragraph::new(Text::from(lines))
        .style(modal_body_style())
        .block(modal_block(title).padding(ratatui::widgets::Padding::horizontal(2)))
        .render(popup, buf);
}

pub fn render(app: &App, full_area: Rect, buf: &mut Buffer) {
    let Some(Mode::Youtube(modal)) = app.modes.active() else {
        return;
    };

    match modal {
        YoutubeModal::EnteringUrl {
            url_input, error, ..
        } => render_entering_url(url_input, error.as_deref(), full_area, buf),
        YoutubeModal::EditingFields(fields) => render_editing_fields(fields, full_area, buf),
        YoutubeModal::ResolvingCollision { existing_path, .. } => {
            render_resolving_collision(existing_path, full_area, buf)
        }
        YoutubeModal::Downloading {
            file_name, progress, ..
        } => render_downloading(file_name, *progress, full_area, buf),
    }
}

fn render_entering_url(url_input: &str, error: Option<&str>, full_area: Rect, buf: &mut Buffer) {
    let mut lines = vec![
        Line::raw(""),
        Line::from(vec![Span::styled(
            format!("{url_input}▏"),
            modal_value_style(true),
        )]),
        Line::raw(""),
    ];
    if let Some(error) = error {
        lines.push(
            Line::from(error.to_string())
                .alignment(Alignment::Center)
                .style(modal_error_style()),
        );
        lines.push(Line::raw(""));
    }
    lines.push(
        Line::from(url_hint())
            .alignment(Alignment::Center)
            .style(modal_hint_style()),
    );

    render_lines(" Download from YouTube ", lines, full_area, buf);
}

fn render_editing_fields(
    fields: &crate::app::YoutubeFieldsModal,
    full_area: Rect,
    buf: &mut Buffer,
) {
    let visible = YoutubeField::visible(fields);
    let mut lines: Vec<Line> = Vec::with_capacity(visible.len() + 8);
    lines.push(Line::raw(""));

    match &fields.fetch_status {
        FetchStatus::Pending => {
            lines.push(
                Line::from("fetching…")
                    .alignment(Alignment::Center)
                    .style(modal_hint_style()),
            );
            lines.push(Line::raw(""));
        }
        FetchStatus::Ready { title, uploader } => {
            let uploader = uploader.as_deref().unwrap_or("unknown");
            lines.push(
                Line::from(title.clone())
                    .alignment(Alignment::Center)
                    .style(modal_value_style(true)),
            );
            lines.push(
                Line::from(format!("by {uploader}"))
                    .alignment(Alignment::Center)
                    .style(modal_hint_style()),
            );
            lines.push(Line::raw(""));
        }
    }

    for field in visible {
        let focused = field == fields.focused;
        let cursor = if focused { "▏" } else { "" };
        let label = format!("{:<16}", field.label());
        lines.push(Line::from(vec![
            Span::styled(label, modal_label_style()),
            Span::styled(
                format!("{}{cursor}", field.value(fields)),
                modal_value_style(focused),
            ),
        ]));
    }

    lines.push(Line::raw(""));
    if let Some(error) = &fields.error {
        lines.push(
            Line::from(error.clone())
                .alignment(Alignment::Center)
                .style(modal_error_style()),
        );
        lines.push(Line::raw(""));
    }
    lines.push(
        Line::from(fields_hint())
            .alignment(Alignment::Center)
            .style(modal_hint_style()),
    );

    render_lines(" Download from YouTube ", lines, full_area, buf);
}

fn render_resolving_collision(existing_path: &std::path::Path, full_area: Rect, buf: &mut Buffer) {
    let lines = vec![
        Line::raw(""),
        Line::from(existing_path.display().to_string()).alignment(Alignment::Center),
        Line::from("already exists").alignment(Alignment::Center),
        Line::raw(""),
        Line::from(vec![
            Span::styled(
                "<o>",
                Style::new().fg(theme::warning()).add_modifier(Modifier::BOLD),
            ),
            Span::raw(" overwrite      "),
            Span::styled(
                "<r>",
                Style::new()
                    .fg(theme::highlight())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" rename"),
        ])
        .alignment(Alignment::Center),
    ];

    render_lines(" File Exists ", lines, full_area, buf);
}

fn render_downloading(file_name: &str, progress: f64, full_area: Rect, buf: &mut Buffer) {
    let percent = progress.clamp(0.0, 100.0);
    let bar_width = 30;
    let filled = ((percent / 100.0) * bar_width as f64).round() as usize;
    let empty = bar_width - filled;
    let bar = format!("[{}{}] {:>3.0}%", "█".repeat(filled), " ".repeat(empty), percent);

    let lines = vec![
        Line::raw(""),
        Line::from("downloading…").alignment(Alignment::Center),
        Line::from(file_name.to_string())
            .alignment(Alignment::Center)
            .style(modal_hint_style()),
        Line::from(bar)
            .alignment(Alignment::Center)
            .style(modal_value_style(false)),
        Line::raw(""),
    ];

    render_lines(" Download from YouTube ", lines, full_area, buf);
}
