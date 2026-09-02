use lyre_core::MetadataEdits;

use crate::strings::{self, plural};
use super::App;
use super::modes::Mode;
use super::state::{
    MetadataEditModal, MetadataField, RomanizedArtistConfirmModal, Row, StatusKind,
    heading_selected_message,
};

impl App {
    pub(super) fn set_metadata_modal(&mut self, modal: MetadataEditModal) {
        self.modes.replace(Mode::MetadataEdit(modal));
    }

    pub(super) fn open_metadata_modal(&mut self) {
        match self.selected_row() {
            Some(Row::Song(id, _)) => {
                let Some(song) = self.library.get(id) else {
                    self.set_status(
                        "selected song is no longer in the library",
                        StatusKind::Error,
                    );
                    return;
                };
                let edits = MetadataEdits::from_metadata(song.metadata());
                let original_artist_sort = edits.artist_sort.clone();
                self.set_metadata_modal(MetadataEditModal {
                    song: id,
                    edits,
                    original_artist_sort,
                    focused: MetadataField::Title,
                    error: None,
                });
            }
            Some(Row::Header(heading)) => {
                self.set_status(heading_selected_message(&heading), StatusKind::Info);
            }
            None => self.set_status(strings::SELECT_SONG_FIRST, StatusKind::Info),
        }
    }

    pub(super) fn save_metadata_edit_and_prompt(&mut self, modal: MetadataEditModal) {
        let MetadataEditModal {
            song,
            edits,
            original_artist_sort,
            focused,
            ..
        } = modal;

        match self.library.update_metadata(song, &edits) {
            Ok(()) => {
                self.library_revision += 1;

                let label = self
                    .library
                    .get(song)
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| strings::UNTITLED_SONG.to_string());
                self.set_status(format!("updated metadata for {label}"), StatusKind::Success);
                self.select_song_by_id(song);

                self.maybe_prompt_romanized_artist(
                    song,
                    &edits.artist_sort,
                    &original_artist_sort,
                );
            }
            Err(e) => {
                self.set_metadata_modal(MetadataEditModal {
                    song,
                    edits,
                    original_artist_sort,
                    focused,
                    error: Some(e.to_string()),
                });
            }
        }
    }

    pub(super) fn maybe_prompt_romanized_artist(
        &mut self,
        saved_song: lyre_core::SongId,
        artist_sort: &str,
        original: &str,
    ) {
        let value = artist_sort.trim();
        if value.is_empty() || value == original.trim() {
            return;
        }
        let Some(reference) = self.library.get(saved_song) else {
            return;
        };
        let artist_sort_key = reference.sort_artist().to_string();
        let artist_display = reference.artist().to_string();

        let count = self
            .library
            .count_matching_artist(&artist_sort_key, saved_song);
        if count == 0 {
            return;
        }

        self.modes.open(Mode::RomanizedArtistConfirm(RomanizedArtistConfirmModal {
            artist_display,
            artist_sort_key,
            value: value.to_string(),
            reference_song: saved_song,
            count,
        }));
    }

    pub(super) fn confirm_romanized_artist_apply(&mut self, confirm: RomanizedArtistConfirmModal) {
        let updated = self.library.update_artist_sort_for_matching(
            &confirm.artist_sort_key,
            &confirm.value,
            confirm.reference_song,
        );

        if !updated.is_empty() {
            self.library_revision += 1;
        }

        let applied = updated.len();
        if applied == 0 {
            self.set_status("no other songs needed the romanized artist", StatusKind::Info);
            return;
        }
        self.set_status(
            format!(
                "applied romanized artist to {applied} other song{}",
                plural(applied, "s")
            ),
            StatusKind::Success,
        );
    }
}
