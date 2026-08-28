use super::state::{
    MetadataEditModal, RomanizedArtistConfirmModal, SongModal, YoutubeModal,
};
use lyre_core::{PlaylistId, SongId};

#[derive(Clone)]
pub enum Mode {
    ConfirmQuit,
    ConfirmRemove(PlaylistId, SongId),
    Help,
    SongModal(SongModal),
    MetadataEdit(MetadataEditModal),
    RomanizedArtistConfirm(RomanizedArtistConfirmModal),
    Youtube(YoutubeModal),
    ChangeDirectory,
    SearchLibrary,
    SearchPlaylists,
}

impl Mode {
    pub(super) fn search_target(&self) -> Option<SearchTarget> {
        match self {
            Mode::SearchLibrary => Some(SearchTarget::Library),
            Mode::SearchPlaylists => Some(SearchTarget::Playlists),
            _ => None,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum SearchTarget {
    Library,
    Playlists,
}

#[derive(Default)]
pub struct Modes(Option<Mode>);

impl Modes {
    pub fn new() -> Modes {
        Modes(None)
    }

    pub fn open(&mut self, mode: Mode) {
        debug_assert!(
            !self.is_active(),
            "a modal is already open; overlays are mutually exclusive"
        );
        self.0 = Some(mode);
    }

    pub fn replace(&mut self, mode: Mode) {
        self.0 = Some(mode);
    }

    pub fn active(&self) -> Option<&Mode> {
        self.0.as_ref()
    }

    pub fn active_mut(&mut self) -> Option<&mut Mode> {
        self.0.as_mut()
    }

    pub fn take(&mut self) -> Option<Mode> {
        self.0.take()
    }

    pub fn is_active(&self) -> bool {
        self.0.is_some()
    }
}

impl super::App {
    pub fn modes_is_empty(&self) -> bool {
        !self.modes.is_active()
    }

    pub fn song_mode_is(&self) -> bool {
        self.song_mode().is_some()
    }

    pub fn metadata_mode_is(&self) -> bool {
        self.metadata_mode().is_some()
    }

    pub fn romanized_mode_is(&self) -> bool {
        self.romanized_mode().is_some()
    }

    pub fn youtube_mode_is(&self) -> bool {
        self.youtube_mode().is_some()
    }

    pub fn confirm_mode_is(&self) -> bool {
        matches!(
            self.modes.active(),
            Some(Mode::ConfirmQuit) | Some(Mode::ConfirmRemove(_, _))
        )
    }

    pub fn help_mode_is(&self) -> bool {
        matches!(self.modes.active(), Some(Mode::Help))
    }

    pub fn search_library_mode_is(&self) -> bool {
        matches!(self.modes.active(), Some(Mode::SearchLibrary))
    }

    pub fn search_playlists_mode_is(&self) -> bool {
        matches!(self.modes.active(), Some(Mode::SearchPlaylists))
    }

    pub fn push_youtube(&mut self, modal: YoutubeModal) {
        self.modes.open(Mode::Youtube(modal));
    }

    pub fn song_mode(&self) -> Option<&SongModal> {
        match self.modes.active() {
            Some(Mode::SongModal(m)) => Some(m),
            _ => None,
        }
    }

    pub fn metadata_mode(&self) -> Option<&MetadataEditModal> {
        match self.modes.active() {
            Some(Mode::MetadataEdit(m)) => Some(m),
            _ => None,
        }
    }

    pub fn metadata_mode_mut(&mut self) -> Option<&mut MetadataEditModal> {
        match self.modes.active_mut() {
            Some(Mode::MetadataEdit(m)) => Some(m),
            _ => None,
        }
    }

    pub fn romanized_mode(&self) -> Option<&RomanizedArtistConfirmModal> {
        match self.modes.active() {
            Some(Mode::RomanizedArtistConfirm(m)) => Some(m),
            _ => None,
        }
    }

    pub fn youtube_mode(&self) -> Option<&YoutubeModal> {
        match self.modes.active() {
            Some(Mode::Youtube(m)) => Some(m),
            _ => None,
        }
    }
}
