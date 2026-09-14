# Third-party software

The optional `trailmix-codecs` crate uses Symphonia 0.6 under the Mozilla Public
License 2.0. trail mix does not copy or modify Symphonia source code.

The MPL permits use in a larger work under different terms, but Symphonia and
any modifications to its covered files remain subject to the MPL. Distributors
must retain the applicable notices and provide the corresponding source for
covered modifications when the license requires it.

The core `trailmix`, `beat-salad`, `key-lime`, and `sampler-platter` crates do
not depend on Symphonia.

## Model benchmarks

Model-based benchmark scripts under `benchmarks/baselines` run external tools
against local audio and local annotations. The repository does not vendor
external model source code, model weights, or converted artifacts. Each
external model comparison must record the source revision, artifact hash, code
license, weight license, training disclosure, preprocessing, and output
conversion before its results are used in a report.

First-party model backend work belongs in `trailmix-models`. Any backend that
wraps an external project needs a separate provenance, license, artifact, and
integration review before it becomes a shipped feature.

Run a dependency-license audit before publishing a crate or distributing a
binary. This file is a project record, not legal advice.
