use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span, Text},
    widgets::{List, ListItem, Paragraph, Widget},
};

use crate::app::{App, QueueSource};
use crate::theme;

use super::{
    content_style, format_duration, marquee_window, plural, titled_block, unfocused_style,
};

fn queue_source_label(app: &App) -> String {
    match app.queue_source() {
        QueueSource::Library => "Library".to_string(),
        QueueSource::Playlist(id) => match app.playlists.get(id) {
            Some(playlist) => {
                let count = playlist.len();
                format!(
                    "Playlist: {} ({count} song{})",
                    playlist.name(),
                    plural(count, "s")
                )
            }
            None => "Playlist".to_string(),
        },
    }
}

pub fn render(app: &App, area: Rect, buf: &mut Buffer) {
    let layout = Layout::vertical([Constraint::Length(5), Constraint::Fill(1)]);
    let [info_area, upnext_area] = area.layout(&layout);

    let current_song = app.queue.current(&app.library);
    let inner_width = info_area.width.saturating_sub(2) as usize;

    let info_text = match current_song {
        Some(song) => Text::from(vec![
            Line::raw(marquee_window(song.title(), inner_width).into_owned()),
            Line::raw(marquee_window(song.artist(), inner_width).into_owned()),
            Line::raw(marquee_window(song.album(), inner_width).into_owned()),
        ]),
        None => Text::from(format!(
            "Nothing playing — select a song and press {}",
            crate::keymap::display_for(crate::keymap::Action::Activate)
        )),
    };
    Paragraph::new(info_text)
        .style(content_style())
        .block(titled_block(" Now Playing ", unfocused_style()))
        .render(info_area, buf);

    let upcoming = app
        .queue
        .upcoming(upnext_area.height.saturating_sub(2) as usize);
    let upnext_title = format!(" Up Next — {} ", queue_source_label(app));

    if upcoming.is_empty() {
        super::render_centered_message(
            upnext_area,
            buf,
            titled_block(upnext_title, unfocused_style()),
            "nothing queued",
        );
        return;
    }

    let items: Vec<ListItem> = upcoming
        .iter()
        .enumerate()
        .map(|(i, &id)| {
            let label = app
                .library
                .get(id)
                .map(|s| s.to_string())
                .unwrap_or_else(|| "<missing>".to_string());
            ListItem::new(format!("{:>2}. {label}", i + 1)).style(content_style())
        })
        .collect();

    Widget::render(
        List::new(items).block(titled_block(upnext_title, unfocused_style())),
        upnext_area,
        buf,
    );
}

pub fn render_position(app: &App, area: Rect, buf: &mut Buffer) {
    let position = app.player.position();
    let duration = app.player.duration();
    let ratio = match (position, duration) {
        (Some(p), Some(d)) if !d.is_zero() => (p.as_secs_f64() / d.as_secs_f64()).clamp(0.0, 1.0),
        _ => 0.0,
    };

    let pos_label = format!(
        " {} ",
        position
            .map(format_duration)
            .unwrap_or_else(|| "--:--".to_string())
    );
    let dur_label = format!(
        " {} ",
        duration
            .map(format_duration)
            .unwrap_or_else(|| "--:--".to_string())
    );

    let layout = Layout::horizontal([
        Constraint::Length(pos_label.len() as u16),
        Constraint::Fill(1),
        Constraint::Length(dur_label.len() as u16),
    ]);
    let [pos_area, gauge_area, dur_area] = area.layout(&layout);

    Paragraph::new(pos_label)
        .style(Style::new().fg(theme::text_muted()))
        .render(pos_area, buf);

    let bar_cells = gauge_area.width as usize;
    let filled = (ratio * bar_cells as f64).round() as usize;
    let empty = bar_cells - filled;

    let fill = Span::styled(
        format!("{}●", "━".repeat(filled)),
        Style::new().fg(theme::gauge_foreground()),
    );
    let track = Span::styled("┉".repeat(empty), Style::new().fg(theme::gauge_background()));

    Paragraph::new(Line::from(vec![fill, track])).render(gauge_area, buf);

    Paragraph::new(dur_label)
        .style(Style::new().fg(theme::text_muted()))
        .alignment(Alignment::Right)
        .render(dur_area, buf);
}
