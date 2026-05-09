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

    def test_pages_workflow_builds_emscripten_xed_artifact(self):
        workflow = repo_root() / ".github" / "workflows" / "pages.yml"

        self.assertTrue(workflow.exists(), f"missing workflow: {workflow}")
        text = workflow.read_text()
        self.assertIn("wasm32-unknown-emscripten", text)
        self.assertTrue("setup-emsdk" in text or "emsdk install" in text)
        self.assertIn("git submodule update --init XED-to-XML mbuild", text)
        self.assertIn("dist/emscripten/uica_emscripten.js", text)
        self.assertIn("dist/emscripten/uica_emscripten.wasm", text)

    def test_web_build_script_keeps_pure_wasm_and_adds_emscripten(self):
        script = repo_root() / "scripts" / "build-web.sh"

        self.assertTrue(script.exists(), f"missing script: {script}")
        text = script.read_text()
        self.assertIn("wasm-pack build", text)
        self.assertIn("rust/uica-wasm", text)
        self.assertIn("dist/pkg", text)
        self.assertIn("build-xed-emscripten.sh", text)
        self.assertTrue("dist/emscripten" in text or "$DIST_DIR/emscripten" in text)
