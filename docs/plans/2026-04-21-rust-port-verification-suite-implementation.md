# uiCA Verification Suite Implementation Plan

> **REQUIRED SUB-SKILL:** Use the executing-plans skill to implement this plan task-by-task.

**Goal:** Add deterministic verification harness + golden corpus in Python repo so Rust port can prove full JSON parity on HSW/SKL/ICL.

**Architecture:** Extend `uiCA.py -json` into stable v1 contract with explicit `summary` metadata, then build standalone `verification/tools` pipeline (`capture`, `verify`, `canonicalize`) driven by case metadata. Back this with small TDD-first unit/integration tests and seed curated corpus, then wire quick CI profile.

**Tech Stack:** Python 3 (`unittest`, `json`, `tomllib`, `subprocess`, `pathlib`), existing `uiCA.py` CLI, GNU assembler (`as`) for fixture binaries, GitHub Actions.

---

## Phase 1 — Contract + core tooling

### Task 1: Test harness bootstrap

**TDD scenario:** New feature — full TDD cycle

**Files:**
- Create: `tests/__init__.py`
- Create: `tests/verification/__init__.py`
- Create: `tests/verification/fixtures/loop_add.s`
- Create: `tests/verification/helpers.py`
- Create: `tests/verification/test_smoke_harness.py`

**Step 1: Write failing test**

```python
# tests/verification/test_smoke_harness.py
import unittest
from tests.verification.helpers import repo_root

class TestSmokeHarness(unittest.TestCase):
   def test_repo_root_contains_uica(self):
      self.assertTrue((repo_root() / 'uiCA.py').exists())
```

**Step 2: Run test to verify fail**

Run: `python3 -m unittest tests.verification.test_smoke_harness -v`
Expected: `ModuleNotFoundError: No module named 'tests.verification.helpers'`

**Step 3: Write minimal implementation**

```python
# tests/verification/helpers.py
from pathlib import Path

def repo_root() -> Path:
   return Path(__file__).resolve().parents[2]
```

`tests/verification/fixtures/loop_add.s`
```asm
.intel_syntax noprefix
l:
  add rax, rbx
  add rbx, rax
  dec r15
  jnz l
```

**Step 4: Run test to verify pass**

Run: `python3 -m unittest tests.verification.test_smoke_harness -v`
Expected: `OK`

**Step 5: Commit**

```bash
git add tests/__init__.py tests/verification/__init__.py tests/verification/helpers.py tests/verification/fixtures/loop_add.s tests/verification/test_smoke_harness.py
git commit -m "test: bootstrap verification test harness"
```

---

### Task 2: JSON contract v1 failing integration test

**TDD scenario:** New feature — full TDD cycle

**Files:**
- Create: `tests/verification/test_uica_json_contract.py`
- Modify: `tests/verification/helpers.py`

**Step 1: Write failing test**

```python
# tests/verification/test_uica_json_contract.py
import json
import tempfile
import unittest
from tests.verification.helpers import assemble_fixture, run_uica_json

class TestUiCAJsonContract(unittest.TestCase):
   def test_json_contains_v1_metadata_and_summary(self):
      with tempfile.TemporaryDirectory() as td:
         obj = assemble_fixture('loop_add.s', td)
         out_json = f"{td}/out.json"
         run_uica_json(obj, out_json, arch='SKL')

         with open(out_json, 'r') as f:
            data = json.load(f)

      self.assertEqual(data['schema_version'], 'uica-result-v1')
      self.assertIn('summary', data)
      self.assertIn('throughput_cycles_per_iteration', data['summary'])
      self.assertIn('bottlenecks_predicted', data['summary'])
```

**Step 2: Run test to verify fail**

Run: `python3 -m unittest tests.verification.test_uica_json_contract -v`
Expected: fail with missing helper functions (`assemble_fixture` / `run_uica_json`)

**Step 3: Add helper functions (minimal)**

```python
# tests/verification/helpers.py
import subprocess
from pathlib import Path

# keep repo_root from Task 1

def assemble_fixture(fixture_name: str, out_dir: str) -> str:
   src = repo_root() / 'tests' / 'verification' / 'fixtures' / fixture_name
   obj = Path(out_dir) / 'fixture.o'
   subprocess.run(['as', str(src), '-o', str(obj)], check=True)
   return str(obj)

def run_uica_json(obj_path: str, out_json: str, arch: str = 'SKL') -> None:
   subprocess.run([
      'python3', str(repo_root() / 'uiCA.py'), obj_path,
      '-arch', arch,
      '-json', out_json,
      '-TPonly'
   ], check=True)
```

**Step 4: Re-run test to verify semantic fail**

Run: `python3 -m unittest tests.verification.test_uica_json_contract -v`
Expected: fail on `KeyError: 'schema_version'`

**Step 5: Commit helper test scaffold**

```bash
git add tests/verification/helpers.py tests/verification/test_uica_json_contract.py
git commit -m "test: add failing uiCA json contract v1 test"
```

---

### Task 3: Implement JSON v1 metadata + summary in `uiCA.py`

**TDD scenario:** Modifying untested code — run existing tests first

**Files:**
- Modify: `uiCA.py` (around `generateJSONOutput` and `runSimulation`)
- Modify: `README.md` (CLI options table add `-json` description)

**Step 1: Run existing tests first**

Run: `python3 -m unittest tests.verification.test_uica_json_contract -v`
Expected: failing assertion on missing v1 fields.

**Step 2: Minimal implementation in `uiCA.py`**

Add summary payload and metadata:

```python
# in runSimulation(...)
return TP, {
   'throughput_cycles_per_iteration': TP,
   'iterations_simulated': len(uopsForRound),
   'cycles_simulated': clock,
   'mode': 'unroll' if frontEnd.unroll else 'loop',
   'bottlenecks_predicted': sorted(bottlenecks),
   'limits': {
      'predecoder': predecLimit,
      'decoder': decLimit,
      'dsb': dsbLimit,
      'lsd': lsdLimit,
      'issue': issueLimit,
      'ports': portUsageLimit,
      'dependencies': depLimit,
   },
}
```

```python
# in generateJSONOutput(...)
result = {
   'schema_version': 'uica-result-v1',
   'engine': 'python',
   'engine_version': 'uiCA-python',
   'uica_commit': os.environ.get('UICA_COMMIT', 'unknown'),
   'invocation': invocation,
   'summary': summary,
   'parameters': parameters,
   'instructions': instrList,
   'cycles': cycles,
}
```

**Step 3: Update caller paths**

Ensure `main()` handles tuple return from `runSimulation` while preserving `-TPonly` output.

**Step 4: Re-run contract test**

Run: `python3 -m unittest tests.verification.test_uica_json_contract -v`
Expected: `OK`

**Step 5: Smoke test CLI behavior**

Run:
```bash
echo ".intel_syntax noprefix; l: add rax, rbx; add rbx, rax; dec r15; jnz l" > /tmp/t.asm
as /tmp/t.asm -o /tmp/t.o
python3 uiCA.py /tmp/t.o -arch SKL -TPonly
python3 uiCA.py /tmp/t.o -arch SKL -json /tmp/t.json -TPonly
python3 - <<'PY'
import json
print(json.load(open('/tmp/t.json'))['schema_version'])
PY
```
Expected: throughput printed + `uica-result-v1` printed.

**Step 6: Commit**

```bash
git add uiCA.py README.md tests/verification/test_uica_json_contract.py
git commit -m "feat: add v1 metadata and summary to json output"
```

---

### Task 4: Canonicalizer module + unit tests

**TDD scenario:** New feature — full TDD cycle

**Files:**
- Create: `verification/tools/canonicalize.py`
- Create: `tests/verification/test_canonicalize.py`

**Step 1: Write failing tests**

```python
# tests/verification/test_canonicalize.py
import unittest
from verification.tools.canonicalize import canonicalize_result

class TestCanonicalize(unittest.TestCase):
   def test_sorts_event_arrays(self):
      raw = {
         'cycles': [
            {'cycle': 0, 'executed': [
               {'rnd': 1, 'instrID': 2, 'uopID': 1},
               {'rnd': 0, 'instrID': 2, 'uopID': 0},
            ]}
         ]
      }
      out = canonicalize_result(raw)
      self.assertEqual(out['cycles'][0]['executed'][0]['rnd'], 0)

   def test_sorts_keys_recursively(self):
      out = canonicalize_result({'b': 1, 'a': {'d': 1, 'c': 2}})
      self.assertEqual(list(out.keys()), ['a', 'b'])
```

**Step 2: Run to verify fail**

Run: `python3 -m unittest tests.verification.test_canonicalize -v`
Expected: import failure for `verification.tools.canonicalize`

**Step 3: Write minimal implementation**

```python
# verification/tools/canonicalize.py
from copy import deepcopy

def _sort_key(d):
   return (
      d.get('rnd', -1), d.get('instrID', -1), d.get('lamUopID', -1),
      d.get('fUopID', -1), d.get('uopID', -1), d.get('source', ''),
      int(bool(d.get('regMergeUop', False))), int(bool(d.get('stackSyncUop', False)))
   )

def _canon(value):
   if isinstance(value, dict):
      out = {k: _canon(v) for k, v in sorted(value.items(), key=lambda kv: kv[0])}
      for k, v in list(out.items()):
         if isinstance(v, list) and v and isinstance(v[0], dict):
            out[k] = sorted(v, key=_sort_key)
      return out
   if isinstance(value, list):
      return [_canon(v) for v in value]
   return value

def canonicalize_result(result: dict) -> dict:
   return _canon(deepcopy(result))
```

**Step 4: Run tests to verify pass**

Run: `python3 -m unittest tests.verification.test_canonicalize -v`
Expected: `OK`

**Step 5: Commit**

```bash
git add verification/tools/canonicalize.py tests/verification/test_canonicalize.py
git commit -m "feat: add semantic canonicalization for verification json"
```

---

### Task 5: Case manifest loader + deterministic runner helpers

**TDD scenario:** New feature — full TDD cycle

**Files:**
- Create: `verification/tools/common.py`
- Create: `tests/verification/test_case_manifest.py`

**Step 1: Write failing test**

```python
# tests/verification/test_case_manifest.py
import tempfile
import textwrap
import unittest
from verification.tools.common import load_case_manifest

class TestCaseManifest(unittest.TestCase):
   def test_load_case_manifest(self):
      with tempfile.TemporaryDirectory() as td:
         p = f"{td}/case.toml"
         with open(p, 'w') as f:
            f.write(textwrap.dedent('''
               id = "curated/add-loop"
               [run]
               arches = ["HSW", "SKL", "ICL"]
               alignmentOffset = 0
            '''))
         manifest = load_case_manifest(p)
      self.assertEqual(manifest['id'], 'curated/add-loop')
      self.assertEqual(manifest['run']['arches'][1], 'SKL')
```

**Step 2: Run to verify fail**

Run: `python3 -m unittest tests.verification.test_case_manifest -v`
Expected: import failure for `verification.tools.common`

**Step 3: Write minimal implementation**

```python
# verification/tools/common.py
from pathlib import Path
import tomllib

def load_case_manifest(path: str) -> dict:
   with open(path, 'rb') as f:
      data = tomllib.load(f)
   if 'id' not in data or 'run' not in data:
      raise ValueError('case manifest missing id/run')
   return data

def iter_case_dirs(root: Path):
   for p in sorted(root.glob('*/*')):
      if p.is_dir() and (p / 'case.toml').exists():
         yield p
```

**Step 4: Run tests to verify pass**

Run: `python3 -m unittest tests.verification.test_case_manifest -v`
Expected: `OK`

**Step 5: Commit**

```bash
git add verification/tools/common.py tests/verification/test_case_manifest.py
git commit -m "feat: add case manifest loader for verification suite"
```

---

### Phase 1 checkpoint

Run:
```bash
python3 -m unittest tests.verification.test_smoke_harness tests.verification.test_uica_json_contract tests.verification.test_canonicalize tests.verification.test_case_manifest -v
```
Expected: all pass.

Commit checkpoint:
```bash
git add -A
git commit -m "chore: phase1 checkpoint for verification suite core"
```

---

## Phase 2 — Capture/verify pipeline + seed corpus + CI

### Task 6: Capture tool (`verification/tools/capture.py`)

**TDD scenario:** New feature — full TDD cycle

**Files:**
- Create: `verification/tools/capture.py`
- Create: `tests/verification/test_capture_tool.py`

**Step 1: Write failing test**

```python
# tests/verification/test_capture_tool.py
import json
import tempfile
import unittest
from verification.tools.capture import write_golden

class TestCaptureTool(unittest.TestCase):
   def test_write_golden(self):
      with tempfile.TemporaryDirectory() as td:
         out = f"{td}/golden.json"
         write_golden({'schema_version': 'uica-result-v1'}, out)
         self.assertEqual(json.load(open(out))['schema_version'], 'uica-result-v1')
```

**Step 2: Run to verify fail**

Run: `python3 -m unittest tests.verification.test_capture_tool -v`
Expected: import failure.

**Step 3: Write minimal implementation**

```python
# verification/tools/capture.py
import json
from pathlib import Path
from verification.tools.canonicalize import canonicalize_result

def write_golden(result: dict, out_path: str) -> None:
   Path(out_path).parent.mkdir(parents=True, exist_ok=True)
   with open(out_path, 'w') as f:
      json.dump(canonicalize_result(result), f, sort_keys=True, separators=(',', ':'))
```

**Step 4: Run test to verify pass**

Run: `python3 -m unittest tests.verification.test_capture_tool -v`
Expected: `OK`

**Step 5: Commit**

```bash
git add verification/tools/capture.py tests/verification/test_capture_tool.py
git commit -m "feat: add golden capture writer"
```

---

### Task 7: Verify tool (`verification/tools/verify.py`) with mismatch reporting

**TDD scenario:** New feature — full TDD cycle

**Files:**
- Create: `verification/tools/verify.py`
- Create: `tests/verification/test_verify_tool.py`

**Step 1: Write failing test**

```python
# tests/verification/test_verify_tool.py
import unittest
from verification.tools.verify import first_mismatch_path

class TestVerifyTool(unittest.TestCase):
   def test_first_mismatch_path(self):
      left = {'summary': {'throughput_cycles_per_iteration': 1.0}}
      right = {'summary': {'throughput_cycles_per_iteration': 1.25}}
      self.assertEqual(
         first_mismatch_path(left, right),
         '$.summary.throughput_cycles_per_iteration'
      )
```

**Step 2: Run to verify fail**

Run: `python3 -m unittest tests.verification.test_verify_tool -v`
Expected: import failure.

**Step 3: Write minimal implementation**

```python
# verification/tools/verify.py

def first_mismatch_path(left, right, path='$'):
   if type(left) != type(right):
      return path
   if isinstance(left, dict):
      keys = sorted(set(left.keys()) | set(right.keys()))
      for k in keys:
         if k not in left or k not in right:
            return f"{path}.{k}"
         p = first_mismatch_path(left[k], right[k], f"{path}.{k}")
         if p:
            return p
      return None
   if isinstance(left, list):
      if len(left) != len(right):
         return f"{path}.length"
      for i, (a, b) in enumerate(zip(left, right)):
         p = first_mismatch_path(a, b, f"{path}[{i}]")
         if p:
            return p
      return None
   return None if left == right else path
```

**Step 4: Run test to verify pass**

Run: `python3 -m unittest tests.verification.test_verify_tool -v`
Expected: `OK`

**Step 5: Commit**

```bash
git add verification/tools/verify.py tests/verification/test_verify_tool.py
git commit -m "feat: add canonical verify mismatch locator"
```

---

### Task 8: Seed curated corpus + profile manifest

**TDD scenario:** New feature — full TDD cycle

**Files:**
- Create: `verification/cases/curated/add_loop_001/snippet.s`
- Create: `verification/cases/curated/add_loop_001/case.toml`
- Create: `verification/cases/curated/fusion_jcc_001/snippet.s`
- Create: `verification/cases/curated/fusion_jcc_001/case.toml`
- Create: `verification/profiles/quick.toml`
- Create: `tests/verification/test_profiles.py`

**Step 1: Write failing profile loader test**

```python
# tests/verification/test_profiles.py
import unittest
from verification.tools.common import load_profile

class TestProfiles(unittest.TestCase):
   def test_load_quick_profile(self):
      p = load_profile('quick')
      self.assertEqual(p['name'], 'quick')
      self.assertIn('curated/add_loop_001', p['cases'])
```

**Step 2: Run to verify fail**

Run: `python3 -m unittest tests.verification.test_profiles -v`
Expected: `ImportError` for missing `load_profile`.

**Step 3: Add profile loader + seed files**

`verification/profiles/quick.toml`
```toml
name = "quick"
cases = [
  "curated/add_loop_001",
  "curated/fusion_jcc_001",
]
arches = ["HSW", "SKL", "ICL"]
```

`verification/tools/common.py` add:
```python
def load_profile(name: str) -> dict:
   base = Path(__file__).resolve().parents[1] / 'profiles'
   with open(base / f'{name}.toml', 'rb') as f:
      return tomllib.load(f)
```

**Step 4: Run test to verify pass**

Run: `python3 -m unittest tests.verification.test_profiles -v`
Expected: `OK`

**Step 5: Commit**

```bash
git add verification/cases/curated verification/profiles/quick.toml verification/tools/common.py tests/verification/test_profiles.py
git commit -m "feat: add seed curated corpus and quick profile"
```

---

### Task 9: Verification README + runnable commands

**TDD scenario:** Trivial change — use judgment

**Files:**
- Create: `verification/README.md`
- Modify: `README.md` (link verification suite)

**Step 1: Write docs with exact commands**

Include:
- how to assemble case fixtures
- capture command
- verify command
- focused verify by case
- golden directory conventions

**Step 2: Dry-run every command manually**

Run:
```bash
python3 -m unittest tests.verification.test_capture_tool tests.verification.test_verify_tool -v
python3 verification/tools/capture.py --help
python3 verification/tools/verify.py --help
```
Expected: tests pass, help text exits 0.

**Step 3: Commit**

```bash
git add verification/README.md README.md
git commit -m "docs: add verification suite usage guide"
```

---

### Task 10: CI quick gate

**TDD scenario:** New feature — full TDD cycle

**Files:**
- Create: `.github/workflows/verification-quick.yml`
- Create: `tests/verification/test_ci_contract.py`

**Step 1: Write failing CI contract test**

```python
# tests/verification/test_ci_contract.py
import unittest
from pathlib import Path

class TestCIContract(unittest.TestCase):
   def test_quick_workflow_exists(self):
      self.assertTrue(Path('.github/workflows/verification-quick.yml').exists())
```

**Step 2: Run to verify fail**

Run: `python3 -m unittest tests.verification.test_ci_contract -v`
Expected: `FAIL` (workflow missing)

**Step 3: Add workflow**

Workflow minimal jobs:
- checkout
- setup python
- run targeted verification tests
- run `python3 verification/tools/verify.py --profile quick --engine python`

**Step 4: Run test to verify pass**

Run: `python3 -m unittest tests.verification.test_ci_contract -v`
Expected: `OK`

**Step 5: Commit**

```bash
git add .github/workflows/verification-quick.yml tests/verification/test_ci_contract.py
git commit -m "ci: add quick verification gate"
```

---

## Final verification before handoff

Run full local verification set:

```bash
python3 -m unittest discover -s tests -p 'test_*.py' -v
python3 verification/tools/verify.py --profile quick --engine python
```

Expected:
- All unittest targets pass.
- Quick profile exits 0.

Final commit:

```bash
git add -A
git commit -m "chore: complete verification suite phase 1+2"
```

## Notes for executor

- Keep tasks minimal. No refactors outside scope.
- Do not implement generated corpus (`~300`) in first PR unless phase 1+2 stable.
- Keep schema additive; do not break existing consumers of old JSON fields.
- If deterministic mismatch appears, debug canonicalizer before changing simulator behavior.
- After task 10, request code review before scaling corpus size.
