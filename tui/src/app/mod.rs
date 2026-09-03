mod form;
mod input;
mod command;
mod metadata;
mod modes;
mod navigation;
mod panels;
mod playback;
mod row_builder;
mod song_modal;
mod state;
mod youtube;

use std::{path::PathBuf, time::Duration};

use crossterm::event;
use ratatui::{DefaultTerminal, widgets::ListState};

use lyre_core::{Library, Player, PlaylistStore, Queue, SaveOutcome};

use crate::{Backend, config, strings};

pub use row_builder::{RowCache, group_label};
pub use state::{
    Category, ChooseActionField, DirScanState, DownloadStatus, FetchStatus, LibraryPanelState,
    MeasuredLayout, MetadataEditModal, MetadataField, Panel, PlaylistDisplayMode,
    PlaylistPanelState, PlaylistView, QueueSource, RomanizedArtistConfirmModal, Row, PlaylistPicker,
    SongModal, Sort, StatusKind, StatusMessage, VisualSelection, YoutubeField, YoutubeFieldsModal,
    YoutubeModal, is_filtering, song_row_count,
};
pub use form::{FormFieldOutcome, FormFields, FormState, handle_form_field_key};
pub use modes::{Mode, Modes};
pub use youtube::{DownloadEvent, start_youtube_fields};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventsChanged {
    Changed,
    Unchanged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyEventHandled {
    Handled,
    Ignored,
}

pub struct App {
    pub library: Library,
    pub queue: Queue,
    queue_source: QueueSource,
    pub player: Player<Backend>,
    pub playlists: PlaylistStore,

    pub(crate) library_revision: u64,
    pub rows: RowCache,

    pub(crate) animating: std::cell::Cell<bool>,
    pub status: StatusMessage,
    should_exit: bool,
    pending_number: String,
    pub panel: Panel,

    pub dir: DirScanState,
    pub library_panel: LibraryPanelState,
    pub playlist_panel: PlaylistPanelState,
    pub measured: MeasuredLayout,
    pub modes: Modes,

    youtube_tx: std::sync::mpsc::Sender<youtube::DownloadEvent>,
    youtube_rx: std::sync::mpsc::Receiver<youtube::DownloadEvent>,

    deferred_warnings: Vec<String>,
}

impl App {
    pub fn new(library: Library, playlists: PlaylistStore, backend: Backend) -> App {
        let queue = Queue::new(library.ids_by_path());

        let mut library_panel = LibraryPanelState::default();
        if !library.is_empty() {
            library_panel.list_state.select(Some(0));
        }

        let status = StatusMessage::new(
            format!(
                "loaded {} song{} from {}",
                library.len(),
                strings::plural(library.len(), "s"),
                library.root().display()
            ),
            StatusKind::Success,
        );
        let dir = DirScanState {
            dir_input: library.root().display().to_string(),
            ..Default::default()
        };

        let (youtube_tx, youtube_rx) = youtube::channel();

        let mut app = App {
            library,
            queue,
            queue_source: QueueSource::Library,
            player: Player::new(backend),
            playlists,
            library_revision: 0,
            rows: RowCache::default(),
            animating: std::cell::Cell::new(false),
            status,
            should_exit: false,
            pending_number: String::new(),
            panel: Panel::Library,
            dir,
            library_panel,
            playlist_panel: PlaylistPanelState::default(),
            measured: MeasuredLayout::default(),
            modes: Modes::new(),
            youtube_tx,
            youtube_rx,
            deferred_warnings: Vec::new(),
        };
        app.reset_playlist_browse_selection();
        app
    }

    pub fn apply_view_state(&mut self, state: config::ViewState) {
        self.library_panel.category = state.library_category;
        self.library_panel.sort = state.library_sort;
        self.library_panel.playlist_mode = state.library_playlist_mode;
        self.playlist_panel.category = state.playlist_category;
        self.playlist_panel.sort = state.playlist_sort;
        self.sync_selection_to_rows();
    }

    pub fn deferred_warnings_for_test(&self) -> &[String] {
        &self.deferred_warnings
    }

    pub(crate) fn report_save(&mut self, outcome: SaveOutcome) {
        if let SaveOutcome::Failed(message) = outcome {
            self.set_status(message.clone(), StatusKind::Error);
            if self.deferred_warnings.last() != Some(&message) {
                self.deferred_warnings.push(message);
            }
        }
    }

    pub fn on_key(&mut self, key: event::KeyEvent) {
        self.handle_key(key);
    }

    pub fn pending_number_for_test(&self) -> &str {
        &self.pending_number
    }

    fn reset_dir_input(&mut self) {
        self.dir.dir_input = self.library.root().display().to_string();
    }

    fn set_status(&mut self, text: impl Into<String>, kind: StatusKind) {
        self.status = StatusMessage::new(text, kind);
    }

    pub fn run(mut self, terminal: &mut DefaultTerminal) -> std::io::Result<Vec<String>> {
        let mut needs_redraw = true;

        while !self.should_exit {
            let status_changed = self.status.expire_if_stale();

            if needs_redraw || status_changed || self.animating.get() {
                terminal.draw(|frame| frame.render_widget(&mut self, frame.area()))?;
            }

            needs_redraw = self.handle_events()? == KeyEventHandled::Handled;

            let flushed = self.playlists.flush_if_due();
            self.report_save(flushed);

            if let Some(dir) = self.dir.pending_scan.take() {
                terminal.draw(|frame| frame.render_widget(&mut self, frame.area()))?;
                self.finish_dir_scan(dir);
                needs_redraw = true;
            }

            if self.drain_player_events() == EventsChanged::Changed {
                needs_redraw = true;
            }

            if self.drain_youtube_events() == EventsChanged::Changed {
                needs_redraw = true;
            }
        }
        Ok(std::mem::take(&mut self.deferred_warnings))
    }

    fn handle_events(&mut self) -> std::io::Result<KeyEventHandled> {
        let timeout = if self.animating.get() {
            Duration::from_millis(120)
        } else {
            Duration::from_millis(400)
        };

        if !event::poll(timeout)? {
            return Ok(KeyEventHandled::Ignored);
        }
        let event = event::read()?;
        if let Some(key) = event.as_key_press_event() {
            self.handle_key(key);
        }
        Ok(KeyEventHandled::Handled)
    }

    fn begin_dir_scan(&mut self) {
        let new_dir = PathBuf::from(self.dir.dir_input.trim());
        self.set_status(format!("scanning {}…", new_dir.display()), StatusKind::Info);
        self.modes.take();
        self.dir.pending_scan = Some(new_dir);
    }

    pub fn finish_dir_scan_for_test(&mut self, new_dir: PathBuf) {
        self.finish_dir_scan(new_dir);
    }

    fn finish_dir_scan(&mut self, new_dir: PathBuf) {
        let cache_path = crate::config::scan_cache_path(&new_dir);

        match Library::scan(&new_dir, &cache_path) {
            Ok((library, stats)) => {
                let stop_error = self.player.stop().err();

                self.cancel_visual_select();
                self.queue = Queue::new(library.ids_by_path());
                self.queue_source = QueueSource::Library;
                self.library_panel.list_state = ListState::default();
                self.sync_selection_to_rows();

                let playlists_path = crate::config::playlists_path(library.root());
                let (playlists, prune_stats) = PlaylistStore::load(playlists_path, &library);
                let flushed = self.playlists.flush();
                self.report_save(flushed);
                self.playlists = playlists;
                self.playlist_panel.view = PlaylistView::Browsing;
                self.playlist_panel.search_query.clear();
                self.reset_playlist_browse_selection();
                self.modes = Modes::new();

                let mut message = format!(
                    "loaded {} song{} from {}",
                    library.len(),
                    strings::plural(library.len(), "s"),
                    library.root().display()
                );
                if stats.skipped() > 0 {
                    message.push_str(&format!(" ({} skipped)", stats.skipped()));
                }
                if prune_stats.songs_removed > 0 {
                    message.push_str(&format!(
                        ", removed {} missing song{} from playlists",
                        prune_stats.songs_removed,
                        strings::plural(prune_stats.songs_removed, "s")
                    ));
                }

                let warnings: Vec<String> = stats
                    .warnings
                    .iter()
                    .chain(prune_stats.warnings.iter())
                    .cloned()
                    .collect();
                if !warnings.is_empty() {
                    message.push_str(&format!(
                        ", {} warning{} — see the output after quitting",
                        warnings.len(),
                        strings::plural(warnings.len(), "s")
                    ));
                    self.deferred_warnings.extend(warnings);
                }
                let saved = crate::config::save_last_dir(library.root());
                if let SaveOutcome::Failed(warning) = saved {
                    message.push_str(&format!(", warning: {warning}"));
                    self.deferred_warnings.push(warning);
                }
                match stop_error {
                    None => self.set_status(message, StatusKind::Success),
                    Some(e) => {
                        message
                            .push_str(&format!(" (failed to stop previous playback cleanly: {e})"));
                        self.set_status(message, StatusKind::Error);
                    }
                }
                self.reset_dir_input();
                self.library_panel.search_query.clear();
                self.library = library;
                self.library_revision += 1;
                self.rows.invalidate();
            }
            Err(e) => {
                self.set_status(
                    format!("failed to scan {}: {e}", new_dir.display()),
                    StatusKind::Error,
                );
            }
        }
    }
}
