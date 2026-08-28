use std::{
    fmt, fs,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use crate::fnv::FnvHasher;
use crate::fuzzy;

use lofty::{
    config::WriteOptions,
    file::{AudioFile, TaggedFileExt},
    probe::Probe,
    tag::{Accessor, ItemKey, Tag, items::Timestamp},
};

pub const SUPPORTED_EXTENSIONS: &[&str] = &[
    "aac", "aif", "aifc", "aiff", "ape", "flac", "m4a", "m4b", "m4p", "mp1", "mp2", "mp3", "mp4",
    "mpc", "mpp", "oga", "ogg", "opus", "spx", "wav", "wave", "wv",
];

const MAX_EXTENSION_LEN: usize = 5;

pub fn is_supported_audio(path: &Path) -> bool {
    let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
        return false;
    };
    if ext.is_empty() || ext.len() > MAX_EXTENSION_LEN || !ext.is_ascii() {
        return false;
    }
    let mut buf = [0u8; MAX_EXTENSION_LEN];
    let bytes = ext.as_bytes();
    #[allow(clippy::indexing_slicing)]
    {
        buf[..bytes.len()].copy_from_slice(bytes);
        buf[..bytes.len()].make_ascii_lowercase();
        let lower = &buf[..bytes.len()];
        SUPPORTED_EXTENSIONS
            .iter()
            .any(|known| known.as_bytes() == lower)
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct SongId(u64);

impl SongId {
    pub fn compute(path: &Path) -> SongId {
        let mut hasher = FnvHasher::default();
        path.hash(&mut hasher);
        SongId(hasher.finish())
    }

    pub fn from_path(path: &Path) -> SongId {
        let canonical = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        SongId::compute(&canonical)
    }
}

impl fmt::Display for SongId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:016x}", self.0)
    }
}

pub fn needs_romanization(text: &str) -> bool {
    !text.is_ascii()
}

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Metadata {
    pub title: Option<Arc<str>>,
    pub artist: Option<Arc<str>>,
    pub album: Option<Arc<str>>,
    pub genre: Option<Arc<str>>,
    pub track: Option<u32>,
    #[serde(with = "timestamp_serde")]
    pub date: Option<Timestamp>,
    pub duration: Duration,
    pub title_sort: Option<Arc<str>>,
    pub artist_sort: Option<Arc<str>>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MetadataEdits {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub genre: String,
    pub track: String,
    pub date: String,
    pub title_sort: String,
    pub artist_sort: String,
}

impl MetadataEdits {
    pub fn from_metadata(metadata: &Metadata) -> MetadataEdits {
        MetadataEdits {
            title: metadata.title.as_deref().unwrap_or_default().to_string(),
            artist: metadata.artist.as_deref().unwrap_or_default().to_string(),
            album: metadata.album.as_deref().unwrap_or_default().to_string(),
            genre: metadata.genre.as_deref().unwrap_or_default().to_string(),
            track: metadata.track.map(|t| t.to_string()).unwrap_or_default(),
            date: metadata.date.map(|d| d.to_string()).unwrap_or_default(),
            title_sort: metadata
                .title_sort
                .as_deref()
                .unwrap_or_default()
                .to_string(),
            artist_sort: metadata
                .artist_sort
                .as_deref()
                .unwrap_or_default()
                .to_string(),
        }
    }
}

fn set_text(tag: &mut Tag, value: &str, set: fn(&mut Tag, String), remove: fn(&mut Tag)) {
    if value.is_empty() {
        remove(tag);
    } else {
        set(tag, value.to_string());
    }
}

fn set_sort_field(tag: &mut Tag, value: &str, key: ItemKey) {
    if value.is_empty() {
        tag.remove_key(key);
    } else {
        tag.insert_text(key, value.to_string());
    }
}

fn parse_track(input: &str) -> Result<Option<u32>, Error> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    trimmed
        .parse::<u32>()
        .map(Some)
        .map_err(|_| Error::InvalidTrack(input.to_string()))
}

fn parse_date(input: &str) -> Result<Option<Timestamp>, Error> {
    use std::str::FromStr;

    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    Timestamp::from_str(trimmed)
        .map(Some)
        .map_err(|_| Error::InvalidDate(input.to_string()))
}

mod timestamp_serde {
    use lofty::tag::items::Timestamp;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::str::FromStr;

    pub fn serialize<S>(value: &Option<Timestamp>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        value.map(|t| t.to_string()).serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Timestamp>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw: Option<String> = Option::deserialize(deserializer)?;
        raw.map(|s| Timestamp::from_str(&s).map_err(serde::de::Error::custom))
            .transpose()
    }
}

impl Metadata {
    pub fn probe(path: &Path) -> Result<Metadata, Error> {
        let probed = Probe::open(path).map_err(|source| Error::Probe {
            path: path.to_path_buf(),
            source,
        })?;
        let tagged_file = probed.read().map_err(|source| Error::Read {
            path: path.to_path_buf(),
            source,
        })?;
        Ok(Metadata::from_tagged_file(&tagged_file))
    }

    pub fn write(path: &Path, edits: &MetadataEdits) -> Result<(), Error> {
        let track = parse_track(&edits.track)?;
        let date = parse_date(&edits.date)?;

        let probed = Probe::open(path).map_err(|source| Error::Probe {
            path: path.to_path_buf(),
            source,
        })?;
        let mut tagged_file = probed.read().map_err(|source| Error::Read {
            path: path.to_path_buf(),
            source,
        })?;

        let tag_type = tagged_file.primary_tag_type();
        if tagged_file.tag(tag_type).is_none() && tagged_file.first_tag().is_none() {
            tagged_file.insert_tag(Tag::new(tag_type));
        }
        let tag = match tagged_file.tag_mut(tag_type) {
            Some(tag) => tag,
            None => match tagged_file.first_tag_mut() {
                Some(tag) => tag,
                None => {
                    return Err(Error::Unwritable {
                        path: path.to_path_buf(),
                    });
                }
            },
        };

        set_text(tag, edits.title.trim(), Tag::set_title, Tag::remove_title);
        set_text(
            tag,
            edits.artist.trim(),
            Tag::set_artist,
            Tag::remove_artist,
        );
        set_text(tag, edits.album.trim(), Tag::set_album, Tag::remove_album);
        set_text(tag, edits.genre.trim(), Tag::set_genre, Tag::remove_genre);
        set_sort_field(tag, edits.title_sort.trim(), ItemKey::TrackTitleSortOrder);
        set_sort_field(tag, edits.artist_sort.trim(), ItemKey::TrackArtistSortOrder);
        match track {
            Some(track) => tag.set_track(track),
            None => tag.remove_track(),
        }
        match date {
            Some(date) => tag.set_date(date),
            None => tag.remove_date(),
        }

        tagged_file
            .save_to_path(path, WriteOptions::default())
            .map_err(|source| Error::Write {
                path: path.to_path_buf(),
                source,
            })
    }

    fn from_tagged_file(tf: &lofty::file::TaggedFile) -> Self {
        let tag = tf.primary_tag().or_else(|| tf.first_tag());
        Metadata {
            title: tag.and_then(|t| t.title()).map(|c| Arc::from(c.as_ref())),
            artist: tag.and_then(|t| t.artist()).map(|c| Arc::from(c.as_ref())),
            album: tag.and_then(|t| t.album()).map(|c| Arc::from(c.as_ref())),
            genre: tag.and_then(|t| t.genre()).map(|c| Arc::from(c.as_ref())),
            track: tag.and_then(|t| t.track()),
            date: tag.and_then(|t| t.date()),
            duration: tf.properties().duration(),
            title_sort: tag
                .and_then(|t| t.get_string(ItemKey::TrackTitleSortOrder))
                .map(Arc::from),
            artist_sort: tag
                .and_then(|t| t.get_string(ItemKey::TrackArtistSortOrder))
                .map(Arc::from),
        }
    }
}

pub const UNKNOWN_TITLE: &str = "Unknown Title";
pub const UNKNOWN_ARTIST: &str = "Unknown Artist";
pub const UNKNOWN_ALBUM: &str = "Unknown Album";

#[derive(Debug)]
struct SortKeys {
    title: Box<str>,
    artist: Box<str>,
    album: Box<str>,
    title_sort: Option<Box<str>>,
    artist_sort: Option<Box<str>>,
}

impl SortKeys {
    fn build(
        title: &str,
        artist: &str,
        album: &str,
        title_sort: Option<&str>,
        artist_sort: Option<&str>,
    ) -> SortKeys {
        SortKeys {
            title: title.chars().flat_map(char::to_lowercase).collect(),
            artist: artist.chars().flat_map(char::to_lowercase).collect(),
            album: album.chars().flat_map(char::to_lowercase).collect(),
            title_sort: title_sort.map(|s| s.chars().flat_map(char::to_lowercase).collect()),
            artist_sort: artist_sort.map(|s| s.chars().flat_map(char::to_lowercase).collect()),
        }
    }

    fn title(&self) -> &str {
        &self.title
    }
    fn artist(&self) -> &str {
        &self.artist
    }
    fn album(&self) -> &str {
        &self.album
    }
    fn title_sort(&self) -> Option<&str> {
        self.title_sort.as_deref()
    }
    fn artist_sort(&self) -> Option<&str> {
        self.artist_sort.as_deref()
    }
}

#[derive(Clone, Debug)]
pub struct Song {
    id: SongId,
    path: Arc<Path>,
    metadata: Arc<Metadata>,
    keys: Arc<SortKeys>,

    mtime_secs: u64,
}

impl Song {
    pub fn load(path: impl AsRef<Path>) -> Result<Song, Error> {
        let path = path.as_ref();
        let metadata = Metadata::probe(path)?;
        let mtime = fs::metadata(path).map(|m| mtime_secs(&m)).unwrap_or(0);
        Ok(Song::assemble(
            SongId::from_path(path),
            Arc::from(path),
            metadata,
            mtime,
        ))
    }

    pub fn from_cached_with_stat(
        path: PathBuf,
        _len: u64,
        modified_secs: u64,
        metadata: Metadata,
    ) -> Song {
        let id = SongId::compute(&path);
        Song::assemble(id, Arc::from(path), metadata, modified_secs)
    }

    fn assemble(id: SongId, path: Arc<Path>, metadata: Metadata, mtime_secs: u64) -> Song {
        let title = metadata.title.as_deref().unwrap_or_else(|| stem_of(&path));
        let artist = metadata.artist.as_deref().unwrap_or(UNKNOWN_ARTIST);
        let album = metadata.album.as_deref().unwrap_or(UNKNOWN_ALBUM);
        let keys = Arc::new(SortKeys::build(
            title,
            artist,
            album,
            metadata.title_sort.as_deref(),
            metadata.artist_sort.as_deref(),
        ));

        Song {
            id,
            path,
            metadata: Arc::new(metadata),
            keys,
            mtime_secs,
        }
    }

    #[inline]
    pub fn id(&self) -> SongId {
        self.id
    }
    #[inline]
    pub fn path(&self) -> &Path {
        &self.path
    }
    #[inline]
    pub fn metadata(&self) -> &Metadata {
        &self.metadata
    }

    #[inline]
    pub fn modified(&self) -> u64 {
        self.mtime_secs
    }

    pub fn title(&self) -> &str {
        self.metadata
            .title
            .as_deref()
            .unwrap_or_else(|| stem_of(&self.path))
    }
    pub fn artist(&self) -> &str {
        self.metadata.artist.as_deref().unwrap_or(UNKNOWN_ARTIST)
    }
    pub fn album(&self) -> &str {
        self.metadata.album.as_deref().unwrap_or(UNKNOWN_ALBUM)
    }

    #[inline]
    pub fn sort_title(&self) -> &str {
        self.keys.title()
    }

    #[inline]
    pub fn sort_artist(&self) -> &str {
        self.keys.artist()
    }

    #[inline]
    pub fn fuzzy_term_score(&self, term: &str) -> Option<u32> {
        if term.is_empty() {
            return Some(0);
        }
        let title_field = self.keys.title();
        let artist_field = self.keys.artist();
        let album_field = self.keys.album();

        let title = fuzzy::subsequence_score(term, title_field)
            .map(|s| fuzzy::normalize_by_length(s, title_field.chars().count()) * 3 / 2);
        let artist = fuzzy::subsequence_score(term, artist_field)
            .map(|s| fuzzy::normalize_by_length(s, artist_field.chars().count()));
        let album = fuzzy::subsequence_score(term, album_field)
            .map(|s| fuzzy::normalize_by_length(s, album_field.chars().count()));

        let title_sort = self.keys.title_sort().and_then(|field| {
            fuzzy::subsequence_score(term, field)
                .map(|s| fuzzy::normalize_by_length(s, field.chars().count()) * 3 / 2)
        });
        let artist_sort = self.keys.artist_sort().and_then(|field| {
            fuzzy::subsequence_score(term, field)
                .map(|s| fuzzy::normalize_by_length(s, field.chars().count()))
        });

        [title, artist, album, title_sort, artist_sort]
            .into_iter()
            .flatten()
            .max()
    }

    pub fn fuzzy_score(&self, terms: &[&str]) -> Option<u32> {
        let mut total = 0u32;
        for term in terms {
            total += self.fuzzy_term_score(term)?;
        }
        Some(total)
    }

    pub fn fingerprint(path: &Path) -> Option<(u64, u64)> {
        let meta = fs::metadata(path).ok()?;
        Some((meta.len(), mtime_secs(&meta)))
    }
}

pub(crate) fn mtime_secs(meta: &fs::Metadata) -> u64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn stem_of(path: &Path) -> &str {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(UNKNOWN_TITLE)
}

impl PartialEq for Song {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}
impl Eq for Song {}
impl Hash for Song {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl fmt::Display for Song {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} - {} ({})", self.artist(), self.title(), self.album())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("could not open or identify audio file at {path}: {source}")]
    Probe {
        path: PathBuf,
        #[source]
        source: lofty::error::LoftyError,
    },
    #[error("failed to read tags from {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: lofty::error::LoftyError,
    },
    #[error("failed to write tags to {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: lofty::error::LoftyError,
    },
    #[error("invalid track number \"{0}\"")]
    InvalidTrack(String),
    #[error("invalid date \"{0}\" (try a year like 2024 or a full date like 2024-01-31)")]
    InvalidDate(String),
    #[error("{} does not support any tag format lyre can write to", path.display())]
    Unwritable { path: PathBuf },
}
