#[cfg(not(unix))]
compile_error!("lyre runs on unix systems only");

pub mod atomic;
pub mod fnv;
pub mod fuzzy;
#[cfg(feature = "gstreamer")]
pub mod gst;
pub mod library;
pub mod player;
pub mod playlist;
pub mod queue;
pub mod random;
pub mod scan_cache;
pub mod song;
pub mod youtube;

pub use fuzzy::{Candidate, FuzzyQuery, Pattern};
pub use library::{InsertOutcome, Library, ScanStats, UpdateMetadataError};
pub use player::{NullBackend, Player};
pub use playlist::{Mutated, Playlist, PlaylistId, PlaylistStore, PruneStats, SaveOutcome};
pub use queue::Queue;
pub use song::{Metadata, MetadataEdits, Song, SongId, needs_romanization};
pub use youtube::generate_file_name;
