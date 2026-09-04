use std::{fs, path::PathBuf, sync::mpsc};

use crossterm::event::KeyCode;

use crate::keymap::{ModalKey, modal_lookup};

use lyre_core::{InsertOutcome, Metadata, MetadataEdits, Song, youtube};

use super::form::{FormFieldOutcome, handle_form_field_key};
use super::modes::Mode;
use super::state::{
    DownloadStatus, FetchStatus, StatusKind, YoutubeField, YoutubeFieldsModal, YoutubeModal,
};
use super::{App, EventsChanged};

pub enum DownloadEvent {
    InfoReady {
        title: String,
        uploader: Option<String>,
    },
    Progress(f64),
    DownloadReady(PathBuf),
    Failed(String),
}

pub(super) fn channel() -> (mpsc::Sender<DownloadEvent>, mpsc::Receiver<DownloadEvent>) {
    mpsc::channel()
}

impl App {
    pub fn handle_youtube_event_for_test(&mut self, event: DownloadEvent) {
        self.handle_youtube_event(event);
    }

    pub fn drain_youtube_events_for_test(&mut self) -> EventsChanged {
        self.drain_youtube_events()
    }

    pub(super) fn drain_youtube_events(&mut self) -> EventsChanged {
        let mut changed = EventsChanged::Unchanged;
        while let Ok(event) = self.youtube_rx.try_recv() {
            changed = EventsChanged::Changed;
            self.handle_youtube_event(event);
        }
        changed
    }

    fn handle_youtube_event(&mut self, event: DownloadEvent) {
        match event {
            DownloadEvent::InfoReady { title, uploader } => {
                if let Some(fields) = self.youtube_fields_mut() {
                    fields.fetch_status = FetchStatus::Ready { title, uploader };
                }
            }
            DownloadEvent::Progress(percent) => {
                if let Some(Mode::Youtube(YoutubeModal::Downloading { progress, .. })) =
                    self.modes.active_mut()
                {
                    *progress = percent;
                }
            }
            DownloadEvent::DownloadReady(path) => match self.take_youtube_modal() {
                Some(YoutubeModal::Downloading {
                    fields, dest_path, ..
                }) => {
                    self.finalize_youtube_download(fields, path, dest_path);
                }
                Some(YoutubeModal::EditingFields(mut fields)) => {
                    fields.download_status = DownloadStatus::Ready(path);
                    self.set_youtube_modal(YoutubeModal::EditingFields(fields));
                }
                Some(YoutubeModal::ResolvingCollision {
                    mut fields,
                    existing_path,
                }) => {
                    fields.download_status = DownloadStatus::Ready(path);
                    self.set_youtube_modal(YoutubeModal::ResolvingCollision {
                        fields,
                        existing_path,
                    });
                }
                other => {
                    if let Some(modal) = other {
                        self.set_youtube_modal(modal);
                    }
                    youtube::discard_temp_file(&path);
                }
            },
            DownloadEvent::Failed(message) => self.interrupt_youtube_with_error(message),
        }
    }

    fn youtube_fields_mut(&mut self) -> Option<&mut YoutubeFieldsModal> {
        match self.modes.active_mut()? {
            Mode::Youtube(YoutubeModal::EditingFields(fields)) => Some(fields),
            Mode::Youtube(YoutubeModal::ResolvingCollision { fields, .. }) => Some(fields),
            Mode::Youtube(YoutubeModal::Downloading { fields, .. }) => Some(fields),
            _ => None,
        }
    }

    fn interrupt_youtube_with_error(&mut self, message: String) {
        let fields = match self.take_youtube_modal() {
            Some(YoutubeModal::EditingFields(fields)) => fields,
            Some(YoutubeModal::ResolvingCollision { fields, .. }) => fields,
            Some(YoutubeModal::Downloading { fields, .. }) => fields,
            _ => return,
        };

        let url_input = fields.url.clone();
        self.set_youtube_modal(YoutubeModal::EnteringUrl {
            url_input,
            error: Some(message),
            restore: Some(fields),
        });
    }

    fn finalize_youtube_download(
        &mut self,
        fields: YoutubeFieldsModal,
        temp_path: PathBuf,
        dest_path: PathBuf,
    ) {
        if let Err(e) = youtube::finalize_download(&temp_path, &dest_path) {
            self.set_status(
                format!("download finished, but failed to save it: {e}"),
                StatusKind::Error,
            );
            return;
        }

        let edits = MetadataEdits {
            title: fields.title.clone(),
            artist: fields.artist.clone(),
            album: fields.album.clone(),
            genre: String::new(),
            track: String::new(),
            date: String::new(),
            title_sort: fields.title_sort.clone(),
            artist_sort: fields.artist_sort.clone(),
        };

        if let Err(e) = Metadata::write(&dest_path, &edits) {
            self.set_status(
                format!("downloaded, but failed to tag: {e}"),
                StatusKind::Error,
            );
            return;
        }

        let song = match Song::load(&dest_path) {
            Ok(song) => song,
            Err(e) => {
                self.set_status(
                    format!("downloaded and tagged, but failed to load it back: {e}"),
                    StatusKind::Error,
                );
                return;
            }
        };

        let label = song.to_string();
        match self.library.insert(song) {
            InsertOutcome::Inserted(id) => {
                if self.queue_source() == super::QueueSource::Library {
                    self.queue.insert(id);
                }
                self.rows.invalidate();
                self.set_status(
                    format!("downloaded and added: {label}"),
                    StatusKind::Success,
                );
                self.select_song_by_id(id);

                self.maybe_prompt_romanized_artist(id, &fields.artist_sort, "");
            }
            InsertOutcome::Collision => {
                self.set_status(
                    "downloaded song already exists in the library",
                    StatusKind::Info,
                );
            }
        }
    }

    fn set_youtube_modal(&mut self, modal: YoutubeModal) {
        self.modes.replace(Mode::Youtube(modal));
    }

    fn take_youtube_modal(&mut self) -> Option<YoutubeModal> {
        match self.modes.take() {
            Some(Mode::Youtube(modal)) => Some(modal),
            other => {
                if let Some(mode) = other {
                    self.modes.replace(mode);
                }
                None
            }
        }
    }

    pub(super) fn open_youtube_modal(&mut self) {
        self.set_youtube_modal(YoutubeModal::EnteringUrl {
            url_input: String::new(),
            error: None,
            restore: None,
        });
    }

    pub(super) fn handle_youtube_modal_key(
        &mut self,
        key: crossterm::event::KeyEvent,
        modal: YoutubeModal,
    ) {
        match modal {
            YoutubeModal::EnteringUrl {
                mut url_input,
                restore,
                ..
            } => {
                let mut error = None;
                let mut submitted = false;

                if let Some(modal_key) = modal_lookup(key) {
                    match modal_key {
                        ModalKey::Confirm => {
                            let url = url_input.trim().to_string();
                            if url.is_empty() {
                                error = Some("enter a URL first".to_string());
                            } else {
                                self.spawn_fetch_and_download(url.clone());
                                self.set_youtube_modal(YoutubeModal::EditingFields(
                                    start_youtube_fields(url, restore.clone()),
                                ));
                                submitted = true;
                            }
                        }
                        ModalKey::Cancel => return,
                        _ => {}
                    }
                } else {
                    match key.code {
                        KeyCode::Backspace => {
                            url_input.pop();
                        }
                        KeyCode::Char(c) => url_input.push(c),
                        _ => {}
                    }
                }

                if !submitted {
                    self.set_youtube_modal(YoutubeModal::EnteringUrl {
                        url_input,
                        error,
                        restore,
                    });
                }
            }
            YoutubeModal::EditingFields(fields) => self.handle_youtube_fields_key(key, fields),
            YoutubeModal::ResolvingCollision {
                fields,
                existing_path,
            } => match key.code {
                KeyCode::Char('o') => self.start_or_await_youtube_download(fields, existing_path),
                KeyCode::Char('r') | KeyCode::Esc => {
                    let mut fields = fields;
                    fields.focused = YoutubeField::FileName;
                    self.set_youtube_modal(YoutubeModal::EditingFields(fields));
                }
                _ => {
                    self.set_youtube_modal(YoutubeModal::ResolvingCollision {
                        fields,
                        existing_path,
                    })
                }
            },
            YoutubeModal::Downloading {
                file_name,
                dest_path,
                fields,
                progress,
            } => {
                if key.code == KeyCode::Esc {
                    self.set_status("download in progress", StatusKind::Info);
                }
                self.set_youtube_modal(YoutubeModal::Downloading {
                    file_name,
                    dest_path,
                    fields,
                    progress,
                })
            }
        }
    }

    fn handle_youtube_fields_key(
        &mut self,
        key: crossterm::event::KeyEvent,
        fields: YoutubeFieldsModal,
    ) {
        match handle_form_field_key(key, fields) {
            FormFieldOutcome::Updated(fields) => {
                self.set_youtube_modal(YoutubeModal::EditingFields(fields));
            }
            FormFieldOutcome::Confirmed(fields) => self.confirm_youtube_fields(fields),
            FormFieldOutcome::Cancelled => {}
        }
    }

    fn confirm_youtube_fields(&mut self, fields: YoutubeFieldsModal) {
        let directory = match resolve_directory(self.library.root(), &fields.directory) {
            Ok(dir) => dir,
            Err(message) => {
                self.set_youtube_modal(YoutubeModal::EditingFields(YoutubeFieldsModal {
                    error: Some(message),
                    ..fields
                }));
                return;
            }
        };

        if fields.file_name.trim().is_empty() {
            self.set_youtube_modal(YoutubeModal::EditingFields(YoutubeFieldsModal {
                error: Some("filename can't be empty".to_string()),
                ..fields
            }));
            return;
        }

        let dest_path = directory.join(&fields.file_name);

        if dest_path.exists() {
            self.set_youtube_modal(YoutubeModal::ResolvingCollision {
                fields,
                existing_path: dest_path,
            });
            return;
        }

        self.start_or_await_youtube_download(fields, dest_path);
    }

    fn start_or_await_youtube_download(&mut self, fields: YoutubeFieldsModal, dest_path: PathBuf) {
        match &fields.download_status {
            DownloadStatus::Ready(temp_path) => {
                let temp_path = temp_path.clone();
                self.finalize_youtube_download(fields, temp_path, dest_path);
            }
            DownloadStatus::Pending => {
                let file_name = fields.file_name.clone();
                self.set_youtube_modal(YoutubeModal::Downloading {
                    file_name,
                    dest_path,
                    fields,
                    progress: 0.0,
                });
            }
        }
    }

    #[cfg(feature = "youtube")]
    fn spawn_fetch_and_download(&self, url: String) {
        let tx = self.youtube_tx.clone();
        std::thread::spawn(move || {
            let Some(binaries_dir) = crate::config::youtube_binaries_dir() else {
                let _ = tx.send(DownloadEvent::Failed(
                    "failed to resolve a cache directory".to_string(),
                ));
                return;
            };
            let scratch_dir = std::env::temp_dir();
            let tx_info = tx.clone();
            let tx_progress = tx.clone();

            let result =
                youtube::fetch_and_download(&url, &binaries_dir, &scratch_dir, move |info| {
                    let _ = tx_info.send(DownloadEvent::InfoReady {
                        title: info.title,
                        uploader: info.uploader,
                    });
                }, move |percent| {
                    let _ = tx_progress.send(DownloadEvent::Progress(percent));
                });

            match result {
                Ok(path) => {
                    let _ = tx.send(DownloadEvent::DownloadReady(path));
                }
                Err(e) => {
                    let _ = tx.send(DownloadEvent::Failed(e.to_string()));
                }
            }
        });
    }

    #[cfg(not(feature = "youtube"))]
    fn spawn_fetch_and_download(&self, _url: String) {
        let _ = self.youtube_tx.send(DownloadEvent::Failed(
            "YouTube support was not built into this binary".to_string(),
        ));
    }
}

pub fn start_youtube_fields(
    url: String,
    restore: Option<YoutubeFieldsModal>,
) -> YoutubeFieldsModal {
    match restore {
        Some(mut fields) => {
            fields.url = url;
            fields.focused = YoutubeField::Title;
            fields.error = None;
            fields.fetch_status = FetchStatus::Pending;
            fields.download_status = DownloadStatus::Pending;
            fields
        }
        None => YoutubeFieldsModal {
            url,
            title: String::new(),
            artist: String::new(),
            album: String::new(),
            title_sort: String::new(),
            artist_sort: String::new(),
            directory: "./".to_string(),
            file_name: String::new(),
            file_name_overridden: false,
            focused: YoutubeField::Title,
            error: None,
            fetch_status: FetchStatus::Pending,
            download_status: DownloadStatus::Pending,
        },
    }
}

fn resolve_directory(root: &std::path::Path, subpath: &str) -> Result<PathBuf, String> {
    let trimmed = subpath.trim();
    let candidate = std::path::Path::new(trimmed);

    if candidate.is_absolute() {
        return Err("directory must be relative to the library root".to_string());
    }
    if candidate
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err("directory can't contain '..'".to_string());
    }

    let joined = root.join(candidate);
    fs::create_dir_all(&joined).map_err(|e| format!("failed to create directory: {e}"))?;

    let canonical_joined = joined
        .canonicalize()
        .map_err(|e| format!("failed to resolve directory: {e}"))?;
    let canonical_root = root
        .canonicalize()
        .map_err(|e| format!("failed to resolve library root: {e}"))?;

    if !canonical_joined.starts_with(&canonical_root) {
        return Err("directory must stay within the library root".to_string());
    }

    Ok(canonical_joined)
}
