use std::{env, path::PathBuf, process::ExitCode};

use lyre_core::{Library, PlaylistStore, SaveOutcome};

use lyre_tui::{Backend, app::App, config};

fn main() -> ExitCode {
    let dir = env::args()
        .nth(1)
        .map(PathBuf::from)
        .or_else(config::load_last_dir)
        .unwrap_or_else(|| PathBuf::from("."));
    let cache_path = config::scan_cache_path(&dir);

    let library = match Library::scan(&dir, &cache_path) {
        Ok((library, stats)) => {
            for warning in &stats.warnings {
                eprintln!("warning: {warning}");
            }
            if stats.skipped() > 0 {
                eprintln!(
                    "warning: {} file(s) could not be loaded during scan",
                    stats.skipped()
                );
            }
            library
        }
        Err(e) => {
            eprintln!("failed to scan {}: {e}", dir.display());
            return ExitCode::FAILURE;
        }
    };

    if let SaveOutcome::Failed(message) = config::save_last_dir(library.root()) {
        eprintln!("warning: {message}");
    }

    let playlists_path = config::playlists_path(library.root());
    let (playlists, prune_stats) = PlaylistStore::load(playlists_path, &library);
    for warning in &prune_stats.warnings {
        eprintln!("warning: {warning}");
    }
    if prune_stats.songs_removed > 0 {
        eprintln!(
            "warning: removed {} missing song(s) across {} playlist(s)",
            prune_stats.songs_removed, prune_stats.playlists_loaded
        );
    }

    if let Some(theme_path) = config::theme_path() {
        lyre_tui::theme::init_from_path(&theme_path);
    }

    let mut app = App::new(library, playlists, Backend::detect());

    let view_state = config::load_view_state();
    app.apply_view_state(view_state);

    match ratatui::run(|terminal| app.run(terminal)) {
        Ok(warnings) => {
            for warning in &warnings {
                eprintln!("warning: {warning}");
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
