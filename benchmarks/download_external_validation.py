"""Download only the audio selected by an external-validation manifest."""

from __future__ import annotations

import argparse
import json
import shutil
import time
from pathlib import Path, PurePosixPath
from typing import Any

import requests
from remotezip import RemoteZip
from requests.adapters import HTTPAdapter
from urllib3.util.retry import Retry

ARCHIVES = {
    "fmakv2": "https://os.unil.cloud.switch.ch/fma/fma_large.zip",
    "fsl10k": "https://zenodo.org/api/records/3967852/files/FSL10K.zip/content",
}


def http_session() -> requests.Session:
    retry = Retry(
        total=5,
        backoff_factor=1.0,
        status_forcelist=(429, 500, 502, 503, 504),
        allowed_methods=frozenset({"GET", "HEAD"}),
        respect_retry_after_header=True,
    )
    session = requests.Session()
    session.mount("https://", HTTPAdapter(max_retries=retry))
    return session


def fsl10k_members(archive: RemoteZip) -> dict[str, str]:
    members = {}
    for name in archive.namelist():
        path = PurePosixPath(name)
        if path.parts[:2] != ("audio", "wav") or not name.endswith(".wav"):
            continue
        sound_id = path.name.split("_", maxsplit=1)[0]
        if sound_id in members:
            raise ValueError(f"duplicate FSL10K audio for sound {sound_id}")
        members[sound_id] = name
    return members


def member_name(
    dataset: str,
    track: dict[str, Any],
    fsl_members: dict[str, str],
) -> str:
    if dataset == "fmakv2":
        track_number = track["source"]["item_id"]
        return f"fma_large/{int(track_number) // 1000:03d}/{int(track_number):06d}.mp3"
    sound_id = track["source"]["item_id"]
    try:
        return fsl_members[sound_id]
    except KeyError as error:
        raise ValueError(f"FSL10K archive has no audio for sound {sound_id}") from error


def download_member(
    archive: RemoteZip,
    member: str,
    target: Path,
    attempts: int = 4,
) -> None:
    target.parent.mkdir(parents=True, exist_ok=True)
    temporary = target.with_suffix(target.suffix + ".part")
    for attempt in range(attempts):
        try:
            with archive.open(member) as source, temporary.open("wb") as output:
                shutil.copyfileobj(source, output)
            temporary.replace(target)
            return
        except (OSError, requests.RequestException):
            temporary.unlink(missing_ok=True)
            if attempt + 1 == attempts:
                raise
            time.sleep(2**attempt)


def download_manifest(manifest_path: Path, dataset: str) -> None:
    manifest = json.loads(manifest_path.read_text())
    archive_url = ARCHIVES[dataset]
    with (
        http_session() as session,
        RemoteZip(
            archive_url,
            session=session,
            timeout=(15, 60),
        ) as archive,
    ):
        fsl_members = fsl10k_members(archive) if dataset == "fsl10k" else {}
        for index, track in enumerate(manifest["tracks"], 1):
            target = manifest_path.parent / track["path"]
            if not target.exists() or target.stat().st_size == 0:
                download_member(
                    archive,
                    member_name(dataset, track, fsl_members),
                    target,
                )
            if index % 25 == 0 or index == len(manifest["tracks"]):
                print(
                    f"{dataset}: {index}/{len(manifest['tracks'])} tracks ready",
                    flush=True,
                )


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--dataset", choices=sorted(ARCHIVES), required=True)
    return parser.parse_args()


def main() -> None:
    arguments = parse_arguments()
    download_manifest(arguments.manifest, arguments.dataset)


if __name__ == "__main__":
    main()
