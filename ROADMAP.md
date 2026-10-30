# Roadmap

This roadmap tracks near-term work for trail mix releases and reproducible
benchmark artifacts. It is intentionally public-facing and focuses on concrete
engineering and evaluation tasks.

## Reproducible key-model releases

- Measure the embedded Rust key lime backend with the selected paper weights.
  Record binary size, embedded weight size, total installed size, latency, and
  exact agreement against the archived ONNX artifact before citing the smaller
  Rust package size in public results.

- Add a release audit command for paper-facing tags. The audit should verify the
  tag target, release notes, artifact hashes, split fingerprints, schema
  versions, selected seed, and reported metric values before a release is
  published.

- Archive paper-facing GitHub releases with Zenodo when account setup is
  available. Release notes should continue to record the commit, selected model
  state, schema versions, split fingerprint, seeds, metrics, artifact hashes,
  and evidence links.

## Evaluation hygiene

- Add machine-readable split manifests for training, validation, holdout,
  export, and latency fixtures. Include track identifiers, hashes when
  available, and a split fingerprint.

- Add an overlap check for benchmark runs. The check should verify that training
  data, fitted profiles, validation data, and reported evaluation sets have zero
  overlap by track ID or file hash.

- Record complete latency metadata for future benchmark artifacts. This should
  include the exact CPU or SoC, backend version, thread count, corpus, warmup
  rule, timing harness, and whether the measurement includes decoding.

## Model and scorer work

- Run a larger seed set for the compact key reranker and its main ablations.
  More seeds will make architecture and recipe comparisons easier to interpret.

- Evaluate a no-position-embedding variant. Position embeddings are a large
  fraction of the compact transformer, so this test can show whether they are
  needed for global-key reranking.

- Test whether the reranker can consume score vectors from other compact DSP key
  scorers. Start with libraries that can expose stable 24-key score vectors, and
  compare them under the same split and artifact rules as the trail mix scorer.

- Check alternative Essentia key profiles using only validation data for model
  or baseline selection. Holdout-only profile selection should be reported as a
  post hoc diagnostic, not as a baseline choice.

- Keep template-profile provenance documented. Any fitted key profile should
  record the corpus used for fitting and the evidence that reported holdouts
  were not used.

## Cross-corpus validation

- Expand external evaluation beyond the current small classical diagnostic set.
  A broader set of public or shareable corpora will make it easier to describe
  where the key reranker generalizes and where more training data is needed.

- Add repeatable summaries for error categories. Track exact, fifth, relative,
  parallel, and other errors for DSP, reranked, and learned baselines across the
  same manifests.

## Developer workflow

- Keep documentation-only CI lightweight so release-process updates are easy to
  review. Full Rust workspace checks should remain part of release readiness and
  code changes.

- Keep benchmark scripts usable without private local state. Scripts should
  either download public corpora, read documented local manifests, or fail with
  a clear message naming the missing input.
