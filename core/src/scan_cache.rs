use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use crate::song::Metadata;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Probed {
    Tags(Metadata),
    Unreadable,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Entry {
    pub size: u64,
    pub mtime: u64,
    pub probed: Probed,
}

#[derive(Default)]
pub struct ScanCache {
    entries: HashMap<PathBuf, Entry>,
}

impl ScanCache {
    pub fn new() -> ScanCache {
        ScanCache::default()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn get(&self, relative: &Path) -> Option<&Entry> {
        self.entries.get(relative)
    }

    pub fn get_fresh(&self, relative: &Path, size: u64, mtime: u64) -> Option<&Probed> {
        let entry = self.entries.get(relative)?;
        (entry.size == size && entry.mtime == mtime).then_some(&entry.probed)
    }

    pub fn insert(&mut self, relative: PathBuf, entry: Entry) {
        self.entries.insert(relative, entry);
    }

    pub fn load(path: &Path) -> ScanCache {
        let entries: Vec<(PathBuf, Entry)> = std::fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        ScanCache {
            entries: entries.into_iter().collect(),
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        let mut ordered: Vec<(&PathBuf, &Entry)> = self.entries.iter().collect();
        ordered.sort_unstable_by(|a, b| a.0.cmp(b.0));

        let json = serde_json::to_vec_pretty(&ordered)
            .map_err(|e| format!("failed to encode the scan cache: {e}"))?;
        crate::atomic::write(path, &json).map_err(|e| {
            format!("failed to persist the scan cache to {}: {e}", path.display())
        })
    }
}
