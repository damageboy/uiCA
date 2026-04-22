import unittest

from verification.tools.common import load_profile


class TestProfiles(unittest.TestCase):
    def test_load_quick_profile(self):
        profile = load_profile("quick")

        self.assertEqual(profile["name"], "quick")
        self.assertIn("curated/add_loop_001", profile["cases"])

    def test_load_curated12_profile(self):
        profile = load_profile("curated12")

        self.assertEqual(profile["name"], "curated12")
        self.assertEqual(len(profile["cases"]), 12)
        self.assertIn("curated/vector256_001", profile["cases"])
