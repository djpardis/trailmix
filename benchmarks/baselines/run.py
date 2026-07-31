"""Run external audio-analysis baselines against a Trail Mix manifest."""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import sys
import time
from concurrent.futures import ProcessPoolExecutor, as_completed
from importlib.metadata import version
from pathlib import Path
from typing import Any

TOOLS = (
    "libkeyfinder",
    "essentia-key-edma",
    "essentia-key-edmm",
    "essentia-tempo-multifeature",
    "essentia-tempo-degara",
    "librosa-tempo",
    "skey",
    "beat-this",
)

POLICY_SCRIPT = Path(__file__).parent.parent / "corpus_policy.py"
DEFAULT_POLICY = Path(__file__).parent.parent / "corpus-policy.json"
DEFAULT_LEDGER = Path(__file__).parent.parent / "local" / "evaluation-ledger.json"

FLAT_TO_SHARP = {
    "Db": "C#",
    "Eb": "D#",
    "Gb": "F#",
    "Ab": "G#",
    "Bb": "A#",
}


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--tool", choices=TOOLS, required=True)
    parser.add_argument("--jobs", type=int, default=4)
    parser.add_argument(
        "--corpus-role",
        choices=("development", "final"),
        required=True,
    )
    parser.add_argument("--corpus-name", required=True)
    parser.add_argument("--configuration", type=Path)
    parser.add_argument("--policy", type=Path, default=DEFAULT_POLICY)
    parser.add_argument("--ledger", type=Path, default=DEFAULT_LEDGER)
    parser.add_argument(
        "--keyfinder-cli",
        type=Path,
        help="Path to keyfinder-cli when --tool=libkeyfinder",
    )
    parser.add_argument(
        "--model-checkpoint",
        type=Path,
        help="Pinned checkpoint path for a model-based tool",
    )
    parser.add_argument(
        "--model-sha256",
        help="Required SHA-256 checksum for a model checkpoint",
    )
    return parser.parse_args()


def normalize_tonic(value: str) -> str:
    tonic = value.strip().replace("♭", "b").replace("♯", "#")
    return FLAT_TO_SHARP.get(tonic, tonic)


def normalize_key(tonic: str, mode: str) -> str:
    normalized_mode = "minor" if mode.lower() in {"m", "minor"} else "major"
    return f"{normalize_tonic(tonic)} {normalized_mode}"


def parse_keyfinder_key(value: str) -> str | None:
    key = value.strip()
    if not key:
        return None
    mode = "minor" if key.endswith("m") else "major"
    tonic = key[:-1] if mode == "minor" else key
    return normalize_key(tonic, mode)


def run_keyfinder(job: dict[str, Any]) -> dict[str, Any]:
    command = [job["keyfinder_cli"], job["path"]]
    output = subprocess.run(
        command,
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    return {"detected_key": parse_keyfinder_key(output)}


def load_essentia_audio(path: str) -> Any:
    import essentia.standard as es

    return es.MonoLoader(filename=path, sampleRate=44_100)()


def run_essentia_key(job: dict[str, Any], profile: str) -> dict[str, Any]:
    import essentia.standard as es

    audio = load_essentia_audio(job["path"])
    key, scale, strength = es.KeyExtractor(
        frameSize=4_096,
        hopSize=2_048,
        hpcpSize=36,
        profileType=profile,
        averageDetuningCorrection=True,
    )(audio)
    return {
        "detected_key": normalize_key(key, scale),
        "strength": float(strength),
    }


def run_essentia_tempo(job: dict[str, Any], method: str) -> dict[str, Any]:
    import essentia.standard as es

    audio = load_essentia_audio(job["path"])
    bpm, beats, confidence, estimates, intervals = es.RhythmExtractor2013(
        method=method,
    )(audio)
    return {
        "detected_bpm": float(bpm),
        "beat_count": len(beats),
        "confidence": float(confidence),
        "candidate_count": len(estimates),
        "interval_count": len(intervals),
    }


def run_librosa_tempo(job: dict[str, Any]) -> dict[str, Any]:
    import librosa
    import numpy as np

    audio, sample_rate = librosa.load(job["path"], sr=22_050, mono=True)
    tempo, beats = librosa.beat.beat_track(y=audio, sr=sample_rate)
    detected = float(np.asarray(tempo).reshape(-1)[0])
    return {
        "detected_bpm": detected,
        "beat_count": len(beats),
    }


def run_skey(job: dict[str, Any]) -> dict[str, Any]:
    import torch
    from skey.key_detection import (
        CropCQT,
        infer_key,
        load_audio,
        load_checkpoint,
        load_model_components,
    )

    cache_key = f"skey:{job['model_checkpoint']}"
    components = MODEL_CACHE.get(cache_key)
    if components is None:
        device = torch.device("cpu")
        checkpoint = load_checkpoint(job["model_checkpoint"])
        components = (*load_model_components(checkpoint, device), device, checkpoint)
        MODEL_CACHE[cache_key] = components
    hcqt, chromanet, crop_fn, device, checkpoint = components
    if not isinstance(crop_fn, CropCQT):
        raise TypeError("invalid S-KEY crop function")
    audio = load_audio(job["path"], checkpoint["audio"]["sr"]).to(device)
    detected = infer_key(hcqt, chromanet, crop_fn, audio, device)
    if detected == "error":
        raise ValueError("S-KEY inference failed")
    tonic, mode = detected.split()
    return {"detected_key": normalize_key(tonic, mode)}


def run_beat_this(job: dict[str, Any]) -> dict[str, Any]:
    import numpy as np
    import soundfile
    from beat_this.inference import Audio2Beats

    cache_key = f"beat-this:{job['model_checkpoint']}"
    analyzer = MODEL_CACHE.get(cache_key)
    if analyzer is None:
        analyzer = Audio2Beats(
            checkpoint_path=job["model_checkpoint"],
            device="cpu",
            dbn=False,
        )
        MODEL_CACHE[cache_key] = analyzer
    audio, sample_rate = soundfile.read(
        job["path"],
        always_2d=True,
        dtype="float32",
    )
    beats, downbeats = analyzer(audio, sample_rate)
    intervals = np.diff(beats)
    plausible = intervals[(intervals >= 0.25) & (intervals <= 2.0)]
    if plausible.size == 0:
        raise ValueError("Beat This returned no plausible beat intervals")
    return {
        "detected_bpm": float(60.0 / np.median(plausible)),
        "beat_count": len(beats),
        "downbeat_count": len(downbeats),
    }


MODEL_CACHE: dict[str, Any] = {}


def run_job(job: dict[str, Any]) -> dict[str, Any]:
    started = time.perf_counter()
    result: dict[str, Any] = {
        "tool": job["tool"],
        "tool_version": job["tool_version"],
        "corpus_role": job["corpus_role"],
        "corpus_fingerprint": job["corpus_fingerprint"],
        "model_sha256": job.get("model_sha256"),
        "track_id": job["track_id"],
        "expected_bpm": job.get("expected_bpm"),
        "expected_key": job.get("expected_key"),
    }
    try:
        tool = job["tool"]
        if tool == "libkeyfinder":
            result.update(run_keyfinder(job))
        elif tool == "essentia-key-edma":
            result.update(run_essentia_key(job, "edma"))
        elif tool == "essentia-key-edmm":
            result.update(run_essentia_key(job, "edmm"))
        elif tool == "essentia-tempo-multifeature":
            result.update(run_essentia_tempo(job, "multifeature"))
        elif tool == "essentia-tempo-degara":
            result.update(run_essentia_tempo(job, "degara"))
        elif tool == "librosa-tempo":
            result.update(run_librosa_tempo(job))
        elif tool == "skey":
            result.update(run_skey(job))
        elif tool == "beat-this":
            result.update(run_beat_this(job))
        else:
            raise ValueError(f"unsupported tool: {tool}")
        result["error"] = None
    except Exception as error:  # noqa: BLE001
        result["error"] = f"{type(error).__name__}: {error}"
    result["elapsed_milliseconds"] = (time.perf_counter() - started) * 1_000.0
    return result


def tool_version(tool: str) -> str:
    if tool.startswith("essentia-"):
        return version("essentia")
    if tool == "librosa-tempo":
        return version("librosa")
    if tool == "skey":
        return version("skey")
    if tool == "beat-this":
        return version("beat-this")
    if tool == "libkeyfinder":
        return subprocess.run(
            ["pkg-config", "--modversion", "libkeyfinder"],
            check=True,
            capture_output=True,
            text=True,
        ).stdout.strip()
    raise ValueError(f"unsupported tool: {tool}")


def authorize_corpus(arguments: argparse.Namespace) -> str:
    command = [
        sys.executable,
        str(POLICY_SCRIPT),
        "--policy",
        str(arguments.policy),
        "--ledger",
        str(arguments.ledger),
    ]
    if arguments.corpus_role == "development":
        command.extend(
            [
                "register-development",
                "--manifest",
                str(arguments.manifest),
                "--name",
                arguments.corpus_name,
            ]
        )
    else:
        if arguments.configuration is None:
            raise ValueError("--configuration is required for a final corpus")
        command.extend(
            [
                "consume",
                "--manifest",
                str(arguments.manifest),
                "--configuration",
                str(arguments.configuration),
            ]
        )
    result = subprocess.run(
        command,
        check=False,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        detail = result.stderr.strip() or result.stdout.strip()
        raise ValueError(f"corpus policy rejected this run: {detail}")
    return result.stdout.strip().rsplit(maxsplit=1)[-1]


def load_completed(path: Path, tool: str) -> set[str]:
    if not path.exists():
        return set()
    completed = set()
    with path.open() as stream:
        for line in stream:
            if not line.strip():
                continue
            result = json.loads(line)
            if result.get("tool") == tool:
                completed.add(result["track_id"])
    return completed


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def model_checkpoint(arguments: argparse.Namespace) -> Path | None:
    if arguments.tool not in {"skey", "beat-this"}:
        return None
    checkpoint = arguments.model_checkpoint
    if checkpoint is None and arguments.tool == "skey":
        from skey.key_detection import DEFAULT_CHECKPOINT_PATH

        checkpoint = DEFAULT_CHECKPOINT_PATH
    if checkpoint is None:
        raise ValueError("--model-checkpoint is required for Beat This")
    if arguments.model_sha256 is None:
        raise ValueError("--model-sha256 is required for model-based tools")
    actual = sha256_file(checkpoint)
    if actual != arguments.model_sha256:
        raise ValueError(
            f"model checksum mismatch: expected {arguments.model_sha256}, found {actual}"
        )
    return checkpoint.resolve()


def build_jobs(
    arguments: argparse.Namespace,
    corpus_fingerprint: str,
) -> list[dict[str, Any]]:
    manifest_path = arguments.manifest.resolve()
    manifest = json.loads(manifest_path.read_text())
    base_directory = manifest_path.parent
    completed = load_completed(arguments.output, arguments.tool)
    version_value = tool_version(arguments.tool)
    checkpoint = model_checkpoint(arguments)
    jobs = []

    for track in manifest["tracks"]:
        is_key_tool = "key" in arguments.tool or arguments.tool == "libkeyfinder"
        if is_key_tool and track.get("expected_key") is None:
            continue
        if not is_key_tool and track.get("expected_bpm") is None:
            continue
        if track["id"] in completed:
            continue

        path = Path(track["path"])
        if not path.is_absolute():
            path = base_directory / path
        job = {
            "tool": arguments.tool,
            "tool_version": version_value,
            "corpus_role": arguments.corpus_role,
            "corpus_fingerprint": corpus_fingerprint,
            "track_id": track["id"],
            "path": str(path),
            "expected_bpm": track.get("expected_bpm"),
            "expected_key": track.get("expected_key"),
        }
        if arguments.tool == "libkeyfinder":
            if arguments.keyfinder_cli is None:
                raise ValueError("--keyfinder-cli is required for libkeyfinder")
            job["keyfinder_cli"] = str(arguments.keyfinder_cli.resolve())
        if checkpoint is not None:
            job["model_checkpoint"] = str(checkpoint)
            job["model_sha256"] = arguments.model_sha256
        jobs.append(job)
    return jobs


def main() -> None:
    arguments = parse_arguments()
    if arguments.jobs < 1:
        raise ValueError("--jobs must be at least 1")
    corpus_fingerprint = authorize_corpus(arguments)
    jobs = build_jobs(arguments, corpus_fingerprint)
    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    total = len(jobs)
    if total == 0:
        print(f"{arguments.tool}: no pending tracks")
        return

    completed = 0
    with (
        arguments.output.open("a") as output,
        ProcessPoolExecutor(max_workers=arguments.jobs) as executor,
    ):
        futures = [executor.submit(run_job, job) for job in jobs]
        for future in as_completed(futures):
            output.write(json.dumps(future.result(), sort_keys=True) + "\n")
            output.flush()
            completed += 1
            if completed % 25 == 0 or completed == total:
                print(
                    f"{arguments.tool}: {completed}/{total} tracks complete",
                    flush=True,
                )


if __name__ == "__main__":
    main()
