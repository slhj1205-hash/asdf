use lyre_core::{PlaylistId, Song};
use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Text},
    widgets::{Clear, ListItem, ListState, Paragraph, StatefulWidget, Widget},
};

use crate::app::{App, ChooseActionField, Mode, Panel, PlaylistView, PlaylistPicker, SongModal};
use crate::keymap::{self, Action, ModalKey};
use crate::theme;

use super::{content_style, dim_area, marker_style, modal_block, side_by_side_rect, styled_list};

const BASE_WIDTH: u16 = 56;
const SIDE_WIDTH: u16 = 46;
const SIDE_HEIGHT: u16 = 14;

fn base_box_height(song: Option<&Song>, batched: bool, has_add: bool, has_remove: bool) -> u16 {
    let mut lines: u16 = 5;
    if has_add {
        lines += 1;
    }
    if has_remove {
        lines += 1;
    }
    if !batched && song.is_some() {
        lines += 1;
    }
    if !batched && song.and_then(|s| s.metadata().album.as_deref()).is_some() {
        lines += 1;
    }
    lines + 4
}

pub fn render(app: &App, full_area: Rect, buf: &mut Buffer) {
    dim_area(full_area, buf);

    let Some(Mode::SongModal(modal)) = app.modes.active() else {
        return;
    };

    let song = app.library.get(modal.song);
    let has_side = modal.picker.is_some();
    let has_remove = match &modal.batch {
        Some(ids) => ids
            .iter()
            .any(|&id| !app.playlists.containing(id).is_empty()),
        None => !app.playlists.containing(modal.song).is_empty(),
    };
    let content_height = base_box_height(
        song,
        modal.batch.is_some(),
        !app.playlists.is_empty(),
        has_remove,
    );
    let height = if has_side {
        content_height.max(SIDE_HEIGHT)
    } else {
        content_height
    };

    let (base_rect, side_rect) =
        side_by_side_rect(BASE_WIDTH, SIDE_WIDTH, height, has_side, full_area);

    render_choose_action(app, modal, base_rect, has_remove, buf);

    match &modal.picker {
        Some(PlaylistPicker::Add {
            options,
            pinned,
            list_state,
        }) => {
            render_add_to_playlist_picker(app, options, pinned, list_state, side_rect, buf);
        }
        Some(PlaylistPicker::Remove {
            options,
            list_state,
        }) => {
            render_remove_from_playlist_picker(app, options, list_state, side_rect, buf);
        }
        None => {}
    }
}

fn render_choose_action(
    app: &App,
    modal: &SongModal,
    popup: Rect,
    has_remove: bool,
    buf: &mut Buffer,
) {
    Widget::render(Clear, popup, buf);

    let song = app.library.get(modal.song);

    let title_line = match &modal.batch {
        Some(ids) => Line::from(format!("{} songs selected", ids.len())),
        None => Line::from(
            song.map(|s| s.title().to_string())
                .unwrap_or_else(|| "this song".to_string()),
        ),
    }
    .alignment(Alignment::Center)
    .style(
        Style::new()
            .fg(theme::text_primary())
            .add_modifier(Modifier::BOLD),
    );

    let artist_line = song.filter(|_| modal.batch.is_none()).map(|s| {
        Line::from(s.artist().to_string())
            .alignment(Alignment::Center)
            .style(Style::new().fg(theme::text_muted()))
    });

    let album_line = song
        .filter(|_| modal.batch.is_none())
        .and_then(|s| s.metadata().album.as_deref())
        .map(|album| {
            Line::from(album.to_string())
                .alignment(Alignment::Center)
                .style(Style::new().fg(theme::text_muted()))
        });

    let add_selected = modal.selected == ChooseActionField::AddToPlaylist;
    let remove_selected = modal.selected == ChooseActionField::RemoveFromPlaylist;
    let create_selected = modal.selected == ChooseActionField::CreatePlaylist;

    let add_line = (!app.playlists.is_empty()).then(|| {
        let (marker, style) = marker_style(add_selected);
        Line::from(format!("{marker}Add to Playlist")).style(style)
    });

    let remove_line = has_remove.then(|| {
        let (marker, style) = marker_style(remove_selected);
        Line::from(format!("{marker}Remove from Playlist")).style(style)
    });

    let create_line = {
        let (marker, style) = marker_style(create_selected);
        let cursor = if create_selected { "▏" } else { "" };
        Line::from(format!(
            "{marker}New playlist: {}{cursor}",
            modal.name_input
        ))
        .style(style)
    };

    let (down, up) = (
        keymap::display_for(Action::MoveDown),
        keymap::display_for(Action::MoveUp),
    );
    let hint = if create_selected {
        format!(
            "{enter} create · {cancel} cancel",
            enter = keymap::display_for(Action::Activate),
            cancel = keymap::modal_display_for(ModalKey::Cancel),
        )
    } else {
        format!(
            "{down}/{up} select · {choose} choose · {cancel} cancel",
            choose = keymap::display_for(Action::Activate),
            cancel = keymap::modal_display_for(ModalKey::Cancel),
        )
    };

    let mut lines = vec![title_line];
    lines.extend(artist_line);
    lines.extend(album_line);
    lines.push(Line::raw(""));
    lines.extend(add_line);
    lines.extend(remove_line);
    lines.push(create_line);
    lines.push(Line::raw(""));
    lines.push(
        Line::from(hint)
            .alignment(Alignment::Center)
            .style(Style::new().fg(theme::text_muted())),
    );

    Paragraph::new(Text::from(lines))
        .style(content_style())
        .block(modal_block(" Song ").padding(ratatui::widgets::Padding::symmetric(2, 1)))
        .render(popup, buf);
}

fn render_add_to_playlist_picker(
    app: &App,
    options: &[PlaylistId],
    pinned: &[PlaylistId],
    list_state: &ListState,
    popup: Rect,
    buf: &mut Buffer,
) {
    Widget::render(Clear, popup, buf);

    let viewing_id = match (app.panel, app.playlist_panel.view) {
        (Panel::Playlists, PlaylistView::Viewing(id)) => Some(id),
        _ => None,
    };

    let mut items: Vec<ListItem> = Vec::new();
    for &id in pinned {
        let name = app
            .playlists
            .get(id)
            .map(|p| p.name())
            .unwrap_or("<deleted>");
        let label = if Some(id) == viewing_id {
            "current"
        } else {
            "already added"
        };
        items.push(
            ListItem::new(format!("{name} ({label})")).style(Style::new().fg(theme::text_dim())),
        );
    }
    for &id in options {
        let name = app
            .playlists
            .get(id)
            .map(|p| p.name())
            .unwrap_or("<deleted>");
        items.push(ListItem::new(name.to_string()).style(content_style()));
    }

    let mut render_state = *list_state;
    if !pinned.is_empty()
        && let Some(i) = list_state.selected()
    {
        render_state.select(Some(i + pinned.len()));
    }

    let list = styled_list(
        items,
        modal_block(" Add to Playlist ").padding(ratatui::widgets::Padding::horizontal(1)),
    );

    StatefulWidget::render(list, popup, buf, &mut render_state);
}

fn render_remove_from_playlist_picker(
    app: &App,
    options: &[PlaylistId],
    list_state: &ListState,
    popup: Rect,
    buf: &mut Buffer,
) {
    Widget::render(Clear, popup, buf);

    let items: Vec<ListItem> = options
        .iter()
        .map(|&id| {
            let name = app
                .playlists
                .get(id)
                .map(|p| p.name())
                .unwrap_or("<deleted>");
            ListItem::new(name.to_string()).style(content_style())
        })
        .collect();

    let mut render_state = *list_state;

    let list = styled_list(
        items,
        modal_block(" Remove from Playlist ").padding(ratatui::widgets::Padding::horizontal(1)),
    );

    StatefulWidget::render(list, popup, buf, &mut render_state);
}
