use ratatui::widgets::ListState;

use lyre_core::{FuzzyQuery, PlaylistId, SongId};

use crate::keymap::{self, Action, Direction};
use crate::strings;

use super::App;
use super::state::{
    Category, Panel, PlaylistView, QueueSource, Row, Sort, StatusKind, VisualSelection,
    is_filtering,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Selected {
    Found,
    NotFound,
}

fn row_to_song_id(row: &Row) -> Option<SongId> {
    match row {
        Row::Song(id, _) => Some(*id),
        Row::Header(_) => None,
    }
}

impl App {
    pub fn queue_source(&self) -> QueueSource {
        self.queue_source
    }

    fn playlist_browse_focused(&self) -> bool {
        self.panel == Panel::Playlists && self.playlist_panel.view == PlaylistView::Browsing
    }

    pub(super) fn active_list_state_mut(&mut self) -> &mut ListState {
        match self.panel {
            Panel::Library => &mut self.library_panel.list_state,
            Panel::Playlists => &mut self.playlist_panel.list_state,
        }
    }

    pub(super) fn active_list_state(&self) -> &ListState {
        match self.panel {
            Panel::Library => &self.library_panel.list_state,
            Panel::Playlists => &self.playlist_panel.list_state,
        }
    }

    pub(super) fn active_page_height(&self) -> usize {
        match self.panel {
            Panel::Library => self.measured.library_page_height,
            Panel::Playlists => self.measured.playlist_page_height,
        }
    }

    pub fn active_visual_for_test(&self) -> bool {
        self.active_visual().is_some()
    }

    pub fn active_selection_for_test(&self) -> Option<usize> {
        self.active_list_state().selected()
    }

    pub fn active_offset_for_test(&self) -> usize {
        self.active_list_state().offset()
    }

    pub(super) fn active_visual(&self) -> Option<VisualSelection> {
        match self.panel {
            Panel::Library => self.library_panel.visual,
            Panel::Playlists => self.playlist_panel.visual,
        }
    }

    pub(super) fn active_visual_mut(&mut self) -> &mut Option<VisualSelection> {
        match self.panel {
            Panel::Library => &mut self.library_panel.visual,
            Panel::Playlists => &mut self.playlist_panel.visual,
        }
    }

    pub fn visual_row_range(&self) -> Option<(usize, usize)> {
        let visual = self.active_visual()?;
        let rows = self.rows_slice();
        let anchor_idx = rows
            .iter()
            .position(|r| matches!(r, Row::Song(id, _) if *id == visual.anchor))?;
        let cursor_idx = self.active_list_state().selected()?;
        Some(if anchor_idx <= cursor_idx {
            (anchor_idx, cursor_idx)
        } else {
            (cursor_idx, anchor_idx)
        })
    }

    pub(super) fn cancel_visual_select(&mut self) {
        self.library_panel.visual = None;
        self.playlist_panel.visual = None;
    }

    pub(super) fn toggle_visual_select(&mut self) {
        if self.playlist_browse_focused() {
            self.set_status(
                "visual selection isn't available while browsing playlists",
                StatusKind::Info,
            );
            return;
        }

        if self.active_visual_mut().take().is_some() {
            self.set_status("cancelled visual selection", StatusKind::Info);
            return;
        }

        let Some(Row::Song(id, _)) = self.selected_row() else {
            self.set_status(strings::SELECT_SONG_FIRST, StatusKind::Info);
            return;
        };
        *self.active_visual_mut() = Some(VisualSelection { anchor: id });
        self.set_status(
            format!(
                "visual selection started — move the cursor, then press {}",
                keymap::display_for(Action::OpenSongModal)
            ),
            StatusKind::Info,
        );
    }

    pub(super) fn selected_song_ids(&mut self) -> Vec<SongId> {
        self.visible_rows();
        if self.visual_row_range().is_none() {
            self.cancel_visual_select();
            return self
                .selected_row()
                .into_iter()
                .filter_map(|r| row_to_song_id(&r))
                .collect();
        }

        let (start, end) = self.visual_row_range().unwrap_or((0, 0));
        self.rows_slice()
            .get(start..=end)
            .into_iter()
            .flatten()
            .filter_map(row_to_song_id)
            .collect()
    }

    pub fn visible_playlist_ids(&self) -> Vec<PlaylistId> {
        if !is_filtering(&self.playlist_panel.search_query) {
            return self.playlists.ids_sorted_by_name().to_vec();
        }
        let query = FuzzyQuery::new(&self.playlist_panel.search_query);

        let mut scored: Vec<(PlaylistId, u32)> = self
            .playlists
            .ids_sorted_by_name()
            .iter()
            .copied()
            .filter_map(|id| {
                let playlist = self.playlists.get(id)?;
                playlist.fuzzy_score(&query).map(|score| (id, score))
            })
            .collect();

        scored.sort_by_key(|&(_, score)| std::cmp::Reverse(score));
        scored.into_iter().map(|(id, _)| id).collect()
    }

    pub(super) fn move_selection(&mut self, delta: isize) {
        if self.playlist_browse_focused() {
            self.move_playlist_browse_selection(delta);
            return;
        }

        let len = self.visible_rows().len();
        if len == 0 {
            self.active_list_state_mut().select(None);
            return;
        }

        let start = self.active_list_state().selected().unwrap_or(0);
        let rows = self.rows_slice();
        let target = wrapping_selectable_index(start, delta, len, |i| {
            matches!(rows.get(i), Some(Row::Song(_, _)))
        });
        self.active_list_state_mut().select(Some(target));
    }

    pub(super) fn move_playlist_browse_selection(&mut self, delta: isize) {
        let len = self.visible_playlist_ids().len();
        if len == 0 {
            self.playlist_panel.list_state.select(None);
            return;
        }
        move_wrapping(&mut self.playlist_panel.list_state, len, delta);
    }

    pub(super) fn jump_page(&mut self, direction: Direction) {
        if self.playlist_browse_focused() {
            self.jump_playlist_browse_page(direction);
            return;
        }

        let len = self.visible_rows().len();
        if len == 0 {
            self.active_list_state_mut().select(None);
            return;
        }
        let height = self.active_page_height();
        let offset = self.active_list_state().offset();

        let (new_offset, target) = {
            let rows = self.rows_slice();
            let is_selectable = |i: usize| matches!(rows.get(i), Some(Row::Song(_, _)));
            compute_jump(offset, len, height, direction, is_selectable)
        };

        let state = self.active_list_state_mut();
        *state.offset_mut() = new_offset;
        state.select(Some(target));
    }

    fn jump_playlist_browse_page(&mut self, direction: Direction) {
        let len = self.visible_playlist_ids().len();
        if len == 0 {
            self.playlist_panel.list_state.select(None);
            return;
        }
        let height = self.measured.playlist_page_height;
        let offset = self.playlist_panel.list_state.offset();

        let (new_offset, target) = compute_jump(offset, len, height, direction, |_| true);

        *self.playlist_panel.list_state.offset_mut() = new_offset;
        self.playlist_panel.list_state.select(Some(target));
    }

    pub(super) fn select_first_row(&mut self) {
        if self.playlist_browse_focused() {
            let len = self.visible_playlist_ids().len();
            self.playlist_panel
                .list_state
                .select(if len == 0 { None } else { Some(0) });
            return;
        }
        let target = self
            .visible_rows()
            .iter()
            .position(|r| matches!(r, Row::Song(_, _)));
        self.active_list_state_mut().select(target);
    }

    pub(super) fn select_last_row(&mut self) {
        if self.playlist_browse_focused() {
            let last = self.visible_playlist_ids().len().checked_sub(1);
            self.playlist_panel.list_state.select(last);
            return;
        }
        let target = self
            .visible_rows()
            .iter()
            .rposition(|r| matches!(r, Row::Song(_, _)));
        self.active_list_state_mut().select(target);
    }

    pub fn selected_row(&mut self) -> Option<Row> {
        let i = self.active_list_state().selected()?;
        self.visible_rows().get(i).cloned()
    }

    fn rows_slice(&self) -> &[Row] {
        self.rows.rows_unchecked()
    }

    pub(super) fn selected_playlist_id(&self) -> Option<PlaylistId> {
        let ids = self.visible_playlist_ids();
        let i = self.playlist_panel.list_state.selected()?;
        ids.get(i).copied()
    }

    pub(super) fn reset_playlist_browse_selection(&mut self) {
        self.playlist_panel.list_state = ListState::default();
        if !self.visible_playlist_ids().is_empty() {
            self.playlist_panel.list_state.select(Some(0));
        }
    }

    pub(super) fn sync_playlist_browse_selection(&mut self) {
        let len = self.visible_playlist_ids().len();
        if len == 0 {
            self.playlist_panel.list_state.select(None);
            return;
        }
        let start = match self.playlist_panel.list_state.selected() {
            Some(i) if i < len => i,
            _ => 0,
        };
        self.playlist_panel.list_state.select(Some(start));
    }

    pub(super) fn sync_playlist_selection(&mut self) {
        match self.playlist_panel.view {
            PlaylistView::Browsing => self.sync_playlist_browse_selection(),
            PlaylistView::Viewing(_) => self.sync_selection_to_rows(),
        }
    }

    pub(super) fn jump_to_current(&mut self) {
        let Some(current) = self.queue.current_id() else {
            self.set_status(strings::NOTHING_PLAYING, StatusKind::Info);
            return;
        };

        if self.select_song_by_id(current) == Selected::NotFound {
            self.set_status(
                "now playing isn't in the current view — clear the search to find it",
                StatusKind::Info,
            );
        }
    }

    pub(super) fn select_song_by_id(&mut self, id: SongId) -> Selected {
        match self
            .visible_rows()
            .iter()
            .position(|row| matches!(row, Row::Song(row_id, _) if *row_id == id))
        {
            Some(i) => {
                self.active_list_state_mut().select(Some(i));
                Selected::Found
            }
            None => Selected::NotFound,
        }
    }

    pub(super) fn sync_selection_to_rows(&mut self) {
        let len = self.visible_rows().len();
        if len == 0 {
            self.active_list_state_mut().select(None);
            return;
        }

        let start = match self.active_list_state().selected() {
            Some(i) if i < len => i,
            _ => 0,
        };

        let landing = nearest_song_row(self.rows_slice(), start);
        self.active_list_state_mut().select(Some(landing));
    }

    pub(super) fn cycle_category(&mut self, direction: Direction) {
        self.cancel_visual_select();
        match self.panel {
            Panel::Library => self.cycle_field(
                direction,
                "grouped by",
                |app| &mut app.library_panel.category,
                Category::next,
                Category::prev,
                |c: Category| c.label(),
            ),
            Panel::Playlists if matches!(self.playlist_panel.view, PlaylistView::Viewing(_)) => {
                self.cycle_field(
                    direction,
                    "grouped by",
                    |app| &mut app.playlist_panel.category,
                    Category::next,
                    Category::prev,
                    |c: Category| c.label(),
                )
            }
            Panel::Playlists => {}
        }
    }

    pub(super) fn cycle_sort(&mut self, direction: Direction) {
        self.cancel_visual_select();
        match self.panel {
            Panel::Library => self.cycle_field(
                direction,
                "sorted by",
                |app| &mut app.library_panel.sort,
                Sort::next,
                Sort::prev,
                |c: Sort| c.label(),
            ),
            Panel::Playlists if matches!(self.playlist_panel.view, PlaylistView::Viewing(_)) => {
                self.cycle_field(
                    direction,
                    "sorted by",
                    |app| &mut app.playlist_panel.sort,
                    Sort::next,
                    Sort::prev,
                    |c: Sort| c.label(),
                )
            }
            Panel::Playlists => {}
        }
    }

    fn cycle_field<T: Copy>(
        &mut self,
        direction: Direction,
        verb: &str,
        field: impl FnOnce(&mut Self) -> &mut T,
        next: impl FnOnce(T) -> T,
        prev: impl FnOnce(T) -> T,
        label: impl FnOnce(T) -> &'static str,
    ) {
        let slot = field(self);
        *slot = match direction {
            Direction::Forwards => next(*slot),
            Direction::Backwards => prev(*slot),
        };
        let updated = *slot;
        self.sync_selection_to_rows();
        self.set_status(format!("{verb} {}", label(updated)), StatusKind::Info);
    }

    pub(super) fn cycle_library_playlist_mode(&mut self) {
        if self.panel != Panel::Library {
            return;
        }
        self.library_panel.playlist_mode = self.library_panel.playlist_mode.cycle();
        self.set_status(
            format!("playlists: {}", self.library_panel.playlist_mode.label()),
            StatusKind::Info,
        );
    }
}

fn nearest_song_row(rows: &[Row], start: usize) -> usize {
    #[allow(clippy::indexing_slicing)]
    if matches!(rows[start], Row::Song(_, _)) {
        return start;
    }
    rows.iter()
        .enumerate()
        .skip(start)
        .find(|(_, r)| matches!(r, Row::Song(_, _)))
        .or_else(|| {
            rows.iter()
                .enumerate()
                .find(|(_, r)| matches!(r, Row::Song(_, _)))
        })
        .map(|(i, _)| i)
        .unwrap_or(start)
}

fn wrapping_selectable_index(
    start: usize,
    delta: isize,
    len: usize,
    is_selectable: impl Fn(usize) -> bool,
) -> usize {
    let len = len as isize;
    let start = start as isize;
    let mut idx = start;
    loop {
        idx = (idx + delta).rem_euclid(len);
        if idx == start || is_selectable(idx as usize) {
            return idx as usize;
        }
    }
}

pub(super) fn move_wrapping(state: &mut ListState, len: usize, delta: isize) {
    if len == 0 {
        return;
    }
    let start = state.selected().unwrap_or(0) as isize;
    let idx = (start + delta).rem_euclid(len as isize);
    state.select(Some(idx as usize));
}

fn compute_jump(
    offset: usize,
    len: usize,
    height: usize,
    direction: Direction,
    is_selectable: impl Fn(usize) -> bool,
) -> (usize, usize) {
    let height = height.max(1);
    let offset = offset.min(len.saturating_sub(1));
    let center = height / 2;

    let last_visible = (offset + height - 1).min(len - 1);
    let max_offset = len.saturating_sub(height.min(len));

    let anchor = match direction {
        Direction::Forwards => (offset..=last_visible)
            .rev()
            .find(|&i| is_selectable(i)),
        Direction::Backwards => (offset..=last_visible).find(|&i| is_selectable(i)),
    };

    let Some(anchor) = anchor else {
        return (
            offset,
            nearest_selectable(offset, len, &is_selectable).unwrap_or(0),
        );
    };

    let new_offset = anchor.saturating_sub(center).min(max_offset);
    (new_offset, anchor)
}

fn nearest_selectable(
    idx: usize,
    len: usize,
    is_selectable: &impl Fn(usize) -> bool,
) -> Option<usize> {
    if len == 0 {
        return None;
    }
    let idx = idx.min(len - 1);
    for radius in 0..len {
        if idx + radius < len && is_selectable(idx + radius) {
            return Some(idx + radius);
        }
        if radius <= idx && is_selectable(idx - radius) {
            return Some(idx - radius);
        }
    }
    None
}
