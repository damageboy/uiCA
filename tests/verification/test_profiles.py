import unittest

from verification.tools.common import load_profile


class TestProfiles(unittest.TestCase):
    def test_load_quick_profile(self):
        profile = load_profile("quick")

        self.assertEqual(profile["name"], "quick")
        self.assertIn("curated/add_loop_001", profile["cases"])
