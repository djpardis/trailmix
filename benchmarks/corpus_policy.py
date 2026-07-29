"""Manage development and sealed evaluation corpus roles."""

from __future__ import annotations

import argparse
import hashlib
import json
import time
from contextlib import contextmanager
from datetime import UTC, datetime
from pathlib import Path
from typing import Any

DEFAULT_POLICY = Path(__file__).with_name("corpus-policy.json")
DEFAULT_LEDGER = Path(__file__).parent / "local" / "evaluation-ledger.json"


class PolicyError(ValueError):
    """Raised when a benchmark action would violate corpus policy."""


def canonical_json(value: Any) -> bytes:
    return json.dumps(
        value,
        ensure_ascii=True,
        separators=(",", ":"),
        sort_keys=True,
    ).encode()


def sha256(value: Any) -> str:
    return hashlib.sha256(canonical_json(value)).hexdigest()


def load_json(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text())


def file_sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def manifest_fingerprint(manifest: dict[str, Any]) -> str:
    tracks = []
    for track in manifest["tracks"]:
        tracks.append(
            {
                key: value
                for key, value in track.items()
                if key not in {"path", "source_path"}
            }
        )
    return sha256(
        {
            "manifest_version": manifest["version"],
            "tracks": sorted(tracks, key=lambda track: track["id"]),
        }
    )


def blocked_development_corpora(
    manifest: dict[str, Any],
    policy: dict[str, Any],
) -> list[str]:
    track_ids = [track["id"] for track in manifest["tracks"]]
    blocked = []
    for corpus in policy["development_corpora"]:
        prefix = corpus["track_id_prefix"]
        source_names = set(corpus.get("source_names", []))
        has_matching_source = any(
            track.get("source", {}).get("name") in source_names
            for track in manifest["tracks"]
        )
        if has_matching_source or any(
            track_id.startswith(prefix) for track_id in track_ids
        ):
            blocked.append(corpus["name"])
    return blocked


def load_ledger(path: Path) -> dict[str, Any]:
    if path.exists():
        return load_json(path)
    return {"version": 1, "corpora": {}}


def write_ledger(path: Path, ledger: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(json.dumps(ledger, indent=2, sort_keys=True) + "\n")
    temporary.replace(path)


@contextmanager
def locked_ledger(path: Path) -> Any:
    path.parent.mkdir(parents=True, exist_ok=True)
    lock_directory = path.with_suffix(path.suffix + ".lock")
    deadline = time.monotonic() + 10.0
    while True:
        try:
            lock_directory.mkdir()
            break
        except FileExistsError:
            try:
                lock_age = time.time() - lock_directory.stat().st_mtime
                if lock_age > 30.0:
                    lock_directory.rmdir()
            except FileNotFoundError:
                continue
            if time.monotonic() >= deadline:
                raise PolicyError(
                    f"timed out waiting for ledger lock: {path}"
                ) from None
            time.sleep(0.05)

    ledger = load_ledger(path)
    try:
        yield ledger
        write_ledger(path, ledger)
    finally:
        lock_directory.rmdir()


def now() -> str:
    return datetime.now(UTC).isoformat()


def register_development(
    manifest_path: Path,
    name: str,
    policy_path: Path,
    ledger_path: Path,
) -> str:
    manifest = load_json(manifest_path)
    fingerprint = manifest_fingerprint(manifest)
    with locked_ledger(ledger_path) as ledger:
        existing = ledger["corpora"].get(fingerprint)
        if existing and existing["state"] != "development":
            raise PolicyError(
                f"{name} was already {existing['state']} and cannot become development data"
            )
        ledger["corpora"][fingerprint] = {
            "name": name,
            "state": "development",
            "registered_at": existing.get("registered_at", now())
            if existing
            else now(),
            "policy_sha256": sha256(load_json(policy_path)),
        }
    return fingerprint


def seal_evaluation(
    manifest_path: Path,
    name: str,
    configuration_path: Path,
    policy_path: Path,
    ledger_path: Path,
    receipt_path: Path | None = None,
) -> str:
    manifest = load_json(manifest_path)
    blocked = blocked_development_corpora(manifest, load_json(policy_path))
    if blocked:
        names = ", ".join(blocked)
        raise PolicyError(f"development corpus cannot be sealed: {names}")

    fingerprint = manifest_fingerprint(manifest)
    configuration_sha256 = file_sha256(configuration_path)
    sealed_at = now()
    receipt = {
        "version": 1,
        "corpus_name": name,
        "corpus_sha256": fingerprint,
        "configuration_sha256": configuration_sha256,
        "track_count": len(manifest["tracks"]),
        "sealed_at": sealed_at,
    }
    with locked_ledger(ledger_path) as ledger:
        existing = ledger["corpora"].get(fingerprint)
        if existing:
            raise PolicyError(
                f"{name} is already registered with state {existing['state']}"
            )
        ledger["corpora"][fingerprint] = {
            "name": name,
            "state": "sealed",
            "sealed_at": sealed_at,
            "policy_sha256": sha256(load_json(policy_path)),
            "configuration_sha256": configuration_sha256,
            "track_count": len(manifest["tracks"]),
        }
    if receipt_path is not None:
        receipt_path.parent.mkdir(parents=True, exist_ok=True)
        try:
            with receipt_path.open("x") as stream:
                json.dump(receipt, stream, indent=2, sort_keys=True)
                stream.write("\n")
        except FileExistsError as error:
            raise PolicyError(f"seal receipt already exists: {receipt_path}") from error
    return fingerprint


def consume_evaluation(
    manifest_path: Path,
    configuration_path: Path,
    policy_path: Path,
    ledger_path: Path,
) -> str:
    manifest = load_json(manifest_path)
    blocked = blocked_development_corpora(manifest, load_json(policy_path))
    if blocked:
        names = ", ".join(blocked)
        raise PolicyError(f"development corpus cannot support a final claim: {names}")

    fingerprint = manifest_fingerprint(manifest)
    configuration_sha256 = file_sha256(configuration_path)
    with locked_ledger(ledger_path) as ledger:
        existing = ledger["corpora"].get(fingerprint)
        if existing is None:
            raise PolicyError("evaluation corpus must be sealed before it is consumed")
        if existing["state"] == "evaluated":
            if existing["configuration_sha256"] != configuration_sha256:
                raise PolicyError(
                    "evaluated corpus cannot be reused with a different configuration"
                )
            return fingerprint
        if existing["state"] != "sealed":
            raise PolicyError(f"corpus state must be sealed, found {existing['state']}")
        if existing["configuration_sha256"] != configuration_sha256:
            raise PolicyError("sealed corpus requires its frozen configuration")

        existing["state"] = "evaluated"
        existing["evaluated_at"] = now()
    return fingerprint


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--policy", type=Path, default=DEFAULT_POLICY)
    parser.add_argument("--ledger", type=Path, default=DEFAULT_LEDGER)
    subparsers = parser.add_subparsers(dest="command", required=True)

    register = subparsers.add_parser("register-development")
    register.add_argument("--manifest", type=Path, required=True)
    register.add_argument("--name", required=True)

    seal = subparsers.add_parser("seal")
    seal.add_argument("--manifest", type=Path, required=True)
    seal.add_argument("--name", required=True)
    seal.add_argument("--configuration", type=Path, required=True)
    seal.add_argument("--receipt", type=Path, required=True)

    consume = subparsers.add_parser("consume")
    consume.add_argument("--manifest", type=Path, required=True)
    consume.add_argument("--configuration", type=Path, required=True)

    subparsers.add_parser("status")
    return parser.parse_args()


def main() -> None:
    arguments = parse_arguments()
    if arguments.command == "register-development":
        fingerprint = register_development(
            arguments.manifest,
            arguments.name,
            arguments.policy,
            arguments.ledger,
        )
        print(f"registered development corpus {fingerprint}")
    elif arguments.command == "seal":
        fingerprint = seal_evaluation(
            arguments.manifest,
            arguments.name,
            arguments.configuration,
            arguments.policy,
            arguments.ledger,
            arguments.receipt,
        )
        print(f"sealed evaluation corpus {fingerprint}")
    elif arguments.command == "consume":
        fingerprint = consume_evaluation(
            arguments.manifest,
            arguments.configuration,
            arguments.policy,
            arguments.ledger,
        )
        print(f"authorized evaluation corpus {fingerprint}")
    elif arguments.command == "status":
        print(json.dumps(load_ledger(arguments.ledger), indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
