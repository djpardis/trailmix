# Benchmark process

Benchmark results are only meaningful when development data, model training
data, and final evaluation data remain separate.

## Corpus roles

- **Development**: results have been inspected or used to choose an algorithm,
  parameter, model, or postprocessor. Development results can guide work but
  cannot support a final comparative claim.
- **Sealed evaluation**: the manifest fingerprint was registered before any
  result was inspected. The implementation and configuration must be frozen
  before this corpus is consumed.
- **Evaluated**: the seal was consumed with one frozen configuration. The same
  configuration may rerun for reproducibility. A changed configuration requires
  a new sealed corpus.

GiantSteps Key 604 and GiantSteps Tempo v2 are development corpora for Trail
Mix because their results were inspected during algorithm work on July 28,
2026. They remain useful for diagnostics and regression measurements.

## Local ledger

`corpus_policy.py` stores corpus fingerprints and state transitions in
`benchmarks/local/evaluation-ledger.json`. The ledger is local because manifests
and annotations may be private. It is ignored by Git.

A final corpus also needs a public seal receipt. The receipt commits to the
corpus fingerprint, track count, and frozen configuration hash without exposing
audio paths or annotations. Commit and publish it before running the evaluation.

Register a development corpus:

```sh
uv run --project benchmarks/baselines python benchmarks/corpus_policy.py \
  register-development \
  --manifest benchmarks/manifest.private.json \
  --name "GiantSteps development corpora"
```

Seal a new evaluation corpus before inspecting results:

```sh
uv run --project benchmarks/baselines python benchmarks/corpus_policy.py seal \
  --manifest benchmarks/final.private.json \
  --name "Reserved final evaluation" \
  --configuration benchmark-results/frozen-configuration.json \
  --receipt benchmarks/seals/final.json
```

Consume the seal immediately before the first final run:

```sh
uv run --project benchmarks/baselines python benchmarks/corpus_policy.py consume \
  --manifest benchmarks/final.private.json \
  --configuration benchmark-results/frozen-configuration.json
```

## Model gate

Every model comparison must record:

- Repository and immutable source revision
- Model artifact URL and SHA-256 checksum
- Code license and weight license
- Training datasets disclosed by the authors
- Known private, copyrighted, overlapping, or unavailable training data
- Input preprocessing, inference runtime, and output conversion

An open-source code license does not establish that training audio can be
redistributed or that a model is independently reproducible.

Before selecting a final corpus, record its annotation license, audio
acquisition terms, and known overlap with every compared model's training data.
If training overlap is unknown, report the result as external validation, not
as a clean held-out comparison.

## Reporting gate

A final report must include the corpus fingerprint, frozen configuration hash,
exact metric implementation, tool and model versions, failure count, and
runtime environment. Development results must be labeled as development
results. A report cannot describe a corpus as held out after its results have
influenced implementation choices.
