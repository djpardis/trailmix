import json
import tempfile
import unittest
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

from corpus_policy import (
    PolicyError,
    consume_evaluation,
    invalidate_evaluation,
    load_ledger,
    manifest_fingerprint,
    register_development,
    seal_evaluation,
)


class CorpusPolicyTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary_directory = tempfile.TemporaryDirectory()
        self.directory = Path(self.temporary_directory.name)
        self.policy = self.directory / "policy.json"
        self.ledger = self.directory / "ledger.json"
        self.configuration = self.directory / "configuration.json"
        self.policy.write_text(
            json.dumps(
                {
                    "version": 1,
                    "development_corpora": [
                        {
                            "name": "Known development set",
                            "track_id_prefix": "development-",
                            "source_names": ["Known source"],
                            "reason": "Used while tuning.",
                        }
                    ],
                }
            )
        )
        self.configuration.write_text('{"model": "frozen"}\n')

    def tearDown(self) -> None:
        self.temporary_directory.cleanup()

    def manifest(self, track_id: str, source_name: str | None = None) -> Path:
        path = self.directory / f"{track_id}.json"
        track = {
            "id": track_id,
            "path": "private.mp3",
            "expected_bpm": 120.0,
        }
        if source_name is not None:
            track["source"] = {"name": source_name}
        path.write_text(
            json.dumps(
                {
                    "version": 1,
                    "tracks": [track],
                }
            )
        )
        return path

    def test_known_development_corpus_cannot_be_sealed(self) -> None:
        with self.assertRaisesRegex(PolicyError, "cannot be sealed"):
            seal_evaluation(
                self.manifest("development-track"),
                "Invalid final set",
                self.configuration,
                self.policy,
                self.ledger,
            )

    def test_renamed_development_corpus_cannot_be_sealed(self) -> None:
        with self.assertRaisesRegex(PolicyError, "cannot be sealed"):
            seal_evaluation(
                self.manifest("renamed-track", "Known source"),
                "Renamed development set",
                self.configuration,
                self.policy,
                self.ledger,
            )

    def test_sealed_corpus_can_be_consumed_once(self) -> None:
        manifest = self.manifest("unseen-track")
        receipt = self.directory / "seal.json"
        fingerprint = seal_evaluation(
            manifest,
            "Unseen set",
            self.configuration,
            self.policy,
            self.ledger,
            receipt,
        )

        self.assertEqual(
            load_ledger(self.ledger)["corpora"][fingerprint]["state"], "sealed"
        )
        self.assertEqual(json.loads(receipt.read_text())["corpus_sha256"], fingerprint)
        self.assertEqual(
            consume_evaluation(
                manifest,
                self.configuration,
                self.policy,
                self.ledger,
            ),
            fingerprint,
        )

    def test_sealed_corpus_rejects_changed_configuration(self) -> None:
        manifest = self.manifest("unseen-track")
        seal_evaluation(
            manifest,
            "Unseen set",
            self.configuration,
            self.policy,
            self.ledger,
        )
        self.configuration.write_text('{"model": "changed"}\n')

        with self.assertRaisesRegex(PolicyError, "frozen configuration"):
            consume_evaluation(
                manifest,
                self.configuration,
                self.policy,
                self.ledger,
            )

    def test_evaluated_corpus_rejects_changed_configuration(self) -> None:
        manifest = self.manifest("unseen-track")
        seal_evaluation(
            manifest,
            "Unseen set",
            self.configuration,
            self.policy,
            self.ledger,
        )
        consume_evaluation(
            manifest,
            self.configuration,
            self.policy,
            self.ledger,
        )
        self.configuration.write_text('{"model": "changed"}\n')

        with self.assertRaisesRegex(PolicyError, "different configuration"):
            consume_evaluation(
                manifest,
                self.configuration,
                self.policy,
                self.ledger,
            )

    def test_invalidated_corpus_cannot_be_consumed_again(self) -> None:
        manifest = self.manifest("unseen-track")
        seal_evaluation(
            manifest,
            "Unseen set",
            self.configuration,
            self.policy,
            self.ledger,
        )
        consume_evaluation(
            manifest,
            self.configuration,
            self.policy,
            self.ledger,
        )
        receipt = self.directory / "invalidation.json"
        fingerprint = invalidate_evaluation(
            manifest,
            "Invalid manifest metadata.",
            self.ledger,
            receipt,
        )

        self.assertEqual(
            json.loads(receipt.read_text())["corpus_sha256"],
            fingerprint,
        )
        with self.assertRaisesRegex(PolicyError, "state must be sealed"):
            consume_evaluation(
                manifest,
                self.configuration,
                self.policy,
                self.ledger,
            )

    def test_sealed_corpus_cannot_be_registered_as_development(self) -> None:
        manifest = self.manifest("unseen-track")
        seal_evaluation(
            manifest,
            "Unseen set",
            self.configuration,
            self.policy,
            self.ledger,
        )

        with self.assertRaisesRegex(PolicyError, "cannot become development"):
            register_development(
                manifest,
                "Unseen set",
                self.policy,
                self.ledger,
            )

    def test_concurrent_registrations_are_preserved(self) -> None:
        manifests = [
            self.manifest("development-first"),
            self.manifest("development-second"),
        ]
        with ThreadPoolExecutor(max_workers=2) as executor:
            futures = [
                executor.submit(
                    register_development,
                    manifest,
                    manifest.stem,
                    self.policy,
                    self.ledger,
                )
                for manifest in manifests
            ]
            for future in futures:
                future.result()

        ledger = load_ledger(self.ledger)
        self.assertEqual(
            set(ledger["corpora"]),
            {
                manifest_fingerprint(json.loads(manifest.read_text()))
                for manifest in manifests
            },
        )


if __name__ == "__main__":
    unittest.main()
