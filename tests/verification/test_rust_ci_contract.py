import unittest

from tests.verification.helpers import repo_root


class TestRustCiContract(unittest.TestCase):
    def test_rust_parity_workflow_exists_and_runs_gate_commands(self):
        workflow = repo_root() / ".github" / "workflows" / "rust-parity.yml"

        self.assertTrue(workflow.exists(), f"missing workflow: {workflow}")
        text = workflow.read_text()
        self.assertIn("cargo test --workspace", text)
        self.assertIn(
            "python3 verification/tools/capture.py --profile quick --engine rust",
            text,
        )
        self.assertIn(
            "python3 verification/tools/verify.py --profile quick --engine rust",
            text,
        )
