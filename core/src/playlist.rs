use std::{
    collections::HashMap,
    fmt, fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use crate::{library::Library, song::SongId};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Playlist {
    id: PlaylistId,
    name: String,
    songs: Vec<SongId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PlaylistId([u8; 16]);

impl PlaylistId {
    pub fn new() -> PlaylistId {
        let mut bytes = [0u8; 16];
        crate::random::random_bytes(&mut bytes);
        bytes[6] = (bytes[6] & 0x0f) | 0x40;
        bytes[8] = (bytes[8] & 0x3f) | 0x80;
        PlaylistId(bytes)
    }
}

impl Default for PlaylistId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for PlaylistId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let b = &self.0;
        write!(
            f,
            "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            b[0],
            b[1],
            b[2],
            b[3],
            b[4],
            b[5],
            b[6],
            b[7],
            b[8],
            b[9],
            b[10],
            b[11],
            b[12],
            b[13],
            b[14],
            b[15]
        )
    }
}

impl serde::Serialize for PlaylistId {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> serde::Deserialize<'de> for PlaylistId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        parse_hex(&s).map_err(serde::de::Error::custom)
    }
}

fn parse_hex(s: &str) -> Result<PlaylistId, String> {
    let cleaned = s.replace(['-', '{', '}'], "");
    if cleaned.len() != 32 || !cleaned.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(format!("invalid playlist id: {s}"));
    }
    let mut bytes = [0u8; 16];
    for (i, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&cleaned[i * 2..i * 2 + 2], 16).map_err(|e| e.to_string())?;
    }
    Ok(PlaylistId(bytes))
}

impl Playlist {
    pub fn new(id: PlaylistId, name: impl Into<String>) -> Playlist {
        Playlist {
            id,
            name: name.into(),
            songs: Vec::new(),
        }
    }

    pub fn id(&self) -> PlaylistId {
        self.id
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn rename(&mut self, name: impl Into<String>) {
        self.name = name.into();
    }

    pub fn songs(&self) -> &[SongId] {
        &self.songs
    }
    pub fn len(&self) -> usize {
        self.songs.len()
    }
    pub fn is_empty(&self) -> bool {
        self.songs.is_empty()
    }

    pub fn add(&mut self, song: SongId) -> Mutated {
        if self.songs.contains(&song) {
            return Mutated::No;
        }
        self.songs.push(song);
        Mutated::Yes
    }

    pub fn remove_all(&mut self, song: SongId) {
        self.songs.retain(|&id| id != song);
    }

    pub fn contains(&self, song: SongId) -> bool {
        self.songs.contains(&song)
    }

    pub fn fuzzy_score(&self, query: &crate::fuzzy::FuzzyQuery) -> Option<u32> {
        let name = crate::fuzzy::Candidate::new(&self.name);

        let mut total = 0u32;
        for term in query.terms() {
            let score = crate::fuzzy::score(term, &name)?;
            total = total.saturating_add(score);
        }
        Some(total)
    }

    pub fn sort_name(&self) -> String {
        self.name.chars().flat_map(char::to_lowercase).collect()
    }

    pub(crate) fn retain_songs(&mut self, keep: impl Fn(SongId) -> bool) {
        self.songs.retain(|&id| keep(id));
    }
}

#[derive(Debug, Default, Clone)]
pub struct PruneStats {
    pub playlists_loaded: usize,
    pub songs_removed: usize,
    pub warnings: Vec<String>,
}

const FLUSH_AFTER: Duration = Duration::from_millis(750);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mutated {
    Yes,
    No,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveOutcome {
    Saved,
    NothingToSave,
    Failed(String),
}

pub struct PlaylistStore {
    path: PathBuf,
    playlists: HashMap<PlaylistId, Playlist>,
    sorted_ids: Vec<PlaylistId>,
    membership: HashMap<SongId, Vec<PlaylistId>>,
    revision: u64,
    dirty: bool,
    dirty_since: Option<Instant>,
}

impl PlaylistStore {
    pub fn load(path: impl AsRef<Path>, library: &Library) -> (PlaylistStore, PruneStats) {
        let path = path.as_ref().to_path_buf();
        let mut stats = PruneStats::default();
        let mut playlists = HashMap::new();

        let loaded: Vec<Playlist> = fs::read(&path)
            .ok()
            .and_then(|contents| serde_json::from_slice(&contents).ok())
            .unwrap_or_default();

        let mut pruned = false;
        for mut playlist in loaded {
            let before = playlist.len();
            playlist.retain_songs(|id| library.contains(id));
            let removed = before - playlist.len();
            if removed > 0 {
                stats.songs_removed += removed;
                pruned = true;
            }

            stats.playlists_loaded += 1;
            playlists.insert(playlist.id(), playlist);
        }

        let mut store = PlaylistStore {
            path,
            playlists,
            sorted_ids: Vec::new(),
            membership: HashMap::new(),
            revision: 0,
            dirty: false,
            dirty_since: None,
        };
        store.reindex();

        if pruned && let SaveOutcome::Failed(message) = store.save() {
            stats.warnings.push(message);
        }
        (store, stats)
    }

    pub fn empty(path: impl Into<PathBuf>) -> PlaylistStore {
        PlaylistStore {
            path: path.into(),
            playlists: HashMap::new(),
            sorted_ids: Vec::new(),
            membership: HashMap::new(),
            revision: 0,
            dirty: false,
            dirty_since: None,
        }
    }

    #[inline]
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn get(&self, id: PlaylistId) -> Option<&Playlist> {
        self.playlists.get(&id)
    }

    pub fn len(&self) -> usize {
        self.playlists.len()
    }
    pub fn is_empty(&self) -> bool {
        self.playlists.is_empty()
    }

    #[inline]
    pub fn ids_sorted_by_name(&self) -> &[PlaylistId] {
        &self.sorted_ids
    }

    #[inline]
    pub fn containing(&self, song: SongId) -> &[PlaylistId] {
        self.membership.get(&song).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn contains(&self, playlist: PlaylistId, song: SongId) -> bool {
        self.containing(song).contains(&playlist)
    }

    pub fn create(&mut self, name: impl Into<String>) -> PlaylistId {
        let id = PlaylistId::new();
        self.playlists.insert(id, Playlist::new(id, name));

        self.reindex();
        self.touch();
        id
    }

    pub fn rename(&mut self, id: PlaylistId, name: impl Into<String>) -> Mutated {
        let Some(playlist) = self.playlists.get_mut(&id) else {
            return Mutated::No;
        };
        playlist.rename(name);
        self.reindex();
        self.touch();
        Mutated::Yes
    }

    pub fn add_song(&mut self, id: PlaylistId, song: SongId) -> Mutated {
        let Some(playlist) = self.playlists.get_mut(&id) else {
            return Mutated::No;
        };
        if playlist.add(song) == Mutated::No {
            return Mutated::No;
        }

        self.membership.entry(song).or_default().push(id);
        self.touch();
        Mutated::Yes
    }

    pub fn remove_song(&mut self, id: PlaylistId, song: SongId) -> Mutated {
        let Some(playlist) = self.playlists.get_mut(&id) else {
            return Mutated::No;
        };
        let before = playlist.len();
        playlist.remove_all(song);
        if playlist.len() == before {
            return Mutated::No;
        }

        if let Some(entry) = self.membership.get_mut(&song) {
            entry.retain(|&other| other != id);
            if entry.is_empty() {
                self.membership.remove(&song);
            }
        }

        self.touch();
        Mutated::Yes
    }

    pub fn delete(&mut self, id: PlaylistId) -> Mutated {
        if self.playlists.remove(&id).is_none() {
            return Mutated::No;
        }
        self.reindex();
        self.touch();
        Mutated::Yes
    }

    fn reindex(&mut self) {
        let mut pairs: Vec<(&PlaylistId, &Playlist)> = self.playlists.iter().collect();
        pairs.sort_by_cached_key(|(id, playlist)| (playlist.name().to_lowercase(), **id));
        self.sorted_ids.clear();
        self.sorted_ids.extend(pairs.into_iter().map(|(id, _)| id));

        self.membership.clear();
        for id in &self.sorted_ids {
            if let Some(playlist) = self.playlists.get(id) {
                for &song in playlist.songs() {
                    let entry = self.membership.entry(song).or_default();
                    if !entry.contains(id) {
                        entry.push(*id);
                    }
                }
            }
        }
    }

    fn touch(&mut self) {
        self.revision += 1;
        self.mark_dirty();
    }

    fn mark_dirty(&mut self) {
        self.dirty = true;
        self.dirty_since.get_or_insert_with(Instant::now);
    }

    pub fn flush_if_due(&mut self) -> SaveOutcome {
        if self.dirty
            && self
                .dirty_since
                .is_some_and(|since| since.elapsed() >= FLUSH_AFTER)
        {
            return self.flush();
        }
        SaveOutcome::NothingToSave
    }

    pub fn flush(&mut self) -> SaveOutcome {
        if !self.dirty {
            return SaveOutcome::NothingToSave;
        }
        let outcome = self.save();
        self.dirty = false;
        self.dirty_since = None;
        outcome
    }

    fn save(&mut self) -> SaveOutcome {
        let all: Vec<&Playlist> = self
            .sorted_ids
            .iter()
            .filter_map(|id| self.playlists.get(id))
            .collect();
        let json = match serde_json::to_vec_pretty(&all) {
            Ok(json) => json,
            Err(e) => return SaveOutcome::Failed(format!("failed to encode playlists: {e}")),
        };

        match crate::atomic::write(&self.path, &json) {
            Ok(()) => SaveOutcome::Saved,
            Err(e) => SaveOutcome::Failed(format!(
                "failed to save playlists to {}: {e}",
                self.path.display()
            )),
        }
    }
}

impl Drop for PlaylistStore {
    fn drop(&mut self) {
        let _ = self.flush();
    }
}
