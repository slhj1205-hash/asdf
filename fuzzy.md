## Implementation Plan

The work should be split into small, independently testable phases. The first phases fix ranking correctness; later phases improve normalization, multi-term relevance, performance, and highlighting.

---

## Phase 1: Lock down current behavior with ranking tests

### Files

- `core/tests/core_tests.rs`
- Potentially a new focused test module in `core/src/fuzzy.rs`

### Add comparison-oriented tests

Existing tests mostly verify whether a match exists. Add tests comparing scores so changes are driven by user-visible ordering.

Cover:

1. **Greedy alignment trap**

   ```rust
   let compact = subsequence_score("ab", "a---ab").unwrap();
   let scattered = subsequence_score("ab", "a---b").unwrap();

   assert!(compact > scattered);
   ```

2. **False consecutive bonus**

   Verify that an unmatched character equal to the previous pattern character does not count as a selected consecutive match.

3. **Exact match ordering**

   ```text
   "blue" > "blue monday" > "electric blue" > "beneath long urban evenings"
   ```

4. **Prefix versus mid-word**

   ```text
   "blue moon" > "electric blue"
   ```

   for the query `blue`.

5. **Contiguous versus scattered**

   ```text
   "beetle" > "beneath the long evening"
   ```

   for a suitable abbreviated query.

6. **Word initials**

   A query such as `dsotm` should rank `Dark Side of the Moon` above an arbitrary scattered match.

7. **Repeated characters**

   Include patterns such as `ana`, `ll`, and `aaa`, where greedy matchers often pick the wrong occurrence.

8. **Unicode and accents**

   Add expected future behavior:

   - `cafe` matches `Café`
   - `beyonce` matches `Beyoncé`
   - uppercase and lowercase forms match
   - decomposed and precomposed Unicode forms match identically

9. **Punctuation**

   - `dont` matches `Don’t Stop`
   - `acdc` matches `AC/DC`
   - `rock roll` matches `Rock-n-Roll`

10. **Multi-field queries**

    Establish expected ranking when terms appear:

    - together in the title;
    - together in another single field;
    - split across title and artist;
    - split across unrelated fields.

### Goal

Before changing the implementation, define expected ordering rather than exact numeric scores wherever possible. Exact score assertions make future tuning unnecessarily difficult.

---

## Phase 2: Introduce compiled query and normalized candidate types

The current interface accepts raw `&str` values:

```rust
subsequence_score(pattern: &str, target: &str)
```

That encourages repeated query processing for every field of every song. Replace it internally with prepared types.

### Files

- `core/src/fuzzy.rs`
- `core/src/song.rs`
- `core/src/playlist.rs`
- `tui/src/app/row_builder.rs`
- `tui/src/app/navigation.rs`

### Proposed core types

```rust
pub struct Pattern {
    normalized: Box<str>,
    chars: Box<[char]>,
}

pub struct FuzzyQuery {
    phrase: Pattern,
    terms: Box<[Pattern]>,
}

pub struct Candidate {
    normalized: Box<str>,
    char_len: u32,
}
```

`FuzzyQuery::new()` should:

1. trim the input;
2. normalize it once;
3. split it into normalized non-empty terms;
4. preserve the full normalized phrase;
5. cache pattern characters.

`Candidate::new()` should normalize a metadata field once and cache its character length.

### API direction

Use:

```rust
pub fn score(pattern: &Pattern, candidate: &Candidate) -> Option<u32>;
```

and:

```rust
impl Song {
    pub fn fuzzy_score(&self, query: &FuzzyQuery) -> Option<u32>;
}
```

Avoid creating a `Pattern` inside `Song::fuzzy_term_score()`. The query should be compiled once per keystroke in:

- `tui/src/app/row_builder.rs`
- `tui/src/app/navigation.rs`

### Compatibility

During migration, retain `subsequence_score()` as a thin wrapper if tests or other code still use it:

```rust
pub fn subsequence_score(pattern: &str, target: &str) -> Option<u32> {
    let pattern = Pattern::new(pattern);
    let target = Candidate::new(target);
    score(&pattern, &target)
}
```

It can be removed later if there are no external API compatibility requirements.

---

## Phase 3: Replace greedy matching with best-alignment scoring

### File

- `core/src/fuzzy.rs`

### Correctness issue to remove

Delete the current consecutive-match condition:

```rust
prev_target_char == prev_pattern_char
```

Consecutiveness must be based on selected target positions, not equal character values.

### Algorithm

Implement dynamic programming that computes the best score for matching the first `i` pattern characters with the current pattern character ending at target position `j`.

Conceptually:

```text
best[i][j] =
    character_bonus(j)
    + max(
        best[i - 1][j - 1] + consecutive_bonus,
        best[i - 1][k] - gap_penalty(j - k - 1), for all k < j - 1
      )
```

Use signed intermediate scores such as `i32`. Penalties are awkward and error-prone with `u32` and `saturating_sub()`.

### Complexity requirement

Do not implement the transition as a nested scan over all prior positions, which would be `O(pattern_len × target_len²)`.

With a linear gap penalty, maintain the best prior gapped value while scanning the target. This permits `O(pattern_len × target_len)` time and `O(target_len)` working memory using two rolling rows.

### Suggested scoring model

Put constants together at the top of `fuzzy.rs`:

```rust
const MATCH: i32 = 16;
const CONSECUTIVE: i32 = 20;
const WORD_BOUNDARY: i32 = 24;
const FIELD_START: i32 = 28;
const GAP_START: i32 = -3;
const GAP_EXTENSION: i32 = -1;
const LEADING_GAP: i32 = -1;
```

The exact values should be tuned through ordering tests and realistic fixtures rather than treated as API.

Desired priorities:

1. exact field;
2. exact prefix;
3. contiguous match at the beginning;
4. contiguous match at a later word boundary;
5. word-initial abbreviation;
6. ordinary contiguous substring;
7. scattered subsequence.

### Bonuses outside the DP

After finding the best alignment, apply explicit intent bonuses:

```rust
if pattern.normalized == candidate.normalized {
    score += EXACT_FIELD_BONUS;
} else if candidate.normalized.starts_with(&*pattern.normalized) {
    score += PREFIX_BONUS;
}
```

Also consider an exact-word bonus when the pattern equals a complete word inside the candidate.

### Overflow handling

- Compute in `i32` or `i64`.
- Return `None` when no alignment exists.
- Convert successful final scores to the public score type in one place.
- Use checked or saturating conversion rather than relying on casts.

### Validation

Run the focused fuzzy tests after this phase before changing song-level scoring. This isolates matcher regressions from field weighting changes.

---

## Phase 4: Add Unicode and punctuation normalization

### Files

- `Cargo.toml`
- `core/Cargo.toml`
- `core/src/fuzzy.rs`
- `core/src/song.rs`
- `core/src/playlist.rs`

### Dependency

Add `unicode-normalization` as a workspace dependency, then enable it in `lyre-core`.

### Normalization pipeline

Use the same normalization function for query patterns and candidate fields:

1. apply Unicode decomposition;
2. remove combining marks;
3. lowercase;
4. normalize whitespace;
5. normalize punctuation consistently;
6. optionally recompose into a stable representation.

Suggested punctuation policy:

- apostrophes such as `'`, `’`, and `ʼ`: remove them;
- hyphens, underscores, slashes, and repeated whitespace: normalize to a single space;
- parentheses and most separators: normalize to spaces;
- trim leading and trailing spaces.

This yields intuitive behavior such as:

```text
Don’t Stop  -> dont stop
AC/DC       -> ac dc
Rock-n-Roll -> rock n roll
Beyoncé     -> beyonce
```

### Boundary preservation

Word-boundary bonuses should operate on normalized boundaries. Do not simply remove every separator, because that would erase meaningful word starts.

Apostrophes are a special case: removing them is desirable for `dont` versus `don’t`, while hyphens and slashes should generally remain logical boundaries.

### Romanized fields

Continue scoring `title_sort` and `artist_sort` as alternative searchable fields. Normalize them through the same pipeline so native and romanized text follow identical rules.

### Tests

Add tests for:

- precomposed and decomposed accents;
- typographic and ASCII apostrophes;
- slash and hyphen boundaries;
- non-Latin fields and existing romanized search behavior.

---

## Phase 5: Cache normalized song search fields and lengths

### File

- `core/src/song.rs`

The existing `SortKeys` already stores lowercase search strings. Extend or replace it so each searchable field is a prepared `Candidate`.

Possible shape:

```rust
struct SearchKeys {
    title: Candidate,
    artist: Candidate,
    album: Candidate,
    title_sort: Option<Candidate>,
    artist_sort: Option<Candidate>,
}
```

If sorting still needs plain lowercase strings, either:

- expose `Candidate::as_str()` and use normalized strings for sorting if that behavior is acceptable; or
- keep sorting keys and fuzzy candidates separate to avoid changing sort behavior unintentionally.

The safer initial implementation is:

```rust
struct SortKeys {
    title: Box<str>,
    artist: Box<str>,
    album: Box<str>,
    title_sort: Option<Box<str>>,
    artist_sort: Option<Box<str>>,

    fuzzy_title: Candidate,
    fuzzy_artist: Candidate,
    fuzzy_album: Candidate,
    fuzzy_title_sort: Option<Candidate>,
    fuzzy_artist_sort: Option<Candidate>,
}
```

This uses more memory but keeps the change behaviorally isolated. The representation can be consolidated after profiling.

### Metadata updates

Ensure `Song::assemble()` and metadata-update paths rebuild all normalized candidates whenever title, artist, album, or sort metadata changes.

### Playlist names

`Playlist::fuzzy_score()` currently allocates and lowercases its name on every search. Either:

- cache a normalized candidate inside `Playlist`; or
- normalize playlist names once while collecting visible playlist scores.

Caching inside `Playlist` is preferable if rename operations can reliably rebuild the cache. Because cached fields are derived data, exclude them from serialization or reconstruct them after deserialization.

### Performance goal

This phase should eliminate repeated:

- lowercasing;
- Unicode normalization;
- `.chars().count()`;

for every field, song, term, and keystroke.

---

## Phase 6: Improve song-level field and phrase ranking

### File

- `core/src/song.rs`

Current behavior takes the best field independently for each term and adds the scores. Preserve that as a baseline because it allows useful queries such as:

```text
artist-name song-title
```

Then add structured bonuses.

### Return field information internally

Introduce:

```rust
enum SearchField {
    Title,
    Artist,
    Album,
    TitleSort,
    ArtistSort,
}

struct TermMatch {
    score: u32,
    field: SearchField,
}
```

For every query term:

1. score all searchable fields;
2. choose the best field match;
3. fail the song if any term has no match;
4. sum the selected term scores.

### Field weights

Retain title preference, but make it explicit:

```rust
const TITLE_WEIGHT: u32 = 150;
const ARTIST_WEIGHT: u32 = 100;
const ALBUM_WEIGHT: u32 = 100;
```

Apply weights with multiplication followed by division to avoid floating-point scoring.

### Same-field bonus

After selecting the best field for each term:

- add a bonus if all terms match the title;
- add a smaller bonus if all terms match one artist or album field;
- do not reject split-field matches.

### Full phrase bonus

Score the full normalized query phrase against each field. If it matches:

- use it as an additional candidate score;
- give the strongest phrase bonus for title;
- give smaller phrase bonuses for artist and album.

This ensures:

```text
Query: dark side
```

ranks a title containing `Dark Side` above a song with `dark` in its artist and `side` in its album.

### Avoid double counting

Treat the final score as a combination of:

```text
sum of required term scores
+ best phrase bonus
+ same-field consistency bonus
```

Do not add every possible field match, or songs with duplicated metadata will receive inflated scores.

### Tests

Add song fixtures proving:

- every term remains required;
- split-field matching still works;
- title phrase beats split-field matches;
- title matches retain a sensible preference over artist/album matches;
- romanized title matches remain available.

---

## Phase 7: Add deterministic relevance tie-breaking

### Files

- `tui/src/app/row_builder.rs`
- `tui/src/app/navigation.rs`

Song results currently fall back to title, which is reasonable. Make the complete order deterministic:

1. descending fuzzy score;
2. ascending normalized title;
3. ascending artist;
4. song ID or path as a final unique tie-breaker.

Playlist search currently sorts only by score:

```rust
scored.sort_by_key(|&(_, score)| Reverse(score));
```

Add a normalized-name or existing name-order tie-breaker so equal scores do not move unpredictably.

Deterministic ordering matters because small score-tuning changes otherwise make result lists appear unstable.

---

## Phase 8: Add optional match positions for highlighting

Do not make every library-wide scoring operation allocate a `Vec<usize>`. That would be expensive for large libraries.

### File

- `core/src/fuzzy.rs`

Provide two APIs:

```rust
pub fn score(pattern: &Pattern, candidate: &Candidate) -> Option<u32>;

pub fn score_with_positions(
    pattern: &Pattern,
    candidate: &Candidate,
) -> Option<FuzzyMatch>;
```

```rust
pub struct FuzzyMatch {
    pub score: u32,
    pub positions: Box<[usize]>,
}
```

### Implementation strategy

- `score()` uses rolling DP rows and no predecessor matrix.
- `score_with_positions()` stores predecessor information and reconstructs the winning alignment.
- Use `score()` while filtering the entire library.
- Call `score_with_positions()` only for visible rows after sorting, or only for the selected row.

This keeps the hot path fast while enabling user-facing highlighting.

### Position mapping

Normalization can change string length, especially when accents or punctuation are removed. `Candidate` should optionally retain a mapping from normalized character positions to original character or byte positions.

For example:

```rust
struct NormalizedChar {
    value: char,
    original_byte_offset: usize,
    word_boundary: bool,
}
```

To avoid memory overhead on every song, consider constructing this detailed mapping only when `score_with_positions()` is requested from the original display string.

### TUI work

Once the matcher exposes original positions:

- highlight matched title characters first;
- optionally highlight artist and album fields;
- ensure Unicode byte boundaries are respected when building Ratatui spans;
- preserve the existing selected-row styling.

Treat highlighting as a separate UI change after ranking is stable.

---

## Phase 9: Benchmark and tune

The repository already documents fuzzy-keystroke benchmarks in `TODO.md`. Re-run or recreate that benchmark with the new algorithm.

### Benchmark cases

Measure:

- 100 songs;
- 1,000 songs;
- 10,000 songs;
- 50,000 songs;
- 100,000 songs.

Queries should include:

- a one-character query;
- a common short query;
- a five-character query;
- multiple terms;
- accented text;
- a query that matches few songs;
- a query that matches most songs.

### Metrics

Capture separately:

1. query normalization;
2. matching all fields;
3. collecting matches;
4. relevance sorting;
5. optional position reconstruction.

### Performance safeguards

The DP matcher is more expensive than the current linear greedy scan. Before merging:

- compile the query once;
- cache normalized candidates and lengths;
- use rolling DP rows;
- avoid allocations per field where possible;
- reuse scratch buffers if profiling shows allocation pressure;
- preserve the pre-sorted library optimization described in `TODO.md`.

Set a responsiveness target, for example:

- common searches under roughly 16–30 ms for normal libraries;
- no more than approximately 50 ms per keystroke for very large libraries.

If full DP is still too expensive, use a two-stage strategy:

1. cheap subsequence existence filter;
2. best-alignment scoring only for candidates that pass.

Because the DP already detects non-matches, only retain the prefilter if benchmarks show a clear benefit.

---

## Phase 10: Documentation and cleanup

### Files

- `core/src/fuzzy.rs`
- `README.md` if search behavior is user-documented
- `TODO.md`

Document:

- normalization policy;
- ranking priorities;
- whether every query term is required;
- whether terms may match different fields;
- score values being internal and unstable.

After migration:

- remove the old greedy implementation;
- remove redundant lowercase conversions in TUI call sites;
- remove repeated `.chars().count()` calls;
- update the fuzzy optimization entry in `TODO.md`;
- avoid exposing raw score constants outside `core/src/fuzzy.rs`.

---

## Recommended delivery sequence

Use separate focused changes so regressions are easy to isolate:

1. Add ranking regression tests.
2. Introduce `Pattern`, `FuzzyQuery`, and `Candidate`.
3. Implement best-alignment DP and exact/prefix bonuses.
4. Add Unicode and punctuation normalization.
5. Cache song and playlist candidates.
6. Add phrase, field, and same-field ranking.
7. Add deterministic tie-breakers.
8. Benchmark and tune constants.
9. Add optional matched-position reconstruction.
10. Add TUI highlighting.

The first six steps deliver the primary user benefit. Highlighting should remain last because it complicates normalization and original-string position mapping without improving ranking correctness itself.
