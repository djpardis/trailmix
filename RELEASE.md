# Release process

trail mix is an independent Rust workspace. Cueport consumes it as an analysis
engine, but trail mix versions are cut on their own semver line.

## Release types

- Core analysis release: `beat-salad`, `key-lime`, `sampler-platter`,
  `trailmix`, and `trailmix-codecs`.
- Research and tooling release: `trailmix-manifest`, `trailmix-datasets`,
  `trailmix-models`, `trailmix-bench`, `trailmix-cli`, and `test-kitchen`.

Publish only crates that are ready to be part of the public API. Mark any
workspace crate that is not intended for crates.io with `publish = false`.

## Before the first crates.io release

Confirm every crate that will be published has:

- `version`, `license`, `repository`, `rust-version`, and `edition`.
- A `description` suitable for crates.io.
- `README.md` or a crate-level readme entry.
- `keywords` and `categories` where useful.
- Only registry dependencies, or path dependencies that also include the exact
  version being published.
- No private datasets, generated benchmark output, local paths, credentials, or
  machine-specific files in the package.

Check package contents before publishing:

```sh
cargo package -p beat-salad --list
cargo package -p key-lime --list
cargo package -p sampler-platter --list
cargo package -p trailmix --list
cargo package -p trailmix-codecs --list
```

## Local release checks

Run the same checks as CI before tagging or publishing:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
./scripts/measure-binary-size.sh
```

Push the release commit and wait for GitHub Actions to pass on Linux, macOS, and
Windows before publishing.

## Version and dependency update

Set the workspace version in `Cargo.toml`. For crates that depend on workspace
crates, include both the local path and the release version. Example:

```toml
trailmix = { version = "0.1.0", path = "../trailmix" }
```

This keeps local development using the workspace while giving `cargo publish`
the registry version it needs.

For the core analysis release, publish in dependency order:

1. `beat-salad`
2. `key-lime`
3. `sampler-platter`
4. `trailmix`
5. `trailmix-codecs`

If tooling crates are released, publish their library dependencies first, then
the tools that depend on them.

## Tag and publish

Use an annotated tag for each release:

```sh
git tag -a v0.1.0 -m "trail mix 0.1.0"
git push origin main
git push origin v0.1.0
```

Publish from the tagged commit:

```sh
git checkout v0.1.0
cargo publish -p beat-salad --dry-run
cargo publish -p beat-salad
cargo publish -p key-lime --dry-run
cargo publish -p key-lime
cargo publish -p sampler-platter --dry-run
cargo publish -p sampler-platter
cargo publish -p trailmix --dry-run
cargo publish -p trailmix
cargo publish -p trailmix-codecs --dry-run
cargo publish -p trailmix-codecs
```

Wait for each published dependency to appear on crates.io before publishing the
crate that depends on it. If a crate version reaches crates.io with a packaging
mistake, yank it if needed and cut a new patch version. Do not reuse a published
version.

## GitHub release

Create a GitHub release for the tag after crates.io publishing succeeds. Include:

- The crates and versions published.
- The analysis profile and schema version.
- Compatibility notes for Cueport.
- Important benchmark or validation artifacts by link, not generated local data.
- License and third-party notice changes, if any.

## Updating Cueport

Until the crates.io release, Cueport should consume trail mix with explicit git
revisions and release metadata that records the exact commit.

After the crates.io release, update Cueport to use crates.io versions:

```toml
trailmix = "0.1.0"
trailmix-codecs = { version = "0.1.0", features = ["common-codecs"] }
```

Then update Cueport release metadata, the Direct analysis profile, and release
checks so the desktop build reports the trail mix crate version instead of a git
commit label.
