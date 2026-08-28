use lyre_core::SongId;
use ratatui::{buffer::Buffer, layout::Rect};

use crate::app::{App, is_filtering};

use super::{
    Mode, PanelHeight, SongListContext, SongListPanelOptions, label_for, render_song_list_panel,
};

pub fn render(app: &mut App, area: Rect, buf: &mut Buffer) {
    let current = app.queue.current_id();

    app.visible_rows();
    let rows = app.rows.rows_unchecked();

    let category = app.library_panel.category;
    let sort = app.library_panel.sort;
    let filtering = is_filtering(&app.library_panel.search_query);
    let playlist_mode = app.library_panel.playlist_mode;
    let playlists = &app.playlists;
    let root = app.library.root();

    let playlist_names = |song_id: SongId| -> Vec<String> {
        playlists
            .containing(song_id)
            .iter()
            .filter_map(|&id| playlists.get(id).map(|p| p.name().to_string()))
            .collect()
    };

    let group_label_fn = |song: &lyre_core::Song| -> Option<String> {
        if filtering {
            None
        } else {
            crate::app::group_label(song, category, root)
        }
    };

    let opts = SongListPanelOptions {
        visual_range: app.visual_row_range(),
        title_prefix: "Library",
        category_label: app.library_panel.category.label(),
        sort_label: app.library_panel.sort.label(),
        search_mode_open: matches!(app.modes.active(), Some(Mode::SearchLibrary)),
        query: &app.library_panel.search_query,
        group_label: Some(&group_label_fn),
    };
    let PanelHeight(height) = render_song_list_panel(
        area,
        buf,
        &mut app.library_panel.list_state,
        rows,
        opts,
        SongListContext {
            current,
            library: &app.library,
            playlist_info: Some((playlist_mode, &playlist_names)),
        },
        |song| {
            if filtering {
                label_for(song, None, sort)
            } else {
                label_for(song, Some(category), sort)
            }
        },
    );
    app.measured.library_page_height = height;
}
