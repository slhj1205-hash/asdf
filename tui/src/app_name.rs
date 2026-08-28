pub const APP_NAME: &str = "Lyre";

pub fn kebab_case() -> String {
    APP_NAME.split_whitespace()
        .map(str::to_lowercase)
        .collect::<Vec<_>>()
        .join("-")
}

pub fn cache_file_name() -> String {
    format!(".{}-cache.json", kebab_case())
}

pub fn playlists_file_name() -> String {
    format!(".{}-playlists.json", kebab_case())
}
