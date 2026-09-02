# Fix plan for commit `25cd855` (scratch-dir rework)

Three issues, ordered by severity. Each has root cause, the fix, and a test to lock it in.

---

## 1. Finalize failure destroys the download

**File:** `core/src/youtube.rs`, `finalize_download`

**Root cause:** the function takes `scratch: Scratch` by value. If both `rename` and `copy` fail, the function returns `Err`, `scratch` goes out of scope, its `Arc` drops to zero, and `ScratchDir::drop` calls `remove_dir_all` — deleting the very file the copy just failed to move. The caller gets an error message and nothing to retry with.

**Fix:** return the scratch back to the caller on failure so it survives. Also clean up a partial file at the destination if `copy` fails partway.

```rust
// core/src/youtube.rs
pub fn finalize_download(scratch: Scratch, dest_path: &Path) -> Result<(), (Scratch, Error)> {
    if fs::rename(scratch.path(), dest_path).is_ok() {
        return Ok(());
    }
    match fs::copy(scratch.path(), dest_path) {
        Ok(_) => Ok(()),
        Err(e) => {
            let _ = fs::remove_file(dest_path);
            Err((scratch, Error::Finalize(e)))
        }
    }
}
```

**Caller update:** `tui/src/app/youtube.rs`, `finalize_youtube_download`. Currently it discards the scratch on error; instead put the fields modal back with the scratch reattached so the user can change the filename/directory and retry without re-downloading.

```rust
// tui/src/app/youtube.rs
fn finalize_youtube_download(
    &mut self,
    mut fields: YoutubeFieldsModal,
    scratch: Scratch,
    dest_path: PathBuf,
) {
    let scratch = match youtube::finalize_download(scratch, &dest_path) {
        Ok(()) => None,
        Err((scratch, e)) => {
            self.set_status(
                format!("download finished, but failed to save it: {e}"),
                StatusKind::Error,
            );
            fields.download_status = DownloadStatus::Ready(scratch);
            self.set_youtube_modal(YoutubeModal::EditingFields(fields));
            return;
        }
    };
    let _ = scratch; // finalized; scratch dir cleans itself up on drop
    // ... existing tagging / Song::load / library insert logic unchanged
}
```

**Test to add** (`core/tests/core_tests.rs`):

```rust
#[cfg(all(feature = "youtube", unix))]
#[test]
fn finalize_download_returns_the_scratch_intact_when_the_destination_is_unwritable() {
    use std::fs;

    let dir = TempDir::new().unwrap();
    let scratch_dir = dir.path().join("scratch");
    fs::create_dir_all(&scratch_dir).unwrap();
    let file = scratch_dir.join("audio.mp3");
    fs::write(&file, b"precious audio").unwrap();
    let scratch = lyre_core::youtube::Scratch::owning(scratch_dir.clone(), file.clone());

    // destination directory doesn't exist -> both rename and copy fail
    let dest = dir.path().join("no-such-subdir/out.mp3");

    let (scratch, _err) = lyre_core::youtube::finalize_download(scratch, &dest).unwrap_err();

    assert!(file.exists(), "source file must survive a failed finalize");
    assert_eq!(scratch.path(), file);
}
```

---

## 2. `sole_file_in` accepts the wrong extension and hard-fails on yt-dlp config side effects

**File:** `core/src/youtube.rs`, `sole_file_in`

**Root cause, part A — extension check dropped.** `newest_mp3_in` required `.mp3`. `sole_file_in` accepts any single file, regardless of extension. If ffmpeg post-processing doesn't produce `.mp3` (e.g. codec unavailable, exits 0 anyway), the wrong file gets renamed to `.mp3` on finalize and handed to the tag writer — silent corruption instead of a clear `OutputMissing`.

**Root cause, part B — no `.mp3` filter means any extra file is fatal.** yt-dlp reads `~/.config/yt-dlp/config` by default. `fetch_and_download` doesn't pass `--ignore-config`, so a user with `--write-thumbnail`, `--write-info-json`, or `--write-subs` set globally gets `OutputAmbiguous` on every single download, even though there's exactly one `.mp3` present.

**Fix:** filter to `.mp3` first, then only treat *multiple* `.mp3` files as ambiguous.

```rust
// core/src/youtube.rs
#[cfg(feature = "youtube")]
fn sole_file_in(dir: &Path) -> Result<PathBuf, Error> {
    let mut found: Option<PathBuf> = None;
    for entry in fs::read_dir(dir).map_err(Error::Temp)? {
        let entry = entry.map_err(Error::Temp)?;
        if !entry.file_type().map_err(Error::Temp)?.is_file() {
            continue;
        }
        let path = entry.path();
        if !path.extension().is_some_and(|ext| ext == "mp3") {
            continue;
        }
        if found.replace(path).is_some() {
            return Err(Error::OutputAmbiguous);
        }
    }
    found.ok_or(Error::OutputMissing)
}
```

Consider also passing `--ignore-config` to the `yt-dlp` invocation in `fetch_and_download` so a user's global config can't change output format or add postprocessors the app doesn't expect — belt-and-suspenders with the filter above:

```rust
// core/src/youtube.rs, in fetch_and_download's Command::new(&ytdlp).args([...])
.args([
    "--ignore-config",
    "-x",
    "--audio-format",
    "mp3",
    "--no-playlist",
    "--newline",
    "--no-warnings",
    "-o",
])
```

**Tests to add** (`core/tests/core_tests.rs`), alongside the existing decoy-file test:

```rust
#[cfg(all(feature = "youtube", unix))]
#[test]
fn fetch_and_download_ignores_non_mp3_siblings_left_by_yt_dlp_postprocessors() {
    // stub yt-dlp writes both audio.mp3 and audio.info.json into the scratch dir;
    // fetch_and_download must still resolve to the .mp3 and succeed.
}

#[cfg(all(feature = "youtube", unix))]
#[test]
fn fetch_and_download_errors_on_two_mp3_files_in_the_scratch_dir() {
    // stub yt-dlp writes audio.mp3 and audio.part.mp3;
    // fetch_and_download must return Error::OutputAmbiguous.
}
```

---

## 3. `Scratch::owning` is a live footgun in the public API

**File:** `core/src/youtube.rs`, `impl Scratch`

**Root cause:** `owning` is `pub`, unconditionally compiled (not behind `#[cfg(test)]` or a feature), and hands an arbitrary `PathBuf` to a type whose `Drop` calls `remove_dir_all` on it. It's only used by tests today, but nothing stops production code from calling `Scratch::owning(some_real_directory, ...)` and later dropping it — a recursive delete of whatever directory was passed, silently.

**Fix:** gate it to test builds only, so it can't be reached from non-test code paths.

```rust
// core/src/youtube.rs
impl Scratch {
    #[cfg(any(test, feature = "test-util"))]
    pub fn owning(dir: PathBuf, file: PathBuf) -> Scratch {
        Scratch {
            dir: Arc::new(ScratchDir(dir)),
            file,
        }
    }

    pub fn path(&self) -> &Path {
        &self.file
    }

    pub fn dir(&self) -> &Path {
        &self.dir.0
    }
}
```

Since `tui/tests/app_tests.rs` also calls `Scratch::owning` across the crate boundary, add a `test-util` feature to `core/Cargo.toml` and have `lyre-tui`'s dev-dependency enable it:

```toml
# core/Cargo.toml
[features]
default = ["youtube", "gstreamer"]
youtube = []
gstreamer = ["dep:gstreamer"]
test-util = []
```

```toml
# tui/Cargo.toml
[dev-dependencies]
tempfile = "3"
lyre-core = { path = "../core", features = ["test-util"] }
```

---

## Smaller items (do alongside the above)

**a) Flaky shared path in `tui/tests/app_tests.rs:2482`**

```rust
// current — fixed path, gets remove_dir_all'd by Scratch's Drop,
// so concurrent test runs (different feature sets / nextest shards) can race
let scratch_dir = std::env::temp_dir().join("lyre-test-download-scratch");
```

Fix: use `TempDir` like the core test does, so each test run gets an isolated directory.

```rust
let scratch_tmp = tempfile::TempDir::new().unwrap();
let scratch_dir = scratch_tmp.path().join("lyre-test-download-scratch");
std::fs::create_dir_all(&scratch_dir).unwrap();
let temp_path = scratch_dir.join("audio.mp3");
std::fs::write(&temp_path, b"audio").unwrap();
let scratch = lyre_core::youtube::Scratch::owning(scratch_dir, temp_path.clone());
// keep scratch_tmp alive for the duration of the test (don't drop early)
```

**b) Test stub can hang forever**

In `core/tests/core_tests.rs`, the fake `yt-dlp` script does:

```sh
while [ "$1" != "-o" ]; do shift; done
```

If `-o` is ever missing from the real args (e.g. someone reorders the `Command::args` call in `fetch_and_download` during a future edit), this spins until the shell runs out of positional args and errors — but only after `shift` on an empty list, which is a silent no-op loop in some shells, not a fast failure. Prefer an explicit scan with a bound:

```sh
found=0
for a in "$@"; do
  if [ "$found" = "1" ]; then
    out=$(echo "$a" | sed 's/%(ext)s/mp3/')
    printf 'ID3' > "$out"
    exit 0
  fi
  [ "$a" = "-o" ] && found=1
done
echo "stub: -o not found in args: $*" >&2
exit 1
```

**c) `Drop` does blocking filesystem I/O wherever the last `Arc<ScratchDir>` reference dies**

For the cancel path in `tui/src/app/youtube.rs` (`drop(scratch)` when the modal is abandoned), this runs on whatever thread drops the value — currently the render/event loop thread, not the download thread. `remove_dir_all` on one small mp3 file is cheap, so this is low priority, but if scratch dirs ever grow (e.g. thumbnails, subs) it's worth moving cleanup to a spawned thread:

```rust
// tui/src/app/youtube.rs, cancel path
Some(other) => {
    if let Some(modal) = other {
        self.set_youtube_modal(modal);
    }
    std::thread::spawn(move || drop(scratch));
}
```

**d) Formatting**

`cargo fmt --all --check` flags several files touched by this commit (`tui/src/app/youtube.rs:377,385`) plus a broader set of pre-existing unformatted files in the repo. Run `cargo fmt --all` before merging so the diff doesn't grow further out of sync.

---

## Suggested order of work

1. Fix #1 (`finalize_download` signature + caller) — highest impact, data-loss bug.
2. Fix #2 (`sole_file_in` filter + `--ignore-config`) — silent corruption + spurious hard failures.
3. Fix #3 (gate `Scratch::owning`) — quick, prevents future misuse.
4. Land items (a)–(d) in the same PR since they touch the same files.
5. Re-run `cargo test --workspace --no-default-features --features youtube` and `cargo clippy --workspace --no-default-features --features youtube --all-targets` before pushing — both are currently clean, so any new failure is from these changes.
