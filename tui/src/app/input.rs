use crossterm::event::{KeyCode, KeyEvent};

use crate::keymap::{ConfirmChoice, ModalKey, confirm_lookup, modal_lookup};
use crate::strings;

use lyre_core::{Mutated, SongId};

use super::playback::SEEK_STEP_SECS;
use super::App;
use super::command::{Command, command_for_key};
use super::form::{FormFieldOutcome, handle_form_field_key};
use super::modes::{Mode, SearchTarget};
use super::state::{
    MetadataEditModal, Panel, PlaylistView, RomanizedArtistConfirmModal, StatusKind, is_filtering,
};

impl App {
    pub(super) fn handle_key(&mut self, key: KeyEvent) {
        if let Some(mode) = self.modes.take() {
            self.handle_mode_key(mode, key);
            return;
        }

        let had_pending_number = !self.pending_number.is_empty();
        let is_digit = matches!(key.code, KeyCode::Char(c) if c.is_ascii_digit());
        if !is_digit && key.code != KeyCode::Char('n') {
            self.pending_number.clear();
        }

        const MAX_PENDING_NUMBER_DIGITS: usize = 3;

        if is_digit {
            if let KeyCode::Char(c) = key.code
                && self.pending_number.len() < MAX_PENDING_NUMBER_DIGITS
            {
                self.pending_number.push(c);
                self.set_status(
                    format!(
                        "jump to Up Next #{} (press {}, <Esc> to cancel)",
                        self.pending_number,
                        crate::keymap::display_for(crate::keymap::Action::NextOrJump)
                    ),
                    StatusKind::Info,
                );
            }
            return;
        }

        if let Some(command) = command_for_key(self, key, had_pending_number) {
            self.run_command(command);
        }
    }

    fn handle_mode_key(&mut self, mode: Mode, key: KeyEvent) {
        match mode {
            Mode::ConfirmQuit => self.handle_confirm_quit_key(key),
            Mode::ConfirmRemove(playlist_id, song_id) => {
                self.handle_confirm_remove_key(key, playlist_id, song_id)
            }
            Mode::Help => {}
            Mode::SongModal(modal) => self.handle_song_modal_key(key, modal),
            Mode::MetadataEdit(modal) => self.handle_metadata_modal_key(key, modal),
            Mode::RomanizedArtistConfirm(confirm) => {
                self.handle_romanized_artist_confirm_key(key, confirm)
            }
            Mode::Youtube(modal) => self.handle_youtube_modal_key(key, modal),
            Mode::ChangeDirectory => self.handle_dir_input_key(key, mode),
            Mode::SearchLibrary | Mode::SearchPlaylists => {
                let target = mode.search_target().unwrap_or(SearchTarget::Library);
                self.handle_search_key(key, target);
            }
        }
    }

    fn run_command(&mut self, command: Command) {
        match command {
            Command::TogglePanel => self.toggle_panel(),
            Command::Move(rows) => self.move_selection(rows),
            Command::JumpPage(direction) => self.jump_page(direction),
            Command::JumpTop => self.select_first_row(),
            Command::JumpBottom => self.select_last_row(),
            Command::JumpToCurrent => self.jump_to_current(),
            Command::Activate => self.activate_selected(),
            Command::TogglePlayback => {
                if let Err(e) = self.player.toggle() {
                    self.set_status(format!("playback error: {e}"), StatusKind::Error);
                }
            }
            Command::NextTrack => self.advance(),
            Command::JumpToUpcoming => self.jump_to_upcoming(),
            Command::PreviousTrack => self.go_back(),
            Command::SeekBack => self.seek_current(-SEEK_STEP_SECS),
            Command::SeekForward => self.seek_current(SEEK_STEP_SECS),
            Command::QueueNext => self.queue_selected_next(),
            Command::OpenSongModal => self.open_song_modal(),
            Command::OpenMetadataEdit => self.open_metadata_modal(),
            Command::OpenYoutube => self.open_youtube_modal(),
            Command::OpenRemoveConfirm => self.open_remove_confirm(),
            Command::BeginChangeDirectory => {
                self.reset_dir_input();
                self.modes.open(Mode::ChangeDirectory);
            }
            Command::ToggleVisualSelect => self.toggle_visual_select(),
            Command::StartSearch => match self.panel {
                Panel::Library => self.modes.open(Mode::SearchLibrary),
                Panel::Playlists => self.modes.open(Mode::SearchPlaylists),
            },
            Command::CycleCategory(direction) => self.cycle_category(direction),
            Command::CycleSort(direction) => self.cycle_sort(direction),
            Command::CyclePlaylistDisplayMode => self.cycle_library_playlist_mode(),
            Command::Shuffle => {
                self.queue.shuffle();
                self.set_status("shuffled", StatusKind::Info);
            }
            Command::Unshuffle => {
                self.queue.unshuffle();
                self.set_status("restored original order", StatusKind::Info);
            }
            Command::VolumeUp => self.player.adjust_volume(0.05),
            Command::VolumeDown => self.player.adjust_volume(-0.05),
            Command::RequestQuit => self.modes.open(Mode::ConfirmQuit),
            Command::ShowHelp => self.modes.open(Mode::Help),
            Command::CancelQueueJump => self.set_status("cancelled queue jump", StatusKind::Info),
            Command::ExitPlaylistView => {
                self.playlist_panel.view = PlaylistView::Browsing;
                self.reset_playlist_browse_selection();
            }
            Command::ClearSearch => match self.panel {
                Panel::Library => {
                    self.library_panel.search_query.clear();
                    self.sync_selection_to_rows();
                    self.set_status(strings::CLEARED_SEARCH, StatusKind::Info);
                }
                Panel::Playlists if matches!(self.playlist_panel.view, PlaylistView::Viewing(_)) => {
                    self.playlist_panel.search_query.clear();
                    self.sync_selection_to_rows();
                    self.set_status(strings::CLEARED_SEARCH, StatusKind::Info);
                }
                Panel::Playlists => {
                    self.playlist_panel.search_query.clear();
                    self.sync_playlist_browse_selection();
                    self.set_status(strings::CLEARED_SEARCH, StatusKind::Info);
                }
            },
        }
    }

    fn handle_confirm_quit_key(&mut self, key: KeyEvent) {
        if !confirm_key(key) {
            if !cancel_key(key) {
                self.modes.replace(Mode::ConfirmQuit);
                return;
            }
            self.set_status("quit cancelled", StatusKind::Info);
            return;
        }
        let flushed = self.playlists.flush();
        self.report_save(flushed);
        let saved = crate::config::save_view_state(&crate::config::ViewState {
            library_category: self.library_panel.category,
            library_sort: self.library_panel.sort,
            library_playlist_mode: self.library_panel.playlist_mode,
            playlist_category: self.playlist_panel.category,
            playlist_sort: self.playlist_panel.sort,
        });
        self.report_save(saved);
        self.should_exit = true;
    }

    fn handle_confirm_remove_key(
        &mut self,
        key: KeyEvent,
        playlist_id: lyre_core::PlaylistId,
        song_id: SongId,
    ) {
        if !confirm_key(key) {
            if !cancel_key(key) {
                self.modes
                    .replace(Mode::ConfirmRemove(playlist_id, song_id));
            }
            return;
        }

        let label = self
            .library
            .get(song_id)
            .map(|s| s.to_string())
            .unwrap_or_else(|| strings::UNTITLED_SONG.to_string());
        if self.playlists.remove_song(playlist_id, song_id) == Mutated::Yes {
            self.set_status(format!("removed {label}"), StatusKind::Success);
            self.sync_selection_to_rows();
        } else {
            self.set_status("failed to remove song from playlist", StatusKind::Error);
        }
    }

    fn handle_dir_input_key(&mut self, key: KeyEvent, mode: Mode) {
        if let Some(modal_key) = modal_lookup(key) {
            match modal_key {
                ModalKey::Confirm => self.begin_dir_scan(),
                ModalKey::Cancel => self.reset_dir_input(),
                _ => self.modes.replace(mode),
            }
            return;
        }

        match key.code {
            KeyCode::Backspace => {
                self.dir.dir_input.pop();
            }
            KeyCode::Char(c) => self.dir.dir_input.push(c),
            _ => {}
        }
        self.modes.replace(mode);
    }

    fn handle_search_key(&mut self, key: KeyEvent, target: SearchTarget) {
        let filtering = match target {
            SearchTarget::Library => is_filtering(&self.library_panel.search_query),
            SearchTarget::Playlists => is_filtering(&self.playlist_panel.search_query),
        };
        let mut replace = true;
        match key.code {
            KeyCode::Enter => replace = false,
            KeyCode::Esc => {
                if filtering {
                    match target {
                        SearchTarget::Library => self.library_panel.search_query.clear(),
                        SearchTarget::Playlists => self.playlist_panel.search_query.clear(),
                    }
                    self.cancel_visual_select();
                    self.sync_search_selection(target);
                } else {
                    return;
                }
            }
            KeyCode::Backspace => {
                match target {
                    SearchTarget::Library => {
                        self.library_panel.search_query.pop();
                    }
                    SearchTarget::Playlists => {
                        self.playlist_panel.search_query.pop();
                    }
                }
                self.cancel_visual_select();
                self.sync_search_selection(target);
            }
            KeyCode::Char(c) => {
                match target {
                    SearchTarget::Library => self.library_panel.search_query.push(c),
                    SearchTarget::Playlists => self.playlist_panel.search_query.push(c),
                }
                self.cancel_visual_select();
                self.sync_search_selection(target);
            }
            _ => {}
        }
        if replace {
            let mode = match target {
                SearchTarget::Library => Mode::SearchLibrary,
                SearchTarget::Playlists => Mode::SearchPlaylists,
            };
            self.modes.replace(mode);
        }
    }

    fn sync_search_selection(&mut self, target: SearchTarget) {
        match target {
            SearchTarget::Library => self.sync_selection_to_rows(),
            SearchTarget::Playlists => self.sync_playlist_selection(),
        }
    }

    fn handle_metadata_modal_key(&mut self, key: KeyEvent, modal: MetadataEditModal) {
        match handle_form_field_key(key, modal) {
            FormFieldOutcome::Updated(modal) => self.set_metadata_modal(modal),
            FormFieldOutcome::Confirmed(modal) => self.save_metadata_edit_and_prompt(modal),
            FormFieldOutcome::Cancelled => {}
        }
    }

    fn handle_romanized_artist_confirm_key(
        &mut self,
        key: KeyEvent,
        confirm: RomanizedArtistConfirmModal,
    ) {
        if confirm_key(key) {
            self.confirm_romanized_artist_apply(confirm);
        } else if !cancel_key(key) {
            self.modes.replace(Mode::RomanizedArtistConfirm(confirm));
        }
    }
}

fn confirm_key(key: KeyEvent) -> bool {
    confirm_lookup(key) == Some(ConfirmChoice::Yes)
}

fn cancel_key(key: KeyEvent) -> bool {
    confirm_lookup(key) == Some(ConfirmChoice::No)
}
