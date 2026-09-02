#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

mod fixtures;

use std::{path::Path, time::Duration};

use fixtures::{write_song, write_untagged_song};
use lyre_core::{
    FuzzyQuery, NullBackend, Player, fuzzy, generate_file_name,
    library::{InsertOutcome, Library},
    needs_romanization,
    player::{AudioBackend, PlaybackState},
    playlist::{Mutated, PlaylistStore},
    queue::Queue,
    scan_cache::{Entry, Probed, ScanCache},
    song::{Metadata, MetadataEdits, Song, SongId, is_supported_audio},
};
use tempfile::TempDir;

#[test]
fn is_supported_audio_accepts_known_extensions_case_insensitively() {
    for name in [
        "track.mp3",
        "track.MP3",
        "track.Flac",
        "track.OGG",
        "track.wav",
        "track.opus",
    ] {
        assert!(
            is_supported_audio(Path::new(name)),
            "{name} should be recognised as audio"
        );
    }
}

#[test]
fn is_supported_audio_rejects_non_audio_and_malformed_names() {
    for name in [
        "cover.jpg",
        "readme.txt",
        "playlist.m3u",
        "no_extension",
        "track.mp3.bak",
        ".",
    ] {
        assert!(
            !is_supported_audio(Path::new(name)),
            "{name} should not be recognised as audio"
        );
    }
}

#[test]
fn library_scan_finds_only_supported_audio_files() {
    let dir = TempDir::new().unwrap();
    write_song(dir.path(), "one.wav", "One", "Artist", "Album");
    write_song(dir.path(), "two.wav", "Two", "Artist", "Album");
    std::fs::write(dir.path().join("cover.jpg"), b"not audio").unwrap();
    std::fs::write(dir.path().join("notes.txt"), b"not audio").unwrap();

    let (library, stats) = Library::scan(dir.path(), dir.path().join("cache.bin")).unwrap();

    assert_eq!(library.len(), 2);
    assert_eq!(stats.files_considered, 2);
}

#[test]
fn library_scan_recurses_into_subdirectories() {
    let dir = TempDir::new().unwrap();
    write_song(
        &dir.path().join("Artist A"),
        "one.wav",
        "One",
        "Artist A",
        "Album",
    );
    write_song(
        &dir.path().join("Artist B").join("Album B"),
        "two.wav",
        "Two",
        "Artist B",
        "Album B",
    );

    let (library, _) = Library::scan(dir.path(), dir.path().join("cache.bin")).unwrap();

    assert_eq!(library.len(), 2);
}

#[test]
fn library_scan_on_a_missing_path_is_an_error() {
    let dir = TempDir::new().unwrap();
    let missing = dir.path().join("does-not-exist");

    let result = Library::scan(&missing, dir.path().join("cache.bin"));

    assert!(result.is_err());
}

#[test]
fn library_scan_on_a_file_rather_than_a_directory_is_an_error() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("not-a-dir.txt");
    std::fs::write(&file, b"hello").unwrap();

    let result = Library::scan(&file, dir.path().join("cache.bin"));

    assert!(result.is_err());
}

#[test]
fn library_get_and_contains_agree_with_each_other() {
    let dir = TempDir::new().unwrap();
    write_song(dir.path(), "one.wav", "One", "Artist", "Album");
    let (library, _) = Library::scan(dir.path(), dir.path().join("cache.bin")).unwrap();

    let id = library.ids_by_path()[0];
    assert!(library.contains(id));
    assert!(library.get(id).is_some());

    let bogus = SongId::compute(Path::new("/nowhere"));
    assert!(!library.contains(bogus));
    assert!(library.get(bogus).is_none());
}

#[test]
fn library_ids_by_path_are_sorted_by_file_path() {
    let dir = TempDir::new().unwrap();
    write_song(dir.path(), "z.wav", "Z Song", "Artist", "Album");
    write_song(dir.path(), "a.wav", "A Song", "Artist", "Album");
    write_song(dir.path(), "m.wav", "M Song", "Artist", "Album");
    let (library, _) = Library::scan(dir.path(), dir.path().join("cache.bin")).unwrap();

    let titles: Vec<&str> = library.songs_by_path().map(|s| s.title()).collect();
    assert_eq!(titles, vec!["A Song", "M Song", "Z Song"]);
}

#[test]
fn library_ids_by_path_stay_sorted_after_an_insert_sorting_before_existing_entries() {
    let dir = TempDir::new().unwrap();
    write_song(dir.path(), "m.wav", "M Song", "Artist", "Album");
    write_song(dir.path(), "z.wav", "Z Song", "Artist", "Album");
    let (mut library, _) = Library::scan(dir.path(), dir.path().join("cache.bin")).unwrap();

    let earlier_path = write_song(dir.path(), "a.wav", "A Song", "Artist", "Album");
    library.insert(Song::load(&earlier_path).unwrap());

    let titles: Vec<&str> = library.songs_by_path().map(|s| s.title()).collect();
    assert_eq!(titles, vec!["A Song", "M Song", "Z Song"]);
}

#[test]
fn library_scan_is_served_from_cache_on_the_second_pass() {
    let dir = TempDir::new().unwrap();
    write_song(dir.path(), "one.wav", "One", "Artist", "Album");
    let cache_path = dir.path().join("cache.bin");

    let (_, first) = Library::scan(dir.path(), &cache_path).unwrap();
    assert_eq!(first.reprobed, 1);
    assert_eq!(first.cache_hits, 0);

    let (_, second) = Library::scan(dir.path(), &cache_path).unwrap();
    assert_eq!(second.reprobed, 0);
    assert_eq!(second.cache_hits, 1);
}

#[test]
fn library_reprobes_a_file_after_it_changes() {
    let dir = TempDir::new().unwrap();
    let path = write_song(dir.path(), "one.wav", "One", "Artist", "Album");
    let cache_path = dir.path().join("cache.bin");
    Library::scan(dir.path(), &cache_path).unwrap();

    std::thread::sleep(Duration::from_millis(1100));
    std::fs::write(
        &path,
        fixtures::wav("One (Remaster)", "Artist", "Album", 400),
    )
    .unwrap();

    let (library, stats) = Library::scan(dir.path(), &cache_path).unwrap();
    assert_eq!(stats.reprobed, 1);
    assert_eq!(
        library.songs_by_path().next().unwrap().title(),
        "One (Remaster)"
    );
}

#[test]
fn library_drops_songs_that_are_deleted_between_scans() {
    let dir = TempDir::new().unwrap();
    let path = write_song(dir.path(), "one.wav", "One", "Artist", "Album");
    let cache_path = dir.path().join("cache.bin");
    let (library, _) = Library::scan(dir.path(), &cache_path).unwrap();
    assert_eq!(library.len(), 1);

    std::fs::remove_file(&path).unwrap();
    let (library, _) = Library::scan(dir.path(), &cache_path).unwrap();
    assert_eq!(library.len(), 0);
}

#[test]
fn metadata_write_round_trips_through_probe() {
    let dir = TempDir::new().unwrap();
    let path = write_song(dir.path(), "one.wav", "One", "Artist", "Album");

    let edits = MetadataEdits {
        title: "New Title".to_string(),
        artist: "New Artist".to_string(),
        album: "New Album".to_string(),
        genre: "Synthwave".to_string(),
        track: "7".to_string(),
        date: "2024".to_string(),
        title_sort: String::new(),
        artist_sort: String::new(),
    };
    Metadata::write(&path, &edits).unwrap();

    let metadata = Metadata::probe(&path).unwrap();
    assert_eq!(metadata.title.as_deref(), Some("New Title"));
    assert_eq!(metadata.artist.as_deref(), Some("New Artist"));
    assert_eq!(metadata.album.as_deref(), Some("New Album"));
    assert_eq!(metadata.genre.as_deref(), Some("Synthwave"));
    assert_eq!(metadata.track, Some(7));
    assert!(metadata.date.is_some());
}

#[test]
fn metadata_write_with_an_empty_field_clears_the_tag() {
    let dir = TempDir::new().unwrap();
    let path = write_song(dir.path(), "one.wav", "One", "Artist", "Album");

    let mut edits = MetadataEdits::from_metadata(&Metadata::probe(&path).unwrap());
    assert_eq!(edits.artist, "Artist");
    edits.artist.clear();
    Metadata::write(&path, &edits).unwrap();

    let metadata = Metadata::probe(&path).unwrap();
    assert_eq!(
        metadata.artist, None,
        "an empty field in the edit form must clear the tag, not leave it untouched"
    );
}

#[test]
fn metadata_write_rejects_a_non_numeric_track() {
    let dir = TempDir::new().unwrap();
    let path = write_song(dir.path(), "one.wav", "One", "Artist", "Album");

    let mut edits = MetadataEdits::from_metadata(&Metadata::probe(&path).unwrap());
    edits.track = "not a number".to_string();

    assert!(Metadata::write(&path, &edits).is_err());
}

#[test]
fn updating_metadata_keeps_the_song_id_stable() {
    let dir = TempDir::new().unwrap();
    write_song(dir.path(), "one.wav", "One", "Artist", "Album");
    let (mut library, _) = Library::scan(dir.path(), dir.path().join("cache.bin")).unwrap();
    let id = library.ids_by_path()[0];

    let mut edits = MetadataEdits::from_metadata(library.get(id).unwrap().metadata());
    edits.title = "One (Remaster)".to_string();

    library.update_metadata(id, &edits).unwrap();

    assert_eq!(library.len(), 1);
    assert!(
        library.contains(id),
        "the song must keep its identity across an edit"
    );
    assert_eq!(library.get(id).unwrap().title(), "One (Remaster)");
    assert_eq!(library.ids_by_path(), &[id]);
}

#[test]
fn library_update_metadata_on_an_unknown_song_is_an_error() {
    let dir = TempDir::new().unwrap();
    let (mut library, _) = Library::scan(dir.path(), dir.path().join("cache.bin")).unwrap();
    let missing = SongId::compute(Path::new("missing.mp3"));

    assert!(
        library
            .update_metadata(missing, &MetadataEdits::default())
            .is_err()
    );
}

#[test]
fn playlists_survive_metadata_edits_without_rewrites() {
    let dir = TempDir::new().unwrap();
    let playlists_dir = dir.path().join("playlists");
    write_song(dir.path(), "song.wav", "Song", "Artist", "Album");
    let (mut library, _) = Library::scan(dir.path(), dir.path().join("cache.bin")).unwrap();
    let id = library.ids_by_path()[0];

    let mut store = PlaylistStore::empty(&playlists_dir);
    let playlist_id = store.create("Favourites");
    store.add_song(playlist_id, id);

    let mut edits = MetadataEdits::from_metadata(library.get(id).unwrap().metadata());
    edits.title = "Song (Remaster)".to_string();
    library.update_metadata(id, &edits).unwrap();

    assert_eq!(
        store.get(playlist_id).unwrap().songs(),
        &[id],
        "playlist membership must be untouched by a metadata edit"
    );
    assert!(store.containing(id).contains(&playlist_id));

    store.flush();

    let (reloaded, stats) = PlaylistStore::load(&playlists_dir, &library);
    assert_eq!(
        stats.songs_removed, 0,
        "the edited song must still be considered present after reload"
    );
    assert_eq!(reloaded.get(playlist_id).unwrap().songs(), &[id]);
}

#[test]
fn scan_cache_round_trips_through_disk() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("cache.bin");

    let mut cache = ScanCache::new();
    cache.insert(
        Path::new("song.mp3").to_path_buf(),
        Entry {
            size: 123,
            mtime: 456,
            probed: Probed::Unreadable,
        },
    );
    cache.save(&path).expect("saving the scan cache to a temp dir must succeed");

    let loaded = ScanCache::load(&path);
    assert_eq!(loaded.len(), 1);
    assert_eq!(
        loaded.get_fresh(Path::new("song.mp3"), 123, 456),
        Some(&Probed::Unreadable)
    );
}

#[test]
fn scan_cache_get_fresh_rejects_a_stale_fingerprint() {
    let mut cache = ScanCache::new();
    cache.insert(
        Path::new("song.mp3").to_path_buf(),
        Entry {
            size: 100,
            mtime: 200,
            probed: Probed::Unreadable,
        },
    );

    assert!(cache.get_fresh(Path::new("song.mp3"), 999, 200).is_none());
    assert!(cache.get_fresh(Path::new("song.mp3"), 100, 999).is_none());
    assert!(cache.get_fresh(Path::new("song.mp3"), 100, 200).is_some());
}

#[test]
fn scan_cache_treats_a_missing_or_corrupt_file_as_empty() {
    let dir = TempDir::new().unwrap();

    let missing = ScanCache::load(&dir.path().join("does-not-exist.bin"));
    assert!(missing.is_empty());

    let corrupt_path = dir.path().join("corrupt.bin");
    std::fs::write(&corrupt_path, b"not a valid cache file").unwrap();
    let corrupt = ScanCache::load(&corrupt_path);
    assert!(corrupt.is_empty());
}

#[test]
fn playlist_store_create_rename_and_delete_round_trip() {
    let dir = TempDir::new().unwrap();
    let mut store = PlaylistStore::empty(dir.path().join("playlists"));

    let id = store.create("Road Trip");
    assert_eq!(store.get(id).unwrap().name(), "Road Trip");

    assert_eq!(store.rename(id, "Summer Trip"), Mutated::Yes);
    assert_eq!(store.get(id).unwrap().name(), "Summer Trip");

    assert_eq!(store.delete(id), Mutated::Yes);
    assert!(store.get(id).is_none());
}

#[test]
fn playlist_store_persists_across_a_reload() {
    let dir = TempDir::new().unwrap();
    let playlists_dir = dir.path().join("playlists");
    write_song(dir.path(), "song.wav", "Song", "Artist", "Album");
    let (library, _) = Library::scan(dir.path(), dir.path().join("cache.bin")).unwrap();
    let song = library.ids_by_path()[0];

    let id = {
        let mut store = PlaylistStore::empty(&playlists_dir);
        let id = store.create("Favourites");
        store.add_song(id, song);
        id
    };

    let (reloaded, _) = PlaylistStore::load(&playlists_dir, &library);
    let playlist = reloaded
        .get(id)
        .expect("playlist should have been persisted to disk");
    assert_eq!(playlist.name(), "Favourites");
    assert_eq!(playlist.songs(), &[song]);
}

#[test]
fn playlist_store_prunes_songs_that_are_no_longer_in_the_library() {
    let dir = TempDir::new().unwrap();
    let playlists_dir = dir.path().join("playlists");
    let missing_song = SongId::compute(Path::new("missing.mp3"));

    {
        let mut store = PlaylistStore::empty(&playlists_dir);
        let id = store.create("Mix");
        store.add_song(id, missing_song);
    }

    let empty_library = Library::empty(dir.path());
    let (store, stats) = PlaylistStore::load(&playlists_dir, &empty_library);

    assert_eq!(stats.songs_removed, 1);
    let id = store.ids_sorted_by_name()[0];
    assert!(store.get(id).unwrap().is_empty());
}

#[test]
fn playlist_store_add_song_is_idempotent() {
    let dir = TempDir::new().unwrap();
    let mut store = PlaylistStore::empty(dir.path().join("playlists"));
    let id = store.create("Mix");
    let song = SongId::compute(Path::new("song.mp3"));

    assert_eq!(store.add_song(id, song), Mutated::Yes);
    assert_eq!(
        store.add_song(id, song),
        Mutated::No,
        "adding the same song twice should be a no-op"
    );
    assert_eq!(store.get(id).unwrap().len(), 1);
}

#[test]
fn playlist_store_remove_song_updates_membership() {
    let dir = TempDir::new().unwrap();
    let mut store = PlaylistStore::empty(dir.path().join("playlists"));
    let id = store.create("Mix");
    let song = SongId::compute(Path::new("song.mp3"));
    store.add_song(id, song);

    assert!(store.contains(id, song));
    assert_eq!(store.remove_song(id, song), Mutated::Yes);
    assert!(!store.contains(id, song));
    assert!(store.containing(song).is_empty());
}

#[test]
fn playlist_store_containing_lists_every_playlist_holding_a_song() {
    let dir = TempDir::new().unwrap();
    let mut store = PlaylistStore::empty(dir.path().join("playlists"));
    let song = SongId::compute(Path::new("song.mp3"));

    let a = store.create("A");
    let b = store.create("B");
    store.add_song(a, song);
    store.add_song(b, song);

    let membership = store.containing(song);
    assert_eq!(membership.len(), 2);
    assert!(membership.contains(&a));
    assert!(membership.contains(&b));
}

#[test]
fn playlist_store_ids_sorted_by_name_are_case_insensitive() {
    let dir = TempDir::new().unwrap();
    let mut store = PlaylistStore::empty(dir.path().join("playlists"));
    store.create("banana");
    store.create("Apple");
    store.create("cherry");

    let names: Vec<&str> = store
        .ids_sorted_by_name()
        .iter()
        .map(|&id| store.get(id).unwrap().name())
        .collect();
    assert_eq!(names, vec!["Apple", "banana", "cherry"]);
}

#[test]
fn playlist_store_defers_writing_to_disk_until_flush_is_due() {
    let dir = TempDir::new().unwrap();
    let playlists_dir = dir.path().join("playlists");
    let mut store = PlaylistStore::empty(&playlists_dir);

    store.create("Mix");
    assert!(
        !playlists_dir.exists(),
        "a fresh mutation should not hit disk immediately"
    );

    store.flush_if_due();
    assert!(
        !playlists_dir.exists(),
        "flush_if_due should not write before the debounce window elapses"
    );

    std::thread::sleep(Duration::from_millis(800));
    store.flush_if_due();
    assert!(
        playlists_dir.exists(),
        "flush_if_due should write once the debounce window has elapsed"
    );
}

#[test]
fn playlist_store_flush_deadline_is_anchored_to_the_first_pending_change() {
    let dir = TempDir::new().unwrap();
    let playlists_dir = dir.path().join("playlists");
    let mut store = PlaylistStore::empty(&playlists_dir);
    let song = SongId::compute(Path::new("song.mp3"));

    let id = store.create("Mix");
    std::thread::sleep(Duration::from_millis(400));
    store.add_song(id, song);

    std::thread::sleep(Duration::from_millis(400));
    store.flush_if_due();
    assert!(
        playlists_dir.exists(),
        "the debounce window should be measured from the first pending change, not reset by later ones"
    );
}

#[test]
fn playlist_store_flush_writes_immediately_regardless_of_the_debounce_window() {
    let dir = TempDir::new().unwrap();
    let playlists_dir = dir.path().join("playlists");
    let mut store = PlaylistStore::empty(&playlists_dir);

    store.create("Mix");
    store.flush();
    assert!(
        playlists_dir.exists(),
        "flush should write immediately without waiting for the debounce window"
    );
}

#[test]
fn playlist_store_flushes_pending_changes_on_drop() {
    let dir = TempDir::new().unwrap();
    let playlists_dir = dir.path().join("playlists");
    {
        let mut store = PlaylistStore::empty(&playlists_dir);
        store.create("Mix");
    }
    assert!(
        playlists_dir.exists(),
        "dropping the store should flush any pending write"
    );
}

#[test]
fn playlist_store_revision_advances_on_mutation_but_not_on_reads() {
    let dir = TempDir::new().unwrap();
    let mut store = PlaylistStore::empty(dir.path().join("playlists"));
    let before = store.revision();

    let id = store.create("Mix");
    assert!(store.revision() > before);

    let after_create = store.revision();
    let _ = store.get(id);
    let _ = store.ids_sorted_by_name();
    assert_eq!(
        store.revision(),
        after_create,
        "read-only calls must not bump the revision"
    );
}

#[test]
fn queue_next_walks_forward_and_wraps_to_the_start() {
    let ids: Vec<SongId> = (0..3)
        .map(|i| SongId::compute(Path::new(&format!("{i}.mp3"))))
        .collect();
    let mut queue = Queue::new(ids.clone());

    assert_eq!(queue.next(), Some(ids[0]));
    assert_eq!(queue.next(), Some(ids[1]));
    assert_eq!(queue.next(), Some(ids[2]));
    assert_eq!(
        queue.next(),
        Some(ids[0]),
        "past the end must wrap to the start"
    );
}

#[test]
fn queue_previous_walks_backward_and_wraps_to_the_end() {
    let ids: Vec<SongId> = (0..3)
        .map(|i| SongId::compute(Path::new(&format!("{i}.mp3"))))
        .collect();
    let mut queue = Queue::new(ids.clone());

    assert_eq!(
        queue.previous(),
        Some(ids[2]),
        "previous with nothing playing must land on the last song"
    );
    assert_eq!(queue.previous(), Some(ids[1]));
}

#[test]
fn queue_on_an_empty_queue_returns_nothing() {
    let mut queue = Queue::new(Vec::new());
    assert_eq!(queue.next(), None);
    assert_eq!(queue.previous(), None);
    assert_eq!(queue.current_id(), None);
}

#[test]
fn queue_play_id_jumps_to_a_specific_song() {
    let ids: Vec<SongId> = (0..3)
        .map(|i| SongId::compute(Path::new(&format!("{i}.mp3"))))
        .collect();
    let mut queue = Queue::new(ids.clone());

    assert_eq!(queue.play_id(ids[2]), Some(ids[2]));
    assert_eq!(queue.current_id(), Some(ids[2]));
}

#[test]
fn queue_play_id_prefers_the_nearest_occurrence_after_the_cursor() {
    let a = SongId::compute(Path::new("a.mp3"));
    let b = SongId::compute(Path::new("b.mp3"));

    let mut queue = Queue::new(vec![a, b, a, b]);
    queue.play_at(1);

    queue.play_id(a);
    assert_eq!(
        queue.current_position(),
        Some(2),
        "should land on the closer occurrence of `a` at index 2"
    );
}

#[test]
fn queue_play_upcoming_jumps_forward_by_n() {
    let ids: Vec<SongId> = (0..5)
        .map(|i| SongId::compute(Path::new(&format!("{i}.mp3"))))
        .collect();
    let mut queue = Queue::new(ids.clone());

    assert_eq!(queue.play_upcoming(3), Some(ids[2]));
}

#[test]
fn queue_play_upcoming_zero_does_nothing() {
    let ids: Vec<SongId> = (0..3)
        .map(|i| SongId::compute(Path::new(&format!("{i}.mp3"))))
        .collect();
    let mut queue = Queue::new(ids);

    assert_eq!(queue.play_upcoming(0), None);
    assert_eq!(queue.current_id(), None);
}

#[test]
fn queue_priority_songs_play_before_the_regular_queue() {
    let ids: Vec<SongId> = (0..3)
        .map(|i| SongId::compute(Path::new(&format!("{i}.mp3"))))
        .collect();
    let priority = SongId::compute(Path::new("priority.mp3"));

    let mut queue = Queue::new(ids.clone());
    queue.queue_next(priority);

    assert_eq!(queue.next(), Some(priority));
    assert_eq!(
        queue.next(),
        Some(ids[0]),
        "after the priority song, playback resumes at the queue's start"
    );
}

#[test]
fn queue_upcoming_lists_priority_songs_then_the_regular_queue() {
    let ids: Vec<SongId> = (0..3)
        .map(|i| SongId::compute(Path::new(&format!("{i}.mp3"))))
        .collect();
    let priority = SongId::compute(Path::new("priority.mp3"));

    let mut queue = Queue::new(ids.clone());
    queue.play_at(0);
    queue.queue_next(priority);

    assert_eq!(queue.upcoming(3), vec![priority, ids[1], ids[2]]);
}

#[test]
fn queue_shuffle_preserves_the_currently_playing_song() {
    let ids: Vec<SongId> = (0..20)
        .map(|i| SongId::compute(Path::new(&format!("{i}.mp3"))))
        .collect();
    let mut queue = Queue::new(ids.clone());
    queue.play_at(5);
    let current = queue.current_id();

    queue.shuffle();

    assert_eq!(
        queue.current_id(),
        current,
        "shuffling must not change which song is playing"
    );
}

#[test]
fn queue_unshuffle_restores_original_order() {
    let ids: Vec<SongId> = (0..5)
        .map(|i| SongId::compute(Path::new(&format!("{i}.mp3"))))
        .collect();
    let mut queue = Queue::new(ids.clone());

    queue.shuffle();
    queue.unshuffle();

    assert_eq!(queue.ordered_ids().collect::<Vec<_>>(), ids);
}

#[test]
fn queue_contains_reflects_membership() {
    let a = SongId::compute(Path::new("a.mp3"));
    let b = SongId::compute(Path::new("b.mp3"));
    let queue = Queue::new(vec![a]);

    assert!(queue.contains(a));
    assert!(!queue.contains(b));
}

#[test]
fn player_toggle_moves_between_playing_and_paused() {
    let mut player = Player::new(NullBackend::new());
    let dir = TempDir::new().unwrap();
    let path = write_song(dir.path(), "song.wav", "Song", "Artist", "Album");
    let song = lyre_core::song::Song::load(&path).unwrap();

    player.play(&song).unwrap();
    assert_eq!(player.state(), PlaybackState::Playing);

    player.toggle().unwrap();
    assert_eq!(player.state(), PlaybackState::Paused);

    player.toggle().unwrap();
    assert_eq!(player.state(), PlaybackState::Playing);
}

#[test]
fn player_stop_returns_to_idle_and_is_a_no_op_when_already_idle() {
    let mut player = Player::new(NullBackend::new());
    assert_eq!(player.state(), PlaybackState::Idle);

    assert!(player.stop().is_ok());
    assert_eq!(player.state(), PlaybackState::Idle);
}

#[test]
fn player_volume_is_clamped_between_zero_and_one() {
    let mut player = Player::new(NullBackend::new());

    player.set_volume(5.0);
    assert_eq!(player.volume(), 1.0);

    player.set_volume(-5.0);
    assert_eq!(player.volume(), 0.0);
}

#[test]
fn player_adjust_volume_clamps_at_the_bounds() {
    let mut player = Player::new(NullBackend::new());
    player.set_volume(0.05);

    player.adjust_volume(-1.0);
    assert_eq!(player.volume(), 0.0);

    player.adjust_volume(1.0);
    assert_eq!(player.volume(), 1.0);
}

#[test]
fn null_backend_reports_no_position_until_something_is_loaded() {
    let backend = NullBackend::new();
    assert_eq!(backend.position(), None);
}

#[test]
fn null_backend_position_advances_after_play_uri() {
    let mut backend = NullBackend::new();
    backend.play_uri("file:///tmp/whatever.mp3").unwrap();
    assert!(backend.position().is_some());

    std::thread::sleep(Duration::from_millis(20));
    assert!(backend.position().unwrap() >= Duration::from_millis(20));
}

#[test]
fn null_backend_stop_clears_the_loaded_track() {
    let mut backend = NullBackend::new();
    backend.play_uri("file:///tmp/whatever.mp3").unwrap();
    assert!(backend.position().is_some());

    backend.stop().unwrap();
    assert_eq!(backend.position(), None);
}

#[test]
fn atomic_write_round_trips_bytes_and_leaves_no_temp_file_behind() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("data.bin");

    lyre_core::atomic::write(&path, b"hello world").unwrap();

    assert_eq!(std::fs::read(&path).unwrap(), b"hello world");
    let leftovers: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().contains(".tmp"))
        .collect();
    assert!(
        leftovers.is_empty(),
        "no temp files should remain after a successful write"
    );
}

#[test]
fn song_falls_back_to_the_file_stem_when_no_title_tag_is_present() {
    let dir = TempDir::new().unwrap();
    let path = write_song(dir.path(), "untitled_track.wav", "", "", "");
    let song = lyre_core::song::Song::load(&path).unwrap();

    assert_eq!(song.title(), "untitled_track");
    assert_eq!(song.artist(), "Unknown Artist");
    assert_eq!(song.album(), "Unknown Album");
}

#[test]
fn song_sort_keys_are_lowercase() {
    let dir = TempDir::new().unwrap();
    let path = write_song(
        dir.path(),
        "song.wav",
        "Song Title",
        "Song Artist",
        "Song Album",
    );
    let song = lyre_core::song::Song::load(&path).unwrap();

    assert_eq!(song.sort_title(), "song title");
    assert_eq!(song.sort_artist(), "song artist");
}

#[test]
fn song_fuzzy_term_score_requires_every_term_to_match() {
    let dir = TempDir::new().unwrap();
    let path = write_song(
        dir.path(),
        "song.wav",
        "Neon Skyline",
        "Static Prairie",
        "Wide Fields",
    );
    let song = lyre_core::song::Song::load(&path).unwrap();

    assert!(song.fuzzy_score(&FuzzyQuery::new("neon")).is_some());
    assert!(song.fuzzy_score(&FuzzyQuery::new("neon prairie")).is_some());
    assert!(
        song.fuzzy_score(&FuzzyQuery::new("neon zzz")).is_none(),
        "a term that matches nothing should fail the whole query"
    );
}

#[test]
fn song_fuzzy_term_score_expects_an_already_lowercased_term() {
    let dir = TempDir::new().unwrap();
    let path = write_song(dir.path(), "song.wav", "Neon Skyline", "Artist", "Album");
    let song = lyre_core::song::Song::load(&path).unwrap();

    assert!(
        song.fuzzy_score(&FuzzyQuery::new("neon")).is_some(),
        "callers are expected to lowercase the query before matching"
    );
}

#[test]
fn song_fuzzy_term_score_rewards_a_match_at_a_word_boundary() {
    let dir = TempDir::new().unwrap();
    let boundary = write_song(dir.path(), "boundary.wav", "Blue Sky", "Artist", "Album");
    let midword = write_song(dir.path(), "midword.wav", "Ruby Sky", "Artist", "Album");
    let boundary_song = lyre_core::song::Song::load(&boundary).unwrap();
    let midword_song = lyre_core::song::Song::load(&midword).unwrap();

    let boundary_score = boundary_song.fuzzy_term_score("b").unwrap();
    let midword_score = midword_song.fuzzy_term_score("b").unwrap();

    assert!(
        boundary_score > midword_score,
        "a match at a word boundary should score higher than mid-word"
    );
}

#[test]
fn song_fuzzy_term_score_of_an_empty_term_matches_everything_with_zero_score() {
    let dir = TempDir::new().unwrap();
    let path = write_song(dir.path(), "song.wav", "Anything", "Artist", "Album");
    let song = lyre_core::song::Song::load(&path).unwrap();

    assert_eq!(song.fuzzy_term_score(""), Some(0));
}

#[test]
fn generate_file_name_capitalizes_each_word_and_strips_apostrophes() {
    assert_eq!(
        generate_file_name("John leSmith's", "38 cats"),
        "JohnLeSmiths-38Cats.mp3"
    );
}

#[test]
fn generate_file_name_splits_words_on_hyphens_but_not_apostrophes() {
    assert_eq!(
        generate_file_name("Rock-n-Roll", "Don't Stop"),
        "RockNRoll-DontStop.mp3"
    );
}

#[test]
fn generate_file_name_drops_a_word_made_entirely_of_punctuation() {
    assert_eq!(generate_file_name("!!!", "Title"), "-Title.mp3");
}

#[test]
fn generate_file_name_handles_an_empty_artist_or_title() {
    assert_eq!(generate_file_name("", "Title"), "-Title.mp3");
    assert_eq!(generate_file_name("Artist", ""), "Artist-.mp3");
    assert_eq!(generate_file_name("", ""), "-.mp3");
}

#[test]
fn library_insert_adds_a_new_song_and_reports_inserted() {
    let dir = TempDir::new().unwrap();
    let path = write_song(dir.path(), "one.wav", "One", "Artist", "Album");
    let song = Song::load(&path).unwrap();
    let id = song.id();

    let mut library = Library::empty(dir.path());
    let outcome = library.insert(song);

    assert_eq!(outcome, InsertOutcome::Inserted(id));
    assert!(library.contains(id));
    assert_eq!(library.ids_by_path(), &[id]);
}

#[test]
fn library_insert_reports_a_collision_without_replacing_the_existing_song() {
    let dir = TempDir::new().unwrap();
    let path = write_song(dir.path(), "one.wav", "One", "Artist", "Album");
    let first = Song::load(&path).unwrap();
    let second = Song::load(&path).unwrap();
    let id = first.id();

    let mut library = Library::empty(dir.path());
    library.insert(first);
    let outcome = library.insert(second);

    assert_eq!(outcome, InsertOutcome::Collision { existing: id });
    assert_eq!(library.len(), 1);
}

#[test]
fn queue_play_id_finds_a_song_inserted_after_construction() {
    let dir = TempDir::new().unwrap();
    let existing = write_song(dir.path(), "one.wav", "One", "Artist", "Album");
    let existing_song = Song::load(&existing).unwrap();
    let existing_id = existing_song.id();

    let mut queue = Queue::new(vec![existing_id]);

    let downloaded = write_song(dir.path(), "two.wav", "Two", "Artist", "Album");
    let downloaded_song = Song::load(&downloaded).unwrap();
    let downloaded_id = downloaded_song.id();
    queue.insert(downloaded_id);

    assert_eq!(queue.play_id(downloaded_id), Some(downloaded_id));
    assert_eq!(queue.current_id(), Some(downloaded_id));
}

#[test]
fn fuzzy_subsequence_score_matches_characters_in_order_but_not_necessarily_contiguous() {
    assert!(fuzzy::subsequence_score("btl", "beetle").is_some());
    assert!(fuzzy::subsequence_score("tbl", "beetle").is_none());
}

#[test]
fn fuzzy_subsequence_score_of_an_empty_pattern_matches_anything_with_zero_score() {
    assert_eq!(fuzzy::subsequence_score("", "anything"), Some(0));
}

#[test]
fn fuzzy_subsequence_score_rejects_a_pattern_longer_than_the_target() {
    assert_eq!(fuzzy::subsequence_score("longer", "short"), None);
}

#[test]
fn fuzzy_subsequence_score_rewards_a_match_starting_at_a_word_boundary() {
    let boundary = fuzzy::subsequence_score("b", "blue sky").unwrap();
    let midword = fuzzy::subsequence_score("b", "ruby sky").unwrap();

    assert!(boundary > midword);
}

#[test]
fn fuzzy_score_prefers_a_shorter_field_for_the_same_match() {
    let short = fuzzy::subsequence_score("blue", "blue").unwrap_or(0);
    let long = fuzzy::subsequence_score("blue", "blue monday").unwrap_or(0);

    assert!(short > long);
}

#[test]
fn fuzzy_score_prefers_a_compact_alignment() {
    let compact = fuzzy::subsequence_score("ab", "a---ab").unwrap_or(0);
    let scattered = fuzzy::subsequence_score("ab", "a---b").unwrap_or(0);

    assert!(compact > scattered);
}

#[test]
fn fuzzy_score_ignores_accents_and_punctuation() {
    assert!(fuzzy::subsequence_score("cafe", "Caf\u{e9}").is_some());
    assert!(fuzzy::subsequence_score("beyonce", "Beyonc\u{e9}").is_some());
    assert!(fuzzy::subsequence_score("dont", "Don\u{2019}t Stop").is_some());
    assert!(fuzzy::subsequence_score("acdc", "AC/DC").is_some());
}

#[test]
fn fuzzy_score_ranks_a_prefix_above_a_mid_string_match() {
    let prefix = fuzzy::subsequence_score("blue", "blue moon").unwrap_or(0);
    let midway = fuzzy::subsequence_score("blue", "electric blue").unwrap_or(0);

    assert!(prefix > midway);
}

#[test]
fn needs_romanization_is_false_for_any_ascii_text_including_punctuation() {
    assert!(!needs_romanization("Cafe De Flore"));
    assert!(!needs_romanization("Song #7 (Remix) - feat. Someone"));
    assert!(!needs_romanization(""));
}

#[test]
fn needs_romanization_is_true_for_non_ascii_characters() {
    assert!(needs_romanization("夜明け"));
    assert!(needs_romanization("Кино"));
    assert!(
        needs_romanization("Café"),
        "accented Latin is still non-ASCII"
    );
    assert!(
        !needs_romanization("Cafe"),
        "the unaccented spelling stays false"
    );
}

#[test]
fn metadata_write_round_trips_romanized_title_and_artist_through_probe() {
    let dir = TempDir::new().unwrap();
    let path = write_untagged_song(dir.path(), "one.wav");

    let edits = MetadataEdits {
        title: "夜明け".to_string(),
        artist: "アーティスト".to_string(),
        album: "Album".to_string(),
        genre: String::new(),
        track: String::new(),
        date: String::new(),
        title_sort: "Yoake".to_string(),
        artist_sort: "Artist".to_string(),
    };
    Metadata::write(&path, &edits).unwrap();

    let metadata = Metadata::probe(&path).unwrap();
    assert_eq!(metadata.title.as_deref(), Some("夜明け"));
    assert_eq!(metadata.title_sort.as_deref(), Some("Yoake"));
    assert_eq!(metadata.artist_sort.as_deref(), Some("Artist"));
}

#[test]
fn metadata_write_of_an_empty_romanized_field_removes_it() {
    let dir = TempDir::new().unwrap();
    let path = write_untagged_song(dir.path(), "one.wav");

    let with_sort = MetadataEdits {
        title: "夜明け".to_string(),
        artist: "Artist".to_string(),
        album: "Album".to_string(),
        genre: String::new(),
        track: String::new(),
        date: String::new(),
        title_sort: "Yoake".to_string(),
        artist_sort: String::new(),
    };
    Metadata::write(&path, &with_sort).unwrap();
    assert_eq!(
        Metadata::probe(&path).unwrap().title_sort.as_deref(),
        Some("Yoake")
    );

    let without_sort = MetadataEdits {
        title_sort: String::new(),
        ..with_sort
    };
    Metadata::write(&path, &without_sort).unwrap();
    assert_eq!(Metadata::probe(&path).unwrap().title_sort, None);
}

#[test]
fn song_fuzzy_score_finds_a_song_by_its_romanized_title() {
    let dir = TempDir::new().unwrap();
    let path = write_untagged_song(dir.path(), "one.wav");

    let edits = MetadataEdits {
        title: "夜明け".to_string(),
        artist: "Artist".to_string(),
        album: "Album".to_string(),
        genre: String::new(),
        track: String::new(),
        date: String::new(),
        title_sort: "Yoake".to_string(),
        artist_sort: String::new(),
    };
    Metadata::write(&path, &edits).unwrap();
    let song = Song::load(&path).unwrap();

    assert!(
        song.fuzzy_score(&FuzzyQuery::new("yoake")).is_some(),
        "the romanized title must be searchable"
    );
    assert!(song.fuzzy_score(&FuzzyQuery::new("zzz")).is_none());
}

#[test]
fn song_sort_title_ignores_the_romanized_field() {
    let dir = TempDir::new().unwrap();
    let path = write_untagged_song(dir.path(), "one.wav");

    let edits = MetadataEdits {
        title: "夜明け".to_string(),
        artist: "Artist".to_string(),
        album: "Album".to_string(),
        genre: String::new(),
        track: String::new(),
        date: String::new(),
        title_sort: "Yoake".to_string(),
        artist_sort: String::new(),
    };
    Metadata::write(&path, &edits).unwrap();
    let song = Song::load(&path).unwrap();

    assert_eq!(
        song.sort_title(),
        "夜明け",
        "romanization must not change what the list sorts by"
    );
}

#[test]
fn library_update_artist_sort_for_matching_applies_to_every_other_song_with_the_same_artist() {
    let dir = TempDir::new().unwrap();
    let cache_path = dir.path().join("cache.bin");

    let a = write_untagged_song(dir.path(), "a.wav");
    Metadata::write(
        &a,
        &MetadataEdits {
            artist: "Alpha".to_string(),
            ..MetadataEdits::default()
        },
    )
    .unwrap();
    let b = write_untagged_song(dir.path(), "b.wav");
    Metadata::write(
        &b,
        &MetadataEdits {
            artist: "Alpha".to_string(),
            ..MetadataEdits::default()
        },
    )
    .unwrap();
    let c = write_untagged_song(dir.path(), "c.wav");
    Metadata::write(
        &c,
        &MetadataEdits {
            artist: "Beta".to_string(),
            ..MetadataEdits::default()
        },
    )
    .unwrap();

    let (mut library, _) = Library::scan(dir.path(), &cache_path).unwrap();
    let song_a = library
        .songs_by_path()
        .find(|s| s.path() == a)
        .unwrap()
        .id();
    let song_b = library
        .songs_by_path()
        .find(|s| s.path() == b)
        .unwrap()
        .id();
    let song_c = library
        .songs_by_path()
        .find(|s| s.path() == c)
        .unwrap()
        .id();
    let artist_key = library.get(song_a).unwrap().sort_artist().to_string();

    assert_eq!(
        library.count_matching_artist(&artist_key, song_a),
        1,
        "only song_b shares the artist"
    );

    let updated = library.update_artist_sort_for_matching(&artist_key, "Arufa", song_a);
    assert_eq!(updated.len(), 1);
    assert_eq!(updated[0], song_b);

    assert_eq!(
        library
            .get(song_b)
            .unwrap()
            .metadata()
            .artist_sort
            .as_deref(),
        Some("Arufa")
    );
    assert_eq!(
        library.get(song_c).unwrap().metadata().artist_sort,
        None,
        "a different artist must be untouched"
    );
}

#[cfg(all(feature = "youtube", unix))]
#[test]
fn fetch_and_download_survives_a_child_that_floods_stderr() {
    use std::{fs, os::unix::fs::PermissionsExt};

    let dir = TempDir::new().unwrap();
    let binaries_dir = dir.path().join("bin");
    let scratch_dir = dir.path().join("scratch");
    fs::create_dir_all(&binaries_dir).unwrap();
    fs::create_dir_all(&scratch_dir).unwrap();

    let stub = binaries_dir.join("yt-dlp");
    fs::write(
        &stub,
        "#!/bin/sh\nif [ \"$1\" = \"--dump-json\" ]; then\n  echo '{\"title\": \"t\", \"is_live\": false}'\n  exit 0\nfi\nyes 'warning: filler line to flood the stderr pipe' | head -c 200000 >&2\nexit 3\n",
    )
    .unwrap();
    fs::set_permissions(&stub, fs::Permissions::from_mode(0o755)).unwrap();

    let result = lyre_core::youtube::fetch_and_download(
        "https://example.com/watch?v=x",
        &binaries_dir,
        &scratch_dir,
        |_| {},
        |_| {},
    );

    match result {
        Err(lyre_core::youtube::Error::DownloadFailed(3)) => {}
        other => panic!(
            "expected DownloadFailed(3) without hanging, got {other:?}"
        ),
    }
}

#[test]
fn the_null_backend_describes_itself_as_silent() {
    let backend = NullBackend::new();
    assert_eq!(backend.describe(), "none (silent mode)");
    assert!(!backend.describe().contains("gstreamer"));
}


#[test]
fn shuffle_returns_a_permutation_and_never_drops_or_duplicates_an_element() {
    use lyre_core::random::shuffle;

    for len in [0usize, 1, 2, 3, 17, 500] {
        let original: Vec<usize> = (0..len).collect();
        let mut shuffled = original.clone();
        shuffle(&mut shuffled);

        assert_eq!(
            shuffled.len(),
            original.len(),
            "shuffle must not change the length of a {len}-element slice"
        );

        let mut sorted = shuffled.clone();
        sorted.sort_unstable();
        assert_eq!(
            sorted, original,
            "shuffle of a {len}-element slice must be a permutation"
        );
    }
}

#[test]
fn shuffle_of_a_slice_with_fewer_than_two_elements_leaves_it_untouched() {
    use lyre_core::random::shuffle;

    let mut empty: Vec<u8> = Vec::new();
    shuffle(&mut empty);
    assert!(empty.is_empty());

    let mut single = vec![42u8];
    shuffle(&mut single);
    assert_eq!(single, vec![42u8]);
}

#[test]
fn shuffle_actually_reorders_a_large_slice() {
    use lyre_core::random::shuffle;

    let original: Vec<usize> = (0..1_000).collect();
    let mut shuffled = original.clone();
    shuffle(&mut shuffled);

    let fixed_points = original
        .iter()
        .zip(shuffled.iter())
        .filter(|(a, b)| a == b)
        .count();
    assert!(
        fixed_points < 100,
        "a shuffle of 1000 elements left {fixed_points} in place -- that is not a shuffle"
    );
}

#[test]
fn entropy_below_always_returns_a_value_under_its_bound() {
    use lyre_core::random::Entropy;

    let mut entropy = Entropy::new();
    for bound in [1u64, 2, 3, 7, 64, 1_000, u64::MAX] {
        for _ in 0..200 {
            let value = entropy.below(bound);
            assert!(
                value < bound.max(1),
                "below({bound}) returned {value}, which is not under the bound"
            );
        }
    }
}

#[test]
fn entropy_below_covers_every_value_in_a_small_range() {
    use lyre_core::random::Entropy;

    let mut entropy = Entropy::new();
    let mut seen = [false; 6];
    for _ in 0..2_000 {
        let value = entropy.below(6) as usize;
        if let Some(slot) = seen.get_mut(value) {
            *slot = true;
        }
    }
    assert!(
        seen.iter().all(|hit| *hit),
        "below(6) never produced some values across 2000 draws: {seen:?}"
    );
}

#[test]
fn a_clean_playlist_store_reports_that_there_was_nothing_to_save() {
    use lyre_core::SaveOutcome;

    let dir = tempfile::TempDir::new().unwrap();
    let library = Library::empty(dir.path());
    let (mut store, _) = PlaylistStore::load(dir.path().join("playlists.json"), &library);

    assert_eq!(store.flush(), SaveOutcome::NothingToSave);
    assert_eq!(store.flush_if_due(), SaveOutcome::NothingToSave);
}

#[test]
fn a_dirty_playlist_store_reports_a_successful_save() {
    use lyre_core::SaveOutcome;

    let dir = tempfile::TempDir::new().unwrap();
    let library = Library::empty(dir.path());
    let (mut store, _) = PlaylistStore::load(dir.path().join("playlists.json"), &library);
    store.create("Favourites");

    assert_eq!(store.flush(), SaveOutcome::Saved);
    assert_eq!(store.flush(), SaveOutcome::NothingToSave);
}

#[test]
fn an_unwritable_playlist_path_reports_the_failure_instead_of_printing_it() {
    use lyre_core::SaveOutcome;

    let dir = tempfile::TempDir::new().unwrap();
    let blocker = dir.path().join("blocker");
    std::fs::write(&blocker, b"not a directory").unwrap();

    let library = Library::empty(dir.path());
    let (mut store, _) = PlaylistStore::load(blocker.join("playlists.json"), &library);
    store.create("Favourites");

    let SaveOutcome::Failed(message) = store.flush() else {
        panic!("saving under a regular file must fail");
    };
    assert!(
        message.contains("playlists.json"),
        "the failure must name the path it could not write, got: {message}"
    );
}

#[test]
fn a_scan_cache_that_cannot_be_written_is_reported_as_a_warning_not_printed_to_stderr() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path().join("music");
    std::fs::create_dir_all(&root).unwrap();
    write_song(&root, "reachable.wav", "Reachable", "Artist", "Album");

    let blocker = dir.path().join("blocker");
    std::fs::write(&blocker, b"not a directory").unwrap();
    let cache_path = blocker.join("nested").join("cache.json");

    let (library, stats) = Library::scan(&root, &cache_path).unwrap();

    assert_eq!(library.len(), 1, "the scan itself must still succeed");
    assert!(
        stats.warnings.iter().any(|w| w.contains("scan cache")),
        "an unwritable cache must appear in stats.warnings, got: {:?}",
        stats.warnings
    );
}

#[test]
fn the_ungated_youtube_helpers_do_not_depend_on_the_youtube_feature() {
    assert_eq!(
        generate_file_name("Boards of Canada", "Roygbiv"),
        "BoardsOfCanada-Roygbiv.mp3"
    );
}
