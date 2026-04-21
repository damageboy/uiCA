import unittest

from tests.verification.helpers import repo_root


class TestSmokeHarness(unittest.TestCase):
    def test_repo_root_contains_uica(self):
        """Verify repo_root() points to correct directory containing uiCA.py"""
        self.assertTrue((repo_root() / "uiCA.py").exists())


if __name__ == "__main__":
    unittest.main()
