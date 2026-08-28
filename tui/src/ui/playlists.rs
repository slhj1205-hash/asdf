use lyre_core::PlaylistId;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{ListItem, ListState, StatefulWidget},
};

use crate::{
    app::{App, PlaylistView, is_filtering},
    theme,
};

use super::{
    Mode, PanelHeight, SongListContext, SongListPanelOptions, focus_style, label_for, plural,
    render_no_matches, render_song_list_panel, search_title, styled_list, titled_block,
    titled_block_split, unfocused_style, viewport,
};

fn playlist_name_style() -> Style {
    Style::new().fg(theme::playlist_name())
}

fn playlist_count_style() -> Style {
    Style::new().fg(theme::playlist_song_count())
}

pub fn render(app: &mut App, area: Rect, buf: &mut Buffer) {
    match app.playlist_panel.view {
        PlaylistView::Browsing => {
            let PanelHeight(height) = render_browsing(app, area, buf);
            app.measured.playlist_page_height = height;
        }
        PlaylistView::Viewing(id) => render_viewing(app, id, area, buf),
    }
}

fn render_browsing(app: &mut App, area: Rect, buf: &mut Buffer) -> PanelHeight {
    let ids = app.visible_playlist_ids();
    let match_count = ids.len();
    let empty_store = app.playlists.is_empty();
    let search_mode_open = matches!(app.modes.active(), Some(Mode::SearchPlaylists));
    let filtering = is_filtering(&app.playlist_panel.search_query);
    let border_style = if search_mode_open {
        focus_style()
    } else {
        unfocused_style()
    };

    let block = if empty_store && !search_mode_open && !filtering {
        let key = crate::keymap::display_for(crate::keymap::Action::OpenSongModal);
        titled_block(
            format!(" Playlists — none yet, press {key} on a song to create one "),
            unfocused_style(),
        )
    } else {
        let left_title = search_title(
            "Playlists",
            search_mode_open,
            &app.playlist_panel.search_query,
            match_count,
            border_style,
        );
        titled_block_split(left_title, None, border_style)
    };

    let inner_height = block.inner(area).height as usize;

    if !empty_store && match_count == 0 && filtering {
        app.playlist_panel.list_state.select(None);
        render_no_matches(
            area,
            buf,
            block,
            &app.playlist_panel.search_query,
            "playlists",
        );
        return PanelHeight(inner_height);
    }

    let window = viewport(&mut app.playlist_panel.list_state, &ids, inner_height);

    let items: Vec<ListItem> = window
        .items
        .iter()
        .map(|&id| {
            let playlist = app.playlists.get(id);
            let name = playlist.map(|p| p.name()).unwrap_or("<unknown>");
            let count = playlist.map(|p| p.len()).unwrap_or(0);
            let count_text = format!("  ({count} song{})", plural(count, "s"));
            ListItem::new(Line::from(vec![
                Span::styled(name.to_string(), playlist_name_style()),
                Span::styled(count_text, playlist_count_style()),
            ]))
        })
        .collect();

    let mut local = ListState::default().with_offset(0);
    local.select(window.selected);

    let list = styled_list(items, block);
    StatefulWidget::render(list, area, buf, &mut local);
    PanelHeight(inner_height)
}

fn render_viewing(app: &mut App, id: PlaylistId, area: Rect, buf: &mut Buffer) {
    let name = app
        .playlists
        .get(id)
        .map(|p| p.name().to_string())
        .unwrap_or_else(|| "<deleted>".to_string());
    app.visible_rows();
    let rows = app.rows.rows_unchecked();
    let current = app.queue.current_id();

    let category = app.playlist_panel.category;
    let sort = app.playlist_panel.sort;
    let filtering = is_filtering(&app.playlist_panel.search_query);
    let root = app.library.root();

    let group_label_fn = |song: &lyre_core::Song| -> Option<String> {
        if filtering {
            None
        } else {
            crate::app::group_label(song, category, root)
        }
    };

    let opts = SongListPanelOptions {
        visual_range: app.visual_row_range(),
        title_prefix: &name,
        category_label: app.playlist_panel.category.label(),
        sort_label: app.playlist_panel.sort.label(),
        search_mode_open: matches!(app.modes.active(), Some(Mode::SearchPlaylists)),
        query: &app.playlist_panel.search_query,
        group_label: Some(&group_label_fn),
    };
    let PanelHeight(height) = render_song_list_panel(
        area,
        buf,
        &mut app.playlist_panel.list_state,
        rows,
        opts,
        SongListContext {
            current,
            library: &app.library,
            playlist_info: None,
        },
        |song| {
            if filtering {
                label_for(song, None, sort)
            } else {
                label_for(song, Some(category), sort)
            }
        },
    );
    app.measured.playlist_page_height = height;
}
