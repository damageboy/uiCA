import unittest

from tests.verification.helpers import repo_root


class TestCiContract(unittest.TestCase):
    def test_quick_verification_workflow_exists(self):
        workflow = repo_root() / ".github" / "workflows" / "verification-quick.yml"

        self.assertTrue(workflow.exists(), f"missing workflow: {workflow}")
        self.assertIn(
            "verification/tools/verify.py --profile quick --engine python",
            workflow.read_text(),
        )
