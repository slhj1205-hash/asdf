use std::{cmp::Ordering, path::Path};

use lyre_core::{FuzzyQuery, Song, SongId};

use super::App;
use super::state::{Category, Panel, PlaylistView, Row, Sort, is_filtering};

#[derive(Default)]
pub struct RowCache {
    rows: Vec<Row>,
    key: Option<RowsKey>,
}

struct RowsKey {
    panel: Panel,
    view: PlaylistView,
    category: Category,
    sort: Sort,
    query: String,
    library_revision: u64,
    playlists_revision: u64,
}

impl RowCache {
    pub fn invalidate(&mut self) {
        self.key = None;
    }
}

impl App {
    fn active_panel_songs(&self) -> Vec<&Song> {
        match self.panel {
            Panel::Library => self.library.songs_by_path().collect(),
            Panel::Playlists => match self.playlist_panel.view {
                PlaylistView::Browsing => Vec::new(),
                PlaylistView::Viewing(id) => match self.playlists.get(id) {
                    Some(playlist) => playlist
                        .songs()
                        .iter()
                        .filter_map(|&song| self.library.get(song))
                        .collect(),
                    None => Vec::new(),
                },
            },
        }
    }

    fn active_panel_grouping(&self) -> (Category, Sort) {
        match self.panel {
            Panel::Library => (self.library_panel.category, self.library_panel.sort),
            Panel::Playlists => (self.playlist_panel.category, self.playlist_panel.sort),
        }
    }

    fn active_search_query(&self) -> &str {
        match self.panel {
            Panel::Library => &self.library_panel.search_query,
            Panel::Playlists => &self.playlist_panel.search_query,
        }
    }

    fn active_playlist_view(&self) -> PlaylistView {
        match self.panel {
            Panel::Library => PlaylistView::Browsing,
            Panel::Playlists => self.playlist_panel.view,
        }
    }

    pub(super) fn queue_order(&self) -> Vec<SongId> {
        let mut songs = self.active_panel_songs();
        let (category, sort) = self.active_panel_grouping();
        sort_songs(&mut songs, category, sort, self.library.root());
        songs.iter().map(|song| song.id()).collect()
    }

    fn row_context(&self) -> (Panel, PlaylistView, Category, Sort, u64, u64) {
        let (category, sort) = self.active_panel_grouping();
        (
            self.panel,
            self.active_playlist_view(),
            category,
            sort,
            self.library_revision,
            self.playlists.revision(),
        )
    }

    pub fn visible_rows(&mut self) -> &[Row] {
        let (panel, view, category, sort, library_revision, playlists_revision) =
            self.row_context();
        let query = self.active_search_query();

        let matches = self.rows.key.as_ref().is_some_and(|key| {
            key.panel == panel
                && key.view == view
                && key.category == category
                && key.sort == sort
                && key.query == query
                && key.library_revision == library_revision
                && key.playlists_revision == playlists_revision
        });

        if !matches {
            let key = RowsKey {
                panel,
                view,
                category,
                sort,
                query: query.to_string(),
                library_revision,
                playlists_revision,
            };
            let mut buffer = std::mem::take(&mut self.rows.rows);
            buffer.clear();
            self.build_rows_into(&mut buffer);
            self.rows.rows = buffer;
            self.rows.key = Some(key);
        }
        &self.rows.rows
    }

    pub fn visible_song_count(&mut self) -> usize {
        super::state::song_row_count(self.visible_rows())
    }

    pub fn visible_song_count_for_test(&mut self) -> usize {
        self.visible_song_count()
    }

    fn build_rows_into(&self, out: &mut Vec<Row>) {
        let songs = self.active_panel_songs();
        let raw_query = self.active_search_query();

        if is_filtering(raw_query) {
            let query = FuzzyQuery::new(raw_query);
            build_relevance_rows(fuzzy_filter_and_sort(songs.into_iter(), &query), out);
        } else {
            let (category, sort) = self.active_panel_grouping();
            build_rows(songs, category, sort, self.library.root(), out);
        }
    }
}

fn fuzzy_filter_and_sort<'a>(
    songs: impl Iterator<Item = &'a Song>,
    query: &FuzzyQuery,
) -> Vec<&'a Song> {
    let mut scored: Vec<(&Song, u32)> = songs
        .filter_map(|song| song.fuzzy_score(query).map(|score| (song, score)))
        .collect();
    scored.sort_unstable_by(|a, b| {
        b.1.cmp(&a.1)
            .then_with(|| a.0.sort_title().cmp(b.0.sort_title()))
            .then_with(|| a.0.sort_artist().cmp(b.0.sort_artist()))
            .then_with(|| a.0.path().cmp(b.0.path()))
    });
    scored.into_iter().map(|(song, _)| song).collect()
}

fn build_relevance_rows(songs: Vec<&Song>, rows: &mut Vec<Row>) {
    rows.reserve(songs.len());
    rows.extend(songs.into_iter().map(|s| Row::Song(s.id(), 0)));
}

fn sort_comparator(sort: Sort) -> impl Fn(&Song, &Song) -> Ordering {
    move |a, b| match sort {
        Sort::Title => a.sort_title().cmp(b.sort_title()),
        Sort::Duration => a
            .metadata()
            .duration
            .cmp(&b.metadata().duration)
            .then_with(|| a.sort_title().cmp(b.sort_title())),
        Sort::Artist => a
            .sort_artist()
            .cmp(b.sort_artist())
            .then_with(|| a.sort_title().cmp(b.sort_title())),
        Sort::Path => a.path().cmp(b.path()),
        Sort::DateModified => b
            .modified()
            .cmp(&a.modified())
            .then_with(|| a.path().cmp(b.path())),
    }
}

fn relative_parent<'a>(song: &'a Song, root: &Path) -> &'a Path {
    let rel = song.path().strip_prefix(root).unwrap_or(song.path());
    rel.parent().unwrap_or(Path::new(""))
}

pub fn group_label(song: &Song, category: Category, root: &Path) -> Option<String> {
    match category {
        Category::None => None,
        Category::Artist => Some(song.artist().to_string()),
        Category::Path => {
            let rel = relative_parent(song, root);
            if rel.as_os_str().is_empty() {
                return None;
            }
            rel.components()
                .next_back()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
        }
    }
}

fn sort_songs(songs: &mut [&Song], category: Category, sort: Sort, root: &Path) {
    let within_group = sort_comparator(sort);

    match category {
        Category::None => songs.sort_unstable_by(|a, b| within_group(a, b)),
        Category::Artist => songs.sort_unstable_by(|a, b| {
            a.sort_artist()
                .cmp(b.sort_artist())
                .then_with(|| within_group(a, b))
        }),
        Category::Path => songs.sort_unstable_by(|a, b| {
            relative_parent(a, root)
                .cmp(relative_parent(b, root))
                .then_with(|| within_group(a, b))
        }),
    }
}

fn build_rows(
    mut songs: Vec<&Song>,
    category: Category,
    sort: Sort,
    root: &Path,
    rows: &mut Vec<Row>,
) {
    rows.reserve(songs.len());
    sort_songs(&mut songs, category, sort, root);

    match category {
        Category::None => {
            rows.extend(songs.into_iter().map(|s| Row::Song(s.id(), 0)));
        }
        Category::Artist => {
            let mut last_artist: Option<&str> = None;
            for song in songs {
                if last_artist != Some(song.artist()) {
                    rows.push(Row::Header(song.artist().to_string()));
                    last_artist = Some(song.artist());
                }
                rows.push(Row::Song(song.id(), 0));
            }
        }
        Category::Path => {
            let mut last_dirs: Vec<String> = Vec::new();
            for song in songs {
                let comps: Vec<_> = relative_parent(song, root).components().collect();

                let shared_depth = comps
                    .iter()
                    .zip(last_dirs.iter())
                    .take_while(|(c, s)| c.as_os_str().to_str() == Some(s.as_str()))
                    .count();

                last_dirs.truncate(shared_depth);

                for (depth, comp) in comps.iter().enumerate().skip(shared_depth) {
                    let name = comp.as_os_str().to_string_lossy().into_owned();
                    let mut header = String::with_capacity(depth * 2 + name.len());
                    for _ in 0..depth {
                        header.push_str("  ");
                    }
                    header.push_str(&name);
                    rows.push(Row::Header(header));
                    last_dirs.push(name);
                }

                rows.push(Row::Song(song.id(), comps.len()));
            }
        }
    }
}

impl RowCache {
    pub fn rows_unchecked(&self) -> &[Row] {
        &self.rows
    }
}
