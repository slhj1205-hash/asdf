use crate::keymap::{self, Action, Direction};

use super::App;
use super::state::{Panel, PlaylistView, is_filtering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    TogglePanel,
    Move(isize),
    JumpPage(Direction),
    JumpTop,
    JumpBottom,
    JumpToCurrent,
    Activate,
    TogglePlayback,
    NextTrack,
    JumpToUpcoming,
    PreviousTrack,
    SeekBack,
    SeekForward,
    QueueNext,
    OpenSongModal,
    OpenMetadataEdit,
    OpenYoutube,
    OpenRemoveConfirm,
    BeginChangeDirectory,
    ToggleVisualSelect,
    StartSearch,
    CycleCategory(Direction),
    CycleSort(Direction),
    CyclePlaylistDisplayMode,
    Shuffle,
    Unshuffle,
    VolumeUp,
    VolumeDown,
    RequestQuit,
    ShowHelp,
    CancelQueueJump,
    ExitPlaylistView,
    ClearSearch,
}

pub fn command_for_key(
    app: &App,
    key: crossterm::event::KeyEvent,
    had_pending_number: bool,
) -> Option<Command> {
    if key.code == crossterm::event::KeyCode::Esc {
        return esc_command(app, had_pending_number);
    }
    match keymap::lookup(key) {
        Some(action) => action_command(action, app),
        None => None,
    }
}

fn esc_command(app: &App, had_pending_number: bool) -> Option<Command> {
    if had_pending_number {
        return Some(Command::CancelQueueJump);
    }
    if app.active_visual().is_some() {
        return Some(Command::ToggleVisualSelect);
    }
    if app.panel == Panel::Playlists
        && matches!(app.playlist_panel.view, PlaylistView::Viewing(_))
    {
        if !is_filtering(&app.playlist_panel.search_query) {
            return Some(Command::ExitPlaylistView);
        }
        return Some(Command::ClearSearch);
    }
    if app.panel == Panel::Playlists
        && app.playlist_panel.view == PlaylistView::Browsing
        && is_filtering(&app.playlist_panel.search_query)
    {
        return Some(Command::ClearSearch);
    }
    if app.panel == Panel::Library && is_filtering(&app.library_panel.search_query) {
        return Some(Command::ClearSearch);
    }
    Some(Command::RequestQuit)
}

fn action_command(action: Action, _app: &App) -> Option<Command> {
    let command = match action {
        Action::TogglePanel => Command::TogglePanel,
        Action::MoveDown => Command::Move(1),
        Action::MoveUp => Command::Move(-1),
        Action::PageDown => Command::JumpPage(keymap::Direction::Forwards),
        Action::PageUp => Command::JumpPage(keymap::Direction::Backwards),
        Action::JumpTop => Command::JumpTop,
        Action::JumpBottom => Command::JumpBottom,
        Action::JumpToCurrent => Command::JumpToCurrent,
        Action::Activate => Command::Activate,
        Action::TogglePlayback => Command::TogglePlayback,
        Action::NextOrJump => {
            if _app.pending_number.is_empty() {
                Command::NextTrack
            } else {
                Command::JumpToUpcoming
            }
        }
        Action::PrevTrack => Command::PreviousTrack,
        Action::SeekBack => Command::SeekBack,
        Action::SeekForward => Command::SeekForward,
        Action::QueueNext => Command::QueueNext,
        Action::OpenSongModal => Command::OpenSongModal,
        Action::OpenMetadataEditModal => Command::OpenMetadataEdit,
        Action::OpenYoutubeModal => Command::OpenYoutube,
        Action::RemoveFromPlaylist => Command::OpenRemoveConfirm,
        Action::ChangeDirectory => Command::BeginChangeDirectory,
        Action::ToggleVisualSelect => Command::ToggleVisualSelect,
        Action::ToggleSearch => Command::StartSearch,
        Action::CycleCategory(direction) => Command::CycleCategory(direction),
        Action::CycleSort(direction) => Command::CycleSort(direction),
        Action::CyclePlaylistDisplayMode => Command::CyclePlaylistDisplayMode,
        Action::Shuffle => Command::Shuffle,
        Action::Unshuffle => Command::Unshuffle,
        Action::VolumeUp => Command::VolumeUp,
        Action::VolumeDown => Command::VolumeDown,
        Action::Quit => Command::RequestQuit,
        Action::ToggleHelp => Command::ShowHelp,
    };
    Some(command)
}
