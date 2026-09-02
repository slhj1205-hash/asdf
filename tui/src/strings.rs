//! User-visible copy and the conventions it follows.
//!
//! Conventions:
//!
//! - Status bar text is lowercase and has no final period.
//! - Block titles are Title Case, padded with one space on each side.
//! - Panel body text is sentence case.
//! - Use an em dash for a separator, never two hyphens.
//! - Use contractions.
//! - Failure messages start with `failed to`.
//! - Never write `(s)`; call [`plural`].
//! - A song with no usable label falls back to [`UNTITLED_SONG`].
//!
//! `Up Next` is a proper noun for a pane and keeps its capitals inside an
//! otherwise lowercase status message.

pub const SELECT_SONG_FIRST: &str = "select a song first";
pub const NOTHING_PLAYING: &str = "nothing is playing";
pub const CLEARED_SEARCH: &str = "cleared search";
pub const UNTITLED_SONG: &str = "this song";
pub const UNTITLED_PLAYLIST: &str = "this playlist";

pub fn plural(count: usize, suffix: &str) -> &str {
    if count == 1 { "" } else { suffix }
}
