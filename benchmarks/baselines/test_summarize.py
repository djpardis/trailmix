import unittest

from summarize import (
    ACCURACY_2_FACTORS,
    summarize_key,
    summarize_tempo,
    tempo_matches,
)


def tempo_result(detected_bpm: float) -> dict:
    return {
        "expected_bpm": 120.0,
        "detected_bpm": detected_bpm,
        "elapsed_milliseconds": 1.0,
        "error": None,
    }


class TempoMetricTests(unittest.TestCase):
    def test_accuracy_2_uses_standard_metrical_factors(self) -> None:
        self.assertEqual(
            ACCURACY_2_FACTORS,
            (1.0, 0.5, 2.0, 1.0 / 3.0, 3.0),
        )

    def test_accuracy_2_accepts_each_standard_factor(self) -> None:
        for factor in ACCURACY_2_FACTORS:
            with self.subTest(factor=factor):
                metrics = summarize_tempo([tempo_result(120.0 * factor)])
                self.assertEqual(metrics["accuracy_2"], 1.0)

    def test_accuracy_1_only_accepts_the_reference_tempo(self) -> None:
        metrics = summarize_tempo([tempo_result(240.0)])

        self.assertEqual(metrics["accuracy_1"], 0.0)
        self.assertEqual(metrics["accuracy_2"], 1.0)

    def test_accuracy_2_rejects_nonstandard_three_halves_alias(self) -> None:
        result = tempo_result(180.0)
        metrics = summarize_tempo([result])

        self.assertFalse(
            any(
                tempo_matches(
                    result["expected_bpm"],
                    result["detected_bpm"],
                    factor,
                )
                for factor in ACCURACY_2_FACTORS
            )
        )
        self.assertEqual(metrics["accuracy_2"], 0.0)

    def test_all_failures_produce_empty_tempo_metrics(self) -> None:
        metrics = summarize_tempo([{"error": "failed", "detected_bpm": None}])

        self.assertEqual(metrics["failed_tracks"], 1)
        self.assertIsNone(metrics["accuracy_1"])

    def test_all_failures_produce_empty_key_metrics(self) -> None:
        metrics = summarize_key([{"error": "failed", "detected_key": None}])

        self.assertEqual(metrics["failed_tracks"], 1)
        self.assertIsNone(metrics["exact_accuracy"])


if __name__ == "__main__":
    unittest.main()
