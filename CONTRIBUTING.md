# Contributing to triangel

What a change to triangel should look like. To set up your machine, see [firmware/README.md](firmware/README.md). You need dabao boards.

## Checks

Install the commit hook once, from the repository root:

    pip install pre-commit
    pre-commit install

Every commit then runs clippy on each build of the eye, the ear and the shared code, with warnings treated as errors, and checks that the generated LED tables are current.

There are no automated tests. What a change does to the lights has to be looked at: build the eye with `previewer` and watch it in the [previewer](docs/how-to/run-the-previewer.md), or run it on a triangel.

## Conventions

### Code

- Idiomatic Rust, with no clippy warnings.
- Sound tuning values go in `firmware/shared/src/tuning.rs`, not in the pattern that uses them.
- Do not edit generated files by hand: `firmware/eye/src/led/geom.rs` (run `tools/gen_geom.py`), `firmware/ear/src/i2s_bio.rs` and `firmware/eye/src/input/nec_rx.rs`.
- The Rust version, the xous-core revision in both `Cargo.toml` files, and the kernels move together. Do not change one alone.

### Patterns

See [firmware/eye/PATTERNS.md](firmware/eye/PATTERNS.md) for how a pattern is built and what keeps it fast enough.
