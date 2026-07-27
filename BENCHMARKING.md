# Benchmarking and annotation

Trail Mix includes a synthetic smoke benchmark and tools for evaluating
recorded music. The synthetic benchmark is useful for regression testing. Only
a legally usable, representative music corpus can support accuracy claims.

## Synthetic smoke benchmark

The deterministic benchmark generates three 20-second signals with expected
tempos of 90, 120, and 128 BPM:

```sh
cargo run --release -p trailmix-bench
```

It prints JSON with the expected and detected BPM, absolute error, confidence,
and analysis time for each case. These generated signals are much simpler than
recorded music and do not test key accuracy or changing tempo.

## Private evaluation manifest

Recorded audio, private annotations, and generated reports must remain outside
version control. The repository ignores:

- common audio-file extensions
- `benchmarks/local/`
- `benchmarks/*.private.json`
- `benchmark-results/`

Use [benchmarks/manifest.example.json](benchmarks/manifest.example.json) as a
schema example:

```sh
cp benchmarks/manifest.example.json benchmarks/manifest.private.json
```

Its tracks and paths are placeholders. Replace them with your own legally
usable files and annotations. Paths in a manifest are resolved relative to the
manifest file.

A manifest can record:

- global BPM and key
- beat timestamps
- local tempo and key segments
- structural, tempo, and key change events
- comparison values from Serato
- dataset provenance, review status, reviewer, and confidence

The shared `trailmix-manifest` crate validates IDs, ranges, confidence values,
and timeline ordering before a manifest is saved or benchmarked.

## Test Kitchen

Test Kitchen is a local annotation interface for private manifests:

```sh
cargo run -p test-kitchen -- benchmarks/manifest.private.json
```

It listens only on a loopback address and opens the interface in the default
browser. If the manifest does not exist, Test Kitchen creates an empty one.
Use `--no-open` to start the server without opening a browser:

```sh
cargo run -p test-kitchen -- --no-open benchmarks/manifest.private.json
```

The interface supports local audio playback, tap tempo, reference beats, global
and local BPM and key annotations, combined change events, Serato observations,
review metadata, and on-demand Trail Mix analysis. Each save validates the
manifest and replaces it through a temporary file in the same directory.

## Real-track benchmark

Run the corpus benchmark against an annotated manifest:

```sh
cargo run --release -p trailmix-bench -- \
  --manifest benchmarks/manifest.private.json
```

The command prints versioned JSON with per-track results and aggregate
statistics. Reports identify tracks by ID and do not include source paths.

The benchmark currently scores:

- global BPM mean absolute error
- octave-aware global BPM mean absolute error
- exact global key accuracy
- local tempo-segment mean absolute error, matched at segment midpoints
- duration-weighted exact local key-segment accuracy
- mean decoding and analysis time

The manifest also supports reference beats, change events, and Serato
observations, but the benchmark does not yet score those fields. See
[ROADMAP.md](ROADMAP.md) for planned beat-grid, boundary, calibration, memory,
and binary-size evaluation.

## GiantSteps datasets

The importer supports the official
[GiantSteps Tempo](https://github.com/GiantSteps/giantsteps-tempo-dataset) and
[GiantSteps Key](https://github.com/GiantSteps/giantsteps-key-dataset)
annotation repositories. Obtain any audio separately by following each
dataset's instructions and terms. Do not commit downloaded audio.

A convenient ignored layout is:

```text
benchmarks/local/datasets/
├── giantsteps-key/
│   ├── annotations/
│   └── audio/
└── giantsteps-tempo/
    ├── annotations_v2/
    └── audio/
```

The importer expects MP3 files named by dataset item ID in each `audio/`
directory. Import the revised Tempo v2 annotations with:

```sh
cargo run -p trailmix-datasets -- giantsteps-tempo \
  benchmarks/local/datasets/giantsteps-tempo \
  benchmarks/local/datasets/giantsteps-tempo/audio \
  benchmarks/manifest.private.json
```

Import the original GiantSteps Key annotations with:

```sh
cargo run -p trailmix-datasets -- giantsteps-key \
  benchmarks/local/datasets/giantsteps-key \
  benchmarks/local/datasets/giantsteps-key/audio \
  benchmarks/manifest.private.json
```

The importer records dataset name, version, item ID, and citation. It skips
non-positive Tempo v2 labels and reports missing audio files without discarding
their annotations. Re-running a command updates tracks with matching imported
IDs instead of creating duplicates.

## Dataset references

- Peter Knees, Ángel Faraldo, Perfecto Herrera, Richard Vogl, Sebastian Böck,
  Florian Hörschläger, and Mickael Le Goff. "Two Data Sets for Tempo Estimation
  and Key Detection in Electronic Dance Music Annotated from User Corrections."
  ISMIR 2015.
- Hendrik Schreiber and Meinard Müller. "A Crowdsourced Experiment for Tempo
  Estimation of Electronic Dance Music." ISMIR 2018. This work provides the
  revised GiantSteps Tempo v2 labels used by the importer.

Preserve dataset provenance and follow each source's citation and redistribution
requirements when publishing results.
