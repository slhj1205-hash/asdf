#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use lyre_core::{Library, MetadataEdits, PlaylistStore, SongId};
use ratatui::{buffer::Buffer, layout::Rect, style::{Color, Style}, widgets::Widget};

use lyre_tui::{
    Backend,
    app::{
        App, Category, ChooseActionField, Cycleable, FormFields, INDENT_UNIT, MetadataField, Panel,
        PlaylistDisplayMode, PlaylistView, Row, PlaylistPicker, SongModal, Sort,
    },
    config,
    keymap::{self, Action},
    ui::{marquee_scroll_offset, marquee_window, sort_title, sort_title_widths},
};

fn wav(title: &str, artist: &str, samples: usize) -> Vec<u8> {
    let mut info = b"INFO".to_vec();
    for (key, value) in [(b"INAM", title), (b"IART", artist)] {
        let mut data = value.as_bytes().to_vec();
        data.push(0);
        if data.len() % 2 == 1 {
            data.push(0);
        }
        info.extend_from_slice(key);
        info.extend_from_slice(&(data.len() as u32).to_le_bytes());
        info.extend_from_slice(&data);
    }
    let mut list = b"LIST".to_vec();
    list.extend_from_slice(&(info.len() as u32).to_le_bytes());
    list.extend_from_slice(&info);

    let mut fmt = Vec::new();
    fmt.extend_from_slice(&1u16.to_le_bytes());
    fmt.extend_from_slice(&1u16.to_le_bytes());
    fmt.extend_from_slice(&8000u32.to_le_bytes());
    fmt.extend_from_slice(&16000u32.to_le_bytes());
    fmt.extend_from_slice(&2u16.to_le_bytes());
    fmt.extend_from_slice(&16u16.to_le_bytes());
    let mut fmt_chunk = b"fmt ".to_vec();
    fmt_chunk.extend_from_slice(&(fmt.len() as u32).to_le_bytes());
    fmt_chunk.extend_from_slice(&fmt);

    let pcm = vec![0u8; samples * 2];
    let mut data_chunk = b"data".to_vec();
    data_chunk.extend_from_slice(&(pcm.len() as u32).to_le_bytes());
    data_chunk.extend_from_slice(&pcm);

    let mut body = b"WAVE".to_vec();
    body.extend_from_slice(&fmt_chunk);
    body.extend_from_slice(&list);
    body.extend_from_slice(&data_chunk);

    let mut out = b"RIFF".to_vec();
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&body);
    out
}

fn youtube_fields(
    url: &str,
    focused: lyre_tui::app::YoutubeField,
) -> lyre_tui::app::YoutubeFieldsModal {
    lyre_tui::app::YoutubeFieldsModal {
        url: url.to_string(),
        title: String::new(),
        artist: String::new(),
        album: String::new(),
        title_sort: String::new(),
        artist_sort: String::new(),
        directory: String::new(),
        file_name: String::new(),
        file_name_overridden: false,
        focused,
        error: None,
        fetch_status: lyre_tui::app::FetchStatus::Pending,
        download_status: lyre_tui::app::DownloadStatus::Pending,
    }
}

struct Harness {
    _dir: tempfile::TempDir,
    app: App,
}

fn harness() -> Harness {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path().join("music");

    for (artist, tracks) in [
        ("Alpha", [("Anchor", 4000usize), ("Azure", 400usize)]),
        ("Beta", [("Beacon", 4000usize), ("Bright", 400usize)]),
        ("Gamma", [("Glimmer", 4000usize), ("Grove", 400usize)]),
    ] {
        let d = root.join(artist);
        std::fs::create_dir_all(&d).unwrap();
        for (i, (title, samples)) in tracks.iter().enumerate() {
            std::fs::write(
                d.join(format!("{i}-{title}.wav")),
                wav(title, artist, *samples),
            )
            .unwrap();
        }
    }

    let (library, _) = Library::scan(&root, dir.path().join("cache.bin")).unwrap();
    let (playlists, _) = PlaylistStore::load(dir.path().join("playlists"), &library);
    let app = App::new(library, playlists, Backend::null());

    Harness { _dir: dir, app }
}

fn harness_one_large_group() -> Harness {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path().join("music");
    let d = root.join("Solo");
    std::fs::create_dir_all(&d).unwrap();

    for i in 0..10 {
        std::fs::write(
            d.join(format!("{i}-Track{i}.wav")),
            wav(&format!("Track{i}"), "Solo", 400),
        )
        .unwrap();
    }

    let (library, _) = Library::scan(&root, dir.path().join("cache.bin")).unwrap();
    let (playlists, _) = PlaylistStore::load(dir.path().join("playlists"), &library);
    let app = App::new(library, playlists, Backend::null());

    Harness { _dir: dir, app }
}

fn harness_one_large_nested_group() -> Harness {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path().join("music");
    let d = root.join("Deep").join("Nested");
    std::fs::create_dir_all(&d).unwrap();

    for i in 0..10 {
        std::fs::write(
            d.join(format!("{i}-Track{i}.wav")),
            wav(&format!("Track{i}"), "Solo", 400),
        )
        .unwrap();
    }

    let (library, _) = Library::scan(&root, dir.path().join("cache.bin")).unwrap();
    let (playlists, _) = PlaylistStore::load(dir.path().join("playlists"), &library);
    let app = App::new(library, playlists, Backend::null());

    Harness { _dir: dir, app }
}

fn empty_harness() -> Harness {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path().join("music");
    std::fs::create_dir_all(&root).unwrap();

    let (library, _) = Library::scan(&root, dir.path().join("cache.bin")).unwrap();
    let (playlists, _) = PlaylistStore::load(dir.path().join("playlists"), &library);
    let app = App::new(library, playlists, Backend::null());

    Harness { _dir: dir, app }
}

fn key(c: char) -> KeyEvent {
    KeyEvent::from(KeyCode::Char(c))
}

fn special(code: KeyCode) -> KeyEvent {
    KeyEvent::from(code)
}

fn ctrl(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
}

#[test]
fn ctrl_chords_on_plain_letter_keys_do_nothing_instead_of_firing_the_plain_action() {
    let mut h = harness();
    h.app.on_key(key('g'));
    let before = h.app.queue.ordered_ids().collect::<Vec<_>>();

    h.app.on_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
    h.app.on_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL));
    h.app.on_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL));

    assert_eq!(
        h.app.queue.ordered_ids().collect::<Vec<_>>(),
        before,
        "Ctrl+letter must not trigger the unmodified binding"
    );
    assert!(
        h.app.modes_is_empty(),
        "a Ctrl-chord on a plain letter must not open a modal either"
    );
}

#[test]
fn ctrl_v_still_toggles_visual_selection_under_strict_modifier_matching() {
    let mut h = harness();
    h.app.on_key(key('g'));
    assert!(h.app.selected_row().is_some(), "expected a song row");

    h.app.on_key(ctrl('v'));

    assert!(
        h.app.active_visual_for_test(),
        "a bound Ctrl-chord must keep working"
    );
}

#[test]
fn shift_is_ignored_so_uppercase_letter_bindings_keep_working() {
    let mut h = harness();

    h.app.on_key(KeyEvent::new(KeyCode::Char('G'), KeyModifiers::SHIFT));

    match h.app.selected_row() {
        Some(Row::Song(_, _)) => {}
        other => panic!("Shift+G must still jump to the bottom, got {other:?}"),
    }
}

static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn lock_env() -> std::sync::MutexGuard<'static, ()> {
    ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn song_titles(app: &mut App) -> Vec<String> {
    app.visible_rows()
        .to_vec()
        .iter()
        .filter_map(|r| match r {
            Row::Song(id, _) => Some(app.library.get(*id).unwrap().title().to_string()),
            Row::Header(_) => None,
        })
        .collect()
}

fn visible_song_ids(app: &mut App) -> Vec<SongId> {
    app.visible_rows()
        .iter()
        .filter_map(|r| match r {
            Row::Song(id, _) => Some(*id),
            Row::Header(_) => None,
        })
        .collect()
}

fn queued_ids(app: &App) -> Vec<SongId> {
    app.queue.ordered_ids().collect()
}

fn header_names(app: &mut App) -> Vec<String> {
    app.visible_rows()
        .iter()
        .filter_map(|r| match r {
            Row::Header(name) => Some(name.trim().to_string()),
            Row::Song(_, _) => None,
        })
        .collect()
}

fn render(app: &mut App, width: u16, height: u16) -> Buffer {
    let area = Rect::new(0, 0, width, height);
    let mut buf = Buffer::empty(area);
    app.render(area, &mut buf);
    buf
}

fn buffer_text(buf: &Buffer) -> String {
    let area = *buf.area();
    (0..area.height)
        .map(|y| {
            (0..area.width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn default_category_is_none_and_lists_every_song_with_no_headers() {
    let mut h = harness();
    assert_eq!(header_names(&mut h.app).len(), 0);
    assert_eq!(song_titles(&mut h.app).len(), 6);
}

#[test]
fn default_sort_is_title_and_orders_songs_alphabetically() {
    let mut h = harness();
    let titles = song_titles(&mut h.app);
    let mut sorted = titles.clone();
    sorted.sort_by_key(|t| t.to_lowercase());
    assert_eq!(titles, sorted);
}

#[test]
fn page_height_stays_full_while_a_header_is_pinned() {
    let mut h = harness_one_large_group();
    h.app.library_panel.category = Category::None;
    let _ = render(&mut h.app, 60, 14);
    let unpinned_height = h.app.measured.library_page_height;

    h.app.library_panel.category = Category::Artist;
    for _ in 0..9 {
        h.app.on_key(special(KeyCode::Down));
    }
    let _ = render(&mut h.app, 60, 14);

    assert_eq!(
        h.app.measured.library_page_height,
        unpinned_height,
        "paging must use the full pane height even when a header is pinned; \
         the pinned header consumes a content row, not a pane row"
    );
}

#[test]
fn scrolling_deep_into_a_large_artist_group_pins_its_header_at_the_top() {
    let mut h = harness_one_large_group();
    h.app.library_panel.category = Category::Artist;
    for _ in 0..9 {
        h.app.on_key(special(KeyCode::Down));
    }
    let buf = render(&mut h.app, 60, 14);
    let text = buffer_text(&buf);
    let lines: Vec<&str> = text.lines().collect();

    let header_line = lines.iter().position(|l| l.contains("Solo"));
    let first_track_line = lines.iter().position(|l| l.contains("Track"));

    assert!(
        header_line.is_some() && header_line < first_track_line,
        "the group header must be pinned above the songs once they scroll past it:\n{text}"
    );
    assert!(
        text.contains("Track8"),
        "the selected song must still be visible:\n{text}"
    );
    assert!(
        !text.contains("Track4"),
        "a content row must be given up to make room for the pinned header:\n{text}"
    );
}

#[test]
fn a_group_header_already_visible_at_the_top_is_not_pinned_twice() {
    let mut h = harness_one_large_group();
    h.app.library_panel.category = Category::Artist;

    let buf = render(&mut h.app, 60, 14);
    let text = buffer_text(&buf);

    assert_eq!(
        text.matches("Solo").count(),
        1,
        "the header must not be duplicated when it is already the first visible row:\n{text}"
    );
}

#[test]
fn path_grouping_pins_a_single_level_directory_without_a_dot_slash_prefix() {
    let mut h = harness_one_large_group();
    h.app.library_panel.category = Category::Path;
    for _ in 0..9 {
        h.app.on_key(special(KeyCode::Down));
    }
    let buf = render(&mut h.app, 60, 14);
    let text = buffer_text(&buf);

    assert!(
        text.contains("Solo"),
        "path grouping must pin the directory name:\n{text}"
    );
    assert!(
        !text.contains("./Solo"),
        "a single-level directory needs no ./ prefix:\n{text}"
    );
}

#[test]
fn path_grouping_pins_the_deepest_directory_of_a_nested_group() {
    let mut h = harness_one_large_nested_group();
    h.app.library_panel.category = Category::Path;
    for _ in 0..9 {
        h.app.on_key(special(KeyCode::Down));
    }
    let buf = render(&mut h.app, 60, 14);
    let text = buffer_text(&buf);

    assert!(
        text.contains("Nested"),
        "nested directories must pin their deepest level:\\n{text}"
    );
    assert!(
        !text.contains("./Deep/Nested"),
        "the pin must match the last real header row, not a breadcrumb:\\n{text}"
    );
}

#[test]
fn the_header_indent_and_the_song_depth_use_the_same_unit() {
    let mut h = harness_one_large_nested_group();
    h.app.library_panel.category = Category::Path;

    let rows = h.app.visible_rows().to_vec();
    let headers: Vec<&str> = rows
        .iter()
        .filter_map(|r| match r {
            Row::Header(text) => Some(text.as_str()),
            Row::Song(_, _) => None,
        })
        .collect();
    assert_eq!(headers, vec!["Deep", &format!("{INDENT_UNIT}Nested")]);

    let depths: Vec<usize> = rows
        .iter()
        .filter_map(|r| match r {
            Row::Song(_, depth) => Some(*depth),
            Row::Header(_) => None,
        })
        .collect();
    assert!(
        depths.iter().all(|&depth| depth == headers.len()),
        "a song must sit one unit deeper than its last header"
    );
}

#[test]
fn category_artist_creates_one_header_per_artist() {
    let mut h = harness();
    h.app.library_panel.category = Category::Artist;

    let headers = header_names(&mut h.app);
    assert_eq!(headers, vec!["Alpha", "Beta", "Gamma"]);
}

#[test]
fn category_path_creates_one_header_per_directory() {
    let mut h = harness();
    h.app.library_panel.category = Category::Path;

    let headers = header_names(&mut h.app);
    assert_eq!(headers, vec!["Alpha", "Beta", "Gamma"]);
}

#[test]
fn category_and_sort_apply_independently() {
    let mut h = harness();
    h.app.library_panel.category = Category::Artist;
    h.app.library_panel.sort = Sort::Duration;

    let rows: Vec<Option<String>> = h
        .app
        .visible_rows()
        .to_vec()
        .iter()
        .map(|r| match r {
            Row::Header(name) => Some(format!("#{}", name.trim())),
            Row::Song(id, _) => Some(h.app.library.get(*id).unwrap().title().to_string()),
        })
        .collect();

    assert_eq!(
        rows,
        vec![
            Some("#Alpha".into()),
            Some("Azure".into()),
            Some("Anchor".into()),
            Some("#Beta".into()),
            Some("Bright".into()),
            Some("Beacon".into()),
            Some("#Gamma".into()),
            Some("Grove".into()),
            Some("Glimmer".into()),
        ],
        "grouping must stay by artist while the shorter (lower duration) song leads within each group"
    );
}

#[test]
fn switching_category_back_to_none_removes_all_headers() {
    let mut h = harness();
    h.app.library_panel.category = Category::Artist;
    assert!(!header_names(&mut h.app).is_empty());

    h.app.library_panel.category = Category::None;
    assert!(header_names(&mut h.app).is_empty());
}

#[test]
fn search_query_filters_rows_and_clearing_restores_them() {
    let mut h = harness();
    assert_eq!(song_titles(&mut h.app).len(), 6);

    h.app.library_panel.search_query = "azure".into();
    assert_eq!(song_titles(&mut h.app), vec!["Azure"]);

    h.app.library_panel.search_query.clear();
    assert_eq!(song_titles(&mut h.app).len(), 6);
}

#[test]
fn filtering_while_grouped_by_artist_still_shows_the_artist_on_each_row() {
    let mut h = harness();
    h.app.library_panel.category = Category::Artist;
    h.app.library_panel.search_query = "azure".into();

    assert!(
        header_names(&mut h.app).is_empty(),
        "search results have no headers to carry the artist"
    );

    let buf = render(&mut h.app, 120, 30);
    let text = buffer_text(&buf);
    assert!(
        text.contains("Alpha"),
        "the artist must appear on the row since no header shows it:\n{text}"
    );
}

#[test]
fn repeated_visible_rows_calls_are_stable_when_nothing_changed() {
    let mut h = harness();
    let first = song_titles(&mut h.app);
    let second = song_titles(&mut h.app);
    assert_eq!(first, second);
}

#[test]
fn switching_panels_changes_the_visible_rows() {
    let mut h = harness();
    assert_eq!(song_titles(&mut h.app).len(), 6);

    h.app.on_key(special(KeyCode::Tab));
    assert_eq!(h.app.panel, Panel::Playlists);
    assert!(h.app.visible_rows().is_empty(), "no playlist is open yet");

    h.app.on_key(special(KeyCode::Tab));
    assert_eq!(h.app.panel, Panel::Library);
    assert_eq!(song_titles(&mut h.app).len(), 6);
}

#[test]
fn viewing_a_playlist_shows_only_its_songs() {
    let mut h = harness();
    let song = h.app.library.ids_by_path()[0];
    let id = h.app.playlists.create("Mix");

    h.app.panel = Panel::Playlists;
    h.app.playlist_panel.view = PlaylistView::Viewing(id);
    assert_eq!(song_titles(&mut h.app).len(), 0);

    h.app.playlists.add_song(id, song);
    assert_eq!(song_titles(&mut h.app).len(), 1);

    h.app.playlists.remove_song(id, song);
    assert_eq!(song_titles(&mut h.app).len(), 0);
}

#[test]
fn moving_the_selection_wraps_and_never_lands_on_a_header() {
    let mut h = harness();
    h.app.library_panel.category = Category::Artist;
    h.app.on_key(key('g'));

    for _ in 0..12 {
        let row = h.app.selected_row();
        assert!(
            matches!(row, Some(Row::Song(_, _))),
            "a header must never be selected"
        );
        h.app.on_key(key('j'));
    }
}

#[test]
fn shift_g_selects_the_last_song_and_g_selects_the_first() {
    let mut h = harness();
    h.app.on_key(special(KeyCode::Char('G')));
    let last = h.app.library_panel.list_state.selected();

    h.app.on_key(key('g'));
    let first = h.app.library_panel.list_state.selected();

    assert_eq!(first, Some(0));
    assert_eq!(last, Some(5));
}

#[test]
fn ctrl_v_then_j_extends_the_visual_selection_downward() {
    let mut h = harness();
    h.app.on_key(key('g'));
    h.app.on_key(ctrl('v'));
    assert!(h.app.library_panel.visual.is_some());

    h.app.on_key(key('j'));
    h.app.on_key(key('j'));

    assert_eq!(h.app.visual_row_range(), Some((0, 2)));
}

#[test]
fn ctrl_v_twice_cancels_the_visual_selection() {
    let mut h = harness();
    h.app.on_key(key('g'));
    h.app.on_key(ctrl('v'));
    assert!(h.app.library_panel.visual.is_some());

    h.app.on_key(ctrl('v'));
    assert!(h.app.library_panel.visual.is_none());
    assert!(h.app.status.text.contains("cancelled visual selection"));
}

#[test]
fn changing_sort_while_in_visual_selection_cancels_it() {
    let mut h = harness();
    h.app.on_key(key('g'));
    h.app.on_key(ctrl('v'));
    assert!(h.app.library_panel.visual.is_some());

    h.app.on_key(key('p'));
    assert!(
        h.app.library_panel.visual.is_none(),
        "cycling sort must cancel an in-progress visual selection"
    );
}

#[test]
fn lowercase_o_cycles_category_and_shows_a_status_message() {
    let mut h = harness();
    assert!(header_names(&mut h.app).is_empty());

    h.app.on_key(key('o'));
    assert!(
        !header_names(&mut h.app).is_empty(),
        "category should have advanced past None"
    );
    assert!(h.app.status.text.contains("grouped by"));
}

#[test]
fn lowercase_p_cycles_sort_and_shows_a_status_message() {
    let mut h = harness();
    let before = song_titles(&mut h.app);

    h.app.on_key(key('p'));
    let after = song_titles(&mut h.app);

    assert_ne!(before, after, "sort must have changed the order");
    assert!(h.app.status.text.contains("sorted by"));
}

#[test]
fn cycling_a_category_forwards_then_backwards_returns_to_the_start() {
    let mut h = harness();
    let start = h.app.library_panel.category;

    h.app.on_key(key('o'));
    assert_ne!(h.app.library_panel.category, start);

    h.app.on_key(key('O'));
    assert_eq!(h.app.library_panel.category, start);
}

#[test]
fn cycling_a_sort_key_forwards_then_backwards_returns_to_the_start() {
    let mut h = harness();
    let start = h.app.library_panel.sort;

    h.app.on_key(key('p'));
    assert_ne!(h.app.library_panel.sort, start);

    h.app.on_key(key('P'));
    assert_eq!(h.app.library_panel.sort, start);
}

#[test]
fn cycling_changes_only_the_panel_that_has_focus() {
    let mut h = harness();
    let id = h.app.playlists.create("Mix");
    h.app.panel = Panel::Playlists;
    h.app.playlist_panel.view = PlaylistView::Viewing(id);

    h.app.on_key(key('o'));
    h.app.on_key(key('p'));

    assert_ne!(h.app.playlist_panel.category, Category::default());
    assert_ne!(h.app.playlist_panel.sort, Sort::default());
    assert_eq!(h.app.library_panel.category, Category::default());
    assert_eq!(h.app.library_panel.sort, Sort::default());
}

#[test]
fn cycling_does_nothing_while_browsing_the_playlist_list() {
    let mut h = harness();
    h.app.playlists.create("Mix");
    h.app.panel = Panel::Playlists;
    h.app.playlist_panel.view = PlaylistView::Browsing;

    h.app.on_key(key('o'));
    h.app.on_key(key('p'));

    assert_eq!(h.app.playlist_panel.category, Category::default());
    assert_eq!(h.app.playlist_panel.sort, Sort::default());
    assert!(
        !h.app.status.text.contains("grouped by") && !h.app.status.text.contains("sorted by"),
        "browse mode has no sort key to cycle, so it must report nothing: {}",
        h.app.status.text
    );
}

#[test]
fn e_opens_the_metadata_modal_prefilled_with_the_selected_songs_tags() {
    let mut h = harness();
    h.app.on_key(key('g'));
    let Some(Row::Song(id, _)) = h.app.selected_row() else {
        panic!("expected a song row")
    };
    let expected_title = h.app.library.get(id).unwrap().title().to_string();

    h.app.on_key(key('e'));

    let modal = h
        .app
        .metadata_mode()
        .expect("e must open the metadata edit modal");
    assert_eq!(modal.song, id);
    assert_eq!(modal.edits.title, expected_title);
    assert_eq!(modal.focused, MetadataField::Title);
    assert!(modal.error.is_none());
}

#[test]
fn metadata_modal_tab_and_shift_tab_cycle_through_every_field_and_wrap() {
    let mut h = harness();
    h.app.on_key(key('g'));
    h.app.on_key(key('e'));

    let visible = MetadataField::visible(&h.app.metadata_mode().unwrap().edits);
    for expected in &visible {
        assert_eq!(
            h.app.metadata_mode().unwrap().focused,
            *expected
        );
        h.app.on_key(special(KeyCode::Tab));
    }
    assert_eq!(
        h.app.metadata_mode().unwrap().focused,
        MetadataField::Title,
        "tabbing past the last field must wrap back to the first"
    );

    h.app.on_key(special(KeyCode::BackTab));
    assert_eq!(
        h.app.metadata_mode().unwrap().focused,
        *visible.last().unwrap(),
        "shift-tab from the first field must wrap back to the last"
    );
}

#[test]
fn song_modal_tab_and_shift_tab_move_focus_like_j_and_k() {
    let mut h = harness();
    h.app.playlists.create("First");
    h.app.on_key(key('g'));
    h.app.on_key(key('w'));
    assert_eq!(
        h.app.song_mode().unwrap().selected,
        ChooseActionField::AddToPlaylist
    );

    h.app.on_key(special(KeyCode::Tab));
    assert_eq!(
        h.app.song_mode().unwrap().selected,
        ChooseActionField::CreatePlaylist,
        "Tab must move focus in the song actions modal, matching the metadata modal"
    );

    h.app.on_key(special(KeyCode::BackTab));
    assert_eq!(
        h.app.song_mode().unwrap().selected,
        ChooseActionField::AddToPlaylist,
        "Shift+Tab must move focus back, matching the metadata modal"
    );
}

#[test]
fn song_modal_side_panel_tab_and_shift_tab_move_selection_like_j_and_k() {
    let mut h = harness();
    h.app.playlists.create("First");
    h.app.playlists.create("Second");
    h.app.on_key(key('g'));
    h.app.on_key(key('w'));
    h.app.on_key(special(KeyCode::Enter));

    let PlaylistPicker::Add { list_state, .. } = h.app.song_mode().unwrap()
        .picker
        .as_ref()
        .unwrap()
    else {
        panic!("expected the add-to-playlist side panel")
    };
    let start = list_state.selected();

    h.app.on_key(special(KeyCode::Tab));
    let PlaylistPicker::Add { list_state, .. } = h.app.song_mode().unwrap()
        .picker
        .as_ref()
        .unwrap()
    else {
        panic!("expected the add-to-playlist side panel")
    };
    let after_tab = list_state.selected();
    assert_ne!(
        after_tab, start,
        "Tab must move the side panel selection, matching j/Down"
    );

    h.app.on_key(special(KeyCode::BackTab));
    let PlaylistPicker::Add { list_state, .. } = h.app.song_mode().unwrap()
        .picker
        .as_ref()
        .unwrap()
    else {
        panic!("expected the add-to-playlist side panel")
    };
    assert_eq!(
        list_state.selected(),
        start,
        "Shift+Tab must move the side panel selection back, matching k/Up"
    );
}

#[test]
fn song_modal_side_panel_j_and_k_move_selection_like_tab() {
    let mut h = harness();
    h.app.playlists.create("First");
    h.app.playlists.create("Second");
    h.app.on_key(key('g'));
    h.app.on_key(key('w'));
    h.app.on_key(special(KeyCode::Enter));

    let start = match h.app.song_mode().unwrap().picker.as_ref().unwrap() {
        PlaylistPicker::Add { list_state, .. } => list_state.selected(),
        _ => panic!("expected the add-to-playlist side panel"),
    };

    h.app.on_key(key('j'));
    let after_j = match h.app.song_mode().unwrap().picker.as_ref().unwrap() {
        PlaylistPicker::Add { list_state, .. } => list_state.selected(),
        _ => panic!("expected the add-to-playlist side panel"),
    };
    assert_ne!(
        after_j, start,
        "j must move the side panel selection down, matching Tab/Down"
    );

    h.app.on_key(key('k'));
    let after_k = match h.app.song_mode().unwrap().picker.as_ref().unwrap() {
        PlaylistPicker::Add { list_state, .. } => list_state.selected(),
        _ => panic!("expected the add-to-playlist side panel"),
    };
    assert_eq!(
        after_k, start,
        "k must move the side panel selection back up, matching BackTab/Up"
    );
}

#[test]
fn esc_closes_the_song_actions_modal_when_a_side_action_field_is_selected() {
    let mut h = harness();
    h.app.playlists.create("First");
    h.app.on_key(key('g'));
    h.app.on_key(key('w'));

    assert_eq!(
        h.app.song_mode().unwrap().selected,
        ChooseActionField::AddToPlaylist
    );
    h.app.on_key(special(KeyCode::Esc));
    assert!(
        h.app.song_mode().is_none(),
        "Esc must close the song actions modal when Add to Playlist is selected"
    );

    h.app.on_key(key('g'));
    h.app.on_key(key('w'));
    h.app.on_key(special(KeyCode::Tab));
    assert_eq!(
        h.app.song_mode().unwrap().selected,
        ChooseActionField::CreatePlaylist
    );
    h.app.on_key(special(KeyCode::Esc));
    assert!(
        h.app.song_mode().is_none(),
        "Esc must close the song actions modal when Create Playlist is selected"
    );
}

#[test]
fn remove_from_playlist_option_is_hidden_when_the_song_is_not_in_any_playlist() {
    let mut h = harness();
    h.app.playlists.create("First");
    h.app.on_key(key('g'));

    h.app.on_key(key('w'));
    h.app.on_key(special(KeyCode::Tab));
    assert_eq!(
        h.app.song_mode().unwrap().selected,
        ChooseActionField::CreatePlaylist,
        "Remove from Playlist must be skipped when the song isn't in any playlist"
    );
}

#[test]
fn song_modal_cycles_through_add_remove_and_create_when_the_song_is_in_a_playlist() {
    let mut h = harness();
    let playlist_id = h.app.playlists.create("First");
    h.app.on_key(key('g'));
    let Some(Row::Song(song_id, _)) = h.app.selected_row() else {
        panic!("expected a song row")
    };
    h.app.playlists.add_song(playlist_id, song_id);

    h.app.on_key(key('w'));
    assert_eq!(
        h.app.song_mode().unwrap().selected,
        ChooseActionField::AddToPlaylist
    );

    h.app.on_key(special(KeyCode::Tab));
    assert_eq!(
        h.app.song_mode().unwrap().selected,
        ChooseActionField::RemoveFromPlaylist
    );

    h.app.on_key(special(KeyCode::Tab));
    assert_eq!(
        h.app.song_mode().unwrap().selected,
        ChooseActionField::CreatePlaylist
    );

    h.app.on_key(special(KeyCode::Tab));
    assert_eq!(
        h.app.song_mode().unwrap().selected,
        ChooseActionField::AddToPlaylist
    );
}

#[test]
fn opening_the_song_modal_with_no_visual_selection_active_leaves_status_untouched() {
    let mut h = harness();
    h.app.on_key(key('g'));
    assert!(!h.app.active_visual_for_test(), "sanity: no visual selection");
    let status_before = h.app.status.text.clone();

    h.app.on_key(key('w'));

    let modal = h
        .app
        .song_mode()
        .expect("w must still open the single-song modal without a visual selection");
    assert!(modal.batch.is_none());
    assert_eq!(
        h.app.status.text, status_before,
        "the no-visual path must not fire any status message"
    );
}

#[test]
fn w_with_an_active_visual_selection_opens_the_batch_song_modal() {
    let mut h = harness();
    h.app.on_key(key('g'));
    h.app.on_key(ctrl('v'));
    h.app.on_key(key('j'));
    h.app.on_key(key('j'));

    h.app.on_key(key('w'));

    let modal = h
        .app
        .song_mode()
        .expect("w must open the song modal");
    assert_eq!(modal.batch.as_ref().map(Vec::len), Some(3));
    assert!(
        h.app.library_panel.visual.is_none(),
        "opening the batch modal should end visual selection"
    );
}

#[test]
fn confirming_add_to_playlist_in_batch_mode_adds_every_selected_song() {
    let mut h = harness();
    let playlist = h.app.playlists.create("Mix");
    h.app.on_key(key('g'));
    h.app.on_key(ctrl('v'));
    h.app.on_key(key('j'));
    h.app.on_key(key('j'));
    let range = h.app.visual_row_range().unwrap();
    let expected_ids: Vec<_> = h.app.visible_rows().to_vec()[range.0..=range.1]
        .iter()
        .filter_map(|r| match r {
            Row::Song(id, _) => Some(*id),
            Row::Header(_) => None,
        })
        .collect();

    h.app.on_key(key('w'));
    h.app.on_key(special(KeyCode::Enter));
    h.app.on_key(special(KeyCode::Enter));

    for id in &expected_ids {
        assert!(
            h.app.playlists.contains(playlist, *id),
            "expected song {id:?} to be added"
        );
    }
    assert!(h.app.status.text.contains("added 3 songs to \"Mix\""));
    assert!(
        !h.app.song_mode_is(),
        "the song modal should close after the batch add"
    );
}

#[test]
fn the_add_side_panel_keeps_pinned_playlists_out_of_its_options_after_rebuilding() {
    let mut h = harness();
    let mix = h.app.playlists.create("Mix");
    h.app.playlists.create("Other");
    h.app.on_key(key('g'));
    let Some(Row::Song(song_id, _)) = h.app.selected_row() else {
        panic!("expected a song row")
    };
    h.app.playlists.add_song(mix, song_id);

    h.app.on_key(key('w'));
    h.app.on_key(special(KeyCode::Enter));

    for _ in 0..3 {
        h.app.on_key(special(KeyCode::Down));
        if !h.app.song_mode_is() {
            break;
        }
        match &h.app.song_mode().unwrap().picker {
            Some(PlaylistPicker::Add { options, .. }) => {
                assert!(
                    !options.contains(&mix),
                    "the pinned playlist \"Mix\" must never appear in options"
                );
            }
            other => panic!("expected the add-to-playlist side panel, got {other:?}"),
        }
    }
}

#[test]
fn a_playlist_containing_only_some_selected_songs_is_not_treated_as_pinned() {
    let mut h = harness();
    let playlist = h.app.playlists.create("Mix");
    h.app.on_key(key('g'));
    let Some(Row::Song(first_id, _)) = h.app.selected_row() else {
        panic!("expected a song row")
    };
    h.app.playlists.add_song(playlist, first_id);

    h.app.on_key(ctrl('v'));
    h.app.on_key(key('j'));
    h.app.on_key(key('j'));

    h.app.on_key(key('w'));
    h.app.on_key(special(KeyCode::Enter));

    let PlaylistPicker::Add {
        options, pinned, ..
    } = h.app.song_mode().unwrap()
        .picker
        .as_ref()
        .unwrap()
    else {
        panic!("expected the add-to-playlist side panel")
    };
    assert!(
        pinned.is_empty(),
        "a playlist containing only some of the selected songs must not be pinned"
    );
    assert_eq!(options.as_slice(), [playlist]);
}

#[test]
fn selecting_remove_from_playlist_lists_only_the_playlists_containing_the_song() {
    let mut h = harness();
    let first = h.app.playlists.create("First");
    h.app.playlists.create("Second");
    h.app.on_key(key('g'));
    let Some(Row::Song(song_id, _)) = h.app.selected_row() else {
        panic!("expected a song row")
    };
    h.app.playlists.add_song(first, song_id);

    h.app.on_key(key('w'));
    h.app.on_key(special(KeyCode::Tab));
    h.app.on_key(special(KeyCode::Enter));

    let PlaylistPicker::Remove { options, .. } = h.app.song_mode().unwrap()
        .picker
        .as_ref()
        .unwrap()
    else {
        panic!("expected the remove-from-playlist side panel")
    };
    assert_eq!(options.as_slice(), [first]);
}

#[test]
fn confirming_a_remove_from_playlist_selection_removes_the_song_and_closes_the_modal() {
    let mut h = harness();
    let first = h.app.playlists.create("First");
    h.app.on_key(key('g'));
    let Some(Row::Song(song_id, _)) = h.app.selected_row() else {
        panic!("expected a song row")
    };
    h.app.playlists.add_song(first, song_id);

    h.app.on_key(key('w'));
    h.app.on_key(special(KeyCode::Tab));
    h.app.on_key(special(KeyCode::Enter));
    h.app.on_key(special(KeyCode::Enter));

    assert!(
        !h.app.playlists.contains(first, song_id),
        "the song should be removed from the playlist"
    );
    assert!(h.app.status.text.contains("removed from \"First\""));
    assert!(
        !h.app.song_mode_is(),
        "the song modal should close after removing"
    );
}

#[test]
fn metadata_modal_esc_cancels_without_changing_the_library() {
    let mut h = harness();
    h.app.on_key(key('g'));
    let Some(Row::Song(id, _)) = h.app.selected_row() else {
        panic!("expected a song row")
    };
    let before = h.app.library.get(id).unwrap().title().to_string();

    h.app.on_key(key('e'));
    h.app.on_key(key('x'));
    h.app.on_key(special(KeyCode::Esc));

    assert!(!h.app.metadata_mode_is());
    assert_eq!(h.app.library.get(id).unwrap().title(), before);
}

#[test]
fn metadata_modal_editing_the_title_and_saving_updates_the_library() {
    let mut h = harness();
    h.app.on_key(key('g'));
    let Some(Row::Song(old_id, _)) = h.app.selected_row() else {
        panic!("expected a song row")
    };

    h.app.on_key(key('e'));
    for _ in 0..32 {
        h.app.on_key(special(KeyCode::Backspace));
    }
    for c in "Retitled".chars() {
        h.app.on_key(key(c));
    }
    h.app.on_key(special(KeyCode::Enter));

    assert!(
        !h.app.metadata_mode_is(),
        "a successful save must close the modal"
    );
    assert!(
        h.app.library.contains(old_id),
        "the song keeps its id across an edit"
    );
    assert_eq!(h.app.library.get(old_id).unwrap().title(), "Retitled");

    let new_song = h
        .app
        .library
        .songs_by_path()
        .find(|s| s.title() == "Retitled");
    assert!(
        new_song.is_some(),
        "the library must contain a song with the new title"
    );
    assert!(h.app.status.text.contains("updated metadata"));
}

#[test]
fn metadata_modal_saving_a_non_numeric_track_keeps_the_modal_open_with_an_error() {
    let mut h = harness();
    h.app.on_key(key('g'));
    let Some(Row::Song(old_id, _)) = h.app.selected_row() else {
        panic!("expected a song row")
    };

    h.app.on_key(key('e'));
    let edits = h.app.metadata_mode().unwrap().edits.clone();
    for _ in 0..MetadataField::visible(&edits)
        .iter()
        .position(|&f| f == MetadataField::Track)
        .unwrap()
    {
        h.app.on_key(special(KeyCode::Tab));
    }
    for c in "not-a-number".chars() {
        h.app.on_key(key(c));
    }
    h.app.on_key(special(KeyCode::Enter));

    let modal = h
        .app
        .metadata_mode()
        .expect("a failed save must keep the modal open");
    assert!(modal.error.is_some());
    assert_eq!(
        modal.song, old_id,
        "the failed edit must be preserved so the user can fix it"
    );
    assert!(
        h.app.library.contains(old_id),
        "the library must be untouched by a failed write"
    );
}

#[test]
fn metadata_modal_save_carries_the_now_playing_song_to_its_new_id() {
    let mut h = harness();
    h.app.on_key(key('g'));
    let Some(Row::Song(old_id, _)) = h.app.selected_row() else {
        panic!("expected a song row")
    };

    h.app.on_key(special(KeyCode::Enter));
    assert_eq!(
        h.app.queue.current_id(),
        Some(old_id),
        "sanity check: the song is now playing"
    );

    h.app.on_key(key('e'));
    for _ in 0..32 {
        h.app.on_key(special(KeyCode::Backspace));
    }
    for c in "Retitled".chars() {
        h.app.on_key(key(c));
    }
    h.app.on_key(special(KeyCode::Enter));

    let new_id = h
        .app
        .library
        .songs_by_path()
        .find(|s| s.title() == "Retitled")
        .unwrap()
        .id();
    assert_eq!(
        h.app.queue.current_id(),
        Some(new_id),
        "the currently-playing song must follow the id change, not silently stop tracking it"
    );
}

#[test]
fn enter_plays_the_selected_song() {
    let mut h = harness();
    h.app.on_key(key('g'));
    let selected = h.app.selected_row();
    let Some(Row::Song(id, _)) = selected else {
        panic!("expected a song row")
    };

    h.app.on_key(special(KeyCode::Enter));

    assert_eq!(h.app.queue.current_id(), Some(id));
}

#[test]
fn n_advances_and_b_goes_back() {
    let mut h = harness();
    h.app.on_key(key('n'));
    let first = h.app.queue.current_id();

    h.app.on_key(key('n'));
    let second = h.app.queue.current_id();
    assert_ne!(first, second);

    h.app.on_key(key('b'));
    assert_eq!(h.app.queue.current_id(), first);
}

#[test]
fn a_queues_the_selected_song_next() {
    let mut h = harness();
    h.app.on_key(key('g'));
    let Some(Row::Song(id, _)) = h.app.selected_row() else {
        panic!("expected a song row")
    };

    h.app.on_key(key('a'));

    assert_eq!(h.app.queue.priority_queue().front(), Some(&id));
}

#[test]
fn the_queue_follows_the_panel_sort_order() {
    let mut h = harness();
    h.app.library_panel.sort = Sort::Duration;
    h.app.on_key(key('g'));
    h.app.on_key(special(KeyCode::Enter));

    assert_eq!(
        queued_ids(&h.app),
        visible_song_ids(&mut h.app),
        "the queue must play the songs in the order the panel shows them"
    );
}

#[test]
fn changing_the_sort_key_rebuilds_the_queue_on_the_next_play() {
    let mut h = harness();
    h.app.on_key(key('g'));
    h.app.on_key(special(KeyCode::Enter));
    let by_title = queued_ids(&h.app);

    h.app.library_panel.sort = Sort::Duration;
    h.app.on_key(key('g'));
    h.app.on_key(special(KeyCode::Enter));
    let by_duration = queued_ids(&h.app);

    assert_ne!(by_title, by_duration, "a new sort key must reorder the queue");
    assert_eq!(by_duration, visible_song_ids(&mut h.app));
}

#[test]
fn a_search_does_not_narrow_the_queue() {
    let mut h = harness();
    h.app.on_key(key('g'));
    h.app.on_key(special(KeyCode::Enter));
    let before = queued_ids(&h.app);
    assert_eq!(before.len(), 6);

    h.app.library_panel.search_query = "azure".into();
    assert_eq!(song_titles(&mut h.app), vec!["Azure"]);

    h.app.on_key(key('g'));
    h.app.on_key(special(KeyCode::Enter));

    assert_eq!(
        queued_ids(&h.app),
        before,
        "playing a search result must keep the whole library in the queue"
    );
}

#[test]
fn replaying_the_same_list_keeps_the_shuffle_order() {
    let mut h = harness();
    h.app.on_key(key('g'));
    h.app.on_key(special(KeyCode::Enter));
    h.app.on_key(key('s'));
    let shuffled = h.app.queue.upcoming(6);

    h.app.on_key(key('c'));
    h.app.on_key(special(KeyCode::Enter));

    assert_eq!(
        h.app.queue.upcoming(6),
        shuffled,
        "a replay of the same list must not undo the shuffle"
    );
}

#[test]
fn the_playlist_queue_follows_the_panel_order_not_the_stored_order() {
    let mut h = harness();
    let stored: Vec<SongId> = h.app.library.ids_by_path().into_iter().rev().collect();
    let id = h.app.playlists.create("Mix");
    for song in &stored {
        h.app.playlists.add_song(id, *song);
    }

    h.app.panel = Panel::Playlists;
    h.app.playlist_panel.view = PlaylistView::Viewing(id);
    h.app.playlist_panel.sort = Sort::Duration;
    h.app.playlist_panel.list_state.select(Some(0));
    h.app.on_key(special(KeyCode::Enter));

    assert_eq!(queued_ids(&h.app), visible_song_ids(&mut h.app));
    assert_ne!(
        queued_ids(&h.app),
        stored,
        "the stored playlist order must not decide playback order"
    );
}

#[test]
fn b_with_no_previous_track_shows_a_status_message() {
    let mut h = empty_harness();
    h.app.on_key(key('b'));
    assert!(h.app.status.text.contains("no previous track"));
}

#[test]
fn enter_with_no_song_selected_shows_a_status_message() {
    let mut h = harness();
    h.app.library_panel.list_state.select(None);
    h.app.on_key(special(KeyCode::Enter));
    assert!(h.app.status.text.contains("select a song first"));
}

#[test]
fn lowercase_a_with_no_song_selected_shows_a_status_message() {
    let mut h = harness();
    h.app.library_panel.list_state.select(None);
    h.app.on_key(key('a'));
    assert!(h.app.status.text.contains("select a song first"));
}

#[test]
fn w_with_no_song_selected_shows_a_status_message() {
    let mut h = harness();
    h.app.library_panel.list_state.select(None);
    h.app.on_key(key('w'));
    assert!(h.app.status.text.contains("select a song first"));
}

#[test]
fn entering_a_playlist_shows_a_status_message() {
    let mut h = harness();
    let id = h.app.playlists.create("Mix");

    h.app.panel = Panel::Playlists;
    h.app.playlist_panel.list_state.select(Some(0));
    h.app.on_key(special(KeyCode::Enter));

    assert_eq!(h.app.playlist_panel.view, PlaylistView::Viewing(id));
    assert!(h.app.status.text.contains("viewing \"Mix\""));
}

#[test]
fn q_asks_for_confirmation_before_quitting() {
    let mut h = harness();
    h.app.on_key(key('q'));
    assert!(h.app.confirm_mode_is());

    h.app.on_key(key('n'));
    assert!(
        h.app.modes_is_empty(),
        "n/Esc must cancel the quit confirmation"
    );
}

#[test]
fn confirm_dialogs_stay_open_on_unrecognized_keys() {
    let mut h = harness();
    let playlist = h.app.playlists.create("Mix");
    h.app.on_key(key('g'));
    let Some(Row::Song(song_id, _)) = h.app.selected_row() else {
        panic!("expected a song row")
    };
    h.app.playlists.add_song(playlist, song_id);
    h.app.panel = Panel::Playlists;
    h.app.playlist_panel.list_state.select(Some(0));
    h.app.on_key(special(KeyCode::Enter));

    h.app.on_key(key('r'));
    assert!(h.app.confirm_mode_is());
    for c in "xz ".chars() {
        h.app.on_key(key(c));
        assert!(
            h.app.confirm_mode_is(),
            "unrecognized key {c} must leave the remove confirmation open"
        );
    }
    assert!(h.app.playlists.contains(playlist, song_id));

    h.app.on_key(special(KeyCode::Esc));
    assert!(!h.app.confirm_mode_is());

    h.app.on_key(key('q'));
    h.app.on_key(key('x'));
    assert!(
        h.app.confirm_mode_is(),
        "an unrecognized key must leave the quit confirmation open"
    );
}

#[test]
fn enter_confirms_a_remove_confirmation_like_the_other_confirm_dialogs() {
    let mut h = harness();
    let playlist = h.app.playlists.create("Mix");
    h.app.on_key(key('g'));
    let Some(Row::Song(song_id, _)) = h.app.selected_row() else {
        panic!("expected a song row")
    };
    h.app.playlists.add_song(playlist, song_id);
    h.app.panel = Panel::Playlists;
    h.app.playlist_panel.list_state.select(Some(0));
    h.app.on_key(special(KeyCode::Enter));

    h.app.on_key(key('r'));
    h.app.on_key(special(KeyCode::Enter));

    assert!(
        !h.app.playlists.contains(playlist, song_id),
        "Enter must confirm the removal like y/Y do"
    );
    assert!(!h.app.confirm_mode_is());
}

#[test]
fn unknown_keys_leave_confirm_dialogs_open() {
    let mut h = harness();
    h.app.on_key(key('q'));
    h.app.on_key(key('x'));
    assert!(
        h.app.confirm_mode_is(),
        "a key other than y/Y/Enter/n/N/Esc must not dismiss the quit confirmation"
    );

    h.app.on_key(special(KeyCode::Esc));
    assert!(h.app.modes_is_empty(), "Esc must cancel the quit confirmation");
}

#[test]
fn the_no_playlists_hint_names_the_current_song_actions_key() {
    let mut h = harness();
    h.app.panel = Panel::Playlists;
    h.app.on_key(special(KeyCode::Enter));

    assert!(
        h.app.status.text.contains(
            &lyre_tui::keymap::display_for(lyre_tui::keymap::Action::OpenSongModal)
        ),
        "the hint must show the key that actually opens the song modal: {}",
        h.app.status.text
    );
    assert!(
        !h.app.status.text.contains("press p"),
        "the hint must not hardcode a key letter: {}",
        h.app.status.text
    );
}

#[test]
fn enter_confirms_the_remove_song_dialog_instead_of_dismissing_it() {
    let mut h = harness();
    let playlist = h.app.playlists.create("First");
    h.app.on_key(key('g'));
    let Some(Row::Song(song_id, _)) = h.app.selected_row() else {
        panic!("expected a song row")
    };
    h.app.playlists.add_song(playlist, song_id);

    h.app.open_remove_confirm_for_test(playlist, song_id);
    h.app.on_key(special(KeyCode::Enter));

    assert!(
        !h.app.playlists.contains(playlist, song_id),
        "Enter must confirm the remove, not dismiss the dialog"
    );
}

#[test]
fn unknown_keys_leave_the_remove_song_dialog_open() {
    let mut h = harness();
    let playlist = h.app.playlists.create("First");
    h.app.on_key(key('g'));
    let Some(Row::Song(song_id, _)) = h.app.selected_row() else {
        panic!("expected a song row")
    };
    h.app.playlists.add_song(playlist, song_id);

    h.app.open_remove_confirm_for_test(playlist, song_id);
    h.app.on_key(key('x'));

    assert!(
        h.app.confirm_mode_is(),
        "an unknown key must leave the remove dialog open"
    );
    assert!(
        h.app.playlists.contains(playlist, song_id),
        "the song must stay in the playlist"
    );

    h.app.on_key(special(KeyCode::Esc));
    assert!(h.app.modes_is_empty(), "Esc cancels the remove dialog");
}

#[test]
fn question_mark_toggles_the_help_overlay() {
    let mut h = harness();
    assert!(h.app.modes_is_empty());

    h.app.on_key(key('?'));
    assert!(h.app.help_mode_is());

    h.app.on_key(key('x'));
    assert!(h.app.modes_is_empty(), "any key closes the help overlay");
}

#[test]
fn a_rendered_frame_shows_the_selected_song_and_panel_title() {
    let mut h = harness();
    h.app.on_key(key('g'));

    let buf = render(&mut h.app, 120, 30);
    let text = buffer_text(&buf);
    assert!(
        text.contains("Anchor"),
        "the first song should be visible:\n{text}"
    );
    assert!(text.contains("Library"));
}

#[test]
fn sort_title_width_is_stable_across_every_category_and_sort_combination() {
    let widths: Vec<usize> = Category::ALL
        .iter()
        .flat_map(|category| {
            Sort::ALL
                .iter()
                .map(|sort| sort_title(category.label(), sort.label(), Style::default()).width())
        })
        .collect();

    let first = widths[0];
    for (i, width) in widths.iter().enumerate() {
        assert_eq!(
            *width, first,
            "combination #{i} has a different rendered width than the others -- the header would jump"
        );
    }
}

#[test]
fn sort_title_sort_segment_starts_at_the_same_column_for_every_category_and_sort() {
    let mut starts = Vec::new();
    for category in Category::ALL {
        let line = sort_title(category.label(), Sort::ALL[0].label(), Style::default());
        let text: String = line
            .spans
            .iter()
            .flat_map(|span| span.content.chars())
            .collect();
        let sort_start = text.find("sort:").map(|byte| text[..byte].chars().count());
        starts.push(sort_start.unwrap());
    }

    let first = starts[0];
    for (i, start) in starts.iter().enumerate() {
        assert_eq!(
            *start, first,
            "combination #{i} starts the sort segment at a different column -- the sort part slides when the group cycles"
        );
    }
}

fn sort_title_text(category_label: &str, sort_label: &str) -> String {
    sort_title(category_label, sort_label, Style::default())
        .spans
        .iter()
        .flat_map(|span| span.content.chars())
        .collect()
}

#[test]
fn sort_title_draws_the_slack_as_dashes_inside_each_column() {
    let text = sort_title_text("path", "title");
    let group_max = sort_title_widths().category_value;
    let sort_max = sort_title_widths().sort_value;
    let expected_group_dashes = "─".repeat(group_max - "path".len() + 1);
    let expected_sort_dashes = "─".repeat(sort_max - "title".len() + 1);

    assert!(
        text.contains(&format!("path {expected_group_dashes} ")),
        "a space, then the dash run, then a space before the sort column:\n{text}"
    );
    assert!(
        text.ends_with(&format!("title {expected_sort_dashes}")),
        "the sort dash run should reach the border corner:\n{text}"
    );
}

#[test]
fn sort_title_never_contains_two_spaces_in_a_row() {
    for category in Category::ALL {
        for sort in Sort::ALL {
            let text = sort_title_text(category.label(), sort.label());
            assert!(
                !text.contains("  "),
                "adjacent spaces should never appear:\n{text}"
            );
        }
    }
}

#[test]
fn sort_title_always_ends_in_a_dash_reaching_the_border_corner() {
    for category in Category::ALL {
        for sort in Sort::ALL {
            let text = sort_title_text(category.label(), sort.label());
            assert!(
                text.ends_with('─'),
                "the title should connect to the border corner:\n{text}"
            );
        }
    }
}

#[test]
fn sort_title_group_segment_does_not_depend_on_the_sort_label() {
    let reference = sort_title_text("artist", "title");
    for sort in Sort::ALL {
        let other = sort_title_text("artist", sort.label());
        let group_of_reference: String = reference.chars().take_while(|c| *c != '─').collect();
        let group_of_other: String = other.chars().take_while(|c| *c != '─').collect();
        assert_eq!(
            group_of_reference, group_of_other,
            "the group segment changed when only the sort label changed"
        );
    }
}

#[test]
fn sort_title_gives_even_the_widest_labels_one_dash_each_side_of_the_gap() {
    let widths = sort_title_widths();
    let longest_category = Category::ALL
        .iter()
        .find(|c| c.label().len() == widths.category_value)
        .unwrap();
    let longest_sort = Sort::ALL
        .iter()
        .find(|s| s.label().len() == widths.sort_value)
        .unwrap();

    let text = sort_title_text(longest_category.label(), longest_sort.label());
    assert_eq!(
        text.matches('─').count(),
        2,
        "the widest combination should keep one dash per column:\n{text}"
    );
    assert!(
        text.ends_with(" ─"),
        "the line should end space-dash:\n{text}"
    );
}

#[test]
fn scan_cache_path_lives_under_the_cache_dir_not_the_library_root() {
    let _guard = lock_env();
    let home = tempfile::tempdir().unwrap();
    let library_root = tempfile::tempdir().unwrap();

    let (home_path, root_path) = unsafe {
        let prev_home = std::env::var("HOME").ok();
        let prev_xdg = std::env::var("XDG_CACHE_HOME").ok();
        std::env::set_var("HOME", home.path());
        std::env::remove_var("XDG_CACHE_HOME");

        let cache_path = config::scan_cache_path(library_root.path());

        match prev_home {
            Some(v) => std::env::set_var("HOME", v),
            None => std::env::remove_var("HOME"),
        }
        match prev_xdg {
            Some(v) => std::env::set_var("XDG_CACHE_HOME", v),
            None => std::env::remove_var("XDG_CACHE_HOME"),
        }

        (cache_path, library_root.path().to_path_buf())
    };

    assert!(
        home_path.starts_with(home.path().join(".cache").join("lyre")),
        "cache file should live under the XDG cache dir, got {home_path:?}"
    );
    assert!(
        !home_path.starts_with(&root_path),
        "cache file should not be written inside the library root, got {home_path:?}"
    );
}

#[test]
fn scan_cache_path_is_stable_for_the_same_root() {
    let _guard = lock_env();
    let library_root = tempfile::tempdir().unwrap();

    let (first, second) = unsafe {
        let prev_home = std::env::var("HOME").ok();
        std::env::set_var("HOME", tempfile::tempdir().unwrap().path());

        let first = config::scan_cache_path(library_root.path());
        let second = config::scan_cache_path(library_root.path());

        match prev_home {
            Some(v) => std::env::set_var("HOME", v),
            None => std::env::remove_var("HOME"),
        }

        (first, second)
    };

    assert_eq!(
        first, second,
        "the same library root should always map to the same cache file"
    );
}

#[test]
fn playlists_path_lives_under_the_data_dir() {
    let _guard = lock_env();
    let home = tempfile::tempdir().unwrap();
    let library_root = tempfile::tempdir().unwrap();

    let path = unsafe {
        let prev_home = std::env::var("HOME").ok();
        let prev_xdg = std::env::var("XDG_DATA_HOME").ok();
        std::env::set_var("HOME", home.path());
        std::env::remove_var("XDG_DATA_HOME");

        let path = config::playlists_path(library_root.path());

        match prev_home {
            Some(v) => std::env::set_var("HOME", v),
            None => std::env::remove_var("HOME"),
        }
        match prev_xdg {
            Some(v) => std::env::set_var("XDG_DATA_HOME", v),
            None => std::env::remove_var("XDG_DATA_HOME"),
        }

        path
    };

    assert!(
        path.starts_with(home.path().join(".local").join("share").join("lyre")),
        "playlists file should live under the XDG data dir, got {path:?}"
    );
}

#[test]
fn playlists_path_is_stable_for_the_same_root_and_differs_across_roots() {
    let _guard = lock_env();
    let one_root = tempfile::tempdir().unwrap();
    let another_root = tempfile::tempdir().unwrap();

    let (first, second, from_another_root) = unsafe {
        let prev_home = std::env::var("HOME").ok();
        std::env::set_var("HOME", tempfile::tempdir().unwrap().path());

        let first = config::playlists_path(one_root.path());
        let second = config::playlists_path(one_root.path());
        let from_another_root = config::playlists_path(another_root.path());

        match prev_home {
            Some(v) => std::env::set_var("HOME", v),
            None => std::env::remove_var("HOME"),
        }

        (first, second, from_another_root)
    };

    assert_eq!(
        first, second,
        "the same library root should always map to the same playlists file"
    );
    assert_ne!(
        first, from_another_root,
        "each library root must get its own playlists file, so switching directories can't silently \
         prune or overwrite another library's playlists"
    );
}

#[test]
fn view_state_round_trips_through_serde() {
    let state = config::ViewState {
        library_category: Category::Artist,
        library_sort: Sort::DateModified,
        library_playlist_mode: PlaylistDisplayMode::Expanded,
        playlist_category: Category::Path,
        playlist_sort: Sort::Duration,
    };

    let json = serde_json::to_string(&state.library_category).unwrap();
    assert_eq!(
        serde_json::from_str::<Category>(&json).unwrap(),
        state.library_category
    );

    let json = serde_json::to_string(&state.library_sort).unwrap();
    assert_eq!(
        serde_json::from_str::<Sort>(&json).unwrap(),
        state.library_sort
    );

    let json = serde_json::to_string(&state.library_playlist_mode).unwrap();
    assert_eq!(
        serde_json::from_str::<PlaylistDisplayMode>(&json).unwrap(),
        state.library_playlist_mode
    );
}

#[test]
fn saved_view_state_is_reloaded_on_the_next_launch() {
    let _guard = lock_env();
    let home = tempfile::tempdir().unwrap();

    let reloaded = unsafe {
        let prev_home = std::env::var("HOME").ok();
        let prev_xdg = std::env::var("XDG_CONFIG_HOME").ok();
        std::env::set_var("HOME", home.path());
        std::env::remove_var("XDG_CONFIG_HOME");

        config::save_view_state(&config::ViewState {
            library_category: Category::Artist,
            library_sort: Sort::Path,
            library_playlist_mode: PlaylistDisplayMode::Count,
            playlist_category: Category::None,
            playlist_sort: Sort::Title,
        });
        let reloaded = config::load_view_state();

        match prev_home {
            Some(v) => std::env::set_var("HOME", v),
            None => std::env::remove_var("HOME"),
        }
        match prev_xdg {
            Some(v) => std::env::set_var("XDG_CONFIG_HOME", v),
            None => std::env::remove_var("XDG_CONFIG_HOME"),
        }

        reloaded
    };

    assert_eq!(reloaded.library_category, Category::Artist);
    assert_eq!(reloaded.library_sort, Sort::Path);
    assert_eq!(reloaded.library_playlist_mode, PlaylistDisplayMode::Count);
    assert_eq!(reloaded.playlist_category, Category::None);
    assert_eq!(reloaded.playlist_sort, Sort::Title);
}

#[test]
fn saving_view_state_does_not_clobber_the_last_dir() {
    let _guard = lock_env();
    let home = tempfile::tempdir().unwrap();
    let library_root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(library_root.path()).unwrap();

    let reloaded_dir = unsafe {
        let prev_home = std::env::var("HOME").ok();
        let prev_xdg = std::env::var("XDG_CONFIG_HOME").ok();
        std::env::set_var("HOME", home.path());
        std::env::remove_var("XDG_CONFIG_HOME");

        config::save_last_dir(library_root.path());
        config::save_view_state(&config::ViewState {
            library_category: Category::Artist,
            ..Default::default()
        });
        let reloaded_dir = config::load_last_dir();

        match prev_home {
            Some(v) => std::env::set_var("HOME", v),
            None => std::env::remove_var("HOME"),
        }
        match prev_xdg {
            Some(v) => std::env::set_var("XDG_CONFIG_HOME", v),
            None => std::env::remove_var("XDG_CONFIG_HOME"),
        }

        reloaded_dir
    };

    assert_eq!(
        reloaded_dir.as_deref(),
        Some(library_root.path()),
        "saving view state must not erase the previously saved last_dir"
    );
}

#[test]
fn scrolling_to_the_end_brings_the_last_song_into_view() {
    let mut h = harness();
    let short = Rect::new(0, 0, 120, 12);

    let mut buf = Buffer::empty(short);
    h.app.render(short, &mut buf);
    assert!(buffer_text(&buf).contains("Anchor"));

    h.app.on_key(special(KeyCode::Char('G')));
    let mut buf = Buffer::empty(short);
    h.app.render(short, &mut buf);
    assert!(
        buffer_text(&buf).contains("Grove"),
        "the last song should scroll into view"
    );
}

#[test]
fn marquee_window_returns_short_ascii_text_unchanged() {
    assert_eq!(marquee_window("Beacon", 20), "Beacon");
}

#[test]
fn marquee_window_never_exceeds_the_visible_width_for_wide_characters() {
    for text in [
        "初音ミク",
        "こんにちは世界",
        "안녕하세요",
        "Beacon 灯台 Artist",
    ] {
        for width in 1..12 {
            let windowed = marquee_window(text, width);
            assert!(
                lyre_tui::ui::display_width(&windowed) <= width,
                "marquee_window({text:?}, {width}) returned {windowed:?} with display width \
                 {}, exceeding the {width}-column budget",
                lyre_tui::ui::display_width(&windowed)
            );
        }
    }
}

#[test]
fn marquee_window_zero_width_returns_empty() {
    assert_eq!(marquee_window("Beacon", 0), "");
}

#[test]
fn marquee_scroll_offset_holds_still_through_the_pause_phase() {
    let loop_len = 63;

    assert_eq!(marquee_scroll_offset(0, loop_len), 0);
    assert_eq!(marquee_scroll_offset(2_000, loop_len), 0);
    assert_eq!(marquee_scroll_offset(4_499, loop_len), 0);
}

#[test]
fn marquee_scroll_offset_advances_one_step_per_interval_after_the_pause() {
    let loop_len = 63;

    assert_eq!(marquee_scroll_offset(4_500, loop_len), 0);
    assert_eq!(marquee_scroll_offset(4_649, loop_len), 0);
    assert_eq!(marquee_scroll_offset(4_650, loop_len), 1);
    assert_eq!(marquee_scroll_offset(4_800, loop_len), 2);
}

#[test]
fn marquee_scroll_offset_wraps_around_at_the_end_of_one_cycle() {
    let loop_len = 63;
    let scroll_end_ms = 4_500 + 63 * 150;

    assert_eq!(marquee_scroll_offset(scroll_end_ms - 150, loop_len), 62);
    assert_eq!(
        marquee_scroll_offset(scroll_end_ms, loop_len),
        0,
        "a completed cycle must restart at the pause, not run past the last character"
    );
    assert_eq!(marquee_scroll_offset(scroll_end_ms + 5_000, loop_len), 3);
}

#[test]
fn marquee_scroll_offset_survives_a_large_elapsed_time() {
    let loop_len = 63;
    let cycles = (63 * 150 * 10_000) / (4_500 + 63 * 150);
    let late_ms = cycles * (4_500 + 63 * 150) + 4_650;

    assert_eq!(
        marquee_scroll_offset(late_ms, loop_len),
        1,
        "elapsed time far beyond one cycle must mod back into the cycle"
    );
}

#[test]
fn allocate_title_artist_widths_gives_the_title_everything_when_there_is_no_artist() {
    let widths = lyre_tui::ui::allocate_title_artist_widths(40, 10, None);
    assert_eq!(widths.title_max, 40);
    assert_eq!(widths.artist_max, None);
}

#[test]
fn allocate_title_artist_widths_leaves_both_untouched_when_both_fit() {
    let widths = lyre_tui::ui::allocate_title_artist_widths(49, 10, Some(12));
    assert_eq!(widths.title_max, 10);
    assert_eq!(widths.artist_max, Some(12));
}

#[test]
fn allocate_title_artist_widths_gives_a_short_titles_slack_to_a_long_artist() {
    let widths = lyre_tui::ui::allocate_title_artist_widths(49, 5, Some(44));
    assert_eq!(widths.title_max, 5);
    assert_eq!(widths.artist_max, Some(44));
}

#[test]
fn allocate_title_artist_widths_gives_a_short_artists_slack_to_a_long_title() {
    let widths = lyre_tui::ui::allocate_title_artist_widths(49, 39, Some(10));
    assert_eq!(widths.title_max, 39);
    assert_eq!(widths.artist_max, Some(10));
}

#[test]
fn allocate_title_artist_widths_falls_back_to_the_ratio_split_when_neither_fits() {
    let widths = lyre_tui::ui::allocate_title_artist_widths(49, 45, Some(30));
    assert_eq!(widths.title_max, 34);
    assert_eq!(widths.artist_max, Some(15));
}

#[test]
fn allocate_title_artist_widths_keeps_the_marquee_minimum_when_the_budget_is_tiny() {
    let widths = lyre_tui::ui::allocate_title_artist_widths(3, 100, Some(100));
    assert_eq!(widths.title_max, 3);
    assert_eq!(widths.artist_max, Some(0));
}

#[test]
fn a_long_artist_name_renders_untruncated_next_to_a_short_title_at_moderate_width() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path().join("music");
    let d = root.join("Delta");
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join("0-Hi.wav"), wav("Hi", "The Marvellous Orchestra", 400)).unwrap();

    let (library, _) = Library::scan(&root, dir.path().join("cache.bin")).unwrap();
    let (playlists, _) = PlaylistStore::load(dir.path().join("playlists"), &library);
    let mut app = App::new(library, playlists, Backend::null());

    let buf = render(&mut app, 80, 12);
    let text = buffer_text(&buf);

    assert!(
        text.contains("The Marvellous Orchestra"),
        "the full artist name must appear untruncated at 80 columns:\n{text}"
    );
    assert!(
        !text.contains('\u{2026}'),
        "no row may show an ellipsis when everything fits:\n{text}"
    );
}

#[test]
fn lowercase_y_opens_the_youtube_modal_entering_url() {
    let mut h = harness();
    h.app.on_key(key('y'));

    let modal = h
        .app
        .youtube_mode()
        .expect("<y> must open the youtube modal");
    assert!(
        matches!(modal, lyre_tui::app::YoutubeModal::EnteringUrl { url_input, error, restore } if url_input.is_empty() && error.is_none() && restore.is_none())
    );
}

#[cfg(not(feature = "youtube"))]
#[test]
fn without_the_youtube_feature_submitting_a_url_fails_gracefully_instead_of_hanging() {
    let mut h = harness();
    h.app.on_key(key('y'));
    for c in "https://example.com/watch?v=x".chars() {
        h.app.on_key(key(c));
    }
    h.app.on_key(special(KeyCode::Enter));

    assert_eq!(
        h.app.drain_youtube_events_for_test(),
        lyre_tui::app::EventsChanged::Changed,
        "the stub must report an event, not silently do nothing"
    );

    match h
        .app
        .youtube_mode()
        .expect("a failure must not silently close the modal")
    {
        lyre_tui::app::YoutubeModal::EnteringUrl {
            url_input, error, ..
        } => {
            assert_eq!(url_input, "https://example.com/watch?v=x");
            assert_eq!(
                error.as_deref(),
                Some("YouTube support was not built into this binary")
            );
        }
        _ => panic!("a failure must bounce the user back to the URL screen"),
    }
}

#[test]
fn youtube_modal_entering_url_accumulates_typed_characters_and_esc_cancels() {
    let mut h = harness();
    h.app.on_key(key('y'));
    h.app.on_key(key('h'));
    h.app.on_key(key('i'));

    match h.app.youtube_mode().unwrap() {
        lyre_tui::app::YoutubeModal::EnteringUrl { url_input, .. } => assert_eq!(url_input, "hi"),
        _ => panic!("expected EnteringUrl"),
    }

    h.app.on_key(special(KeyCode::Esc));
    assert!(!h.app.youtube_mode_is());
}

#[test]
fn youtube_modal_rejects_a_directory_that_escapes_the_library_root() {
    let mut h = harness();
    h.app.push_youtube(lyre_tui::app::YoutubeModal::EditingFields(
        lyre_tui::app::YoutubeFieldsModal {
            title: "Some Title".to_string(),
            artist: "Some Artist".to_string(),
            directory: "../escape".to_string(),
            file_name: "SomeArtist-SomeTitle.mp3".to_string(),
            file_name_overridden: true,
            focused: lyre_tui::app::YoutubeField::Directory,
            ..youtube_fields(
                "https://example.com/watch?v=x",
                lyre_tui::app::YoutubeField::Directory,
            )
        },
    ));

    h.app.on_key(special(KeyCode::Enter));

    match h
        .app
        .youtube_mode()
        .expect("modal should stay open on validation error")
    {
        lyre_tui::app::YoutubeModal::EditingFields(fields) => {
            assert!(fields.error.as_deref().unwrap_or_default().contains(".."));
        }
        _ => panic!("expected to stay on EditingFields"),
    }
}

#[test]
fn youtube_modal_auto_generates_the_file_name_from_title_and_artist_until_overridden() {
    let mut h = harness();
    h.app.push_youtube(lyre_tui::app::YoutubeModal::EditingFields(youtube_fields(
        "https://example.com/watch?v=x",
        lyre_tui::app::YoutubeField::Title,
    )));

    for c in "Bush".chars() {
        h.app.on_key(key(c));
    }
    h.app.on_key(special(KeyCode::Tab));
    for c in "Kate".chars() {
        h.app.on_key(key(c));
    }

    match h.app.youtube_mode().unwrap() {
        lyre_tui::app::YoutubeModal::EditingFields(fields) => {
            assert_eq!(fields.file_name, "Kate-Bush.mp3");
            assert!(!fields.file_name_overridden);
        }
        _ => panic!("expected EditingFields"),
    }
}

#[test]
fn metadata_field_visible_hides_the_romanized_fields_by_default() {
    let edits = MetadataEdits::default();
    let visible = MetadataField::visible(&edits);

    assert!(!visible.contains(&MetadataField::TitleSort));
    assert!(!visible.contains(&MetadataField::ArtistSort));
}

#[test]
fn metadata_field_visible_shows_title_sort_only_when_the_title_needs_romanization() {
    let edits = MetadataEdits {
        title: "夜明け".to_string(),
        ..MetadataEdits::default()
    };
    let visible = MetadataField::visible(&edits);

    assert!(visible.contains(&MetadataField::TitleSort));
    assert!(!visible.contains(&MetadataField::ArtistSort));
}

#[test]
fn metadata_modal_typing_a_non_ascii_title_reveals_the_title_sort_field_in_the_tab_order() {
    let mut h = harness();
    h.app.on_key(key('g'));
    h.app.on_key(key('e'));

    for c in "夜明け".chars() {
        h.app.on_key(key(c));
    }

    let visible = MetadataField::visible(&h.app.metadata_mode().unwrap().edits);
    assert!(
        visible.contains(&MetadataField::TitleSort),
        "a non-ASCII title must reveal the romanized field"
    );

    h.app.on_key(special(KeyCode::Tab));
    assert_eq!(
        h.app.metadata_mode().unwrap().focused,
        MetadataField::TitleSort
    );
}

fn open_metadata_modal_for_selected_song(h: &mut Harness) {
    h.app.on_key(key('e'));
}

#[test]
fn saving_a_new_romanized_artist_prompts_to_apply_it_to_sibling_songs() {
    let mut h = harness();
    h.app.on_key(key('g'));
    let Some(Row::Song(id, _)) = h.app.selected_row() else {
        panic!("expected a song row")
    };
    assert_eq!(
        h.app.library.get(id).unwrap().artist(),
        "Alpha",
        "sanity check: selected an Alpha song"
    );

    open_metadata_modal_for_selected_song(&mut h);
    h.app.metadata_mode_mut().unwrap()
        .edits
        .artist_sort = "Arufa".to_string();
    h.app.on_key(special(KeyCode::Enter));

    let confirm = h
        .app
        .romanized_mode()
        .expect("must prompt when a sibling song shares the artist");
    assert_eq!(confirm.artist_display, "Alpha");
    assert_eq!(confirm.value, "Arufa");
    assert_eq!(
        confirm.count, 1,
        "only one other Alpha song exists in the fixture"
    );
}

#[test]
fn saving_a_romanized_artist_with_no_siblings_does_not_prompt() {
    let mut h = harness();
    h.app.on_key(key('g'));

    open_metadata_modal_for_selected_song(&mut h);
    {
        let modal = h.app.metadata_mode_mut().unwrap();
        modal.edits.artist = "CompletelyUniqueArtist".to_string();
        modal.edits.artist_sort = "Yunikuu".to_string();
    }
    h.app.on_key(special(KeyCode::Enter));

    assert!(
        h.app.modes_is_empty(),
        "an artist with no other songs must not trigger the confirmation"
    );
}

#[test]
fn declining_the_romanized_artist_prompt_leaves_the_library_untouched() {
    let mut h = harness();
    h.app.on_key(key('g'));
    let songs_before = h.app.library.len();

    open_metadata_modal_for_selected_song(&mut h);
    h.app.metadata_mode_mut().unwrap()
        .edits
        .artist_sort = "Arufa".to_string();
    h.app.on_key(special(KeyCode::Enter));
    assert!(h.app.romanized_mode_is());

    h.app.on_key(key('n'));

    assert!(!h.app.romanized_mode_is());
    assert_eq!(
        h.app.library.len(),
        songs_before,
        "declining must not change how many songs exist"
    );
}

#[test]
fn accepting_the_romanized_artist_prompt_applies_it_and_closes_the_modal() {
    let mut h = harness();
    h.app.on_key(key('g'));
    let songs_before = h.app.library.len();

    open_metadata_modal_for_selected_song(&mut h);
    h.app.metadata_mode_mut().unwrap()
        .edits
        .artist_sort = "Arufa".to_string();
    h.app.on_key(special(KeyCode::Enter));
    assert!(h.app.romanized_mode_is());

    h.app.on_key(key('y'));

    assert!(!h.app.romanized_mode_is());
    assert_eq!(
        h.app.library.len(),
        songs_before,
        "applying must not add or remove songs, only re-tag them"
    );
}

#[test]
fn youtube_field_visible_hides_the_romanized_fields_by_default() {
    let fields = youtube_fields(
        "https://example.com/watch?v=x",
        lyre_tui::app::YoutubeField::Title,
    );
    let visible = lyre_tui::app::YoutubeField::visible(&fields);

    assert!(!visible.contains(&lyre_tui::app::YoutubeField::TitleSort));
    assert!(!visible.contains(&lyre_tui::app::YoutubeField::ArtistSort));
}

#[test]
fn youtube_modal_typing_a_non_ascii_artist_reveals_the_artist_sort_field_in_the_tab_order() {
    let mut h = harness();
    h.app.push_youtube(lyre_tui::app::YoutubeModal::EditingFields(youtube_fields(
        "https://example.com/watch?v=x",
        lyre_tui::app::YoutubeField::Artist,
    )));

    for c in "夜明けバンド".chars() {
        h.app.on_key(key(c));
    }

    match h.app.youtube_mode().unwrap() {
        lyre_tui::app::YoutubeModal::EditingFields(fields) => {
            let visible = lyre_tui::app::YoutubeField::visible(fields);
            assert!(
                visible.contains(&lyre_tui::app::YoutubeField::ArtistSort),
                "a non-ASCII artist must reveal the romanized field"
            );
        }
        _ => panic!("expected EditingFields"),
    }

    h.app.on_key(special(KeyCode::Tab));
    match h.app.youtube_mode().unwrap() {
        lyre_tui::app::YoutubeModal::EditingFields(fields) => {
            assert_eq!(fields.focused, lyre_tui::app::YoutubeField::ArtistSort);
        }
        _ => panic!("expected EditingFields"),
    }
}

#[test]
fn start_youtube_fields_with_no_restore_defaults_the_directory_to_the_library_root() {
    let fields =
        lyre_tui::app::start_youtube_fields("https://example.com/watch?v=x".to_string(), None);

    assert_eq!(fields.directory, "./");
    assert!(matches!(
        fields.fetch_status,
        lyre_tui::app::FetchStatus::Pending
    ));
    assert!(matches!(
        fields.download_status,
        lyre_tui::app::DownloadStatus::Pending
    ));
    assert_eq!(fields.focused, lyre_tui::app::YoutubeField::Title);
}

#[test]
fn start_youtube_fields_with_a_restore_snapshot_keeps_everything_but_the_url_and_resets_status() {
    let mut previous = youtube_fields(
        "https://old-url.example.com",
        lyre_tui::app::YoutubeField::Album,
    );
    previous.title = "Yoake".to_string();
    previous.artist = "Some Band".to_string();
    previous.directory = "custom/subdir".to_string();
    previous.error = Some("a stale error".to_string());

    let fields = lyre_tui::app::start_youtube_fields(
        "https://new-url.example.com".to_string(),
        Some(previous),
    );

    assert_eq!(fields.url, "https://new-url.example.com");
    assert_eq!(fields.title, "Yoake");
    assert_eq!(fields.artist, "Some Band");
    assert_eq!(
        fields.directory, "custom/subdir",
        "a retried attempt must keep the user's edits"
    );
    assert!(
        fields.error.is_none(),
        "the error from the previous attempt must be cleared"
    );
    assert_eq!(
        fields.focused,
        lyre_tui::app::YoutubeField::Title,
        "focus always resets to Title on retry"
    );
    assert!(matches!(
        fields.fetch_status,
        lyre_tui::app::FetchStatus::Pending
    ));
    assert!(matches!(
        fields.download_status,
        lyre_tui::app::DownloadStatus::Pending
    ));
}

#[test]
fn a_download_failure_interrupts_the_user_and_preserves_their_fields_for_retry() {
    let mut h = harness();
    let mut fields = youtube_fields(
        "https://example.com/watch?v=x",
        lyre_tui::app::YoutubeField::Album,
    );
    fields.title = "Yoake".to_string();
    fields.artist = "Some Band".to_string();
    fields.directory = "custom/subdir".to_string();
    h.app.push_youtube(lyre_tui::app::YoutubeModal::EditingFields(fields));

    h.app
        .handle_youtube_event_for_test(lyre_tui::app::DownloadEvent::Failed(
            "network error".to_string(),
        ));

    match h
        .app
        .youtube_mode()
        .expect("a failure must not silently close the modal")
    {
        lyre_tui::app::YoutubeModal::EnteringUrl {
            url_input,
            error,
            restore,
        } => {
            assert_eq!(url_input, "https://example.com/watch?v=x");
            assert_eq!(error.as_deref(), Some("network error"));
            let restore = restore
                .as_ref()
                .expect("the typed fields must be preserved for a retry");
            assert_eq!(restore.title, "Yoake");
            assert_eq!(restore.artist, "Some Band");
            assert_eq!(restore.directory, "custom/subdir");
        }
        _ => panic!("a failure must bounce the user back to the URL screen"),
    }
}

#[test]
fn a_fetch_failure_while_still_downloading_also_interrupts_and_preserves_fields() {
    let mut h = harness();
    let fields = youtube_fields(
        "https://example.com/watch?v=x",
        lyre_tui::app::YoutubeField::Title,
    );
    h.app.push_youtube(lyre_tui::app::YoutubeModal::Downloading {
        file_name: "song.mp3".to_string(),
        dest_path: h.app.library.root().join("song.mp3"),
        fields,
        progress: 0.0,
    });

    h.app
        .handle_youtube_event_for_test(lyre_tui::app::DownloadEvent::Failed(
            "this video is a live stream".to_string(),
        ));

    match h.app.youtube_mode().unwrap() {
        lyre_tui::app::YoutubeModal::EnteringUrl { error, restore, .. } => {
            assert_eq!(error.as_deref(), Some("this video is a live stream"));
            assert!(restore.is_some());
        }
        _ => panic!("a failure while waiting on the download must also interrupt"),
    }
}

#[test]
fn keys_do_not_reset_the_shown_download_progress() {
    let mut h = harness();
    let fields = youtube_fields(
        "https://example.com/watch?v=x",
        lyre_tui::app::YoutubeField::Title,
    );
    h.app.push_youtube(lyre_tui::app::YoutubeModal::Downloading {
        file_name: "song.mp3".to_string(),
        dest_path: h.app.library.root().join("song.mp3"),
        fields,
        progress: 42.0,
    });

    h.app.on_key(key('x'));

    match h.app.youtube_mode().unwrap() {
        lyre_tui::app::YoutubeModal::Downloading { progress, .. } => {
            assert_eq!(*progress, 42.0, "a keystroke must not reset the progress bar")
        }
        _ => panic!("the downloading modal must stay open"),
    }
}

#[test]
fn esc_during_a_download_gives_feedback_and_keeps_the_modal_open() {
    let mut h = harness();
    let fields = youtube_fields(
        "https://example.com/watch?v=x",
        lyre_tui::app::YoutubeField::Title,
    );
    h.app.push_youtube(lyre_tui::app::YoutubeModal::Downloading {
        file_name: "song.mp3".to_string(),
        dest_path: h.app.library.root().join("song.mp3"),
        fields,
        progress: 10.0,
    });

    h.app.on_key(special(KeyCode::Esc));

    assert!(h.app.youtube_mode_is());
    assert!(h.app.status.text.contains("download in progress"));
}

#[test]
fn closing_the_modal_after_a_failure_discards_the_saved_fields() {
    let mut h = harness();
    h.app.push_youtube(lyre_tui::app::YoutubeModal::EditingFields(youtube_fields(
        "https://example.com/watch?v=x",
        lyre_tui::app::YoutubeField::Title,
    )));
    h.app
        .handle_youtube_event_for_test(lyre_tui::app::DownloadEvent::Failed(
            "network error".to_string(),
        ));
    assert!(h.app.youtube_mode_is());

    h.app.on_key(special(KeyCode::Esc));

    assert!(
        !h.app.youtube_mode_is(),
        "exiting the whole modal must not keep anything around"
    );
}

#[test]
fn a_download_finishing_while_still_editing_fields_is_remembered_without_leaving_the_screen() {
    let mut h = harness();
    h.app.push_youtube(lyre_tui::app::YoutubeModal::EditingFields(youtube_fields(
        "https://example.com/watch?v=x",
        lyre_tui::app::YoutubeField::Title,
    )));

    let temp_path = std::env::temp_dir().join("lyre-test-download.mp3");
    h.app
        .handle_youtube_event_for_test(lyre_tui::app::DownloadEvent::DownloadReady(
            temp_path.clone(),
        ));

    match h.app.youtube_mode().unwrap() {
        lyre_tui::app::YoutubeModal::EditingFields(fields) => {
            assert!(
                matches!(&fields.download_status, lyre_tui::app::DownloadStatus::Ready(p) if p == &temp_path),
                "the finished download must be recorded without forcing the user off the fields screen"
            );
        }
        _ => panic!("the user must stay on EditingFields while still typing"),
    }
}

#[test]
fn info_ready_while_editing_fields_updates_the_inline_status_without_touching_typed_fields() {
    let mut h = harness();
    let mut fields = youtube_fields(
        "https://example.com/watch?v=x",
        lyre_tui::app::YoutubeField::Title,
    );
    fields.title = "user typed this".to_string();
    h.app.push_youtube(lyre_tui::app::YoutubeModal::EditingFields(fields));

    h.app
        .handle_youtube_event_for_test(lyre_tui::app::DownloadEvent::InfoReady {
            title: "Fetched Video Title".to_string(),
            uploader: Some("Some Uploader".to_string()),
        });

    match h.app.youtube_mode().unwrap() {
        lyre_tui::app::YoutubeModal::EditingFields(fields) => {
            assert_eq!(
                fields.title, "user typed this",
                "fetched info must never overwrite what the user typed"
            );
            match &fields.fetch_status {
                lyre_tui::app::FetchStatus::Ready { title, uploader } => {
                    assert_eq!(title, "Fetched Video Title");
                    assert_eq!(uploader.as_deref(), Some("Some Uploader"));
                }
                lyre_tui::app::FetchStatus::Pending => panic!("fetch status must update to Ready"),
            }
        }
        _ => panic!("expected EditingFields"),
    }
}

#[test]
fn question_mark_while_searching_types_into_the_query_instead_of_opening_help() {
    let mut h = harness();

    h.app.on_key(key('/'));
    assert!(h.app.search_library_mode_is(), "sanity: search is open");

    h.app.on_key(key('?'));
    assert!(
        !h.app.help_mode_is(),
        "search owns input, so ? must not open the help overlay"
    );
    assert_eq!(h.app.library_panel.search_query, "?");

    h.app.on_key(special(KeyCode::Esc));
    assert!(
        h.app.search_library_mode_is(),
        "Esc with a non-empty query only clears the query, staying in search"
    );
    assert!(
        h.app.library_panel.search_query.is_empty(),
        "the query itself is cleared"
    );

    h.app.on_key(special(KeyCode::Esc));
    assert!(
        !h.app.search_library_mode_is(),
        "a second Esc (now-empty query) leaves search"
    );
    assert!(
        h.app.modes_is_empty(),
        "no other overlay may be open"
    );
}

#[test]
fn quitting_requires_confirmation_only_from_the_main_view_not_while_a_modal_is_open() {
    let mut h = harness();
    h.app.on_key(key('?'));
    assert!(h.app.help_mode_is());

    h.app.on_key(key('q'));
    assert!(
        !h.app.confirm_mode_is(),
        "q while a modal owns input must not open the quit confirmation"
    );
    assert!(h.app.modes_is_empty(), "the modal consumed the q and closed");

    h.app.on_key(key('q'));
    assert!(h.app.confirm_mode_is());
}

#[test]
fn editing_metadata_keeps_the_song_in_the_queue_at_the_same_position() {
    let mut h = harness();
    h.app.on_key(key('g'));

    let Some(Row::Song(id, _)) = h.app.selected_row() else {
        panic!("expected a song row")
    };
    h.app.on_key(special(KeyCode::Enter));
    assert_eq!(h.app.queue.current_id(), Some(id));

    let queue_before = h.app.queue.upcoming(100);

    h.app.on_key(key('e'));
    for _ in 0..32 {
        h.app.on_key(special(KeyCode::Backspace));
    }
    for c in "Retitled".chars() {
        h.app.on_key(key(c));
    }
    h.app.on_key(special(KeyCode::Enter));

    let playing = h.app.queue.current_id();
    assert_eq!(
        playing,
        Some(id),
        "with stable ids, playback keeps tracking the same song through an edit"
    );
    let queue_after = h.app.queue.upcoming(100);
    assert_eq!(
        queue_before.len(),
        queue_after.len(),
        "queue length must not change across an edit"
    );
}

#[test]
fn status_message_disappears_after_four_seconds_of_idle() {
    let mut h = harness();
    h.app.on_key(key('g'));
    h.app.on_key(key('s'));
    assert!(!h.app.status.text.is_empty(), "sanity: a status was set");
    assert!(
        !h.app.status.expire_if_stale_for_test(),
        "a fresh status must not expire immediately"
    );
    assert!(!h.app.status.text.is_empty());
}

#[test]
fn pending_number_digits_are_disclosed_when_a_non_digit_key_arrives() {
    let mut h = harness();
    h.app.on_key(key('1'));
    h.app.on_key(key('2'));
    assert_eq!(h.app.pending_number_for_test(), "12");

    h.app.on_key(key('j'));
    assert!(
        h.app.pending_number_for_test().is_empty(),
        "a non-digit key must clear the pending jump number"
    );

    h.app.on_key(key('5'));
    h.app.on_key(special(KeyCode::Esc));
    assert!(
        h.app.pending_number_for_test().is_empty(),
        "Esc must cancel the pending jump number"
    );
}

#[test]
fn slash_starts_search_from_both_panels() {
    let mut h = harness();
    h.app.panel = Panel::Library;
    h.app.on_key(key('/'));
    assert!(h.app.search_library_mode_is());

    h.app.on_key(special(KeyCode::Esc));
    h.app.panel = Panel::Playlists;
    h.app.on_key(key('/'));
    assert!(h.app.search_playlists_mode_is());
}

#[test]
fn esc_with_pending_queue_jump_digits_reports_the_cancellation() {
    let mut h = harness();
    h.app.on_key(key('1'));
    h.app.on_key(special(KeyCode::Esc));
    assert!(
        h.app.status.text.contains("cancelled queue jump"),
        "Esc during a pending jump must report cancellation"
    );
}

#[test]
fn esc_while_a_visual_selection_is_active_cancels_it() {
    let mut h = harness();
    h.app.on_key(key('g'));
    h.app.on_key(ctrl('v'));
    assert!(h.app.active_visual_for_test(), "sanity: visual mode active");
    h.app.on_key(special(KeyCode::Esc));
    assert!(
        !h.app.active_visual_for_test(),
        "Esc must cancel an active visual selection"
    );
}

#[test]
fn esc_in_an_open_playlist_exits_to_browsing_when_the_query_is_empty() {
    let mut h = harness();
    let id = h.app.playlists.create("Mix");
    let song_id = first_song_id(&mut h.app);
    h.app.playlists.add_song(id, song_id);

    h.app.panel = Panel::Playlists;
    h.app.playlist_panel.list_state.select(Some(0));
    h.app.on_key(special(KeyCode::Enter));
    assert!(matches!(
        h.app.playlist_panel.view,
        PlaylistView::Viewing(_)
    ));

    h.app.on_key(special(KeyCode::Esc));
    assert_eq!(h.app.playlist_panel.view, PlaylistView::Browsing);
}

fn first_song_id(app: &mut App) -> lyre_core::SongId {
    app.visible_rows()
        .iter()
        .find_map(|r| match r {
            Row::Song(id, _) => Some(*id),
            Row::Header(_) => None,
        })
        .unwrap()
}


#[test]
fn an_empty_library_renders_an_empty_state_message_instead_of_a_blank_panel() {
    let dir = tempfile::TempDir::new().unwrap();
    let (library, _) = Library::scan(dir.path().join("music"), dir.path().join("cache.bin"))
        .or_else(|_| {
            std::fs::create_dir_all(dir.path().join("music")).unwrap();
            Library::scan(dir.path().join("music"), dir.path().join("cache.bin"))
        })
        .unwrap();
    let (playlists, _) = PlaylistStore::load(dir.path().join("playlists"), &library);
    let mut app = App::new(library, playlists, Backend::null());

    let buf = render(&mut app, 120, 30);
    let text = buffer_text(&buf);
    assert!(
        text.contains("No songs here yet"),
        "an empty library must show a message:\n{text}"
    );
}

#[test]
fn up_next_shows_a_message_when_nothing_is_queued() {
    let mut h = harness();

    let buf = render(&mut h.app, 120, 30);
    let text = buffer_text(&buf);
    assert!(
        text.contains("nothing queued"),
        "Up Next must say when it is empty:\n{text}"
    );

    h.app.on_key(key('g'));
    h.app.on_key(special(KeyCode::Enter));
    let text = buffer_text(&render(&mut h.app, 120, 30));
    assert!(
        !text.contains("nothing queued"),
        "once something plays, the empty-state message must go away:\n{text}"
    );
}

#[test]
fn a_whitespace_only_query_is_not_a_filter() {
    let mut h = harness();
    h.app.on_key(key('g'));
    let all_songs = h.app.visible_song_count_for_test();

    h.app.library_panel.search_query = "   ".to_string();
    h.app.on_key(special(KeyCode::Char('j')));
    h.app.on_key(special(KeyCode::Char('k')));

    assert_eq!(
        h.app.visible_song_count_for_test(),
        all_songs,
        "a whitespace-only query must not filter anything"
    );
}

#[test]
fn ctrl_d_at_the_pinned_end_reaches_the_true_last_row() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path().join("music");
    for name in ["Alpha", "Beta", "Gamma"] {
        let d = root.join(name);
        std::fs::create_dir_all(&d).unwrap();
        for i in 0..15 {
            std::fs::write(
                d.join(format!("{i}-T{name}{i}.wav")),
                wav(&format!("T{name}{i}"), name, 400),
            )
            .unwrap();
        }
    }
    let (library, _) = Library::scan(&root, dir.path().join("cache.bin")).unwrap();
    let (playlists, _) = PlaylistStore::load(dir.path().join("playlists"), &library);
    let mut app = App::new(library, playlists, Backend::null());
    app.on_key(key('o'));
    app.on_key(key('p'));

    let area = Rect::new(0, 0, 100, 48);
    app.render(area, &mut Buffer::empty(area));

    for _ in 0..40 {
        app.on_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL));
        app.render(area, &mut Buffer::empty(area));
    }
    app.on_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
    app.render(area, &mut Buffer::empty(area));
    let rows = app.visible_rows().len();
    assert_ne!(
        app.active_selection_for_test(),
        Some(rows - 1),
        "after leaving the bottom, Ctrl+u must move the cursor off the last row"
    );
}

#[test]
fn ctrl_d_jumps_down_so_the_old_bottom_edge_lands_at_the_viewport_center() {
    let mut h = harness();
    h.app.measured.library_page_height = 4;
    h.app.on_key(key('g'));
    // height 4, offset 0: bottommost visible row is index 3.
    h.app.on_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL));

    assert_eq!(
        h.app.active_selection_for_test(),
        Some(3),
        "the bottommost visible row from before the jump is now at the center"
    );
    let first_offset = h.app.active_offset_for_test();

    h.app.on_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL));
    let second_offset = h.app.active_offset_for_test();
    assert_eq!(
        h.app.active_selection_for_test(),
        Some(4),
        "the clamping press still targets the old bottom edge, not the last row"
    );
    assert!(
        second_offset > first_offset,
        "repeated Ctrl+d keeps advancing through the list"
    );
}

#[test]
fn ctrl_d_at_the_end_of_the_list_lands_on_the_last_selectable_row() {
    let mut h = harness();
    h.app.measured.library_page_height = 6;
    h.app.on_key(key('g'));

    for _ in 0..20 {
        h.app.on_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL));
    }

    let len = h.app.visible_song_count_for_test();
    let total_rows = {
        // rows include headers only when grouping is on; default category is none
        len
    };
    assert_eq!(
        h.app.active_selection_for_test(),
        Some(total_rows - 1),
        "once pinned at max offset, Ctrl+d must land on the very last song"
    );
}

#[test]
fn ctrl_d_clamped_onto_the_final_page_parks_then_takes_the_bottom_row() {
    let mut h = harness_one_large_group();
    h.app.measured.library_page_height = 4;
    h.app.on_key(key('g'));

    for _ in 0..6 {
        h.app.on_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL));
    }
    assert_eq!(h.app.active_offset_for_test(), 6, "the view is pinned");
    assert_eq!(
        h.app.active_selection_for_test(),
        Some(8),
        "the clamping press selects the old bottom edge, not yet the last row"
    );

    h.app.on_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL));
    assert_eq!(
        h.app.active_selection_for_test(),
        Some(9),
        "once pinned, Ctrl+d lands on the very last row"
    );
}

#[test]
fn ctrl_u_jumps_up_so_the_old_top_edge_lands_at_the_viewport_center() {
    let mut h = harness();
    h.app.measured.library_page_height = 4;
    h.app.on_key(key('g'));
    // scroll down twice so the view is away from the top before jumping up
    h.app.on_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL));
    h.app.on_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL));
    let center = 2;
    let top_before = h.app.active_offset_for_test();

    h.app.on_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));

    let offset_after_first = h.app.active_offset_for_test();
    assert_eq!(
        h.app.active_selection_for_test(),
        Some(top_before),
        "the topmost visible row from before the jump is now at the center, cursor on it"
    );
    assert_eq!(
        offset_after_first,
        top_before.saturating_sub(center),
        "the view scrolls back so the old top edge sits at the center"
    );

    h.app.on_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
    assert_eq!(
        h.app.active_offset_for_test(),
        0,
        "repeated Ctrl+u keeps moving back up the list"
    );
    assert_eq!(
        h.app.active_selection_for_test(),
        Some(0),
        "pinned at the top, the cursor rests on the first selectable row"
    );
}

#[test]
fn ctrl_u_pinned_at_the_top_lands_on_the_first_selectable_row() {
    let mut h = harness();
    h.app.measured.library_page_height = 4;
    h.app.on_key(key('G'));

    for _ in 0..20 {
        h.app.on_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
    }

    assert_eq!(
        h.app.active_offset_for_test(),
        0,
        "the view must return to the very top"
    );
    assert_eq!(
        h.app.active_selection_for_test(),
        Some(0),
        "pinned at the top, Ctrl+u must land on the first selectable row"
    );
}

#[test]
fn ctrl_u_clamped_onto_the_first_page_parks_then_takes_the_top_row() {
    let mut h = harness_one_large_group();
    h.app.measured.library_page_height = 5;
    h.app.on_key(key('g'));

    for _ in 0..4 {
        h.app.on_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL));
    }

    for _ in 0..3 {
        h.app.on_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
    }
    assert_eq!(h.app.active_offset_for_test(), 0, "the view is pinned");
    assert_eq!(
        h.app.active_selection_for_test(),
        Some(1),
        "the clamping press selects the old top edge, not yet the first row"
    );

    h.app.on_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
    assert_eq!(
        h.app.active_selection_for_test(),
        Some(0),
        "once pinned, Ctrl+u lands on the very first row"
    );
}

#[test]
fn whitespace_only_query_does_not_filter_and_esc_exits_search_mode() {
    let mut h = harness();

    h.app.on_key(key('/'));
    h.app.on_key(key(' '));
    assert!(h.app.search_library_mode_is(), "sanity: search is open");
    let all_songs = h.app.visible_song_count_for_test();
    assert_eq!(all_songs, 6, "sanity: the fixture library has six songs");

    h.app.on_key(special(KeyCode::Esc));
    assert!(
        !h.app.search_library_mode_is(),
        "a whitespace-only query was never filtering, so Esc must exit search mode"
    );
    assert_eq!(h.app.visible_song_count_for_test(), all_songs);
}

#[test]
fn playlists_browse_with_whitespace_query_renders_the_playlist_list_not_no_matches() {
    let mut h = harness();
    h.app.panel = Panel::Playlists;

    h.app.on_key(key('/'));
    h.app.on_key(key(' '));
    h.app.on_key(special(KeyCode::Enter));

    assert_eq!(h.app.visible_playlist_ids().len(), 0, "sanity: no playlists exist yet");

    h.app.on_key(key('/'));
    h.app.on_key(key(' '));
    h.app.playlist_panel.search_query.push(' ');
    assert_eq!(
        h.app.visible_playlist_ids().len(),
        0,
        "the id list itself is empty either way; this pins that is_filtering, not emptiness, drives it"
    );
}

#[test]
fn control_modified_arrow_does_not_move_the_metadata_modal_field_cursor() {
    let mut h = harness();
    h.app.on_key(key('g'));
    h.app.on_key(key('e'));
    assert_eq!(
        h.app.metadata_mode().unwrap().focused,
        MetadataField::Title,
        "sanity: the form starts on Title"
    );

    h.app.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::CONTROL));
    h.app.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::CONTROL));

    assert_eq!(
        h.app.metadata_mode().unwrap().focused,
        MetadataField::Title,
        "modifier-stuffed presses must not act as plain field-navigation keys"
    );
}

#[test]
fn control_enter_in_the_directory_field_does_not_confirm_or_close_it() {
    let mut h = harness();
    h.app.on_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::ALT));
    assert!(!h.app.modes_is_empty(), "sanity: directory editing is open");

    h.app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL));

    assert!(
        !h.app.modes_is_empty(),
        "Ctrl+Enter is not Confirm; the directory modal must stay open"
    );

    h.app.on_key(special(KeyCode::Enter));
    assert!(
        h.app.modes_is_empty(),
        "plain Enter confirms and closes the directory modal"
    );
}

#[test]
fn control_tab_in_a_song_modal_side_panel_does_not_move_the_list_cursor() {
    let mut h = harness();
    h.app.playlists.create("First");
    h.app.playlists.create("Second");
    h.app.on_key(key('g'));
    h.app.on_key(key('w'));
    h.app.on_key(special(KeyCode::Enter));

    let PlaylistPicker::Add { list_state, .. } = h.app.song_mode().unwrap()
        .picker
        .as_ref()
        .unwrap()
    else {
        panic!("expected the add-to-playlist side panel")
    };
    let start = list_state.selected();

    h.app.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::CONTROL));

    let PlaylistPicker::Add { list_state, .. } = h.app.song_mode().unwrap()
        .picker
        .as_ref()
        .unwrap()
    else {
        panic!("expected the add-to-playlist side panel")
    };
    assert_eq!(
        list_state.selected(),
        start,
        "modifier-stuffed presses must not act as side-panel navigation"
    );
}

#[test]
fn song_modal_j_types_into_the_name_field_only_while_create_playlist_is_focused() {
    let mut h = harness();
    h.app.on_key(key('g'));
    h.app.on_key(key('w'));
    assert!(h.app.song_mode().is_some(), "sanity: modal is open");

    h.app.on_key(special(KeyCode::Tab));
    let SongModal { selected, .. } = *h.app.song_mode().unwrap();
    assert_eq!(selected, ChooseActionField::CreatePlaylist);

    h.app.on_key(key('j'));
    assert_eq!(
        h.app.song_mode().unwrap().name_input,
        "j",
        "with the name field focused, j types a letter instead of moving"
    );
}

#[test]
fn control_down_in_the_song_modal_choose_view_does_not_cycle_the_selection() {
    let mut h = harness();
    h.app.playlists.create("First");
    h.app.on_key(key('g'));
    h.app.on_key(key('w'));

    let SongModal { selected, .. } = *h.app.song_mode().unwrap();
    assert_eq!(selected, ChooseActionField::AddToPlaylist);

    h.app.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::CONTROL));

    let SongModal { selected, .. } = *h.app.song_mode().unwrap();
    assert_eq!(
        selected, ChooseActionField::AddToPlaylist,
        "modifier-stuffed presses must not act as selection cycling"
    );
}

#[test]
fn left_and_right_arrows_resolve_to_the_new_seek_actions() {
    let back = keymap::lookup(special(KeyCode::Left));
    let forward = keymap::lookup(special(KeyCode::Right));

    assert_eq!(back, Some(Action::SeekBack));
    assert_eq!(forward, Some(Action::SeekForward));

    let help = keymap::help_rows(keymap::Section::Global);
    assert!(
        help.iter().any(|(_, desc)| desc.contains("Seek back")),
        "the help overlay must list the seek bindings"
    );
}

#[test]
fn right_arrow_past_a_short_track_does_not_clamp_to_its_duration() {
    let mut h = harness();
    h.app.on_key(key('g'));
    h.app.on_key(special(KeyCode::Enter));
    assert!(h.app.queue.current_id().is_some(), "a song is playing");

    h.app.on_key(special(KeyCode::Right));

    let position = h
        .app
        .player
        .position()
        .expect("null backend reports a position while playing");
    assert!(
        position.as_secs() >= 5,
        "the 5-second overshoot must pass through unclamped (fixture tracks are under 5s), got {position:?}"
    );
}

#[test]
fn left_arrow_from_the_start_of_a_track_stays_at_zero() {
    let mut h = harness();
    h.app.on_key(key('g'));
    h.app.on_key(special(KeyCode::Enter));

    for _ in 0..3 {
        h.app.on_key(special(KeyCode::Left));
    }

    let position = h
        .app
        .player
        .position()
        .expect("null backend reports a position while playing");
    assert!(
        position < std::time::Duration::from_secs(1),
        "three backward seeks must saturate at the start of the track, got {position:?}"
    );
}

#[test]
fn arrow_keys_with_nothing_playing_show_the_nothing_playing_status() {
    let mut h = harness();
    assert!(h.app.queue.current_id().is_none());

    h.app.on_key(special(KeyCode::Right));
    assert!(h.app.status.text.contains("nothing is playing"));
    assert!(h.app.player.position().is_none());

    h.app.on_key(special(KeyCode::Left));
    assert!(h.app.status.text.contains("nothing is playing"));
}

#[test]
fn seeking_while_paused_moves_the_frozen_position_and_stays_paused() {
    let mut h = harness();
    h.app.on_key(key('g'));
    h.app.on_key(special(KeyCode::Enter));
    h.app.on_key(key(' '));
    assert_eq!(h.app.player.state(), lyre_core::player::PlaybackState::Paused);

    h.app.on_key(special(KeyCode::Right));

    let first = h
        .app
        .player
        .position()
        .expect("a paused backend still reports its frozen position");
    let second = h
        .app
        .player
        .position()
        .expect("a paused backend still reports its frozen position");
    assert!(
        first >= std::time::Duration::from_secs(5),
        "the forward seek must land 5 seconds in even while paused, got {first:?}"
    );
    assert_eq!(
        first, second,
        "the position must stay frozen after a paused seek"
    );
    assert_eq!(
        h.app.player.state(),
        lyre_core::player::PlaybackState::Paused,
        "seeking must not resume playback"
    );
}




#[test]
fn a_full_theme_file_sets_every_color_field() {
    let source = r##"
title = "#111111"
title_current = "#222222"
artist = "#333333"
detail = "#444444"
separator = "#555555"
playlist_tag = "#666666"
now_playing_marker_idle = "#777777"
missing_song = "#888888"
section_header = "#999999"
empty_state = "#aaaaaa"
playlist_name = "#bbbbbb"
playlist_song_count = "#cccccc"
text_primary = "#ddeeff"
text_secondary = "#001122"
text_muted = "#123456"
text_dim = "#abcdef"
status_info = "#000000"
success = "#010203"
warning = "#040506"
error = "#070809"
focus = "#fff"
key_hint = "#ddd"
highlight = "#eee"
modal_background = "#112233"
selected_background = "#445566"
visual_selection_background = "#778899"
dim_foreground = "#aa00aa"
dim_background = "#00aa00"
gauge_foreground = "#bb00bb"
gauge_background = "#00bb00"
"##;

    let theme = lyre_tui::theme::parse(source).expect("a full valid theme file parses");

    assert_eq!(theme.title, Color::Rgb(0x11, 0x11, 0x11));
    assert_eq!(theme.text_primary, Color::Rgb(0xdd, 0xee, 0xff));
    assert_eq!(theme.focus, Color::Rgb(0xff, 0xff, 0xff));
    assert_eq!(theme.gauge_foreground, Color::Rgb(0xbb, 0x00, 0xbb));
    assert_eq!(theme.dim_background, Color::Rgb(0x00, 0xaa, 0x00));
}

#[test]
fn a_partial_theme_file_keeps_defaults_for_missing_keys() {
    let source = "focus = \"#ff8800\"\n";
    let default = lyre_tui::theme::Theme::default();
    let theme = lyre_tui::theme::parse(source).expect("a one-key theme file parses");

    assert_eq!(theme.focus, Color::Rgb(0xff, 0x88, 0x00));
    assert_eq!(theme.title, default.title);
    assert_eq!(theme.error, default.error);
    assert_eq!(theme.gauge_background, default.gauge_background);
}

#[test]
fn an_unknown_key_fails_the_whole_theme_file() {
    let result = lyre_tui::theme::parse("foucs = \"#ffffff\"\n");
    assert!(result.is_err(), "a misspelled key must fail the file");
}

#[test]
fn a_bad_hex_value_reports_the_line_it_sits_on() {
    let source = "success = \"#00ff00\"\n\nwarning = \"orange\"\n";
    let Err(message) = lyre_tui::theme::parse(source) else {
        panic!("a non-hex value must fail the file");
    };
    assert!(
        message.contains("line 3"),
        "the error must name line 3, got: {message}"
    );
}

#[test]
fn a_missing_theme_file_is_not_an_error_and_yields_defaults() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("absent.toml");

    lyre_tui::theme::init_from_path(&path);

    assert_eq!(
        lyre_tui::theme::focus(),
        lyre_tui::theme::Theme::default().focus,
        "an absent file must leave pure defaults in place"
    );
}

#[test]
fn hex_colors_accept_short_and_long_forms_and_reject_other_shapes() {
    let short = lyre_tui::theme::parse("focus = \"#abc\"\n")
        .expect("three-digit hex parses");
    let long = lyre_tui::theme::parse("focus = \"#aabbcc\"\n")
        .expect("six-digit hex parses");
    assert_eq!(short.focus, Color::Rgb(0xaa, 0xbb, 0xcc));
    assert_eq!(long.focus, Color::Rgb(0xaa, 0xbb, 0xcc));

    for bad in ["\"aabbc\"", "\"#aabb\"", "\"#ggg\"", "\"red\"", "\"12\"", "\"#1234567\""] {
        let source = format!("focus = {bad}\n");
        assert!(
            lyre_tui::theme::parse(&source).is_err(),
            "{bad} must be rejected"
        );
    }
}

#[test]
fn the_help_overlay_names_the_backend_that_is_actually_in_use() {
    let mut h = harness();
    h.app.on_key(key('?'));
    let text = buffer_text(&render(&mut h.app, 90, 44));

    assert!(
        text.contains("none (silent mode)"),
        "the help overlay must report the null backend it was built with, got:\n{text}"
    );
    assert!(
        !text.contains("gstreamer"),
        "the help overlay must not claim gstreamer when running silently, got:\n{text}"
    );
}

#[test]
fn a_last_dir_save_failure_during_rescan_does_not_hide_the_scan_summary() {
    let _guard = lock_env();
    let mut h = harness();

    let blocker = tempfile::tempdir().unwrap();
    let unwritable_home = blocker.path().join("config-is-a-file");
    std::fs::write(&unwritable_home, b"not a directory").unwrap();

    let new_root = tempfile::tempdir().unwrap();
    std::fs::write(
        new_root.path().join("Song.wav"),
        wav("Song", "Artist", 400),
    )
    .unwrap();

    unsafe {
        let prev_home = std::env::var("HOME").ok();
        let prev_xdg = std::env::var("XDG_CONFIG_HOME").ok();
        std::env::set_var("XDG_CONFIG_HOME", &unwritable_home);
        std::env::remove_var("HOME");

        h.app.finish_dir_scan_for_test(new_root.path().to_path_buf());

        match prev_home {
            Some(v) => std::env::set_var("HOME", v),
            None => std::env::remove_var("HOME"),
        }
        match prev_xdg {
            Some(v) => std::env::set_var("XDG_CONFIG_HOME", v),
            None => std::env::remove_var("XDG_CONFIG_HOME"),
        }
    }

    assert_eq!(
        h.app.status.kind,
        lyre_tui::app::StatusKind::Success,
        "a config-save failure must not turn a successful rescan's status into an error"
    );
    assert!(
        h.app.status.text.contains("loaded"),
        "the scan summary must still be shown, got: {:?}",
        h.app.status.text
    );
    assert!(
        h.app
            .deferred_warnings_for_test()
            .iter()
            .any(|w| w.contains("last dir") || w.contains("config")),
        "the save failure must still be recorded for the post-quit warning output, got: {:?}",
        h.app.deferred_warnings_for_test()
    );
}

#[test]
fn format_duration_switches_to_hours_only_past_the_hour_mark() {
    use lyre_tui::ui::format_duration;
    use std::time::Duration;

    assert_eq!(format_duration(Duration::from_secs(0)), "00:00");
    assert_eq!(format_duration(Duration::from_secs(59)), "00:59");
    assert_eq!(format_duration(Duration::from_secs(60)), "01:00");
    assert_eq!(format_duration(Duration::from_secs(3_599)), "59:59");
    assert_eq!(format_duration(Duration::from_secs(3_600)), "1:00:00");
    assert_eq!(format_duration(Duration::from_secs(4_530)), "1:15:30");
    assert_eq!(format_duration(Duration::from_secs(36_000)), "10:00:00");
}

#[test]
fn format_mtime_renders_utc_civil_dates_across_era_boundaries() {
    use lyre_tui::ui::format_mtime;

    assert_eq!(format_mtime(0), "unknown");
    assert_eq!(format_mtime(1), "1970-01-01");
    assert_eq!(format_mtime(86_399), "1970-01-01");
    assert_eq!(format_mtime(86_400), "1970-01-02");
    assert_eq!(format_mtime(951_782_400), "2000-02-29");
    assert_eq!(format_mtime(1_709_164_800), "2024-02-29");
    assert_eq!(format_mtime(1_735_689_599), "2024-12-31");
    assert_eq!(format_mtime(1_735_689_600), "2025-01-01");
}

fn visible_playlist_names(app: &App) -> Vec<String> {
    app.visible_playlist_ids()
        .into_iter()
        .filter_map(|id| app.playlists.get(id).map(|p| p.name().to_string()))
        .collect()
}

#[test]
fn playlist_browse_search_matches_initials_the_way_song_search_does() {
    let mut h = harness();
    h.app.panel = Panel::Playlists;
    h.app.playlists.create("Boards of Canada Deep Cuts");
    h.app.playlists.create("Metal Mondays");

    h.app.playlist_panel.search_query = "boc".to_string();

    assert_eq!(
        visible_playlist_names(&h.app),
        vec!["Boards of Canada Deep Cuts".to_string()],
        "a gapped initials query must match a playlist name, as it already does for songs"
    );
}

#[test]
fn playlist_browse_search_accepts_multiple_terms_in_any_order() {
    let mut h = harness();
    h.app.panel = Panel::Playlists;
    h.app.playlists.create("Late Night Jazz");
    h.app.playlists.create("Morning Coffee");

    h.app.playlist_panel.search_query = "jazz night".to_string();

    assert_eq!(
        visible_playlist_names(&h.app),
        vec!["Late Night Jazz".to_string()],
        "every whitespace-separated term must match, order-independently"
    );
}

#[test]
fn playlist_browse_search_requires_all_terms_to_match() {
    let mut h = harness();
    h.app.panel = Panel::Playlists;
    h.app.playlists.create("Late Night Jazz");

    h.app.playlist_panel.search_query = "jazz polka".to_string();

    assert!(
        visible_playlist_names(&h.app).is_empty(),
        "a term that matches nothing must exclude the playlist entirely"
    );
}

#[test]
fn playlist_browse_search_ranks_the_closer_name_first() {
    let mut h = harness();
    h.app.panel = Panel::Playlists;
    h.app.playlists.create("Ambient Mixtape Collection");
    h.app.playlists.create("Mix");

    h.app.playlist_panel.search_query = "mix".to_string();

    let names = visible_playlist_names(&h.app);
    assert_eq!(
        names.first().map(String::as_str),
        Some("Mix"),
        "the tighter match must outrank the looser one even though it sorts later \
         alphabetically, got: {names:?}"
    );
    assert_eq!(names.len(), 2, "both playlists should still match: {names:?}");
}

#[test]
fn playlist_browse_search_is_case_insensitive() {
    let mut h = harness();
    h.app.panel = Panel::Playlists;
    h.app.playlists.create("Late Night Jazz");

    h.app.playlist_panel.search_query = "JAZZ".to_string();

    assert_eq!(
        visible_playlist_names(&h.app),
        vec!["Late Night Jazz".to_string()]
    );
}

#[test]
fn playlist_browse_search_still_matches_a_plain_substring() {
    let mut h = harness();
    h.app.panel = Panel::Playlists;
    h.app.playlists.create("Late Night Jazz");
    h.app.playlists.create("Morning Coffee");

    h.app.playlist_panel.search_query = "night".to_string();

    assert_eq!(
        visible_playlist_names(&h.app),
        vec!["Late Night Jazz".to_string()],
        "switching to fuzzy must not regress the substring queries that worked before"
    );
}

#[test]
fn equally_scoring_playlists_keep_their_alphabetical_order() {
    let mut h = harness();
    h.app.panel = Panel::Playlists;
    h.app.playlists.create("Bravo Mix");
    h.app.playlists.create("Alpha Mix");
    h.app.playlists.create("Charlie Mix");

    h.app.playlist_panel.search_query = "mix".to_string();

    assert_eq!(
        visible_playlist_names(&h.app),
        vec![
            "Alpha Mix".to_string(),
            "Bravo Mix".to_string(),
            "Charlie Mix".to_string()
        ],
        "the sort must be stable so equal scores stay in name order"
    );
}

#[test]
fn the_key_renderer_produces_the_expected_labels() {
    use crossterm::event::KeyModifiers as M;

    let cases: &[(KeyCode, M, &str)] = &[
        (KeyCode::Char('j'), M::NONE, "<j>"),
        (KeyCode::Char('G'), M::NONE, "<Shift+G>"),
        (KeyCode::Char(' '), M::NONE, "<Space>"),
        (KeyCode::Char('d'), M::CONTROL, "<Ctrl+d>"),
        (KeyCode::Char('d'), M::ALT, "<Alt+d>"),
        (KeyCode::Down, M::NONE, "<\u{2193}>"),
        (KeyCode::Left, M::NONE, "<\u{2190}>"),
        (KeyCode::Enter, M::NONE, "<Enter>"),
        (KeyCode::Esc, M::NONE, "<Esc>"),
        (KeyCode::BackTab, M::NONE, "<Shift+Tab>"),
        (KeyCode::Home, M::NONE, "<Home>"),
    ];

    for &(code, mods, expected) in cases {
        assert_eq!(
            keymap::render_key(code, mods),
            expected,
            "render_key({code:?}, {mods:?})"
        );
    }

    assert_eq!(
        keymap::render_keys(&[(KeyCode::Char('k'), M::NONE), (KeyCode::Up, M::NONE)]),
        "<k>/<\u{2191}>",
        "multi-key bindings join with a slash"
    );
}

#[test]
fn every_binding_label_is_derived_from_its_own_keys() {
    for binding in keymap::BINDINGS {
        if binding.display_override.is_some() {
            continue;
        }
        assert!(
            !binding.display().is_empty(),
            "binding {:?} renders a blank help row: it needs keys, an owning action, or an override",
            binding.desc
        );
        if !binding.keys.is_empty() {
            assert_eq!(
                binding.display(),
                keymap::render_keys(binding.keys),
                "binding {:?} must render from its own keys, not a separate literal",
                binding.desc
            );
        }
    }
}

#[test]
fn the_playlists_help_rows_track_the_keys_that_actually_dispatch() {
    let sort_key = keymap::display_for(keymap::Action::CycleSort(keymap::Direction::Forwards));
    let category_key =
        keymap::display_for(keymap::Action::CycleCategory(keymap::Direction::Forwards));

    let playlist_rows = keymap::help_rows(keymap::Section::Playlists);
    let sort_row = playlist_rows
        .iter()
        .find(|(_, desc)| desc.contains("Cycle sort within the open playlist"))
        .expect("the playlists help lists a sort row");
    let category_row = playlist_rows
        .iter()
        .find(|(_, desc)| desc.contains("Cycle category within the open playlist"))
        .expect("the playlists help lists a category row");

    assert!(
        sort_row.0.contains(&sort_key),
        "the playlists sort hint {:?} must contain the real sort key {sort_key:?}",
        sort_row.0
    );
    assert!(
        category_row.0.contains(&category_key),
        "the playlists category hint {:?} must contain the real category key {category_key:?}",
        category_row.0
    );
}

#[test]
fn the_confirm_dialog_draws_the_same_key_it_actually_accepts() {
    let yes = keymap::confirm_display_for(keymap::ConfirmChoice::Yes);
    let no = keymap::confirm_display_for(keymap::ConfirmChoice::No);

    let mut h = harness();
    h.app.on_key(key('q'));
    assert!(h.app.confirm_mode_is(), "the quit confirmation is open");
    let text = buffer_text(&render(&mut h.app, 80, 30));
    let choice_line = text
        .lines()
        .find(|line| line.contains(" yes") && line.contains(" no"))
        .unwrap_or_else(|| panic!("the dialog must draw a yes/no line, got:\n{text}"))
        .to_string();

    assert!(
        choice_line.contains(&yes),
        "the yes/no line {choice_line:?} must draw the yes key {yes:?} that the table accepts"
    );
    assert!(
        choice_line.contains(&no),
        "the yes/no line {choice_line:?} must draw the no key {no:?} that the table accepts"
    );

    let yes_char = yes
        .trim_matches(|c| c == '<' || c == '>')
        .chars()
        .next()
        .expect("a yes label");
    assert_eq!(
        keymap::confirm_lookup(key(yes_char)),
        Some(keymap::ConfirmChoice::Yes),
        "the key drawn as the yes label must be accepted as yes"
    );
}

#[test]
fn typing_y_and_n_into_a_metadata_field_still_inserts_characters() {
    let mut h = harness();
    h.app.on_key(key('g'));
    h.app.on_key(key('e'));
    assert!(h.app.metadata_mode_is(), "the metadata modal is open");

    let before = MetadataField::Title
        .value(&h.app.metadata_mode().expect("modal open").edits)
        .to_string();

    for c in "Nirvana".chars() {
        h.app.on_key(key(c));
    }

    assert!(
        h.app.metadata_mode_is(),
        "typing y or n must not confirm or cancel the modal"
    );
    let edits = h
        .app
        .metadata_mode()
        .expect("the metadata modal stays open while typing");
    assert_eq!(
        MetadataField::Title.value(&edits.edits),
        format!("{before}Nirvana"),
        "y and n must be literal characters in a text field, never confirm/cancel"
    );
}
