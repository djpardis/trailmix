# Open-source baselines

These scripts run external analyzers against the same local audio and reference
labels used by `trailmix-bench`. Predictions and reports are written under the
ignored `benchmark-results/` directory.

All runs follow the corpus roles and local ledger described in
[`../PROCESS.md`](../PROCESS.md). The runner refuses a final run unless the
corpus was sealed before evaluation.

The tools are evaluation-only dependencies. Trail Mix does not link to or copy
their source code.

## Python environment

The baseline environment uses `uv` and a pinned Python version:

```sh
uv sync
```

It includes:

- Essentia for HPCP key estimation and multifeature or Degara beat tracking
- librosa for spectral-flux onset strength and dynamic-programming beat tracking
- S-KEY for model-based key detection
- Beat This for model-based beat and downbeat tracking

Pinned model revisions, checksums, licenses, training disclosures, and inference
settings are recorded in [`models.json`](models.json). Model-based runs require
the recorded SHA-256 checksum.

## libKeyFinder

Install `libkeyfinder`, FFmpeg, and CMake, then build
[`keyfinder-cli`](https://github.com/evanpurkhiser/keyfinder-cli). Pass the
resulting executable with `--keyfinder-cli`.

libKeyFinder and its CLI are GPL-licensed external programs. Their binaries and
source checkouts must remain outside this repository.

## Run

Each tool appends resumable JSON Lines output:

```sh
uv run python run.py \
  --manifest ../manifest.private.json \
  --output ../../benchmark-results/open-source-baselines.jsonl \
  --tool essentia-key-edma \
  --corpus-role development \
  --corpus-name "GiantSteps development corpora"
```

Available tool configurations:

- `libkeyfinder`
- `essentia-key-edma`
- `essentia-key-edmm`
- `essentia-tempo-multifeature`
- `essentia-tempo-degara`
- `librosa-tempo`
- `skey`
- `beat-this`

[`models.json`](models.json) records artifact hashes, source revisions, licenses,
training disclosures, and exclusions. KeyMyna is excluded despite its stronger
April 2026 literature result because neither audited repository provides an
explicit code or weight license.

The Essentia key configurations use 36-bin HPCP, average detuning correction,
and the named EDM profile. Tempo configurations use the corresponding
`RhythmExtractor2013` method. librosa uses its default 22,050 Hz load rate and
dynamic-programming beat tracker.

Summarize all completed predictions:

```sh
uv run python summarize.py \
  --input ../../benchmark-results/open-source-baselines.jsonl \
  --output ../../benchmark-results/open-source-baselines.json
```

Tempo `accuracy_1` requires a prediction within 4 percent of the reference.
Standard `accuracy_2` also accepts factors of 2, 3, 1/2, and 1/3, following
the [`tempo_eval.equal2`](https://tempoeval.github.io/tempo_eval/generated_functions/tempo_eval.equal2.html)
definition. Key scoring follows the MIREX relationship weights and accepts
perfect fifth errors in either direction.
