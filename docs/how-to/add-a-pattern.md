# How to add a pattern

For a guided first try, see [Your first pattern](../tutorials/first-pattern.md). For everything a pattern can use, see the [Pattern toolkit](../reference/pattern-toolkit.md). Paths here are relative to `firmware/eye/`.

## 1. Write the pattern

Put it in its own file: `src/patterns/ambient/` for an ambient pattern, `src/patterns/reactive/` for a sound-reactive one. It is a struct that implements `Pattern`.

If it keeps anything over time, such as particles, a start time or a random seed, reset it in `on_enter`, which runs each time the pattern comes back on screen.

## 2. Add it to its folder's module list

In `src/patterns/ambient/mod.rs` or `src/patterns/reactive/mod.rs`, add a line in alphabetical order:

```rust
pub mod your_pattern;
```

## 3. Add it to the setlist

In `src/setlist.rs`, add it to the `use crate::patterns::ambient::{...}` or `use crate::patterns::reactive::{...}` list, then add it to `ambient_patterns()` or `reactive_patterns()`:

```rust
Box::new(YourPattern::new()),
```

The setlist plays the patterns in the order listed. While you work on it, putting it first makes it the one you see when the eye starts.

## 4. Put sound tuning in the shared crate

For a sound-reactive pattern, put its tuning constants in their own module in `firmware/shared/src/tuning.rs`, next to the detection thresholds they interact with, not in the pattern's file.

## 5. Check it

Build the eye with the `previewer` option and watch it, as in [How to run the previewer](run-the-previewer.md). For a sound-reactive pattern, play music from real speakers: a phone speaker cannot play the low notes.

Then check it on a real triangel. The previewer cannot show whether a pattern is fast enough; see [How to make a pattern fast enough](make-a-pattern-fast-enough.md).
