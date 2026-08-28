use crossterm::event::{KeyCode, KeyEvent};

use ratatui::widgets::ListState;

use crate::keymap::{ModalKey, modal_lookup};

use lyre_core::{Mutated, PlaylistId, SongId};

use super::App;
use super::navigation::move_wrapping;
use super::state::{ChooseActionField, PlaylistPicker, SongModal, StatusKind, cycle};

impl App {
    pub(super) fn handle_song_modal_key(&mut self, key: KeyEvent, modal: SongModal) {
        match modal.picker {
            Some(picker) => self.handle_picker_key(
                key,
                modal.song,
                modal.batch,
                modal.selected,
                modal.name_input,
                picker,
            ),
            None => self.handle_choose_action_key(
                key,
                modal.song,
                modal.batch,
                modal.selected,
                modal.name_input,
            ),
        }
    }

    fn handle_choose_action_key(
        &mut self,
        key: KeyEvent,
        song: SongId,
        batch: Option<Vec<SongId>>,
        mut selected: ChooseActionField,
        mut name_input: String,
    ) {
        if selected == ChooseActionField::CreatePlaylist {
            let modal_key = modal_lookup(key);
            match modal_key {
                Some(ModalKey::NextField) | Some(ModalKey::PrevField) => {
                    let visible = self.visible_choose_action_fields(song, batch.as_deref());
                    selected = cycle(
                        &visible,
                        selected,
                        if modal_key == Some(ModalKey::PrevField) { -1 } else { 1 },
                    );
                }
                Some(ModalKey::Cancel) => return,
                _ => match key.code {
                    KeyCode::Enter => {
                        let trimmed = name_input.trim();
                        if trimmed.is_empty() {
                            self.set_status("playlist name can't be empty", StatusKind::Error);
                            self.set_song_modal(song, batch, selected, name_input, None);
                            return;
                        }
                        let id = self.playlists.create(trimmed);
                        let songs: Vec<SongId> = batch.clone().unwrap_or_else(|| vec![song]);
                        for &song_id in &songs {
                            self.playlists.add_song(id, song_id);
                        }
                        let message = if songs.len() > 1 {
                            format!("created \"{trimmed}\" and added {} songs", songs.len())
                        } else {
                            format!("created \"{trimmed}\" and added the song")
                        };
                        self.set_status(message, StatusKind::Success);
                        return;
                    }
                    KeyCode::Backspace => {
                        name_input.pop();
                    }
                    KeyCode::Char(c) => name_input.push(c),
                    _ => {}
                },
            }
            self.set_song_modal(song, batch, selected, name_input, None);
            return;
        }

        let modal_key = modal_lookup(key);
        if modal_key == Some(ModalKey::Cancel) {
            return;
        }
        if modal_key == Some(ModalKey::NextField) || modal_key == Some(ModalKey::PrevField) {
            let visible = self.visible_choose_action_fields(song, batch.as_deref());
            selected = cycle(
                &visible,
                selected,
                if modal_key == Some(ModalKey::PrevField) { -1 } else { 1 },
            );
        } else {
            let direction = match key.code {
                KeyCode::Char('j') => Some(1),
                KeyCode::Char('k') => Some(-1),
                _ => None,
            };
            if let Some(delta) = direction {
                let visible = self.visible_choose_action_fields(song, batch.as_deref());
                selected = cycle(&visible, selected, delta);
            } else if key.code == KeyCode::Enter {
                match selected {
                        ChooseActionField::AddToPlaylist => {
                            match self.build_add_to_playlist_picker(song, batch.as_deref()) {
                                Some(picker) => {
                                    self.set_song_modal(
                                        song,
                                        batch,
                                        selected,
                                        name_input,
                                        Some(picker),
                                    );
                                    return;
                                }
                                None => {
                                    self.set_status(
                                        "no other playlists to add to -- select Create Playlist instead",
                                        StatusKind::Info,
                                    );
                                }
                            }
                        }
                        ChooseActionField::RemoveFromPlaylist => {
                            match self.build_remove_from_playlist_picker(song, batch.as_deref()) {
                                Some(picker) => {
                                    self.set_song_modal(
                                        song,
                                        batch,
                                        selected,
                                        name_input,
                                        Some(picker),
                                    );
                                    return;
                                }
                                None => {
                                    let message = if batch.is_some() {
                                        "none of the selected songs are in a playlist"
                                    } else {
                                        "song is not in any playlist"
                                    };
                                    self.set_status(message, StatusKind::Info);
                                }
                            }
                        }
                        ChooseActionField::CreatePlaylist => {}
                }
            }
        }
        self.set_song_modal(song, batch, selected, name_input, None);
    }

    fn handle_picker_key(
        &mut self,
        key: KeyEvent,
        song: SongId,
        batch: Option<Vec<SongId>>,
        selected: ChooseActionField,
        name_input: String,
        picker: PlaylistPicker,
    ) {
        let modal_key = modal_lookup(key);

        let (kind, options, pinned, mut list_state) = match picker {
            PlaylistPicker::Add {
                options,
                pinned,
                list_state,
            } => (PickerKind::Add, options, Some(pinned), list_state),
            PlaylistPicker::Remove {
                options,
                list_state,
            } => (PickerKind::Remove, options, None, list_state),
        };

        if modal_key == Some(ModalKey::Cancel) {
            self.set_song_modal(song, batch, selected, name_input, None);
            return;
        }

        let list_delta = match modal_key {
            Some(ModalKey::NextField) => 1,
            Some(ModalKey::PrevField) => -1,
            _ => match key.code {
                KeyCode::Char('j') => 1,
                KeyCode::Char('k') => -1,
                _ => 0,
            },
        };
        if list_delta != 0 && !options.is_empty() {
            move_wrapping(&mut list_state, options.len(), list_delta);
            self.set_song_modal(
                song,
                batch,
                selected,
                name_input,
                Some(rebuild_picker(kind, &options, pinned, list_state)),
            );
            return;
        }

        if modal_key == Some(ModalKey::Confirm) {
            if let Some(&target) = list_state.selected().and_then(|i| options.get(i)) {
                let name = self
                    .playlists
                    .get(target)
                    .map(|p| p.name().to_string())
                    .unwrap_or_default();
                let songs: Vec<SongId> = batch.clone().unwrap_or_else(|| vec![song]);
                let done = songs
                    .iter()
                    .filter(|&&id| match kind {
                        PickerKind::Add => self.playlists.add_song(target, id) == Mutated::Yes,
                        PickerKind::Remove => {
                            self.playlists.remove_song(target, id) == Mutated::Yes
                        }
                    })
                    .count();
                let message = match kind {
                    PickerKind::Add => add_batch_message(done, songs.len(), &name),
                    PickerKind::Remove => remove_batch_message(done, songs.len(), &name),
                };
                self.set_status(message, status_kind_for(done));
            }
            return;
        }

        self.set_song_modal(
            song,
            batch,
            selected,
            name_input,
            Some(rebuild_picker(kind, &options, pinned, list_state)),
        );
    }
}

fn status_kind_for(count: usize) -> StatusKind {
    if count == 0 {
        StatusKind::Info
    } else {
        StatusKind::Success
    }
}

enum PickerKind {
    Add,
    Remove,
}

fn rebuild_picker(
    kind: PickerKind,
    options: &[PlaylistId],
    pinned: Option<Vec<PlaylistId>>,
    list_state: ListState,
) -> PlaylistPicker {
    match kind {
        PickerKind::Add => PlaylistPicker::Add {
            options: options.to_vec(),
            pinned: pinned.unwrap_or_default(),
            list_state,
        },
        PickerKind::Remove => PlaylistPicker::Remove {
            options: options.to_vec(),
            list_state,
        },
    }
}

fn batch_message(verb: &str, prep: &str, noop: &str, done: usize, total: usize, name: &str) -> String {
    match (done, total) {
        (0, _) => format!("{noop} \"{name}\""),
        (1, 1) => format!("{verb} {prep} \"{name}\""),
        (n, t) if n == t => format!("{verb} {n} songs {prep} \"{name}\""),
        (n, t) => format!("{verb} {n} of {t} songs {prep} \"{name}\""),
    }
}

fn add_batch_message(added: usize, total: usize, name: &str) -> String {
    batch_message("added", "to", "already in", added, total, name)
}

fn remove_batch_message(removed: usize, total: usize, name: &str) -> String {
    batch_message("removed", "from", "not in", removed, total, name)
}
