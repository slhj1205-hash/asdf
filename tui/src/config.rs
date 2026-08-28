use std::path::{Path, PathBuf};

use lyre_core::SaveOutcome;
use lyre_core::fnv::FnvHasher;

use crate::app::{Category, PlaylistDisplayMode, Sort};
use crate::app_name;

#[derive(Default, serde::Serialize, serde::Deserialize)]
struct Config {
    last_dir: Option<PathBuf>,
    library_category: Option<Category>,
    library_sort: Option<Sort>,
    library_playlist_mode: Option<PlaylistDisplayMode>,
    playlist_category: Option<Category>,
    playlist_sort: Option<Sort>,
}

#[derive(Default)]
pub struct ViewState {
    pub library_category: Category,
    pub library_sort: Sort,
    pub library_playlist_mode: PlaylistDisplayMode,
    pub playlist_category: Category,
    pub playlist_sort: Sort,
}

fn config_path() -> Option<PathBuf> {
    let dir_name = app_name::kebab_case();
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME")
        && !xdg.is_empty()
    {
        return Some(PathBuf::from(xdg).join(&dir_name).join("config.json"));
    }
    if let Ok(home) = std::env::var("HOME") {
        return Some(
            PathBuf::from(home)
                .join(".config")
                .join(&dir_name)
                .join("config.json"),
        );
    }
    None
}

pub fn theme_path() -> Option<PathBuf> {
    config_dir().map(|dir| dir.join("theme.toml"))
}

fn config_dir() -> Option<PathBuf> {
    let dir_name = app_name::kebab_case();
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME")
        && !xdg.is_empty()
    {
        return Some(PathBuf::from(xdg).join(&dir_name));
    }
    if let Ok(home) = std::env::var("HOME") {
        return Some(PathBuf::from(home).join(".config").join(&dir_name));
    }
    None
}

pub fn data_dir() -> Option<PathBuf> {
    let dir_name = app_name::kebab_case();
    if let Ok(xdg) = std::env::var("XDG_DATA_HOME")
        && !xdg.is_empty()
    {
        return Some(PathBuf::from(xdg).join(&dir_name));
    }
    if let Ok(home) = std::env::var("HOME") {
        return Some(
            PathBuf::from(home)
                .join(".local")
                .join("share")
                .join(&dir_name),
        );
    }
    None
}

pub fn cache_dir() -> Option<PathBuf> {
    let dir_name = app_name::kebab_case();
    if let Ok(xdg) = std::env::var("XDG_CACHE_HOME")
        && !xdg.is_empty()
    {
        return Some(PathBuf::from(xdg).join(&dir_name));
    }
    if let Ok(home) = std::env::var("HOME") {
        return Some(PathBuf::from(home).join(".cache").join(&dir_name));
    }
    None
}

pub fn youtube_binaries_dir() -> Option<PathBuf> {
    cache_dir().map(|dir| dir.join("yt-dlp"))
}

fn hashed_root_filename(root: &Path, suffix: &str) -> String {
    use std::hash::{Hash, Hasher};

    let canonical = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let mut hasher = FnvHasher::default();
    canonical.hash(&mut hasher);
    format!("{:016x}{suffix}", hasher.finish())
}

pub fn scan_cache_path(root: &Path) -> PathBuf {
    let filename = hashed_root_filename(root, ".json");

    match cache_dir() {
        Some(dir) => dir.join(filename),
        None => root.join(app_name::cache_file_name()),
    }
}

pub fn playlists_path(root: &Path) -> PathBuf {
    let filename = hashed_root_filename(root, "-playlists.json");

    match data_dir() {
        Some(dir) => dir.join(filename),
        None => root.join(app_name::playlists_file_name()),
    }
}

pub fn load_last_dir() -> Option<PathBuf> {
    let path = config_path()?;
    let contents = std::fs::read_to_string(path).ok()?;
    let config: Config = serde_json::from_str(&contents).ok()?;
    config.last_dir.filter(|dir| dir.is_dir())
}

pub fn load_view_state() -> ViewState {
    let Some(path) = config_path() else {
        return ViewState::default();
    };
    let Ok(contents) = std::fs::read_to_string(path) else {
        return ViewState::default();
    };
    let Ok(config) = serde_json::from_str::<Config>(&contents) else {
        return ViewState::default();
    };

    ViewState {
        library_category: config.library_category.unwrap_or_default(),
        library_sort: config.library_sort.unwrap_or_default(),
        library_playlist_mode: config.library_playlist_mode.unwrap_or_default(),
        playlist_category: config.playlist_category.unwrap_or_default(),
        playlist_sort: config.playlist_sort.unwrap_or_default(),
    }
}

pub fn save_last_dir(dir: &Path) -> SaveOutcome {
    let Some(path) = config_path() else {
        return SaveOutcome::NothingToSave;
    };
    let mut config = read_config(&path);
    config.last_dir = Some(dir.to_path_buf());
    write_config(&path, &config)
}

pub fn save_view_state(state: &ViewState) -> SaveOutcome {
    let Some(path) = config_path() else {
        return SaveOutcome::NothingToSave;
    };
    let mut config = read_config(&path);
    config.library_category = Some(state.library_category);
    config.library_sort = Some(state.library_sort);
    config.library_playlist_mode = Some(state.library_playlist_mode);
    config.playlist_category = Some(state.playlist_category);
    config.playlist_sort = Some(state.playlist_sort);
    write_config(&path, &config)
}

fn read_config(path: &Path) -> Config {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|contents| serde_json::from_str(&contents).ok())
        .unwrap_or_default()
}

fn write_config(path: &Path, config: &Config) -> SaveOutcome {
    let json = match serde_json::to_string_pretty(config) {
        Ok(json) => json,
        Err(e) => return SaveOutcome::Failed(format!("failed to encode the config: {e}")),
    };
    match lyre_core::atomic::write(path, json.as_bytes()) {
        Ok(()) => SaveOutcome::Saved,
        Err(e) => SaveOutcome::Failed(format!(
            "failed to persist the config to {}: {e}",
            path.display()
        )),
    }
}
