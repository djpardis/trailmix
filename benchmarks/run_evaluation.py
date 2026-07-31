"""Run trailmix-bench against sealed evaluation corpora."""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import sys
from datetime import UTC, datetime
from pathlib import Path
from typing import Any

CORPORA = {
    "fmakv2": "fmakv2-external.private.json",
    "fsl10k": "fsl10k-external.private.json",
}

BENCHMARKS_DIR = Path(__file__).parent
DEFAULT_RESULTS_DIR = BENCHMARKS_DIR.parent / "benchmark-results"
DEFAULT_CONFIGURATION = BENCHMARKS_DIR / "configurations" / "candidate-2026-07-30.json"
POLICY_SCRIPT = BENCHMARKS_DIR / "corpus_policy.py"
DEFAULT_POLICY = BENCHMARKS_DIR / "corpus-policy.json"
DEFAULT_LEDGER = BENCHMARKS_DIR / "local" / "evaluation-ledger.json"


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--corpus",
        choices=sorted(CORPORA),
        nargs="+",
        default=sorted(CORPORA),
        help="Which corpora to evaluate (default: all)",
    )
    parser.add_argument(
        "--configuration",
        type=Path,
        default=DEFAULT_CONFIGURATION,
    )
    parser.add_argument("--results", type=Path, default=DEFAULT_RESULTS_DIR)
    parser.add_argument("--policy", type=Path, default=DEFAULT_POLICY)
    parser.add_argument("--ledger", type=Path, default=DEFAULT_LEDGER)
    parser.add_argument(
        "--corpus-role",
        choices=("development", "final"),
        default="development",
    )
    parser.add_argument(
        "--release",
        action="store_true",
        help="Build and run trailmix-bench in release mode",
    )
    return parser.parse_args()


def file_sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def source_revision() -> str | None:
    result = subprocess.run(
        ["git", "rev-parse", "HEAD"],
        capture_output=True,
        text=True,
        cwd=BENCHMARKS_DIR.parent,
    )
    if result.returncode == 0:
        return result.stdout.strip()
    return None


def build_bench(release: bool) -> Path:
    command = ["cargo", "build", "-p", "trailmix-bench"]
    if release:
        command.append("--release")
    subprocess.run(
        command,
        check=True,
        cwd=BENCHMARKS_DIR.parent,
    )
    import os

    target_dir = os.environ.get("CARGO_TARGET_DIR")
    if target_dir:
        base = Path(target_dir)
    else:
        base = BENCHMARKS_DIR.parent / "target"
    profile = "release" if release else "debug"
    return base / profile / "trailmix-bench"


def authorize_corpus(
    manifest_path: Path,
    corpus_name: str,
    corpus_role: str,
    configuration_path: Path,
    policy_path: Path,
    ledger_path: Path,
) -> str:
    command = [
        sys.executable,
        str(POLICY_SCRIPT),
        "--policy",
        str(policy_path),
        "--ledger",
        str(ledger_path),
    ]
    if corpus_role == "development":
        command.extend([
            "register-development",
            "--manifest",
            str(manifest_path),
            "--name",
            corpus_name,
        ])
    else:
        command.extend([
            "consume",
            "--manifest",
            str(manifest_path),
            "--configuration",
            str(configuration_path),
        ])
    result = subprocess.run(command, check=False, capture_output=True, text=True)
    if result.returncode != 0:
        detail = result.stderr.strip() or result.stdout.strip()
        raise RuntimeError(f"corpus policy rejected this run: {detail}")
    return result.stdout.strip().rsplit(maxsplit=1)[-1]


def run_bench(binary: Path, manifest_path: Path) -> dict[str, Any]:
    result = subprocess.run(
        [str(binary), "--manifest", str(manifest_path)],
        check=True,
        capture_output=True,
        text=True,
    )
    return json.loads(result.stdout)


def main() -> None:
    arguments = parse_arguments()
    arguments.results.mkdir(parents=True, exist_ok=True)
    binary = build_bench(arguments.release)
    revision = source_revision()
    configuration_sha256 = file_sha256(arguments.configuration)

    for corpus_name in arguments.corpus:
        manifest_filename = CORPORA[corpus_name]
        manifest_path = BENCHMARKS_DIR / manifest_filename
        if not manifest_path.exists():
            print(f"skipping {corpus_name}: manifest not found at {manifest_path}")
            continue

        display_name = f"{corpus_name} external validation"
        fingerprint = authorize_corpus(
            manifest_path,
            display_name,
            arguments.corpus_role,
            arguments.configuration,
            arguments.policy,
            arguments.ledger,
        )

        print(f"running trailmix-bench on {corpus_name} ({fingerprint[:12]}...)")
        report = run_bench(binary, manifest_path)
        report["evaluation_metadata"] = {
            "corpus_name": display_name,
            "corpus_fingerprint": fingerprint,
            "corpus_role": arguments.corpus_role,
            "configuration_sha256": configuration_sha256,
            "source_revision": revision,
            "evaluated_at": datetime.now(UTC).isoformat(),
        }

        output_path = arguments.results / f"trailmix-{corpus_name}.json"
        output_path.write_text(json.dumps(report, indent=2) + "\n")
        print(f"  wrote {output_path}")
        print(f"  summary: {json.dumps(report['summary'], indent=4)}")


if __name__ == "__main__":
    main()
