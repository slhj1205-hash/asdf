# TODO

## Row-building optimisation pass: pre-sorted index + decorated sort + cached fuzzy lengths

Status: benchmarked, not started.
Bench harness: /tmp/lyre-bench (clone of this repo at 3acda5c). /tmp is
ephemeral -- the full raw data is embedded below, so the harness is only
needed to re-run, not to read the numbers.

Environment: release build (opt-level 3, thin LTO, codegen-units 1),
synthetic library, real lyre-core types. Timing: adaptive rounds to >=0.2 s,
warm-up pass first. Correctness checks passed before timing: identical row
streams, identical fuzzy ranking order, identical path order.

### Full raw results

ns per operation unless noted. "extra" column for scan_cache_load is the
cache file size in bytes.

```csv
op,variant,n,ns_per_op,extra
path_order_vec,current,100,19148,
path_order_vec,presorted,100,1129,
rows_group_by_path,current,100,33513,
rows_group_by_path,decorated,100,16994,
fuzzy_keystroke,current,100,31582,
fuzzy_keystroke,presorted+cachedlen,100,16684,
scan_cache_load,pretty_json,100,86124,47892 B
scan_cache_load,compact_json,100,67382,26091 B
queue_shuffle,fisher_yates,100,4715,
queue_unshuffle,clone+rescan,100,4723,
library_get_x_n,siphash,100,987,
library_get_x_n,fnv_hasher,100,401,
path_order_vec,current,1000,308603,
path_order_vec,presorted,1000,10218,
rows_group_by_path,current,1000,2337518,
rows_group_by_path,decorated,1000,361121,
fuzzy_keystroke,current,1000,468120,
fuzzy_keystroke,presorted+cachedlen,1000,198814,
scan_cache_load,pretty_json,1000,813684,479892 B
scan_cache_load,compact_json,1000,654848,261891 B
queue_shuffle,fisher_yates,1000,36128,
queue_unshuffle,clone+rescan,1000,36222,
library_get_x_n,siphash,1000,9618,
library_get_x_n,fnv_hasher,1000,4041,
path_order_vec,current,10000,4763117,
path_order_vec,presorted,10000,119390,
rows_group_by_path,current,10000,27412526,
rows_group_by_path,decorated,10000,4129631,
fuzzy_keystroke,current,10000,6533631,
fuzzy_keystroke,presorted+cachedlen,10000,2280932,
scan_cache_load,pretty_json,10000,8512249,4808892 B
scan_cache_load,compact_json,10000,6457755,2628891 B
queue_shuffle,fisher_yates,10000,365913,
queue_unshuffle,clone+rescan,10000,368579,
library_get_x_n,siphash,10000,103046,
library_get_x_n,fnv_hasher,10000,43090,
path_order_vec,current,25000,14390151,
path_order_vec,presorted,25000,369293,
rows_group_by_path,current,25000,72713971,
rows_group_by_path,decorated,25000,10228126,
fuzzy_keystroke,current,25000,19534988,
fuzzy_keystroke,presorted+cachedlen,25000,6304019,
scan_cache_load,pretty_json,25000,21747532,12038892 B
scan_cache_load,compact_json,25000,16867829,6588891 B
queue_shuffle,fisher_yates,25000,926162,
queue_unshuffle,clone+rescan,25000,936878,
library_get_x_n,siphash,25000,337742,
library_get_x_n,fnv_hasher,25000,113817,
path_order_vec,current,50000,32972450,
path_order_vec,presorted,50000,761836,
rows_group_by_path,current,50000,155285806,
rows_group_by_path,decorated,50000,21252111,
fuzzy_keystroke,current,50000,42043500,
fuzzy_keystroke,presorted+cachedlen,50000,14834076,
scan_cache_load,pretty_json,50000,45286725,24088892 B
scan_cache_load,compact_json,50000,34228924,13188891 B
queue_shuffle,fisher_yates,50000,1872573,
queue_unshuffle,clone+rescan,50000,1887879,
library_get_x_n,siphash,50000,729979,
library_get_x_n,fnv_hasher,50000,231431,
path_order_vec,current,100000,73236286,
path_order_vec,presorted,100000,1601674,
rows_group_by_path,current,100000,337455957,
rows_group_by_path,decorated,100000,44225614,
fuzzy_keystroke,current,100000,105581023,
fuzzy_keystroke,presorted+cachedlen,100000,33914272,
scan_cache_load,pretty_json,100000,94928482,48188892 B
scan_cache_load,compact_json,100000,67219863,26388891 B
queue_shuffle,fisher_yates,100000,3833431,
queue_unshuffle,clone+rescan,100000,3901271,
library_get_x_n,siphash,100000,1489328,
library_get_x_n,fnv_hasher,100000,577327,
```

### What each measurement includes

- **path_order_vec** (A): full collect into a Vec of path-sorted songs --
  the first step of every row rebuild.
- **rows_group_by_path** (B): sort plus header emission; both variants emit
  byte-identical rows.
- **fuzzy_keystroke** (C): average over typing "night" one character at a
  time; each step is a full library filter+score+sort including the final
  relevance sort.
- **scan_cache_load** (D): file read + JSON parse + HashMap build. Compact
  variant measured as a plain compact array; same data, same types.
- **queue_shuffle / queue_unshuffle** (E): shuffle alone, and unshuffle
  (clone + cursor reindex). Fisher-Yates is O(n)-unavoidable, so no variant
  can win much here.
- **library_get_x_n** (F): n lookups against a pre-built map, isolating the
  hasher only.

### The three changes

A. **Pre-sorted index in Library** (`core/src/library.rs`)

   Problem: `ids_by_path()` and `songs_by_path()` rebuild and sort a
   `Vec<(&Path, SongId)>` on every call (library.rs:159-179). The TUI calls
   `songs_by_path()` on every row-cache miss -- that is, on every keystroke
   while searching and every sort/category change.

   Change:
   - Add a field `by_path: Vec<(Arc<Path>, SongId)>` to `Library`, kept sorted
     by path at all times.
   - Build it once in `scan()`: the files vec is already
     `sort_unstable()`-ed before probing (library.rs:43), so collect from
     `files.into_iter().zip(outcomes)` in order instead of sorting again.
     In `insert()`, push and re-sort, or binary-search for the insert point
     (`partition_point`) and splice.
   - Rewrite `ids_by_path()` to return a clone of the id column
     (`self.by_path.iter().map(|(_, id)| *id).collect()`), O(n) no sort.
   - Rewrite `songs_by_path()` to iterate
     `self.by_path.iter().filter_map(|(_, id)| self.songs.get(*id))`.
     Keep returning `impl Iterator<Item = &Song> + '_`.
   - `update_metadata()` replaces a song under the same id; path does not
     change, so the index stays valid. No touch needed there.
   - Storing `Arc<Path>` (not `&Path`) makes the index self-contained; Song
     already holds its path as `Arc<Path>` (song.rs:324), so clone the Arc,
     no new allocation.

   Measured (ns/op, release, current vs pre-sorted):
   - n=100:      19_148 -> 1_129
   - n=1_000:   308_603 -> 10_218
   - n=10_000: 4_763_117 -> 119_390
   - n=50_000: 32_972_450 -> 761_836
   - n=100_000: 73_236_286 -> 1_601_674  (46x)

B. **Decorate-sort-undecorate for Category::Path rows**
   (`tui/src/app/row_builder.rs`)

   Problem: in `build_rows()` the Path arm's comparator calls
   `relative_parent()` (strip_prefix + parent) on BOTH sides of EVERY
   comparison (row_builder.rs:244-247). That is O(n log n) prefix strips
   instead of n.

   Change:
   - Before the sort, build
     `let decorated: Vec<(&Song, &Path)> = songs.into_iter()
        .map(|s| (s, relative_parent(s, root))).collect();`
   - Sort the decorated vec by `a.1.cmp(b.1).then_with(|| within-group key)`.
     Keep the existing tie-break: `sort_title()` comparison.
   - Then run the existing header-emission loop over the decorated vec; use
     the stored relative parent instead of calling `relative_parent(song)`
     again inside the loop (it is also called there today, row_builder.rs:252).
   - Do NOT change the Artist or None arms; their keys are already
     precomputed lowercase strings on SortKeys, so they do not have this bug.

   Correctness bar: byte-identical row streams vs current code (headers,
   ids, depths, order). The bench harness asserted this at n=1000.

   Measured (ns/op, release):
   - n=100:       33_513 -> 16_994
   - n=1_000:   2_337_518 -> 361_122
   - n=10_000: 27_412_526 -> 4_129_631
   - n=50_000: 155_285_806 -> 21_252_111
   - n=100_000: 337_455_957 -> 44_225_614  (7.6x)

C. **Cache fuzzy field char lengths** (`core/src/song.rs`, used by
   `tui/src/app/row_builder.rs::fuzzy_filter_and_sort`)

   Problem: `fuzzy_term_score()` calls `.chars().count()` on title, artist
   and album on every call (song.rs:426-430). That recount runs per field,
   per term, per song, per keystroke.

   Change:
   - Extend `SortKeys` (song.rs:278-285) with
     `title_len: u32, artist_len: u32, album_len: u32`.
   - Fill them in `SortKeys::build()` alongside the lowercase strings
     (`.chars().count()` of the same lowered string).
   - Add accessors `title_len()`, `artist_len()`, `album_len()`.
   - Use them in `fuzzy_term_score()` where the counts happen now.
     Keep the exact scoring math (`normalize_by_length` inputs unchanged);
     ranking must stay identical. The harness compared full ranked id lists.
   - Note: the real song.rs also scores title_sort/artist_sort fields. If
     those get lengths too, add `title_sort_len`/`artist_sort_len` as
     Option<u32>. The bench only exercised the three main fields.

   Measured keystroke cost = average of typing "n", "ni", "nig", "nigh",
   "night", each a full library filter+score+sort (ns/op, release; combined
   with pre-sorted iteration from A):
   - n=1_000:    468_120 -> 198_814
   - n=10_000: 6_533_631 -> 2_280_932
   - n=50_000: 42_043_500 -> 14_834_076
   - n=100_000: 105_581_023 -> 33_914_272  (3.1x)
   At 100k songs a keystroke drops ~106 ms -> ~34 ms.

### Why these three together

They sit on one path. Every search keystroke and every sort change does:
`songs_by_path()` (A) -> group/sort into rows (B) -> fuzzy score all songs
(C). Fixing them together removes the whole chain. A alone also speeds up
every non-search row rebuild (46x on the collect step).

Not in scope (benchmarked separately, see results.csv): compact scan-cache
JSON (1.4x CPU, halves file size -- trivial, can ride along later),
FNV hasher for Library's map (2.6x on lookups but tiny absolute numbers),
queue shuffle (already optimal).

### Test plan

1. Existing tests must pass unchanged:
   `cargo test -p lyre-core -p lyre-tui`
2. Row-stream equality: add a test in tui comparing rows built by the old
   comparator order against the decorated version over a fixture library
   with nested dirs, duplicate artists, empty-dir edge cases.
3. Fuzzy ranking equality: score a fixed query set before/after; ranked id
   lists must be identical.
4. Index integrity: after `insert()`, `update_metadata()`, rescan, assert
   `by_path` is sorted and has one entry per song
   (`debug_assert!(is_sorted)` is enough for debug builds).
5. Bench numbers above were release-only. Re-run /tmp/lyre-bench after
   landing if you want to confirm in-repo gains.

### Pitfalls

- `Library::empty()` must init `by_path: Vec::new()`.
- `ids_by_path()` returns `Vec<SongId>` today; keep the signature, callers
  exist in src/main.rs and tui/src/app/mod.rs.
- `songs_by_path()` borrows `self.songs` inside an iterator over
  `self.by_path`; both are fields of `self`, which is fine for an
  immutable iterator, but watch lifetimes if you switch to `Vec<&Song>`
  internally.
- The scan-time shortcut (reuse the presorted files vec) depends on
  `collect_files` output being sorted AFTER collection (library.rs:43);
  keep that sort, it is the cheap one.
