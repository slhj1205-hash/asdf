use ratatui::widgets::ListState;

use lyre_core::{PlaylistId, SongId};

use super::App;
use super::modes::Mode;
use super::state::{ChooseActionField, Panel, PlaylistView, PlaylistPicker, SongModal};

impl App {
    pub(super) fn set_song_modal(
        &mut self,
        song: lyre_core::SongId,
        batch: Option<Vec<SongId>>,
        selected: ChooseActionField,
        name_input: String,
        picker: Option<PlaylistPicker>,
    ) {
        self.modes.replace(Mode::SongModal(SongModal {
            song,
            batch,
            selected,
            name_input,
            picker,
        }));
    }

    pub(super) fn open_song_modal(&mut self) {
        let selected = if self.playlists.is_empty() {
            ChooseActionField::CreatePlaylist
        } else {
            ChooseActionField::AddToPlaylist
        };

        if self.active_visual().is_some() {
            let ids = self.selected_song_ids();
            self.cancel_visual_select();
            if let [first, ..] = ids.as_slice()
                && ids.len() > 1
            {
                self.set_song_modal(*first, Some(ids), selected, String::new(), None);
                return;
            }
        }

        let Some(id) = self.selected_song_or_warn() else {
            return;
        };
        self.set_song_modal(id, None, selected, String::new(), None);
    }

    pub(super) fn open_remove_confirm(&mut self) {
        if self.panel != Panel::Playlists {
            return;
        }
        let PlaylistView::Viewing(playlist_id) = self.playlist_panel.view else {
            return;
        };

        let Some(song_id) = self.selected_song_or_warn() else {
            return;
        };
        self.modes.open(Mode::ConfirmRemove(playlist_id, song_id));
    }

    pub fn open_remove_confirm_for_test(&mut self, playlist_id: PlaylistId, song_id: SongId) {
        self.modes.open(Mode::ConfirmRemove(playlist_id, song_id));
    }

    pub(super) fn toggle_panel(&mut self) {
        self.cancel_visual_select();
        self.panel = match self.panel {
            Panel::Library => Panel::Playlists,
            Panel::Playlists => Panel::Library,
        };
    }

    pub(super) fn build_add_to_playlist_picker(
        &self,
        song: lyre_core::SongId,
        batch: Option<&[SongId]>,
    ) -> Option<PlaylistPicker> {
        let currently_viewing = if let (Panel::Playlists, PlaylistView::Viewing(id)) =
            (self.panel, self.playlist_panel.view)
        {
            Some(id)
        } else {
            None
        };
        let songs: Vec<SongId> = batch.map(<[SongId]>::to_vec).unwrap_or_else(|| vec![song]);
        add_to_playlist_picker(&self.playlists, currently_viewing, &songs)
    }

    pub(super) fn build_remove_from_playlist_picker(
        &self,
        song: SongId,
        batch: Option<&[SongId]>,
    ) -> Option<PlaylistPicker> {
        let songs: Vec<SongId> = batch.map(<[SongId]>::to_vec).unwrap_or_else(|| vec![song]);
        remove_from_playlist_picker(&self.playlists, &songs)
    }

    pub(super) fn visible_choose_action_fields(
        &self,
        song: SongId,
        batch: Option<&[SongId]>,
    ) -> Vec<ChooseActionField> {
        let songs: Vec<SongId> = batch.map(<[SongId]>::to_vec).unwrap_or_else(|| vec![song]);
        ChooseActionField::ALL
            .iter()
            .copied()
            .filter(|field| match field {
                ChooseActionField::AddToPlaylist => !self.playlists.is_empty(),
                ChooseActionField::RemoveFromPlaylist => songs
                    .iter()
                    .any(|&id| !self.playlists.containing(id).is_empty()),
                ChooseActionField::CreatePlaylist => true,
            })
            .collect()
    }
}

fn add_to_playlist_picker(
    playlists: &lyre_core::PlaylistStore,
    currently_viewing: Option<PlaylistId>,
    songs: &[SongId],
) -> Option<PlaylistPicker> {
    let mut pinned: Vec<PlaylistId> = currently_viewing.into_iter().collect();

    for &id in playlists.ids_sorted_by_name() {
        if pinned.contains(&id) {
            continue;
        }
        if songs.iter().all(|&song| playlists.contains(id, song)) {
            pinned.push(id);
        }
    }

    let mut options = playlists.ids_sorted_by_name().to_vec();
    options.retain(|id| !pinned.contains(id));

    if options.is_empty() {
        return None;
    }

    let mut list_state = ListState::default();
    list_state.select(Some(0));

    Some(PlaylistPicker::Add {
        options,
        pinned,
        list_state,
    })
}

fn remove_from_playlist_picker(
    playlists: &lyre_core::PlaylistStore,
    songs: &[SongId],
) -> Option<PlaylistPicker> {
    let options: Vec<PlaylistId> = playlists
        .ids_sorted_by_name()
        .iter()
        .copied()
        .filter(|&id| songs.iter().any(|&song| playlists.contains(id, song)))
        .collect();

    if options.is_empty() {
        return None;
    }

    let mut list_state = ListState::default();
    list_state.select(Some(0));

    Some(PlaylistPicker::Remove {
        options,
        list_state,
    })
}
