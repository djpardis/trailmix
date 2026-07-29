import csv
import json
import tempfile
import unittest
from pathlib import Path

from build_external_validation import build_fmakv2, build_fsl10k
from download_external_validation import member_name


class ExternalValidationManifestTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary_directory = tempfile.TemporaryDirectory()
        self.directory = Path(self.temporary_directory.name)

    def tearDown(self) -> None:
        self.temporary_directory.cleanup()

    def test_fmakv2_sample_is_deterministic_and_uses_fma_paths(self) -> None:
        annotations = self.directory / "fmakv2.csv"
        with annotations.open("w", newline="") as stream:
            writer = csv.DictWriter(
                stream,
                fieldnames=["", "key_and_mode", "track_id", "spotify_uri"],
            )
            writer.writeheader()
            writer.writerows(
                [
                    {
                        "": "0",
                        "key_and_mode": "F# Major",
                        "track_id": "10",
                        "spotify_uri": "",
                    },
                    {
                        "": "1",
                        "key_and_mode": "A minor",
                        "track_id": "1234",
                        "spotify_uri": "",
                    },
                ]
            )

        first = build_fmakv2(annotations, Path("audio"), 1)
        second = build_fmakv2(annotations, Path("audio"), 1)

        self.assertEqual(first, second)
        self.assertEqual(len(first["tracks"]), 1)
        self.assertTrue(first["tracks"][0]["path"].endswith(".mp3"))

    def test_fsl10k_requires_two_close_usable_annotations(self) -> None:
        annotations = self.directory / "annotations"
        for annotator, bpm in [("1", "120"), ("2", "120.5")]:
            directory = annotations / annotator
            directory.mkdir(parents=True)
            (directory / "sound-42.json").write_text(
                json.dumps(
                    {
                        "bpm": bpm,
                        "discard": False,
                        "well_cut": True,
                    }
                )
            )
        rejected = annotations / "1" / "sound-99.json"
        rejected.write_text(
            json.dumps(
                {
                    "bpm": "90",
                    "discard": False,
                    "well_cut": True,
                }
            )
        )

        manifest = build_fsl10k(annotations, Path("audio"), 1)

        self.assertEqual(len(manifest["tracks"]), 1)
        self.assertEqual(manifest["tracks"][0]["id"], "fsl10k-000042")
        self.assertEqual(manifest["tracks"][0]["expected_bpm"], 120.25)

    def test_archive_members_are_resolved_from_source_ids(self) -> None:
        fma_track = {"source": {"item_id": "1234"}}
        fsl_track = {"source": {"item_id": "42"}}

        self.assertEqual(
            member_name("fmakv2", fma_track, {}),
            "fma_large/001/001234.mp3",
        )
        self.assertEqual(
            member_name("fsl10k", fsl_track, {"42": "audio/wav/42_user.wav.wav"}),
            "audio/wav/42_user.wav.wav",
        )


if __name__ == "__main__":
    unittest.main()
