use lyre_core::SongId;
use ratatui::{buffer::Buffer, layout::Rect};

use crate::app::App;

use super::{Mode, PanelHeight, SongListContext, SongPanelInputs, render_song_panel};

pub fn render(app: &mut App, area: Rect, buf: &mut Buffer) {
    let current = app.queue.current_id();
    let visual_range = app.visual_row_range();

    app.visible_rows();
    let rows = app.rows.cached_rows();

    let playlists = &app.playlists;
    let playlist_names = |song_id: SongId| -> Vec<String> {
        playlists
            .containing(song_id)
            .iter()
            .filter_map(|&id| playlists.get(id).map(|p| p.name().to_string()))
            .collect()
    };

    let PanelHeight(height) = render_song_panel(
        area,
        buf,
        &mut app.library_panel.list_state,
        rows,
        SongListContext {
            current,
            library: &app.library,
            playlist_info: Some((app.library_panel.playlist_mode, &playlist_names)),
        },
        SongPanelInputs {
            title_prefix: "Library",
            category: app.library_panel.category,
            sort: app.library_panel.sort,
            query: &app.library_panel.search_query,
            search_mode_open: matches!(app.modes.active(), Some(Mode::SearchLibrary)),
            visual_range,
            root: app.library.root(),
        },
    );
    app.measured.library_page_height = height;
}
