use lyre_core::{PlaylistId, SongId};
use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Clear, Paragraph, Widget},
};

use crate::app::{App, Mode};
use crate::app_name::APP_NAME;
use crate::keymap::{self, Section};
use crate::theme;

use super::{centered_rect, dim_area, display_width, modal_block, modal_body_style};

fn key_style() -> Style {
    Style::new()
        .fg(theme::key_hint())
        .add_modifier(Modifier::BOLD)
}

fn yes_no_line() -> Line<'static> {
    Line::from(vec![
        Span::styled(
            keymap::confirm_display_for(keymap::ConfirmChoice::Yes),
            Style::new().fg(theme::success()).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" yes      "),
        Span::styled(
            keymap::confirm_display_for(keymap::ConfirmChoice::No),
            Style::new().fg(theme::error()).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" no"),
    ])
    .alignment(Alignment::Center)
}

fn render_confirm(
    title: &str,
    lines: Vec<Line<'static>>,
    min_width: u16,
    full_area: Rect,
    buf: &mut Buffer,
) {
    dim_area(full_area, buf);

    let inner_width = (min_width.saturating_sub(2)) as usize;
    let wrapped = lines
        .iter()
        .map(|line| match line.alignment {
            Some(Alignment::Center) | Some(Alignment::Right) => {
                display_width(&line.to_string()).div_ceil(inner_width.max(1)).max(1)
            }
            _ => 1,
        })
        .sum::<usize>();
    let height = (lines.len() + wrapped.saturating_sub(lines.len()) + 3) as u16;
    let width = min_width
        .max(
            lines
                .iter()
                .map(|line| display_width(&line.to_string()) as u16)
                .max()
                .unwrap_or(0)
                .saturating_add(2),
        )
        .min(full_area.width);

    let popup = centered_rect(width, height, full_area);
    Widget::render(Clear, popup, buf);

    Paragraph::new(Text::from(lines))
        .style(modal_body_style())
        .block(modal_block(title))
        .render(popup, buf);
}

pub fn render_quit_confirm(full_area: Rect, buf: &mut Buffer) {
    let lines = vec![
        Line::raw(""),
        Line::from(format!("Quit {APP_NAME}?")).alignment(Alignment::Center),
        Line::raw(""),
        yes_no_line(),
    ];

    render_confirm(" Quit ", lines, 40, full_area, buf);
}

pub fn render_remove_confirm(
    app: &App,
    playlist_id: PlaylistId,
    song_id: SongId,
    full_area: Rect,
    buf: &mut Buffer,
) {
    let song_label = app
        .library
        .get(song_id)
        .map(|s| s.to_string())
        .unwrap_or_else(|| "this song".to_string());
    let playlist_label = app
        .playlists
        .get(playlist_id)
        .map(|p| p.name().to_string())
        .unwrap_or_else(|| "this playlist".to_string());

    let lines = vec![
        Line::raw(""),
        Line::from(song_label).alignment(Alignment::Center),
        Line::from(format!("Remove from \"{playlist_label}\"?")).alignment(Alignment::Center),
        Line::raw(""),
        yes_no_line(),
    ];

    render_confirm(" Remove Song ", lines, 46, full_area, buf);
}

pub fn render_romanized_artist_confirm(app: &App, full_area: Rect, buf: &mut Buffer) {
    let Some(Mode::RomanizedArtistConfirm(confirm)) = app.modes.active() else {
        return;
    };

    let lines = vec![
        Line::raw(""),
        Line::from(format!(
            "Apply \"{}\" as the romanized artist",
            confirm.value
        ))
        .alignment(Alignment::Center),
        Line::from(format!(
            "to {} other song{} by {}?",
            confirm.count,
            crate::ui::plural(confirm.count, "s"),
            confirm.artist_display
        ))
        .alignment(Alignment::Center),
        Line::raw(""),
        yes_no_line(),
    ];

    render_confirm(" Apply Romanized Artist ", lines, 54, full_area, buf);
}

pub fn render_help_overlay(app: &App, full_area: Rect, buf: &mut Buffer) {
    dim_area(full_area, buf);

    let header_style = Style::new().fg(theme::warning()).add_modifier(Modifier::BOLD);
    let note_style = Style::new().fg(theme::text_muted());

    let row = |key: &str, desc: &str| -> Line<'static> {
        Line::from(vec![
            Span::styled(format!("  {key:<20}"), key_style()),
            Span::raw(desc.to_string()),
        ])
    };

    let section_lines = |section: Section| -> Vec<Line<'static>> {
        keymap::help_rows(section)
            .into_iter()
            .map(|(key, desc)| row(&key, desc))
            .collect()
    };

    let backend_note = app.player.describe_backend();

    let mut lines = vec![Line::styled("Global", header_style)];
    lines.extend(section_lines(Section::Global));
    lines.push(Line::raw(""));
    lines.push(Line::styled("Library", header_style));
    lines.extend(section_lines(Section::Library));
    lines.push(Line::raw(""));
    lines.push(Line::styled("Playlists", header_style));
    lines.extend(section_lines(Section::Playlists));
    lines.extend([
        Line::raw(""),
        Line::styled(format!("  Backend: {backend_note}"), note_style),
        Line::styled("  Mouse: not supported", note_style),
        Line::raw(""),
        Line::from("press any key to close")
            .alignment(Alignment::Center)
            .style(note_style),
    ]);

    let height = (lines.len() as u16 + 2).min(full_area.height);
    let popup = centered_rect(58, height, full_area);
    Widget::render(Clear, popup, buf);

    let visible_lines = popup.height.saturating_sub(2) as usize;
    lines.truncate(visible_lines);

    Paragraph::new(Text::from(lines))
        .style(modal_body_style())
        .block(modal_block(" Help "))
        .render(popup, buf);
}
