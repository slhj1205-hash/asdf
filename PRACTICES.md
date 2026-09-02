## Code Clarity

Do not write comments to explain what the code does. The code itself should be sufficiently clear to communicate its behaviour.

Prefer descriptive function, variable, type, and enum names over comments. Structure code so that its intent can be understood by reading it.

Function names and return values should communicate meaningful intent rather than implementation details.

Avoid:

```rust
// Check if the track can be played
if !track.1 {
    return false;
}
```

Prefer:

```rust
if !track.is_playable() {
    return false;
}
```

Likewise, prefer meaningful return values over ambiguous values that require the reader to inspect the implementation.

Avoid:

```rust
if get_track_state(track) == 0 {
    ...
}
```

Prefer:

```rust
if !track.is_playable() {
    ...
}
```

Do not add comments when renaming, restructuring, or simplifying the code can make the behaviour self-explanatory.

## Single Source of Truth

There should be only one authoritative source for any piece of state, configuration, or business logic.

Do not duplicate the same state or logic across multiple parts of the codebase. Other components should derive the information they need from the authoritative source rather than maintaining their own copy.

Avoid maintaining multiple representations of the same state:

```rust
player.current_track
ui.current_track
audio.current_track
```

Prefer having one authoritative state and deriving the others from it.

The same principle applies to logic. Rules, calculations, and behaviour should have one authoritative implementation. Do not reimplement the same logic in multiple modules and rely on them remaining consistent.

When information can be derived from an existing source, derive it rather than storing another independent copy.

When a piece of behaviour needs to change, there should be one place that needs to be changed.

## Optimisation

Do not write optimisation-specific code directly into normal application logic when doing so makes the code harder to read or understand.

Keep optimisations isolated behind clearly named helper functions or abstractions. The calling code should express the intent of the operation, while the implementation detail of how it is optimised should remain inside the helper.

For example, avoid:

```rust
let index = (position + capacity - 1) & (capacity - 1);
```

Prefer:

```rust
let index = wrap_index(position, capacity);
```

with the optimisation contained in:

```rust
#[inline]
fn wrap_index(position: usize, capacity: usize) -> usize {
    (position + capacity - 1) & (capacity - 1)
}
```

This keeps optimisation details separate from application logic and makes the purpose of the operation clear.

Do not introduce helpers purely for the sake of abstraction. Optimise only when there is a demonstrated performance benefit, and keep the optimised implementation isolated from the rest of the codebase.

## Simplicity

Keep the code clean, simple, and easy to understand.

Prefer straightforward solutions over clever or overly abstract ones. Do not introduce unnecessary layers, abstractions, or complexity when a simpler implementation is sufficient.

Do not pursue an optimisation when the resulting code becomes significantly more complicated or difficult to maintain. Readability and maintainability take priority over small or unmeasured performance improvements.

Prefer:

```rust
let samples = decode_audio(data)?;
```

over introducing multiple layers of abstractions solely to optimise the decoding path.

Optimisations should have a clear and measurable benefit. When an optimisation requires substantially more complexity, keep the simpler implementation unless there is a demonstrated need for the additional performance.

The best code is code that is easy to understand, easy to modify, and does not solve problems that do not exist.

## How to write replies

- Use small words. Keep sentences short. Keep paragraphs short.
- One idea per sentence.
- If you must use a big word or a term of art, explain it in the next sentence.
- When a decision is needed, present it as clear options. Give context for each option. Say which option you would pick and why.
- Keep all file paths and commands exact.
- Write in ASD-STE100 Simplified Technical English. This is a controlled-English standard used in aerospace docs: active voice, only common words, no slang, no idioms.
