use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use rayon::prelude::*;

use crate::{
    scan_cache::{Entry, Probed, ScanCache},
    song::{self, Metadata, MetadataEdits, Song, SongId, is_supported_audio, mtime_secs},
};

pub struct Library {
    root: PathBuf,
    songs: HashMap<SongId, Song>,
}

impl Library {
    pub fn scan(
        root: impl AsRef<Path>,
        cache_path: impl AsRef<Path>,
    ) -> Result<(Library, ScanStats), Error> {
        let root = root.as_ref();
        let cache_path = cache_path.as_ref();

        if !root.exists() {
            return Err(Error::PathNotFound(root.to_path_buf()));
        }
        if !root.is_dir() {
            return Err(Error::NotADirectory(root.to_path_buf()));
        }
        let root = root
            .canonicalize()
            .map_err(|_| Error::PathNotFound(root.to_path_buf()))?;

        let cache = ScanCache::load(cache_path);
        let mut stats = ScanStats::default();

        let mut files = Vec::new();
        collect_files(&root, &mut files, &mut stats);

        files.sort_unstable();
        stats.files_considered = files.len();

        let outcomes: Vec<Outcome> = files
            .par_iter()
            .map(|path| probe_file(path, &root, &cache))
            .collect();

        let mut songs: HashMap<SongId, Song> = HashMap::with_capacity(files.len());
        let mut next_cache = ScanCache::new();
        let mut cache_changed = false;

        for (path, outcome) in files.into_iter().zip(outcomes) {
            let Outcome {
                size,
                mtime,
                result,
            } = outcome;

            let relative = path.strip_prefix(&root).unwrap_or(&path).to_path_buf();

            match result {
                ProbeResult::NoMetadata => {
                    stats.skipped_files += 1;
                    continue;
                }
                ProbeResult::Unreadable { freshly_probed } => {
                    stats.skipped_files += 1;
                    if freshly_probed {
                        stats.reprobed += 1;
                        cache_changed = true;
                    } else {
                        stats.cache_hits += 1;
                    }
                    next_cache.insert(
                        relative,
                        Entry {
                            size,
                            mtime,
                            probed: Probed::Unreadable,
                        },
                    );
                }
                ProbeResult::Tags {
                    metadata,
                    freshly_probed,
                } => {
                    if freshly_probed {
                        stats.reprobed += 1;
                        cache_changed = true;
                    } else {
                        stats.cache_hits += 1;
                    }
                    next_cache.insert(
                        relative,
                        Entry {
                            size,
                            mtime,
                            probed: Probed::Tags(metadata.clone()),
                        },
                    );

                    let song = Song::from_cached_with_stat(path, size, mtime, metadata);
                    let skipped_path = song.path().to_path_buf();
                    if let InsertOutcome::Collision { existing } = insert_into(&mut songs, song) {
                        stats.skipped_files += 1;
                        if let Some(kept) = songs.get(&existing)
                            && kept.path() != skipped_path
                        {
                            stats.warnings.push(format!(
                                "song id collision between {} and {} -- kept the first, skipped the second",
                                kept.path().display(),
                                skipped_path.display()
                            ));
                        }
                    }
                }
            }
        }

        if next_cache.len() != cache.len() {
            cache_changed = true;
        }
        if cache_changed && let Err(message) = next_cache.save(cache_path) {
            stats.warnings.push(message);
        }

        Ok((Library { root, songs }, stats))
    }

    pub fn empty(root: impl Into<PathBuf>) -> Library {
        Library {
            root: root.into(),
            songs: HashMap::new(),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn get(&self, id: SongId) -> Option<&Song> {
        self.songs.get(&id)
    }

    pub fn contains(&self, id: SongId) -> bool {
        self.songs.contains_key(&id)
    }

    pub fn len(&self) -> usize {
        self.songs.len()
    }
    pub fn is_empty(&self) -> bool {
        self.songs.is_empty()
    }

    pub fn ids_by_path(&self) -> Vec<SongId> {
        let mut pairs: Vec<(&Path, SongId)> = self
            .songs
            .values()
            .map(|song| (song.path(), song.id()))
            .collect();
        pairs.sort_unstable();
        pairs.into_iter().map(|(_, id)| id).collect()
    }

    pub fn songs_by_path(&self) -> impl Iterator<Item = &Song> + '_ {
        let mut pairs: Vec<(&Path, SongId)> = self
            .songs
            .values()
            .map(|song| (song.path(), song.id()))
            .collect();
        pairs.sort_unstable();
        pairs
            .into_iter()
            .filter_map(move |(_, id)| self.songs.get(&id))
    }

    pub fn ids(&self) -> impl Iterator<Item = SongId> + '_ {
        self.songs.keys().copied()
    }

    pub fn update_metadata(
        &mut self,
        id: SongId,
        edits: &MetadataEdits,
    ) -> Result<(), UpdateMetadataError> {
        let Some(song) = self.songs.get(&id) else {
            return Err(UpdateMetadataError::NotFound);
        };
        let path = song.path().to_path_buf();

        Metadata::write(&path, edits)?;

        let updated = Song::load(&path)?;
        self.songs.insert(id, updated);

        Ok(())
    }

    pub fn count_matching_artist(&self, artist_sort_key: &str, exclude: SongId) -> usize {
        self.songs
            .values()
            .filter(|s| s.id() != exclude && s.sort_artist() == artist_sort_key)
            .count()
    }

    pub fn update_artist_sort_for_matching(
        &mut self,
        artist_sort_key: &str,
        artist_sort_value: &str,
        exclude: SongId,
    ) -> Vec<SongId> {
        let matching: Vec<SongId> = self
            .songs
            .values()
            .filter(|s| s.id() != exclude && s.sort_artist() == artist_sort_key)
            .map(Song::id)
            .collect();

        let mut updated_ids = Vec::with_capacity(matching.len());
        for id in matching {
            let Some(song) = self.songs.get(&id) else {
                continue;
            };
            let mut edits = MetadataEdits::from_metadata(song.metadata());
            edits.artist_sort = artist_sort_value.to_string();
            if self.update_metadata(id, &edits).is_ok() {
                updated_ids.push(id);
            }
        }
        updated_ids
    }

    pub fn insert(&mut self, song: Song) -> InsertOutcome {
        insert_into(&mut self.songs, song)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertOutcome {
    Inserted(SongId),
    Collision { existing: SongId },
}

#[derive(Debug, thiserror::Error)]
pub enum UpdateMetadataError {
    #[error("song not found in library")]
    NotFound,
    #[error(transparent)]
    Metadata(#[from] song::Error),
}

#[derive(Debug, Default, Clone)]
pub struct ScanStats {
    pub warnings: Vec<String>,

    pub cache_hits: usize,

    pub reprobed: usize,

    pub skipped_files: usize,

    pub unreadable_dirs: usize,

    pub files_considered: usize,
}

impl ScanStats {
    pub fn skipped(&self) -> usize {
        self.skipped_files + self.unreadable_dirs
    }
}

fn collect_files(dir: &Path, files: &mut Vec<PathBuf>, stats: &mut ScanStats) {
    let mut pending = vec![dir.to_path_buf()];

    while let Some(current) = pending.pop() {
        let entries = match fs::read_dir(&current) {
            Ok(entries) => entries,
            Err(e) => {
                stats.warnings.push(format!(
                    "failed to read directory {}: {e}",
                    current.display()
                ));
                stats.unreadable_dirs += 1;
                continue;
            }
        };

        for entry in entries {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    stats
                        .warnings
                        .push(format!("failed to read a directory entry: {e}"));
                    stats.unreadable_dirs += 1;
                    continue;
                }
            };

            let file_type = match entry.file_type() {
                Ok(ft) => ft,
                Err(e) => {
                    stats
                        .warnings
                        .push(format!("failed to stat {}: {e}", entry.path().display()));
                    stats.skipped_files += 1;
                    continue;
                }
            };

            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                pending.push(entry.path());
                continue;
            }
            if !file_type.is_file() {
                continue;
            }

            let path = entry.path();

            if is_supported_audio(&path) {
                files.push(path);
            }
        }
    }
}

struct Outcome {
    size: u64,
    mtime: u64,
    result: ProbeResult,
}

enum ProbeResult {
    Tags {
        metadata: Metadata,
        freshly_probed: bool,
    },
    Unreadable {
        freshly_probed: bool,
    },
    NoMetadata,
}

fn probe_file(path: &Path, root: &Path, cache: &ScanCache) -> Outcome {
    let Some(meta) = fs::metadata(path).ok() else {
        return Outcome {
            size: 0,
            mtime: 0,
            result: ProbeResult::NoMetadata,
        };
    };
    let (size, mtime) = (meta.len(), mtime_secs(&meta));

    let relative = path.strip_prefix(root).unwrap_or(path);

    match cache.get_fresh(relative, size, mtime) {
        Some(Probed::Tags(metadata)) => {
            return Outcome {
                size,
                mtime,
                result: ProbeResult::Tags {
                    metadata: metadata.clone(),
                    freshly_probed: false,
                },
            };
        }
        Some(Probed::Unreadable) => {
            return Outcome {
                size,
                mtime,
                result: ProbeResult::Unreadable {
                    freshly_probed: false,
                },
            };
        }
        None => {}
    }

    let probed = Metadata::probe(path);

    match probed {
        Ok(metadata) => Outcome {
            size,
            mtime,
            result: ProbeResult::Tags {
                metadata,
                freshly_probed: true,
            },
        },
        Err(_) => Outcome {
            size,
            mtime,
            result: ProbeResult::Unreadable {
                freshly_probed: true,
            },
        },
    }
}

fn insert_into(songs: &mut HashMap<SongId, Song>, song: Song) -> InsertOutcome {
    let id = song.id();
    if songs.contains_key(&id) {
        return InsertOutcome::Collision { existing: id };
    }
    songs.insert(id, song);
    InsertOutcome::Inserted(id)
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("path does not exist: {}", .0.display())]
    PathNotFound(PathBuf),
    #[error("not a directory: {}", .0.display())]
    NotADirectory(PathBuf),
}
