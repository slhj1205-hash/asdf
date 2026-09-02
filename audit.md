# Senior Rust Audit — `lyre` (lyre-core + lyre-tui)

Repository: `https://github.com/slhj1205-hash/asdf` at `063f878`
Toolchain: rustc 1.98.0, edition 2024
Scope: ~14,900 lines of Rust across two library crates and one binary. ~5,160 of those are integration tests.

Repository status before and after the audit: `nothing to commit, working tree clean`. No files were modified.

---

## Executive Summary

This is a well-built codebase. Everything compiles clean, clippy is silent under a genuinely strict lint config (`unwrap_used`, `expect_used`, `panic`, `indexing_slicing` all denied at the workspace level), and 256 tests pass. The test suite is the strongest thing here — the test names read like a specification, and several of them cover exactly the failure modes a lazier project would have skipped (unwritable playlist paths, a yt-dlp child that floods stderr, cache staleness, flush debounce anchoring). The error types are proper `thiserror` enums with path context. The `Row`/`RowCache` design with a revision-keyed cache is a good answer to "don't rebuild the list every frame."

The concerns fall into three buckets.

**Data-loss paths that fail silently.** `PlaylistStore::flush` clears the dirty flag even when the save failed, so a transient write error permanently discards the change. `PlaylistStore::load` swallows a parse error and returns an empty store, and the next mutation writes that empty store over the user's playlists. `finish_dir_scan` loads the new playlist file before flushing pending edits to the old one. These are the findings I would fix first.

**The YouTube download path picks up the wrong file.** `newest_mp3_in` scans the scratch directory for the most recently modified `.mp3`, and the TUI passes `std::env::temp_dir()` as that scratch directory. Any unrelated `.mp3` in `/tmp` that is newer than the download gets moved into the user's library and tagged with the metadata they typed. Related: `resolve_directory` calls `create_dir_all` before it validates that the target is inside the library root.

**Repetition that scales badly.** `theme.rs` names each of 30 colors five times — struct field, `Default` impl, `ThemeFile` field, a hand-written closure in `apply_overrides`, and a free accessor function. Adding one color means touching five places. `config.rs` has four copies of the same XDG-or-`$HOME` lookup. `modes.rs` has fourteen near-identical accessor methods. None of this is broken, but it is where future changes will get expensive.

There is also a `TODO.md` in the repo containing benchmark results for three performance problems I found independently by reading. The authors have measured them (17x on path ordering, 2x on grouped row building, 2x on fuzzy keystrokes) and marked them "not started." I have kept those findings in the report since they are real, but credit where it is due: they were already known.

One structural note that cuts across everything. There are eleven `*_for_test` methods on the public API of `lyre-tui` (`deferred_warnings_for_test`, `pending_number_for_test`, `finish_dir_scan_for_test`, `visible_song_count_for_test`, `active_visual_for_test`, `expire_if_stale_for_test`, and others). They exist because integration tests in `tests/` cannot see `pub(crate)` items. That is a real constraint, but the current answer leaks test scaffolding into the shipped API.

---

## Findings

Ordered by severity.

### [High] `flush()` clears the dirty flag even when the save failed

**Location:** `core/src/playlist.rs:368` — `PlaylistStore::flush`
**Category:** Bug

```rust
pub fn flush(&mut self) -> SaveOutcome {
    if !self.dirty {
        return SaveOutcome::NothingToSave;
    }
    let outcome = self.save();
    self.dirty = false;        // <- runs even when outcome is Failed
    self.dirty_since = None;
    outcome
}
```

**Problem:** On a failed write the pending changes are marked clean. Every subsequent `flush()` and `flush_if_due()` returns `NothingToSave`, and `Drop` does nothing. The user's playlist edits are gone with no second attempt.

**Why it matters:** This is the main persistence path for playlists. A full disk, a transient permission problem, or the read-only-parent case that `an_unwritable_playlist_path_reports_the_failure_instead_of_printing_it` already sets up in the tests all lose data. The existing test asserts the *message* is right but never re-flushes to check the state, so the bug sits directly under passing coverage.

**Recommendation:** Only clear on success, and keep the retry alive otherwise.

```rust
pub fn flush(&mut self) -> SaveOutcome {
    if !self.dirty {
        return SaveOutcome::NothingToSave;
    }
    let outcome = self.save();
    if matches!(outcome, SaveOutcome::Saved) {
        self.dirty = false;
        self.dirty_since = None;
    }
    outcome
}
```

Note that `dirty` and `dirty_since` are redundant — `Option<Instant>` already encodes "is there a pending change." Collapsing them to one field makes this class of desync impossible.

---

### [High] The YouTube download can import an unrelated file from `/tmp`

**Location:** `core/src/youtube.rs:184` — `newest_mp3_in`, used by `fetch_and_download:105`; scratch dir supplied at `tui/src/app/youtube.rs:373`
**Category:** Bug

```rust
// core/src/youtube.rs
let downloaded = newest_mp3_in(scratch_dir)?.ok_or(Error::OutputMissing)?;

// tui/src/app/youtube.rs
let scratch_dir = std::env::temp_dir();
```

**Problem:** yt-dlp is invoked with a fully deterministic output template (`lyre-dl.%(ext)s`), so the produced file's path is already known. Instead of using it, the code scans the whole scratch directory and takes the newest `.mp3` by mtime. The TUI passes the shared system temp directory. Any `.mp3` that another process wrote to `/tmp` during the download wins.

**Why it matters:** The winner gets `fs::rename`d into the user's music library and then has the user's typed title/artist/album written into its tags by `Metadata::write`. That is a wrong file imported and a foreign file mutated. Two concurrent lyre downloads also collide on the fixed `lyre-dl` name.

**Recommendation:** Two changes, either of which fixes it; do both.

1. Return the known path rather than searching. The template fixes the extension to `mp3` because of `--audio-format mp3`, so `scratch_dir.join("lyre-dl.mp3")` is the file. Delete `newest_mp3_in`.
2. Give each download a private scratch directory (`tempfile::TempDir`, already a dev-dependency and cheap to promote) instead of the shared `temp_dir()`. This also fixes the concurrent-download collision and makes cleanup automatic.

---

### [High] `PlaylistStore::load` turns a corrupt file into an empty store, which then overwrites it

**Location:** `core/src/playlist.rs:188` — `PlaylistStore::load`
**Category:** Bug

```rust
let loaded: Vec<Playlist> = fs::read(&path)
    .ok()
    .and_then(|contents| serde_json::from_slice(&contents).ok())
    .unwrap_or_default();
```

**Problem:** A missing file and a malformed file are treated identically. If one playlist entry fails to deserialize — a truncated write, a hand-edit, a `PlaylistId` that fails `parse_hex` — the whole file is discarded and the store starts empty. The user sees no playlists, creates one, and `touch()` marks the store dirty; the next flush writes the one-playlist file over the original.

**Why it matters:** Silent, unrecoverable loss of user data, with no warning even though `PruneStats::warnings` exists and is already plumbed to stderr in `main.rs`.

**Recommendation:** Distinguish the cases. `NotFound` is fine to treat as empty; a parse failure should push a warning into `PruneStats::warnings` and set a flag that suppresses saving for the session, so the on-disk file is preserved for the user to inspect.

```rust
let loaded: Vec<Playlist> = match fs::read(&path) {
    Err(e) if e.kind() == io::ErrorKind::NotFound => Vec::new(),
    Err(e) => { stats.warnings.push(format!("could not read {}: {e}", path.display())); readonly = true; Vec::new() }
    Ok(bytes) => match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(e) => { stats.warnings.push(format!("{} is corrupt and will not be overwritten: {e}", path.display())); readonly = true; Vec::new() }
    },
};
```

`tui/src/config.rs:174` (`read_config`) has the same shape and the same consequence for `last_dir`, though the blast radius there is one path, not the playlist set.

---

### [High] Directory is created before the containment check

**Location:** `tui/src/app/youtube.rs:437` — `resolve_directory`
**Category:** Bug

```rust
let joined = root.join(candidate);
fs::create_dir_all(&joined).map_err(...)?;          // side effect happens first

let canonical_joined = joined.canonicalize()...;
let canonical_root = root.canonicalize()...;
if !canonical_joined.starts_with(&canonical_root) { // validation happens second
    return Err("directory must stay within the library root".to_string());
}
```

**Problem:** The textual `..` rejection above this does not catch symlinks. If the library root contains a symlink (`~/music/usb -> /media/usb`, or anything pointing outside), typing `usb/newalbum` calls `create_dir_all` on the resolved target first, and only then fails the check. The directory is left behind outside the root.

**Why it matters:** A validation function with a filesystem side effect that fires before validation. The damage here is limited to creating empty directories, but the ordering is the bug and it would become worse the moment anything else is written before the check.

**Recommendation:** Canonicalize the root first, resolve the target against it without creating anything, check containment, then create.

```rust
let canonical_root = root.canonicalize().map_err(...)?;
let joined = canonical_root.join(candidate);
// resolve the longest existing ancestor and check that it is under canonical_root
// before creating the remainder
```

---

### [Medium] Playlist edits can be lost when changing directory

**Location:** `tui/src/app/mod.rs:240` — `App::finish_dir_scan`
**Category:** Bug

```rust
let playlists_path = crate::config::playlists_path(library.root());
let (playlists, prune_stats) = PlaylistStore::load(playlists_path, &library);  // reads disk
let flushed = self.playlists.flush();                                          // writes disk
self.report_save(flushed);
self.playlists = playlists;
```

**Problem:** The new store is loaded from disk before the old store's pending changes are flushed. If the user re-scans the same directory (a common way to pick up new files) within the 750 ms debounce window, the load reads the stale file and the subsequent flush writes to the same path — after which the in-memory store is replaced by the stale copy. The just-written changes are discarded from memory.

**Why it matters:** Silent loss of a recent playlist edit, triggered by an ordinary refresh.

**Recommendation:** Flush before loading.

```rust
let flushed = self.playlists.flush();
self.report_save(flushed);
let playlists_path = crate::config::playlists_path(library.root());
let (playlists, prune_stats) = PlaylistStore::load(playlists_path, &library);
self.playlists = playlists;
```

---

### [Medium] Failed download finalization drops the modal and leaks the temp file

**Location:** `tui/src/app/youtube.rs:118` — `App::finalize_youtube_download`
**Category:** Bug

**Problem:** The caller reached this function via `take_youtube_modal()`, so the modal is already gone from `self.modes`. All three error paths (`finalize_download` fails, `Metadata::write` fails, `Song::load` fails) call `set_status` and `return` without restoring a modal and without calling `youtube::discard_temp_file`. The downloaded file stays in `/tmp` forever, and in the tagging-failure case the file has already been moved into the library but is never inserted into it, leaving an orphan on disk that the user cannot see until the next scan.

**Why it matters:** The user's only feedback is a status line that expires after four seconds, with no way to retry.

**Recommendation:** Restore the modal on failure so the user can retry, and discard the temp file on the paths where it is still in the scratch directory. The `interrupt_youtube_with_error` helper already does exactly the "put the fields back with an error" thing — reuse it.

---

### [Medium] `sort_title` can underflow

**Location:** `tui/src/ui/style.rs:170` — `sort_title`
**Category:** Potential bug

```rust
let group_dashes = widths.category_value - display_width(category_label) + 1;
let sort_dashes  = widths.sort_value - display_width(sort_label) + 1;
// ...
"─".repeat(group_dashes)
```

**Problem:** `widths.category_value` is the max label width over `Category::ALL`, but `category_label` is a `&str` the caller supplies. Nothing in the type system ties them together. A caller passing a longer string — a `Sort` label into the category slot, or a future label added to one list and not the other — underflows the `usize`.

**Why it matters:** In debug this panics. In release the profile sets `overflow-checks = false`, so it wraps to roughly `usize::MAX` and `"─".repeat(...)` tries to allocate exabytes. A crash either way, in the render path, in a workspace that denies `clippy::panic`.

**Recommendation:** `saturating_sub`, or better, take the enum instead of the label so the invariant holds by construction:

```rust
pub fn sort_title(category: Category, sort: Sort, border_style: Style) -> Line<'static> {
```

`compute_jump` (`tui/src/app/navigation.rs:479`) has a milder version of the same inconsistency: line 487 uses `len.saturating_sub(1)` but line 490 uses a bare `len - 1`. Both callers check `len == 0` first, so it is currently unreachable, but the function defends itself on one line and not the next.

---

### [Medium] Two ID derivations for the same song, and the map key can diverge from `song.id()`

**Location:** `core/src/song.rs:52` — `SongId::compute` vs `SongId::from_path`; `core/src/library.rs:185` — `Library::update_metadata`
**Category:** Potential bug

```rust
pub fn compute(path: &Path) -> SongId { /* hashes the path as given */ }

pub fn from_path(path: &Path) -> SongId {
    let canonical = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    SongId::compute(&canonical)
}
```

`Library::scan` uses `Song::from_cached_with_stat`, which calls `compute` on the raw path. `Song::load` calls `from_path`, which canonicalizes. Both feed the same `HashMap<SongId, Song>`.

**Problem:** Two different functions produce the identity for the same concept, and which one runs depends on the code path. Today they agree because `scan` canonicalizes the root and skips symlinks, so entries below it are already canonical — but that is an invariant held by three separate pieces of code with nothing enforcing it. `update_metadata` compounds it:

```rust
let updated = Song::load(&path)?;   // recomputes the id via from_path
self.songs.insert(id, updated);     // stores under the OLD key
```

If the two ever disagree, `library.get(id).unwrap().id() != id`, and every lookup that goes through `song.id()` (e.g. `Queue::play_id`, `select_song_by_id`) silently stops matching.

**Why it matters:** The failure is invisible — no panic, no error, just songs that cannot be selected or played.

**Recommendation:** Pick one derivation. `from_path` is the safer default. Make `compute` private, and have `update_metadata` assert or handle the case:

```rust
let updated = Song::load(&path)?;
debug_assert_eq!(updated.id(), id, "reloading a song must not change its id");
self.songs.insert(updated.id(), updated);
```

There is a test (`updating_metadata_keeps_the_song_id_stable`) covering the happy path, which is good — it just does not cover the divergence.

---

### [Medium] Re-activating a song in the playlist you are already playing wipes the queue

**Location:** `tui/src/app/playback.rs:86` — `play_selected_in_playlist`
**Category:** Bug / Consistency

```rust
// play_selected_library — guards
if self.queue_source != QueueSource::Library {
    self.queue = Queue::new(self.display_order.clone());
    self.queue_source = QueueSource::Library;
}

// play_selected_in_playlist — no guard
self.queue = Queue::from_playlist(playlist);
self.queue_source = QueueSource::Playlist(id);
```

**Problem:** The library path rebuilds the queue only when the source changed. The playlist path rebuilds unconditionally. Pressing Enter on a song inside the playlist that is already playing throws away the shuffle order and the entire "queue next" priority list.

**Why it matters:** Two functions doing the same job with different rules, and the difference is user-visible data loss (the priority queue). The library behaviour is the correct one.

**Recommendation:**

```rust
if self.queue_source != QueueSource::Playlist(id) {
    self.queue = Queue::from_playlist(playlist);
    self.queue_source = QueueSource::Playlist(id);
}
```

While here: line 87 does `self.playlists.get(id).is_none()` and line 92 does `self.playlists.get(id)` again. One lookup suffices.

---

### [Medium] `upcoming(n)` and `play_upcoming(n)` disagree on an idle queue

**Location:** `core/src/queue.rs:165,176` — `Queue::play_upcoming`, `Queue::upcoming`
**Category:** Bug

```rust
pub fn upcoming(&self, n: usize) -> Vec<SongId> {
    let mut out: Vec<SongId> = self.priority.iter().copied().take(n).collect();
    if out.len() >= n || self.order.is_empty() || self.playing.is_none() {
        out.truncate(n);
        return out;     // <- returns empty when nothing is playing
    }
    ...
```

`play_upcoming(n)` calls `next()` n times; `next()` on an idle queue moves the cursor to 0 and returns `order[0]`.

**Problem:** On a fresh queue with nothing playing, the "Up Next" panel shows nothing, but pressing `1` then `n` plays the first track. The display and the action disagree about whether an idle queue has upcoming songs.

**Why it matters:** `jump_to_upcoming` reports "Up Next is empty" based on `play_upcoming` returning `None`, while the panel that told the user what to type was populated by `upcoming`. The two functions are the read and write halves of one feature and should share a definition.

**Recommendation:** Have `upcoming` treat `playing == None` the same way `next()` does — start the walk at index 0 rather than bailing out. Better still, express `upcoming` and `play_upcoming` in terms of one shared "sequence of positions from here" iterator so they cannot drift again.

---

### [Medium] `songs_by_path` sorts the whole library, then the caller sorts it again

**Location:** `core/src/library.rs:169` — `Library::songs_by_path`; consumed at `tui/src/app/row_builder.rs:103,114`
**Category:** Performance / Design

```rust
pub fn songs_by_path(&self) -> impl Iterator<Item = &Song> + '_ {
    let mut pairs: Vec<(&Path, SongId)> = self.songs.values()
        .map(|song| (song.path(), song.id()))
        .collect();
    pairs.sort_unstable();
    pairs.into_iter().filter_map(move |(_, id)| self.songs.get(&id))
}
```

**Problem:** Three separate inefficiencies stacked.

1. It collects `(&Path, SongId)` and then looks each `SongId` back up in the `HashMap` — one hash lookup per song to recover a reference it already had. This is the borrow checker being worked around rather than designed around; `Vec<&Song>` has no such problem.
2. Every caller in `row_builder.rs` immediately re-sorts by the user's chosen `Sort`. For `Sort::Title` the path sort is pure waste, and because the second sort is `sort_unstable_by`, the pre-sort does not even reliably provide tie-breaking.
3. It is declared as returning an iterator, which reads as lazy, but does all the work eagerly.

`ids_by_path` (line 159) duplicates the same body. Both are called with `.to_vec()` at `tui/src/app/mod.rs:78` and `:234`, cloning the already-owned `Vec` a third time.

**Why it matters:** This is on the row-rebuild path, which runs on every keystroke while searching. `TODO.md` benchmarks it at 19,148 ns vs 1,129 ns for a pre-sorted index — a 17x difference the authors already measured.

**Recommendation:**

```rust
pub fn songs_by_path(&self) -> Vec<&Song> {
    let mut songs: Vec<&Song> = self.songs.values().collect();
    songs.sort_unstable_by_key(|s| s.path());
    songs
}

pub fn songs(&self) -> impl Iterator<Item = &Song> { self.songs.values() }
```

Then have `build_rows_into` call `songs()` — unsorted — since `build_rows` sorts anyway, and reserve `songs_by_path` for the places that genuinely need path order. Drop the `.to_vec()` calls on `ids_by_path`.

---

### [Medium] Fuzzy scoring recounts characters for every song on every keystroke

**Location:** `core/src/song.rs:417` — `Song::fuzzy_term_score`
**Category:** Performance

```rust
let title = fuzzy::subsequence_score(term, title_field)
    .map(|s| fuzzy::normalize_by_length(s, title_field.chars().count()) * 3 / 2);
```

**Problem:** `chars().count()` is O(n) and runs up to five times per song per search term, on every keystroke. `SortKeys` is built once at load and is the natural home for these lengths.

**Why it matters:** Search is the hottest interactive path in the app. `TODO.md` measures 31,582 ns vs 16,684 ns per keystroke with cached lengths.

**Recommendation:** Store the char count alongside each key in `SortKeys`. The same applies to `Playlist::fuzzy_score` (`core/src/playlist.rs:132`), which additionally allocates a fresh lowercased `String` per playlist per call — an inconsistency worth noting on its own, since `Song` precomputes its lowercase keys and `Playlist` does not.

---

### [Medium] `Library::scan` allocates a `PathBuf` per song to handle a case that almost never happens

**Location:** `core/src/library.rs:105`
**Category:** Performance / Complexity

```rust
let song = Song::from_cached_with_stat(path, size, mtime, metadata);
let skipped_path = song.path().to_path_buf();          // allocated for EVERY song
if let InsertOutcome::Collision { existing } = insert_into(&mut songs, song) {
    stats.skipped_files += 1;
    if let Some(kept) = songs.get(&existing)
        && kept.path() != skipped_path
    { /* warn */ }
}
```

**Problem:** The `PathBuf` is cloned unconditionally, before the insert, purely so the collision branch can compare paths after `song` has been moved. `insert_into` also does `contains_key` followed by `insert` — two hashes per song. And `InsertOutcome::Collision { existing }` carries a field that is always equal to the id being inserted, which makes it look informative when it is not.

**Recommendation:** The entry API removes the clone, the double hash, and the branch awkwardness at once.

```rust
use std::collections::hash_map::Entry;
match songs.entry(song.id()) {
    Entry::Vacant(slot) => { slot.insert(song); }
    Entry::Occupied(slot) => {
        stats.skipped_files += 1;
        if slot.get().path() != song.path() {
            stats.warnings.push(format!(
                "song id collision between {} and {} -- kept the first, skipped the second",
                slot.get().path().display(), song.path().display()));
        }
    }
}
```

---

### [Medium] `handle_choose_action_key` threads five loose parameters and duplicates its own cycle logic

**Location:** `tui/src/app/song_modal.rs:34`
**Category:** Complexity

**Problem:** 120 lines, six levels of nesting, and five parameters (`song`, `batch`, `selected`, `name_input`, plus the implicit picker) that are precisely the fields of `SongModal`. They are destructured out in `handle_song_modal_key`, passed individually, and reassembled by `set_song_modal` on ten different return paths. The `cycle(&visible, selected, if PrevField { -1 } else { 1 })` expression appears three times in the file. The function has an early-return block for `CreatePlaylist` that re-implements the field cycling from the block below it.

**Why it matters:** Ten exit paths that each have to remember to call `set_song_modal`, or the modal silently closes. That is a large surface for a "forgot one branch" bug, and it is not visible from the type system.

**Recommendation:** Pass the `SongModal` by value and mutate it, with one restore at the end.

```rust
fn handle_choose_action_key(&mut self, key: KeyEvent, mut modal: SongModal) {
    let visible = self.visible_choose_action_fields(modal.song, modal.batch.as_deref());
    match action_for(key, modal.selected) {
        Action::Cancel  => return,                       // drop = close
        Action::Cycle(d) => modal.selected = cycle(&visible, modal.selected, d),
        Action::Confirm  => { if self.confirm_choice(&mut modal) { return; } }
        Action::Type(ev) => apply_text_edit(&mut modal.name_input, ev),
    }
    self.modes.replace(Mode::SongModal(modal));
}
```

Same file, `handle_picker_key` (line 157): it destructures `PlaylistPicker` into four locals and `rebuild_picker` reassembles it with `options.to_vec()` — a full `Vec<PlaylistId>` clone on every arrow-key press, purely to put the enum back together. Mutating `list_state` in place inside the existing value avoids it entirely.

---

### [Medium] Every color is written out five times

**Location:** `tui/src/theme.rs` (whole file, 355 lines)
**Category:** Maintainability

**Problem:** For each of 30 colors:

1. a `Theme` struct field (lines 9–38)
2. a `Default` impl entry (44–73)
3. a `ThemeFile` field (108–137)
4. a hand-written `override_color(source, "name", file.name, |t, c| t.name = c, theme)?` line (141–170)
5. a free `pub fn name() -> Color { current().name }` (237–355)

Adding one color means five edits in four places, and steps 3 and 4 must agree on a string literal that nothing checks.

**Why it matters:** 250 of the file's 355 lines are mechanical repetition. This is the largest single block of avoidable code in the project.

**Recommendation:** Derive `Deserialize` on `Theme` directly with `#[serde(default, deny_unknown_fields)]` and a `deserialize_with` for the hex parser. That eliminates items 2 through 4 outright. Then replace the 30 accessors with one:

```rust
pub fn current() -> &'static Theme { THEME.get_or_init(Theme::default) }
```

Callers become `theme::current().title` instead of `theme::title()` — one extra token at the call site, ~250 lines removed, and adding a color becomes a one-line change. The `toml::Spanned` line-number reporting in error messages is a genuinely nice touch and is worth keeping; a custom `Deserialize` impl on the color newtype can preserve it.

---

### [Medium] Four copies of the XDG directory lookup

**Location:** `tui/src/config.rs:28,50,63,81` — `config_path`, `config_dir`, `data_dir`, `cache_dir`
**Category:** Duplication

**Problem:** Four functions with the same body and different constants. `config_path` is the worst case: it is a byte-for-byte copy of `config_dir` with `.join("config.json")` appended, so it could be a one-liner.

**Recommendation:**

```rust
fn xdg_dir(var: &str, fallback: &[&str]) -> Option<PathBuf> {
    let name = app_name::kebab_case();
    if let Ok(v) = std::env::var(var) && !v.is_empty() {
        return Some(PathBuf::from(v).join(name));
    }
    let home = std::env::var("HOME").ok()?;
    let mut p = PathBuf::from(home);
    p.extend(fallback);
    Some(p.join(name))
}

fn config_dir()  -> Option<PathBuf> { xdg_dir("XDG_CONFIG_HOME", &[".config"]) }
fn data_dir()    -> Option<PathBuf> { xdg_dir("XDG_DATA_HOME",   &[".local", "share"]) }
fn cache_dir()   -> Option<PathBuf> { xdg_dir("XDG_CACHE_HOME",  &[".cache"]) }
fn config_path() -> Option<PathBuf> { config_dir().map(|d| d.join("config.json")) }
```

Roughly 60 lines to 12, with the fallback rule stated once.

---

### [Low] The keymap table encodes four different kinds of entry in one struct

**Location:** `tui/src/keymap.rs:51,123` — `Binding`, `BINDINGS`
**Category:** Design

**Problem:** `Binding` serves both key dispatch and help rendering, disambiguated by a `dispatch: bool`. Entries come in four shapes: keys + action + dispatch (real binding); keys + no action + no dispatch (Esc, handled specially in `esc_command`); no keys + action + no dispatch (help-only aliases in the Playlists section); no keys + no action + `display_override` (the `<1-9> then <n>` documentation row). `Binding::display()` needs a three-way fallback to cope, and `help_rows` merges adjacent entries by comparing `desc` strings, which couples the help output to table ordering.

**Recommendation:** Split into a dispatch table (`&[(&[(KeyCode, KeyModifiers)], Action)]`) and a help table (`&[(HelpKey, &str, Section)]` where `HelpKey` is either an `Action` to look up or a literal string). Each becomes obvious, and `lookup` stops needing a filter.

---

### [Low] `nearest_song_row` and `nearest_selectable` solve the same problem differently

**Location:** `tui/src/app/navigation.rs:435` and `:511`
**Category:** Duplication / Consistency

**Problem:** Both find the closest selectable row to an index. `nearest_song_row` checks the start, then scans forward, then wraps to the beginning. `nearest_selectable` does a bidirectional radius expansion. They live 70 lines apart in one file and can return different answers for the same input. `nearest_song_row` also indexes with `rows[start]` under an `#[allow(clippy::indexing_slicing)]` — safe today because its one caller checks `len` first, but it is a panic waiting for a second caller in a workspace that denies `panic`.

**Recommendation:** Keep the bidirectional version, delete the other, and use `rows.get(start)` so the `allow` can go.

The same file has `move_wrapping` (mutates a `ListState`) and `wrapping_selectable_index` (returns an index) for the same wrapping-move concept. Pick one shape.

---

### [Low] Playlist rows are recomputed on every call while song rows are cached

**Location:** `tui/src/app/navigation.rs:149` — `visible_playlist_ids`
**Category:** Performance / Consistency

**Problem:** Songs get `RowCache`, keyed on panel, view, category, sort, query, and two revisions — a good design. Playlists get nothing: `visible_playlist_ids` allocates a fresh `Vec` and, when a search is active, re-scores and re-sorts every playlist. It is called from seven places, several of which only want `.len()`.

**Recommendation:** Either extend `RowCache` to cover the playlist browse list (the revision key is already there) or, at minimum, add a `visible_playlist_count()` that avoids the allocation for the length-only callers.

---

### [Low] Fourteen near-identical mode accessors

**Location:** `tui/src/app/modes.rs:73–151`
**Category:** Complexity

**Problem:** Nine `*_mode_is() -> bool` predicates and five `*_mode() -> Option<&T>` accessors, most of which are `self.x_mode().is_some()` or a four-line `match ... { Some(Mode::X(m)) => Some(m), _ => None }`. The naming is also awkward (`song_mode_is`, `modes_is_empty`). `Modes` itself is a newtype over `Option<Mode>` whose only addition is a `debug_assert!` in `open` — and since the release profile sets `debug-assertions = false`, that assertion never fires in a shipped binary while `replace` exists as an unchecked alternative.

**Recommendation:** Let the UI match on `Option<&Mode>` directly, which it already does in `ui/mod.rs:63`. Keep at most the two or three accessors with real call sites and delete the rest.

---

### [Low] The marquee drives the frame rate through hidden thread-local state

**Location:** `tui/src/ui/style.rs:249` (thread-locals), `:284` (`marquee_window`), `tui/src/ui/mod.rs:49,61`
**Category:** Design

**Problem:** `marquee_window` looks like a pure text-formatting function but sets a thread-local `MARQUEE_ACTIVE` flag as a side effect. `App::render` calls `reset_marquee_activity()` before drawing and `marquee_active()` after, then stashes the result in `App.animating` (a `Cell<bool>` that exists only for this), which `handle_events` reads to choose a 120 ms or 400 ms poll timeout. So a formatting helper controls the event loop's tick rate through two layers of hidden global state.

**Why it matters:** `marquee_window` cannot be tested or reasoned about in isolation, and the coupling is invisible at both ends.

**Recommendation:** Return the flag: `fn marquee_window(text, width) -> (Cow<'_, str>, bool)`, and let the render functions accumulate it into a value threaded back to the caller. `App.animating` can then be a plain `bool` instead of a `Cell`.

---

### [Low] `atomic::write` does not fsync the parent directory

**Location:** `core/src/atomic.rs:18`
**Category:** Potential bug

**Problem:** The file contents are `sync_all`'d and the rename is atomic, but the directory entry itself is not synced. On a crash the rename can be lost even though the data was durable.

**Why it matters:** This is the write path for playlists, the scan cache, and the config. The function's name promises more durability than it delivers.

**Recommendation:** Open the parent with `File::open` and call `sync_all()` on it after the rename. Cheap, and it completes the guarantee the name implies.

---

### [Low] The consecutive-match bonus in fuzzy scoring compares characters, not positions

**Location:** `core/src/fuzzy.rs:27` — `subsequence_score`
**Category:** Potential bug

```rust
if prev_target_char.is_some() && prev_target_char == prev_pattern_char {
    score += 10;   // "these matched adjacently"
}
```

**Problem:** This infers adjacency by comparing the previous target character to the previously matched pattern character. With repeated characters they can be equal without being adjacent. Pattern `ab` against target `aab`: the `b` at index 2 gets the adjacency bonus because `prev_target` (`a`) equals `prev_pattern` (`a`), even though the match was not contiguous.

**Why it matters:** Wrong ranking in search results. It is not a crash and the effect is small, but the bonus is silently wrong for a common class of input.

**Recommendation:** Track the index of the last match instead of its character:

```rust
if last_match_index == Some(target_index.wrapping_sub(1)) { score += 10; }
```

---

### [Low] Errors swallowed in `update_artist_sort_for_matching`

**Location:** `core/src/library.rs:210`
**Category:** Design

**Problem:** `if self.update_metadata(id, &edits).is_ok() { updated_ids.push(id); }` discards every error. The caller (`confirm_romanized_artist_apply`) reports "applied romanized artist to N other songs" and the user has no way to learn that 3 of 10 failed because the files are read-only.

**Recommendation:** Return `(Vec<SongId>, Vec<(SongId, song::Error)>)` or a small result struct, and have the status message mention failures.

---

### [Low] `is_supported_audio` hand-rolls a lowercase buffer

**Location:** `core/src/song.rs:26`
**Category:** Style

```rust
let mut buf = [0u8; MAX_EXTENSION_LEN];
let bytes = ext.as_bytes();
#[allow(clippy::indexing_slicing)]
{
    buf[..bytes.len()].copy_from_slice(bytes);
    buf[..bytes.len()].make_ascii_lowercase();
    let lower = &buf[..bytes.len()];
    SUPPORTED_EXTENSIONS.iter().any(|known| known.as_bytes() == lower)
}
```

**Problem:** A fixed buffer, a length constant, and a lint suppression to avoid an allocation the standard library already avoids.

**Recommendation:**

```rust
pub fn is_supported_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| SUPPORTED_EXTENSIONS.iter().any(|k| ext.eq_ignore_ascii_case(k)))
}
```

Same behaviour, no allocation, no `allow`, no `MAX_EXTENSION_LEN`. Note this drops the length and ASCII pre-checks, which were only there to make the buffer trick safe.

---

### [Low] `resolve_ytdlp_binary` and `which_exists` duplicate the PATH walk

**Location:** `core/src/youtube.rs:19,43`
**Category:** Duplication

**Recommendation:** One function, two uses.

```rust
fn which(program: &str) -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|dir| dir.join(program))
        .find(|c| c.is_file())
}

pub fn ffmpeg_available() -> bool { which("ffmpeg").is_some() }
```

`resolve_ytdlp_binary` becomes the cached-path check plus `which("yt-dlp").ok_or(Error::NotFound)`. Its error message ("not found on PATH") is also slightly wrong, since it checks `binaries_dir` first.

---

### [Low] `Entropy` is constructed per call in `shuffle` and `random_bytes`

**Location:** `core/src/random.rs:93,126`
**Category:** Performance

**Problem:** Each call opens `/dev/urandom` and carries a fresh 512-byte buffer. `random_bytes::<16>` reads up to 512 bytes to use 16. `PlaylistId::new()` does this per playlist created.

**Why it matters:** Small in absolute terms, but it makes the buffering in `Entropy` pointless — the buffer never survives to be reused.

**Recommendation:** A thread-local `Entropy` gives the buffering its intended effect. Alternatively, drop the buffer from the one-shot path.

---

### [Nit] `action_command` takes a used parameter named `_app` and always returns `Some`

**Location:** `tui/src/app/command.rs:84`

```rust
fn action_command(action: Action, _app: &App) -> Option<Command> {
    // ...
    Action::NextOrJump => if _app.pending_number.is_empty() { ... }
```

The underscore says "unused" and the body uses it. The `Option` return is never `None`. Rename to `app` and return `Command`.

---

### [Nit] `is_filtering` is a roundabout `!trim().is_empty()`

**Location:** `tui/src/app/state.rs:116`

```rust
pub fn is_filtering(query: &str) -> bool { query.split_whitespace().next().is_some() }
```

`!query.trim().is_empty()` says the same thing and reads directly.

---

### [Nit] `SortKeys` accessor methods add nothing

**Location:** `core/src/song.rs:304–318`

Five one-line getters on a private struct in the same module as its only consumer. `self.keys.title` is as good as `self.keys.title()`.

---

### [Nit] `Song::from_cached_with_stat` takes an unused `_len` parameter

**Location:** `core/src/song.rs:344`

The size is threaded in and dropped. Either use it or remove it from the signature.

---

### [Nit] `content_style()` and `unfocused_style()` are identical

**Location:** `tui/src/ui/style.rs:27,31`

Both are `Style::new().fg(theme::text_secondary())`. Fine if they are expected to diverge; worth a moment's thought, since a change to one will not affect the other and the call sites do not distinguish them.

---

### [Nit] `display_width` wraps a single method call

**Location:** `tui/src/ui/style.rs:19`

`pub fn display_width(s: &str) -> usize { s.width() }`, re-exported publicly. Callers can `use unicode_width::UnicodeWidthStr` directly.

---

### [Nit] Redundant feature forwarding in the root manifest

**Location:** `Cargo.toml:22`

`youtube = ["lyre-tui/youtube", "lyre-core/youtube"]` — but `lyre-tui/youtube` already implies `lyre-core/youtube`. Harmless, and arguably clearer as-is since the root binary also depends on `lyre-core` directly. Mentioned only for completeness.

---

## Cross-Crate Consistency

The two crates are broadly consistent in style — same naming conventions, same module layout, same use of small marker enums (`Mutated`, `InsertOutcome`, `Selected`, `EventsChanged`, `KeyEventHandled`) instead of bare `bool`, which is a good habit and applied evenly. The differences below are the ones that look unintentional.

**Error representation is split three ways.** `lyre-core` uses `thiserror` enums for `song::Error`, `library::Error`, `player::Error`, `youtube::Error`, and `UpdateMetadataError` — all good. But `ScanCache::save` returns `Result<(), String>`, `PlaylistStore` returns a `SaveOutcome::Failed(String)` enum, and `player::BackendError` is `Box<dyn Error + Send + Sync>`. `lyre-tui` then adds a fourth: `resolve_directory` and `theme::parse` return `Result<_, String>`.

*Standard:* `thiserror` enums for anything a caller might branch on; `SaveOutcome` is reasonable where the three-way Saved/Nothing/Failed distinction matters, but its `Failed` arm should carry a typed error rather than a pre-formatted `String`. `BackendError` as a boxed trait object is defensible for a plugin-style trait — leave it.

**`SaveOutcome` is reused for a case it does not describe.** `tui/src/config.rs:154` returns `SaveOutcome::NothingToSave` when `$HOME` is unset — that is "I could not determine where to save," not "there was nothing to save." The caller cannot distinguish a successful no-op from a configuration failure.

**Modal state has two protocols.** Forms use an explicit `FormFieldOutcome::{Updated, Confirmed, Cancelled}` returned by value — clear and hard to get wrong. Every other modal uses the implicit `modes.take()` in `handle_key` plus a `modes.replace()` that each handler must remember on each branch. Ten exit paths in `handle_choose_action_key` alone.

*Standard:* the `FormFieldOutcome` approach. Make `handle_mode_key` return a `ModeOutcome::{Keep(Mode), Close}` so the compiler enforces what is currently a convention.

**Precomputed sort keys in one place, recomputed in the other.** `Song` builds `SortKeys` once at load. `Playlist::fuzzy_score` lowercases its name on every call. Both feed the same fuzzy search UI.

*Standard:* precompute, as `Song` does.

**Caching applied to one list and not the other.** `RowCache` for songs, nothing for playlists (`visible_playlist_ids`). Same panel, same search box, same interaction.

**Warning delivery is inconsistent.** `Library::scan` and `PlaylistStore::load` collect warnings into stats structs, which `main.rs` and `App` render properly. But `Backend::detect` (`tui/src/backend.rs:23`) and `theme::init_from_path` (`tui/src/theme.rs:89`) `eprintln!` directly. Both happen before the terminal is put into raw mode so nothing is corrupted today, but the mechanism differs for no stated reason.

*Standard:* return warnings; let `main` decide how to show them.

**Test-only API surface differs in kind.** Both crates test through integration tests in `tests/`, but only `lyre-tui` had to punch eleven `*_for_test` methods through its public API to do it. `lyre-core` has a cleaner public API and did not need them.

*Standard:* gate the accessors behind a `test-util` cargo feature, or make the fields `pub` under `#[doc(hidden)]`. Either is better than eleven permanent public methods whose names document that they should not exist.

---

## Complexity / Simplification Opportunities

Ranked by how much simpler the result would be.

### 1. `theme.rs` — 30 colors × 5 restatements

Covered above. Derive `Deserialize` on `Theme`, expose `current() -> &'static Theme`. Roughly 250 of 355 lines disappear and adding a color becomes one line instead of five.

### 2. `RowsKey` is built twice, once as an anonymous 6-tuple

**Location:** `tui/src/app/row_builder.rs:31,54`

`row_context()` returns `(Panel, PlaylistView, Category, Sort, u64, u64)` — a positional tuple with no field names — which `visible_rows` destructures, compares field-by-field against the cached key in a seven-line `is_some_and` closure, and then reassembles into a `RowsKey`.

*Why it is not justified:* the tuple **is** `RowsKey` minus one field. Two representations of one concept, plus a hand-written equality check that must be kept in sync with both.

*Simplification:* derive `PartialEq` on `RowsKey`, have `row_context` build one directly, and compare with `!=`.

```rust
#[derive(PartialEq)]
struct RowsKey { /* unchanged */ }

pub fn visible_rows(&mut self) -> &[Row] {
    let key = self.rows_key();
    if self.rows.key.as_ref() != Some(&key) {
        let mut buffer = std::mem::take(&mut self.rows.rows);
        buffer.clear();
        self.build_rows_into(&mut buffer);
        self.rows.rows = buffer;
        self.rows.key = Some(key);
    }
    &self.rows.rows
}
```

The tuple, the destructuring, and the seven-clause comparison all go away.

### 3. `cycle_field` — a six-argument higher-order function used twice

**Location:** `tui/src/app/navigation.rs:404`, called from `cycle_category:354` and `cycle_sort:379`

```rust
fn cycle_field<T: Copy>(
    &mut self,
    direction: Direction,
    verb: &str,
    field: impl FnOnce(&mut Self) -> &mut T,
    next: impl FnOnce(T) -> T,
    prev: impl FnOnce(T) -> T,
    label: impl FnOnce(T) -> &'static str,
) { ... }
```

*Why it is not justified:* three of the four closures (`next`, `prev`, `label`) are always the inherent methods of the type being cycled. The abstraction exists to paper over the fact that `Category` and `Sort` have identical interfaces but no shared trait. Meanwhile `cycle_category` and `cycle_sort` are themselves 20-line near-duplicates of each other.

*Simplification:* give the types the trait they already satisfy.

```rust
pub trait Cycle: Copy + PartialEq + 'static {
    const ALL: &'static [Self];
    fn label(self) -> &'static str;
    fn step(self, d: Direction) -> Self {
        cycle(Self::ALL, self, match d { Direction::Forwards => 1, Direction::Backwards => -1 })
    }
}
```

`Category` and `Sort` already have `ALL`, `label`, `next`, and `prev` — the impls are two lines each. Then `cycle_category` and `cycle_sort` become four lines apiece with no closures.

### 4. `marquee_window` — two manual accumulation loops over a `Vec<char>`

**Location:** `tui/src/ui/style.rs:284`

*What makes it complicated:* it collects the text into `Vec<char>`, collects the constant `MARQUEE_GAP` into a second `Vec<char>` on every call, then runs two structurally similar hand-written loops — one for the truncate-with-ellipsis case, one for the scrolling case — and the scrolling loop splices two slices by hand:

```rust
let c = chars.get(idx).or_else(|| gap.get(idx - chars.len())).copied();
```

*Why it is not justified:* the splice is `chars.iter().chain(gap.iter()).cycle().skip(offset)`, and both loops are the same "take characters until the display width budget runs out" operation with different sources and a different tail.

*Simplification:* one helper `fn take_to_width(iter: impl Iterator<Item = char>, budget: usize) -> String`, called from both branches. The gap `Vec` becomes `MARQUEE_GAP.chars()`. `unicode-truncate` is already in the dependency tree via `ratatui-core` if you would rather not hand-roll the truncation at all.

### 5. `Metadata::write`'s tag-selection dance

**Location:** `core/src/song.rs:210`

```rust
let tag_type = tagged_file.primary_tag_type();
if tagged_file.tag(tag_type).is_none() && tagged_file.first_tag().is_none() {
    tagged_file.insert_tag(Tag::new(tag_type));
}
let tag = match tagged_file.tag_mut(tag_type) {
    Some(tag) => tag,
    None => match tagged_file.first_tag_mut() {
        Some(tag) => tag,
        None => return Err(Error::Unwritable { path: path.to_path_buf() }),
    },
};
```

Two immutable probes, a conditional insert, then two mutable probes repeating the same logic. The intent is "use the primary tag, fall back to any tag, create one if there is none."

*Simplification:* insert unconditionally when the primary type is absent, then one lookup:

```rust
let tag_type = tagged_file.primary_tag_type();
if tagged_file.first_tag().is_none() {
    tagged_file.insert_tag(Tag::new(tag_type));
}
let tag = tagged_file.primary_tag_mut()
    .or_else(|| tagged_file.first_tag_mut())
    .ok_or_else(|| Error::Unwritable { path: path.to_path_buf() })?;
```

Also in this file, `set_text(tag, value, set: fn(&mut Tag, String), remove: fn(&mut Tag))` takes two function pointers for four call sites; a small macro or four explicit `if value.is_empty()` blocks would read more plainly.

### 6. Three copies of "batch, or just this one song"

**Location:** `tui/src/app/panels.rs:102,111,120` and `tui/src/app/song_modal.rs:63,213`

```rust
let songs: Vec<SongId> = batch.map(<[SongId]>::to_vec).unwrap_or_else(|| vec![song]);
```

Five occurrences, each allocating a `Vec` only to iterate over it.

*Simplification:* no allocation needed at all.

```rust
let songs: &[SongId] = batch.unwrap_or(std::slice::from_ref(&song));
```

### 7. `Outcome` / `ProbeResult` carry `freshly_probed` in two variants

**Location:** `core/src/library.rs:336–351`

```rust
enum ProbeResult {
    Tags { metadata: Metadata, freshly_probed: bool },
    Unreadable { freshly_probed: bool },
    NoMetadata,
}
```

The flag is orthogonal to the variant and duplicated across two of three. Move it up to `Outcome` as a single field and the `match` in `scan` loses two identical `if freshly_probed` blocks.

### 8. `Queue::play_id`'s modular nearest-occurrence search

**Location:** `core/src/queue.rs:146`

```rust
let target = self.order.iter().enumerate()
    .filter(|&(_, &song)| song == id)
    .map(|(position, _)| position)
    .min_by_key(|&position| (position + len - start) % len)?;
```

This finds the occurrence nearest after the cursor, wrapping. It is correct and there is a test for it. It is only meaningful if `order` can contain duplicates, which `Queue::insert` does permit — so the complexity is arguably earned. Worth a look at whether duplicates should be allowed at all; if not, `self.order.iter().position(|&s| s == id)` replaces the whole thing.

### 9. Miscellaneous smaller ones

- `core/src/library.rs:20` — `Library::scan` is 110 lines covering traversal, parallel probing, cache rebuild, stats, collision reporting, and cache persistence. Extracting the "rebuild cache + collect songs" loop into its own function would make each half readable on its own.
- `tui/src/app/mod.rs:226` — `finish_dir_scan` is 78 lines, most of it incremental `message.push_str` construction interleaved with state mutation. Separating "do the scan and update state" from "build the summary message" would help.
- `tui/src/app/navigation.rs:129` — `selected_song_ids` calls `visible_row_range()` twice, once to test `is_none()` and once with `.unwrap_or((0, 0))` that can never fire. A `let ... else` expresses it once.
- `tui/src/app/input.rs:68` — `mode.search_target().unwrap_or(SearchTarget::Library)` handles an impossible case created by merging two match arms. Separate arms remove the `unwrap_or`.
- `tui/src/app/youtube.rs:279` — the `Downloading` arm destructures four fields and rebuilds the identical value. `modal @ YoutubeModal::Downloading { .. }` binds it whole.
- `tui/src/theme.rs:223` — `hex_digit` and `hex_channel` take `&mut dyn Iterator<Item = char>`; `impl Iterator` monomorphizes and reads the same.

---

## Positive Findings

Worth saying explicitly, because a lot here is done well.

**The test suite.** 256 tests, and the names are the documentation: `playlist_store_flush_deadline_is_anchored_to_the_first_pending_change`, `library_reprobes_a_file_after_it_changes`, `fetch_and_download_survives_a_child_that_floods_stderr`. The fixtures build real WAV files with real RIFF INFO chunks (`core/tests/fixtures/mod.rs`) rather than mocking the tag library, so the metadata tests exercise the actual `lofty` path. Several tests target failure modes most projects skip entirely — unwritable paths, corrupt caches, deadlock-prone subprocess output.

**The lint configuration.** Denying `unwrap_used`, `expect_used`, `panic`, and `indexing_slicing` at the workspace level is a strong stance, and the code genuinely holds to it — clippy reports zero warnings across all targets and all features. The handful of `#[allow]`s are localized and each has a defensible reason, even where I would rewrite the code to remove the need.

**`RowCache`.** Keying the cache on a revision counter for the library plus one for the playlist store, rather than trying to invalidate precisely, is the right trade-off for a TUI. It also reuses the `Vec` buffer via `std::mem::take` instead of reallocating. Good instincts.

**The `dispatch!` macro in `tui/src/backend.rs`.** Enum dispatch across a cfg-gated variant is genuinely annoying to write by hand, and this three-line macro handles it without obscuring anything. Eleven trait methods stay one line each.

**Marker enums instead of `bool`.** `Mutated::{Yes, No}`, `InsertOutcome`, `Selected::{Found, NotFound}`, `EventsChanged`, `KeyEventHandled`, `SaveOutcome`. Call sites read clearly and none of them can be passed the wrong way round.

**`Song`'s use of `Arc`.** `path: Arc<Path>`, `metadata: Arc<Metadata>`, `keys: Arc<SortKeys>`, with `Metadata` fields as `Arc<str>`. Songs are cloned into rows and queues constantly; this makes it cheap and the choice is applied consistently rather than sprinkled.

**`FormFieldOutcome`.** The explicit `Updated`/`Confirmed`/`Cancelled` return, with the form moved by value, makes the modal lifecycle checkable by the compiler. It is the pattern the rest of the modal code should adopt.

**`core/src/lib.rs`'s `compile_error!`.** The non-Unix guard names both reasons (`/dev/urandom` and raw path bytes) and points at the two modules that would need porting. That is a genuinely useful failure message.

**`TODO.md`.** The performance work is benchmarked, with a CSV of results, the commit the harness was built from, and an explicit note that correctness was verified before timing. Most projects' TODO files are a wish list.

---

## Verification Results

Environment note: the workspace does not build out of the box on a bare container. `gstreamer-sys` requires the `gstreamer-1.0` system library via pkg-config, and the first `cargo check` failed with:

```
The system library `gstreamer-1.0` required by crate `gstreamer-sys` was not found.
```

This is an environment prerequisite, not a code defect — the crate is correctly marked optional behind the `gstreamer` feature, and `cargo check --workspace --no-default-features` would avoid it. I installed `libgstreamer1.0-dev` (1.24.2) and re-ran everything. The `packaging/PKGBUILD` should be checked to confirm it declares this dependency.

| Command | Result |
|---|---|
| `cargo check --workspace --all-features` | **Pass** — 0 errors, 0 warnings, finished in 27.58s |
| `cargo test --workspace --all-features` | **Pass** — 256 tests, 0 failed (90 in `core_tests`, 166 in `app_tests`), 0 ignored |
| `cargo clippy --workspace --all-targets --all-features` | **Pass** — 0 warnings, 0 errors |
| `cargo fmt --all -- --check` | **Fail** — 112 diff hunks across 23 files |

**On the `cargo fmt` failure.** This is purely mechanical, not a code problem. The affected files:

```
core/src/scan_cache.rs          tui/src/app_name.rs
core/tests/core_tests.rs        tui/src/theme.rs
tui/src/app/command.rs          tui/src/ui/header.rs
tui/src/app/form.rs             tui/src/ui/mod.rs
tui/src/app/input.rs            tui/src/ui/modals.rs
tui/src/app/metadata.rs         tui/src/ui/now_playing.rs
tui/src/app/mod.rs              tui/src/ui/rows.rs
tui/src/app/modes.rs            tui/src/ui/song_modal.rs
tui/src/app/navigation.rs       tui/src/ui/youtube_modal.rs
tui/src/app/panels.rs           tui/tests/app_tests.rs
tui/src/app/song_modal.rs
tui/src/app/state.rs
tui/src/app/youtube.rs
```

The diffs are line wrapping, import ordering, and trailing whitespace — for example `scan_cache.rs:66` wants a `format!` call split across four lines, and `core_tests.rs:1283` has a stray blank line at EOF. Running `cargo fmt --all` fixes all of it. Worth adding to CI so it does not accumulate.

**Dependencies.** `cargo tree -d` reports duplicate versions of `hashbrown` (0.16/0.17), `itertools` (0.14/0.15), `syn` (1/2/3), `thiserror` (1/2), `bitflags`, `getrandom`, and `r-efi`. Every one of these is transitive through `ratatui` or `gstreamer` — nothing the workspace can resolve, and nothing to act on. 233 crates total for a TUI music player with tag parsing and a GStreamer backend is reasonable. Direct dependencies are all pinned through `[workspace.dependencies]`, which is the right structure, and the optional `gstreamer` / `youtube` features are wired correctly (the `youtube` feature gates no external crate at all — it only enables code paths that shell out to `yt-dlp`, which is a clean way to make it optional).

**Test coverage gaps** worth filling, in rough priority order:

- `player::path_to_uri` — zero tests. It hand-rolls percent-encoding over raw Unix path bytes, which is exactly the kind of function that should have cases for spaces, `#`, `?`, non-UTF-8 bytes, and multibyte characters.
- `PlaylistStore::flush` after a failed save — the High-severity bug above sits directly next to a passing test. Add: fail a save, then assert the second `flush()` still returns `Failed` rather than `NothingToSave`.
- `youtube::newest_mp3_in` and `finalize_download` — zero tests, and both are on the path that touches the user's library files.
- `playlist::parse_hex` — zero tests. It slices a `String` by byte offsets after a `replace`; the validation makes it safe today, but that is worth pinning down (empty, wrong length, non-hex, braces, uppercase).
- `ui::style::format_mtime` — 10 references in tests, but the function is a hand-rolled civil-from-days conversion. Confirm it covers leap years, century boundaries (2000 vs 1900), and `mtime_secs == 0`.
- `navigation::compute_jump` — only tested indirectly through key handling. It has the `len - 1` underflow noted above and would benefit from direct unit tests at `len == 0`, `len < height`, and all-headers-in-view.
- `fuzzy::subsequence_score` with repeated characters — would catch the adjacency-bonus bug.

The two crates also test at different levels: `core_tests.rs` is mostly unit-level against public functions, while `app_tests.rs` drives the `App` through synthetic key events. Both approaches are appropriate to their crate, so this is a difference rather than a problem — but it is why `lyre-tui` needed the `*_for_test` accessors and `lyre-core` did not.

---

## Top 5 Recommendations

**1. Fix the three silent data-loss paths.**
`PlaylistStore::flush` clearing `dirty` on failure (`core/src/playlist.rs:368`), `PlaylistStore::load` treating a corrupt file as empty and then overwriting it (`:188`), and `finish_dir_scan` loading before flushing (`tui/src/app/mod.rs:240`). All three are small edits. All three currently lose user data with no error the user can act on. Do these first.

**2. Stop guessing which file yt-dlp produced, and give each download a private scratch directory.**
Delete `newest_mp3_in` and use the known output path; replace `std::env::temp_dir()` with a per-download `TempDir`. This closes the wrong-file import, the foreign-file mutation, and the concurrent-download collision in one change. While there, reorder `resolve_directory` so validation precedes `create_dir_all`, and make `finalize_youtube_download`'s error paths restore the modal and clean up the temp file.

**3. Collapse `theme.rs` and `config.rs`.**
Derive `Deserialize` on `Theme` with `#[serde(default)]` and expose `theme::current()`, removing four of the five places each color is named — about 250 lines. Extract one `xdg_dir` helper in `config.rs`, removing three copies of the same lookup — about 50 lines. Neither changes behaviour, both remove the places where a future change has to be made in four files at once. This is the cheapest large win in the codebase.

**4. Make the modal lifecycle compiler-checked.**
Change `handle_mode_key` to return `ModeOutcome::{Keep(Mode), Close}` and have `handle_key` do the single `replace`. Then pass `SongModal` by value into `handle_choose_action_key` and mutate it instead of threading five loose parameters through ten exit paths. The `FormFieldOutcome` pattern already in the codebase is the model; this just extends it to the modals that do not use it yet.

**5. Do the row-building performance pass that `TODO.md` already specifies.**
Have `songs_by_path` return `Vec<&Song>` instead of round-tripping through the `HashMap`; stop calling it from `build_rows_into`, which re-sorts anyway; drop the `.to_vec()` on `ids_by_path` at two call sites; cache char counts in `SortKeys`; switch `insert_into` to the entry API to drop the per-song `PathBuf`. The benchmarks are already in the repo (17x on path ordering, 2x on grouped rows, 2x on fuzzy keystrokes) and the correctness criteria are already written down. It is the rare performance task where the hard part is done.
