import unittest

from evaluate_segments import boundaries, match_boundaries


class SegmentMetricTests(unittest.TestCase):
    def test_boundaries_only_include_key_transitions(self):
        segments = [
            {"start_seconds": 0.0, "key": "A minor"},
            {"start_seconds": 24.0, "key": "A minor"},
            {"start_seconds": 48.0, "key": "C major"},
        ]
        self.assertEqual(boundaries(segments), [48.0])

    def test_matching_is_one_to_one(self):
        true_positives, false_positives, false_negatives = match_boundaries(
            [30.0, 60.0],
            [29.0, 31.0, 100.0],
            tolerance=3.0,
        )
        self.assertEqual((true_positives, false_positives, false_negatives), (1, 2, 1))

    def test_boundary_outside_tolerance_is_missed(self):
        self.assertEqual(
            match_boundaries([30.0], [34.0], tolerance=3.0),
            (0, 1, 1),
        )


if __name__ == "__main__":
    unittest.main()
