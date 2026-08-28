use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    TogglePanel,
    MoveDown,
    MoveUp,
    PageDown,
    PageUp,
    JumpTop,
    JumpBottom,
    JumpToCurrent,
    Activate,
    TogglePlayback,
    NextOrJump,
    PrevTrack,
    SeekBack,
    SeekForward,
    QueueNext,
    OpenSongModal,
    OpenMetadataEditModal,
    OpenYoutubeModal,
    RemoveFromPlaylist,
    ChangeDirectory,
    ToggleSearch,
    ToggleVisualSelect,
    CycleCategory(Direction),
    CycleSort(Direction),
    CyclePlaylistDisplayMode,
    Shuffle,
    Unshuffle,
    VolumeUp,
    VolumeDown,
    Quit,
    ToggleHelp,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Global,
    Library,
    Playlists,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Forwards,
    Backwards,
}

pub struct Binding {
    pub keys: &'static [(KeyCode, KeyModifiers)],
    pub action: Option<Action>,
    pub display_override: Option<&'static str>,
    pub desc: &'static str,
    pub section: Section,
    pub dispatch: bool,
}

impl Binding {
    pub fn display(&self) -> String {
        if let Some(text) = self.display_override {
            return text.to_string();
        }
        if !self.keys.is_empty() {
            return render_keys(self.keys);
        }
        match self.action {
            Some(action) => display_for(action),
            None => String::new(),
        }
    }
}

const NONE: KeyModifiers = KeyModifiers::NONE;
const CTRL: KeyModifiers = KeyModifiers::CONTROL;
const ALT: KeyModifiers = KeyModifiers::ALT;

fn render_key_code(code: KeyCode) -> String {
    match code {
        KeyCode::Char(' ') => "Space".to_string(),
        KeyCode::Char(c) if c.is_ascii_uppercase() => format!("Shift+{c}"),
        KeyCode::Char(c) => c.to_string(),
        KeyCode::Up => "\u{2191}".to_string(),
        KeyCode::Down => "\u{2193}".to_string(),
        KeyCode::Left => "\u{2190}".to_string(),
        KeyCode::Right => "\u{2192}".to_string(),
        KeyCode::Enter => "Enter".to_string(),
        KeyCode::Esc => "Esc".to_string(),
        KeyCode::Tab => "Tab".to_string(),
        KeyCode::BackTab => "Shift+Tab".to_string(),
        KeyCode::Home => "Home".to_string(),
        KeyCode::End => "End".to_string(),
        KeyCode::Backspace => "Backspace".to_string(),
        KeyCode::PageUp => "PageUp".to_string(),
        KeyCode::PageDown => "PageDown".to_string(),
        KeyCode::Delete => "Delete".to_string(),
        KeyCode::Insert => "Insert".to_string(),
        other => format!("{other:?}"),
    }
}

pub fn render_key(code: KeyCode, mods: KeyModifiers) -> String {
    let mut out = String::from("<");
    if mods.contains(KeyModifiers::CONTROL) {
        out.push_str("Ctrl+");
    }
    if mods.contains(KeyModifiers::ALT) {
        out.push_str("Alt+");
    }
    out.push_str(&render_key_code(code));
    out.push('>');
    out
}

pub fn render_keys(keys: &[(KeyCode, KeyModifiers)]) -> String {
    keys.iter()
        .map(|&(code, mods)| render_key(code, mods))
        .collect::<Vec<_>>()
        .join("/")
}

pub const BINDINGS: &[Binding] = &[
    Binding {
        keys: &[(KeyCode::Tab, NONE)],
        action: Some(Action::TogglePanel),
        display_override: None,
        desc: "Switch between Library / Playlists",
        section: Section::Global,
        dispatch: true,
    },
    Binding {
        keys: &[(KeyCode::Char('j'), NONE), (KeyCode::Down, NONE)],
        action: Some(Action::MoveDown),
        display_override: None,
        desc: "Move selection",
        section: Section::Global,
        dispatch: true,
    },
    Binding {
        keys: &[(KeyCode::Char('k'), NONE), (KeyCode::Up, NONE)],
        action: Some(Action::MoveUp),
        display_override: None,
        desc: "Move selection",
        section: Section::Global,
        dispatch: true,
    },
    Binding {
        keys: &[(KeyCode::Char('d'), CTRL)],
        action: Some(Action::PageDown),
        display_override: None,
        desc: "Jump a page down / up",
        section: Section::Global,
        dispatch: true,
    },
    Binding {
        keys: &[(KeyCode::Char('u'), CTRL)],
        action: Some(Action::PageUp),
        display_override: None,
        desc: "Jump a page down / up",
        section: Section::Global,
        dispatch: true,
    },
    Binding {
        keys: &[(KeyCode::Char('g'), NONE), (KeyCode::Home, NONE)],
        action: Some(Action::JumpTop),
        display_override: None,
        desc: "Jump to top / bottom",
        section: Section::Global,
        dispatch: true,
    },
    Binding {
        keys: &[(KeyCode::Char('G'), NONE), (KeyCode::End, NONE)],
        action: Some(Action::JumpBottom),
        display_override: None,
        desc: "Jump to top / bottom",
        section: Section::Global,
        dispatch: true,
    },
    Binding {
        keys: &[(KeyCode::Char('c'), NONE)],
        action: Some(Action::JumpToCurrent),
        display_override: None,
        desc: "Jump to now playing",
        section: Section::Global,
        dispatch: true,
    },
    Binding {
        keys: &[(KeyCode::Enter, NONE)],
        action: Some(Action::Activate),
        display_override: None,
        desc: "Play selected song / open selected playlist",
        section: Section::Global,
        dispatch: true,
    },
    Binding {
        keys: &[(KeyCode::Char(' '), NONE)],
        action: Some(Action::TogglePlayback),
        display_override: None,
        desc: "Pause / resume",
        section: Section::Global,
        dispatch: true,
    },
    Binding {
        keys: &[(KeyCode::Char('n'), NONE)],
        action: Some(Action::NextOrJump),
        display_override: None,
        desc: "Next track",
        section: Section::Global,
        dispatch: true,
    },
    Binding {
        keys: &[],
        action: None,
        display_override: Some("<1-9> then <n>"),
        desc: "Jump to Nth song in Up Next (<Esc> cancels)",
        section: Section::Global,
        dispatch: false,
    },
    Binding {
        keys: &[(KeyCode::Char('b'), NONE)],
        action: Some(Action::PrevTrack),
        display_override: None,
        desc: "Previous track",
        section: Section::Global,
        dispatch: true,
    },
    Binding {
        keys: &[(KeyCode::Left, NONE)],
        action: Some(Action::SeekBack),
        display_override: None,
        desc: "Seek back 5 seconds",
        section: Section::Global,
        dispatch: true,
    },
    Binding {
        keys: &[(KeyCode::Right, NONE)],
        action: Some(Action::SeekForward),
        display_override: None,
        desc: "Seek forward 5 seconds (past the end skips to the next track)",
        section: Section::Global,
        dispatch: true,
    },
    Binding {
        keys: &[(KeyCode::Char('a'), NONE)],
        action: Some(Action::QueueNext),
        display_override: None,
        desc: "Queue selected song next",
        section: Section::Global,
        dispatch: true,
    },
    Binding {
        keys: &[(KeyCode::Char('v'), CTRL)],
        action: Some(Action::ToggleVisualSelect),
        display_override: None,
        desc: "Start/cancel visual selection (select a range with j/k etc, then <w>)",
        section: Section::Global,
        dispatch: true,
    },
    Binding {
        keys: &[(KeyCode::Char('s'), NONE)],
        action: Some(Action::Shuffle),
        display_override: None,
        desc: "Shuffle",
        section: Section::Global,
        dispatch: true,
    },
    Binding {
        keys: &[(KeyCode::Char('u'), NONE)],
        action: Some(Action::Unshuffle),
        display_override: None,
        desc: "Un-shuffle",
        section: Section::Global,
        dispatch: true,
    },
    Binding {
        keys: &[(KeyCode::Char('['), NONE), (KeyCode::Char('-'), NONE)],
        action: Some(Action::VolumeDown),
        display_override: None,
        desc: "Volume down / up",
        section: Section::Global,
        dispatch: true,
    },
    Binding {
        keys: &[(KeyCode::Char(']'), NONE), (KeyCode::Char('='), NONE)],
        action: Some(Action::VolumeUp),
        display_override: None,
        desc: "Volume down / up",
        section: Section::Global,
        dispatch: true,
    },
    Binding {
        keys: &[(KeyCode::Char('w'), NONE)],
        action: Some(Action::OpenSongModal),
        display_override: None,
        desc: "Song actions: add to / remove from / create playlist",
        section: Section::Global,
        dispatch: true,
    },
    Binding {
        keys: &[(KeyCode::Char('E'), NONE)],
        action: Some(Action::OpenMetadataEditModal),
        display_override: None,
        desc: "Edit song metadata (title/artist/album/genre/track/date)",
        section: Section::Global,
        dispatch: true,
    },
    Binding {
        keys: &[(KeyCode::Char('y'), NONE)],
        action: Some(Action::OpenYoutubeModal),
        display_override: None,
        desc: "Download audio from a YouTube URL",
        section: Section::Global,
        dispatch: true,
    },
    Binding {
        keys: &[(KeyCode::Char('d'), ALT)],
        action: Some(Action::ChangeDirectory),
        display_override: None,
        desc: "Change directory (used by both Library and Playlists)",
        section: Section::Global,
        dispatch: true,
    },
    Binding {
        keys: &[(KeyCode::Char('q'), NONE)],
        action: Some(Action::Quit),
        display_override: None,
        desc: "Quit (with confirmation)",
        section: Section::Global,
        dispatch: true,
    },
    Binding {
        keys: &[(KeyCode::Esc, NONE)],
        action: None,
        display_override: None,
        desc: "Quit (with confirmation)",
        section: Section::Global,
        dispatch: false,
    },
    Binding {
        keys: &[(KeyCode::Char('?'), NONE)],
        action: Some(Action::ToggleHelp),
        display_override: None,
        desc: "Toggle this help",
        section: Section::Global,
        dispatch: true,
    },
    Binding {
        keys: &[(KeyCode::Char('/'), NONE)],
        action: Some(Action::ToggleSearch),
        display_override: None,
        desc: "Search the library (live filter)",
        section: Section::Library,
        dispatch: true,
    },
    Binding {
        keys: &[(KeyCode::Char('o'), NONE)],
        action: Some(Action::CycleCategory(Direction::Forwards)),
        display_override: None,
        desc: "Cycle library category (grouping)",
        section: Section::Library,
        dispatch: true,
    },
    Binding {
        keys: &[(KeyCode::Char('O'), NONE)],
        action: Some(Action::CycleCategory(Direction::Backwards)),
        display_override: None,
        desc: "Cycle library category (grouping)",
        section: Section::Library,
        dispatch: true,
    },
    Binding {
        keys: &[(KeyCode::Char('p'), NONE)],
        action: Some(Action::CycleSort(Direction::Forwards)),
        display_override: None,
        desc: "Cycle library sort (order within group)",
        section: Section::Library,
        dispatch: true,
    },
    Binding {
        keys: &[(KeyCode::Char('P'), NONE)],
        action: Some(Action::CycleSort(Direction::Backwards)),
        display_override: None,
        desc: "Cycle library sort (order within group)",
        section: Section::Library,
        dispatch: true,
    },
    Binding {
        keys: &[(KeyCode::Char('m'), NONE)],
        action: Some(Action::CyclePlaylistDisplayMode),
        display_override: None,
        desc: "Cycle playlist display: hidden / count / names",
        section: Section::Library,
        dispatch: true,
    },
    Binding {
        keys: &[],
        action: Some(Action::ToggleSearch),
        display_override: None,
        desc: "Search by name / within the open playlist (live filter)",
        section: Section::Playlists,
        dispatch: false,
    },
    Binding {
        keys: &[],
        action: Some(Action::Activate),
        display_override: None,
        desc: "Open playlist / play selected song within it",
        section: Section::Playlists,
        dispatch: false,
    },
    Binding {
        keys: &[(KeyCode::Esc, NONE)],
        action: None,
        display_override: None,
        desc: "Back to playlist browser",
        section: Section::Playlists,
        dispatch: false,
    },
    Binding {
        keys: &[],
        action: Some(Action::CycleCategory(Direction::Forwards)),
        display_override: None,
        desc: "Cycle category within the open playlist",
        section: Section::Playlists,
        dispatch: false,
    },
    Binding {
        keys: &[],
        action: Some(Action::CycleCategory(Direction::Backwards)),
        display_override: None,
        desc: "Cycle category within the open playlist",
        section: Section::Playlists,
        dispatch: false,
    },
    Binding {
        keys: &[],
        action: Some(Action::CycleSort(Direction::Forwards)),
        display_override: None,
        desc: "Cycle sort within the open playlist",
        section: Section::Playlists,
        dispatch: false,
    },
    Binding {
        keys: &[],
        action: Some(Action::CycleSort(Direction::Backwards)),
        display_override: None,
        desc: "Cycle sort within the open playlist",
        section: Section::Playlists,
        dispatch: false,
    },
    Binding {
        keys: &[(KeyCode::Char('r'), NONE)],
        action: Some(Action::RemoveFromPlaylist),
        display_override: None,
        desc: "Remove selected song from playlist (confirm)",
        section: Section::Playlists,
        dispatch: true,
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModalKey {
    NextField,
    PrevField,
    Confirm,
    Cancel,
}

pub struct ModalBinding {
    pub keys: &'static [(KeyCode, KeyModifiers)],
    pub key: ModalKey,
}

pub const MODAL_BINDINGS: &[ModalBinding] = &[
    ModalBinding {
        keys: &[(KeyCode::Tab, NONE), (KeyCode::Down, NONE)],
        key: ModalKey::NextField,
    },
    ModalBinding {
        keys: &[(KeyCode::BackTab, NONE), (KeyCode::Up, NONE)],
        key: ModalKey::PrevField,
    },
    ModalBinding {
        keys: &[(KeyCode::Enter, NONE)],
        key: ModalKey::Confirm,
    },
    ModalBinding {
        keys: &[(KeyCode::Esc, NONE)],
        key: ModalKey::Cancel,
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmChoice {
    Yes,
    No,
}

pub struct ConfirmBinding {
    pub keys: &'static [(KeyCode, KeyModifiers)],
    pub choice: ConfirmChoice,
}

pub const CONFIRM_BINDINGS: &[ConfirmBinding] = &[
    ConfirmBinding {
        keys: &[
            (KeyCode::Char('y'), NONE),
            (KeyCode::Char('Y'), NONE),
            (KeyCode::Enter, NONE),
        ],
        choice: ConfirmChoice::Yes,
    },
    ConfirmBinding {
        keys: &[
            (KeyCode::Char('n'), NONE),
            (KeyCode::Char('N'), NONE),
            (KeyCode::Esc, NONE),
        ],
        choice: ConfirmChoice::No,
    },
];

pub fn confirm_lookup(key: KeyEvent) -> Option<ConfirmChoice> {
    CONFIRM_BINDINGS
        .iter()
        .find(|b| {
            b.keys
                .iter()
                .any(|&(code, mods)| code == key.code && mods_match(mods, key.modifiers))
        })
        .map(|b| b.choice)
}

pub fn confirm_display_for(choice: ConfirmChoice) -> String {
    CONFIRM_BINDINGS
        .iter()
        .find(|b| b.choice == choice)
        .and_then(|b| b.keys.first())
        .map(|&(code, mods)| render_key(code, mods))
        .unwrap_or_default()
}

pub fn modal_lookup(key: KeyEvent) -> Option<ModalKey> {
    MODAL_BINDINGS
        .iter()
        .find(|b| {
            b.keys
                .iter()
                .any(|&(code, mods)| code == key.code && mods_match(mods, key.modifiers))
        })
        .map(|b| b.key)
}

pub fn modal_display_for(key: ModalKey) -> String {
    MODAL_BINDINGS
        .iter()
        .find(|b| b.key == key)
        .map(|b| render_keys(b.keys))
        .unwrap_or_default()
}

pub fn footer_hint() -> String {
    let slots: &[(Action, &str)] = &[
        (Action::MoveUp, "select"),
        (Action::Activate, "play"),
        (Action::TogglePlayback, "pause"),
        (Action::TogglePanel, "playlists"),
        (Action::ToggleHelp, "more"),
        (Action::Quit, "quit"),
    ];
    slots
        .iter()
        .map(|(action, verb)| format!("{} {verb}", display_for(*action)))
        .collect::<Vec<_>>()
        .join(" · ")
}

pub fn lookup(key: KeyEvent) -> Option<Action> {
    BINDINGS
        .iter()
        .filter(|b| b.dispatch)
        .find(|b| {
            b.keys
                .iter()
                .any(|&(code, mods)| code == key.code && mods_match(mods, key.modifiers))
        })
        .and_then(|b| b.action)
}

fn mods_match(binding: KeyModifiers, pressed: KeyModifiers) -> bool {
    let shift = KeyModifiers::SHIFT;
    (binding - shift) == (pressed - shift)
}

pub fn display_for(action: Action) -> String {
    BINDINGS
        .iter()
        .find(|b| b.action == Some(action) && !b.keys.is_empty())
        .map(|b| render_keys(b.keys))
        .unwrap_or_default()
}

pub fn help_rows(section: Section) -> Vec<(String, &'static str)> {
    let mut rows: Vec<(String, &'static str)> = Vec::new();
    for b in BINDINGS.iter().filter(|b| b.section == section) {
        if let Some(last) = rows.last_mut()
            && last.1 == b.desc
        {
            last.0.push_str(" / ");
            last.0.push_str(&b.display());
            continue;
        }
        rows.push((b.display(), b.desc));
    }
    rows
}
