# Contributing to Triangel

What a change to Triangel should look like. To set up your machine, see [How to set up a development machine](docs/how-to/set-up-a-development-machine.md). You need dabao boards.

New to Triangel? Start with [Your first run](docs/tutorials/first-run.md).

## Checks

Install the commit hook once, from the repository root:

    pip install pre-commit
    pre-commit install

Every commit then runs clippy on each build of the eye, the ear and the shared code, with warnings treated as errors, and checks that the code is formatted with `cargo fmt`.

There are no automated tests. What a change does to the lights has to be looked at: build the eye with `previewer` and watch it in the [previewer](docs/how-to/run-the-previewer.md), or run it on a Triangel.

## Conventions

### Code

- Idiomatic Rust, formatted with `cargo fmt`, with no clippy warnings.
- Sound tuning values go in `firmware/shared/src/tuning.rs`, not in the pattern that uses them. See [How to tune the sound response](docs/how-to/tune-the-sound-response.md).
- Do not edit generated files by hand: `firmware/eye/src/led/map.rs`, `triangel-previewer/led_map.js` and `firmware/eye/src/led/geom.rs` (see [How to regenerate the LED map](docs/how-to/regenerate-the-led-map.md)), `firmware/ear/src/i2s_bio.rs` (see [How to regenerate the mic program](docs/how-to/regenerate-the-mic-program.md)) and `firmware/eye/src/input/nec_rx.rs`.
- The Rust version, the xous-core revision in both `Cargo.toml` files, and the kernels move together. Do not change one alone.

### Patterns

See [How to add a pattern](docs/how-to/add-a-pattern.md), [How to make a pattern fast enough](docs/how-to/make-a-pattern-fast-enough.md) and the [Pattern toolkit](docs/reference/pattern-toolkit.md).
