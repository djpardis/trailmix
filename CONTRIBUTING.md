# Contributing to trail mix

Thank you for helping improve trail mix. This document covers the local checks,
the commit style, and the data rules that apply to every change. Release steps
are documented separately in [RELEASE.md](RELEASE.md), and the analyzer design
is described in [ARCHITECTURE.md](ARCHITECTURE.md).

## Local checks

CI runs on Linux, macOS, and Windows. Run the same gates locally before opening
a pull request:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
./scripts/measure-binary-size.sh
```

The Python benchmark scripts have their own unit tests, which run without the
model baselines:

```sh
uv run --no-project --python 3.13 --with requests --with remotezip \
  python -m unittest discover -s benchmarks -p 'test_*.py'
uv run --no-project --python 3.13 \
  python -m unittest discover -s benchmarks/baselines -p 'test_*.py'
```

CI uses the current stable Rust toolchain, so run `rustup update stable` when a
new Clippy lint appears in CI but not locally. The minimum supported Rust
version is 1.85.

## Website

The project site at [trailmix.tools](https://trailmix.tools) is generated from
`README.md`, `ARCHITECTURE.md`, and `TRANSFORMER.md` with Eleventy. Preview
changes with:

```sh
npm ci
npm start
```

## Changes to analysis output

The serialized `Analysis` shape is the integration contract for applications
that store results. Additive fields are acceptable within the shipped version-1
schema. Renaming or removing a field, or changing the meaning of an existing
value, requires a new top-level schema version and a note in
[INTEGRATION.md](INTEGRATION.md).

Changes that affect tempo, key, beat, or waveform accuracy should include a
benchmark comparison under the same corpus, configuration, and metric
definitions as the previous result. [benchmarks/PROCESS.md](benchmarks/PROCESS.md)
describes the corpus roles and the evaluation procedure, and
[BACKENDS.md](BACKENDS.md) lists the evidence required for learned backends.

## Data and artifacts

Do not commit audio files, private manifests (`*.private.json`), generated
benchmark output, or model checkpoints that lack recorded provenance. Any
shipped model weights must document their training data, the data license, the
weight license, and an artifact hash in [THIRD_PARTY.md](THIRD_PARTY.md).

## Commits and pull requests

Write one short, capitalized commit subject in the imperative mood, with an
optional plain prefix such as `Docs:`, `Key:`, or `CI:`. Keep each commit to one
coherent change, and squash fixup commits before a pull request is merged.

## License

trail mix is available under the MIT License or the Apache License, Version
2.0, at your option. Unless you state otherwise, any contribution you
intentionally submit for inclusion is dual licensed under the same terms,
without additional terms or conditions.
