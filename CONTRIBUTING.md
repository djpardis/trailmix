# Contributing

Trail Mix is a Rust 2024 workspace and requires Rust 1.85 or newer. The project
is still a research prototype, so changes should distinguish measured behavior
from intended behavior and avoid unsupported accuracy claims.

## Development checks

Run the same checks used by CI before submitting a change:

```sh
cargo fmt --all -- --check
cargo test --workspace --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
```

CI runs the test suite on Linux, macOS, and Windows. Keep platform-specific file
handling and command behavior portable across all three systems.

## Documentation maintenance

Documentation is part of each change, not a separate cleanup task. Update the
relevant document in the same change when you alter:

- public behavior, result fields, supported formats, or prerequisites
- command names, arguments, setup steps, or development checks
- workspace crates or boundaries between crates
- known limitations, benchmark metrics, or confidence semantics
- completed work or future milestones
- dependencies with licensing or redistribution requirements

Use each document for one purpose:

- [README.md](README.md) introduces the project and its main usage paths.
- [ARCHITECTURE.md](ARCHITECTURE.md) explains boundaries, crates, result
  versioning, and current technical limitations.
- [BENCHMARKING.md](BENCHMARKING.md) documents evaluation, datasets,
  annotations, metrics, and Test Kitchen.
- [ROADMAP.md](ROADMAP.md) tracks planned work rather than current behavior.
- [THIRD_PARTY.md](THIRD_PARTY.md) records third-party licensing obligations.

Check relative links, examples, package names, and flags after editing
documentation. Prefer a link to the authoritative document over copying the
same explanation into several files.

## Test and benchmark data

Do not commit commercial recordings, private manifests, generated benchmark
reports, or data without clear redistribution terms. Common audio formats and
local benchmark paths are ignored, but contributors remain responsible for
checking staged changes.

Synthetic tests should be deterministic. Any public fixture must include enough
provenance and licensing information for another contributor to verify that it
can be redistributed.

See [BENCHMARKING.md](BENCHMARKING.md) for the private evaluation workflow and
supported metrics.
