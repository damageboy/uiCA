---
name: uica-verification-bug-hunt
description: Use when uiCA Rust output mismatches unmodified Python golden JSON or event traces, especially after parity fixes where previous discrepancies must stay fixed.
---

# uiCA Verification Bug Hunt

## Overview

Hunt one Rust-vs-Python discrepancy at a time. Python `uiCA.py` is oracle. Fix Rust by tracking Python behavior in mainline simulator/data/model/decoder code, not by hiding mismatches in verification or JSON canonicalization.

Core rule: every fix must answer: "Which Python state, predicate, or ordering does Rust now mirror?"

## When to Use

Use for:

- `verification/tools/verify.py` mismatch against Python goldens
- JSON path mismatch like `$.cycles[...]` or `$.summary.limits...`
- structured event trace divergence between `-eventTrace` and `--event-trace`
- regression check after parity fixes
- requests like "find next regression, debug, fix, compare to Python"

Do not use for:

- generated datapack regeneration work alone
- changing verification tools to normalize away differences
- adding runtime `instructions.json` fallback

## Non-Negotiables

- Python golden comes from unmodified `uiCA.py`.
- One discrepancy per session unless shared root cause is proven.
- Compare behavior to Python code before patching.
- Rust code should preserve Python concept names where practical.
- Fixes must preserve Python-like code shape: same state, same owner, same timing, same data flow where possible.
- Prefer adding missing Python-equivalent state over reconstructing that state later from unrelated Rust storage.
- Never edit canonicalization or JSON output to conceal mismatch.
- Do not commit `rust/uica-data/generated/` datapacks.
- After fix, rerun current case, known previous failing cases, and quick profile.
- Before final response, check `git status --short`; if user asked for a fix, commit code changes unless explicitly told not to.

## Workflow

### 1. Capture fresh Python oracle

```bash
cargo build -q -p uica-cli
TMP=$(mktemp -d)
TAG=py-debug-$(date +%Y%m%d-%H%M%S)
python3 verification/tools/capture.py \
  --profile curated12 \
  --engine python \
  --golden-root "$TMP" \
  --golden-tag "$TAG" \
  --jobs 4
mkdir -p "$TMP/rust"
cp -R "$TMP/python/$TAG" "$TMP/rust/$TAG"
```

### 2. Find first mismatch

```bash
python3 verification/tools/verify.py \
  --profile curated12 \
  --engine rust \
  --rust-bin target/debug/uica-cli \
  --golden-root "$TMP" \
  --golden-tag "$TAG" \
  --jobs 4 \
  --dump-diff "$TMP/curated12.diff" || true
sed -n '1,80p' "$TMP/curated12.diff"
```

Pick first mismatch path. Stay focused.

### 3. Compare JSON first

Inspect exact cycle or summary node in Python/candidate JSON. Use small Python snippets, not visual guessing.

```bash
python3 - <<'PY'
import json
from pathlib import Path
root=Path('/tmp/...')
tag='py-debug-...'
case='curated/...'; arch='HSW'
py=json.load(open(root/'rust'/tag/case/f'{arch}.json'))
rs=json.load(open(root/'_candidates/rust'/case/f'{arch}.json'))
for c in range(4):
    print('cycle', c)
    print('PY', py['cycles'][c])
    print('RS', rs['cycles'][c])
PY
```

### 4. Escalate to event traces only when JSON lacks cause

```bash
mkdir -p /tmp/uica-debug-case
clang --target=x86_64-linux-gnu -c verification/cases/curated/CASE/snippet.s \
  -o /tmp/uica-debug-case/case.o

python3 uiCA.py /tmp/uica-debug-case/case.o \
  -arch HSW \
  -json /tmp/uica-debug-case/python.json \
  -eventTrace /tmp/uica-debug-case/python.trace \
  -TPonly

target/debug/uica-cli /tmp/uica-debug-case/case.o \
  --arch HSW \
  --json /tmp/uica-debug-case/rust.json \
  --event-trace /tmp/uica-debug-case/rust.trace \
  --tp-only

python3 verification/tools/compare_event_traces.py \
  /tmp/uica-debug-case/python.trace \
  /tmp/uica-debug-case/rust.trace \
  --context 12
```

### 5. Read Python implementation and mirror concept

Before editing Rust, locate matching Python state or predicate. Patch Rust so code structure maps to Python behavior.

Port-shape gate before code:

1. Name the exact Python state/predicate/order being mirrored.
2. Name the Python owner that maintains it (class/function/module).
3. Name the Rust owner that should maintain it. If none exists, add the missing Python-equivalent owner/state.
4. Confirm timing: state is updated in the same phase as Python, not reconstructed after the fact.
5. Only then edit Rust.

Good fix shape:

- Rust field/comment names Python state.
- Commit message says what Python concept is mirrored.
- Code uses simulator/model facts, not test-specific case names.
- Rust state lives where the equivalent Python state lives (`Renamer`, `ReorderBuffer`, `FrontEnd`, `runSimulation`, etc.).
- The resulting code base retains Python-like shape, naming, timing, and semantics so reviewers can compare port to source.

Bad fix shape:

- Special-case case/arch names.
- Sort/delete JSON fields after computation.
- Add "if mismatch path" hacks.
- Reconstruct Python state later by scanning unrelated global storage when Python maintained that state incrementally.
- Hide a model mismatch behind a broad guard such as schema/profile/case filtering.

## Patterns From Recent Fixes

### Pattern A: Summary frontend limits must follow simulated sources

Symptom:

- JSON mismatch in `$.summary.limits.decoder`, `predecoder`, `dsb`, or `lsd`.
- Event traces match, summary differs.

Python behavior:

- Loop frontend limits are reported only for actual simulated uop source(s).
- If loop runs from LSD, Python leaves MITE decoder/predecoder and DSB limits as `None`.

Rust fix shape:

- After simulation, inspect generated instruction sources.
- Clear summary limits for sources not used.
- Recompute `bottlenecks_predicted` after clearing.

Key check:

```text
If frontend source = LSD, decoder/predecoder/dsb limits must be None unless generated instrs prove otherwise.
```

### Pattern B: Pseudo operands persist until last uop of instruction

Symptom:

- `$.cycles[*].addedToRS[*].dependsOn.length` mismatch.
- Trace shows Rust loses dependency when one instruction's laminated uops issue across cycles.

Python behavior in `uiCA.py Renamer`:

```python
self.curInstrPseudoOpDict = {}
...
if isinstance(inpOp, PseudoOperand):
    renOp = self.curInstrPseudoOpDict[inpOp]
...
if isinstance(outOp, PseudoOperand):
    self.curInstrPseudoOpDict[outOp] = renOp
...
if uop.prop.isLastUopOfInstr:
    self.curInstrPseudoOpDict.clear()
```

Rust fix shape:

```rust
/// Python parity: `Renamer.curInstrPseudoOpDict` persists pseudo operands
/// across rename cycles until `isLastUopOfInstr` clears it.
pub cur_instr_pseudo_op_dict: HashMap<OperandKey, Shared<RenamedOperand>>;
```

Do not key pseudo state by test case. Prefer direct Python-equivalent renamer state.

### Pattern C: Macro-fusion changes analytical port usage

Symptom:

- `$.summary.limits.ports` mismatch while trace may match.

Python behavior:

- Macro-fused branch side is skipped in port usage.
- Previous fused instruction's port data may be rewritten to branch ports (`6` on HSW/SKL, `06` on ICL-like configs).

Rust fix shape:

- Apply macro-fusion port override before analytical port usage.
- Still skip `macro_fused_with_prev` instruction.

### Pattern D: Simulated TP and runtime bottlenecks must use `uopsForRound`

Symptom:

- `$.summary.throughput_cycles_per_iteration` or `$.summary.bottlenecks_predicted` mismatch.
- Limits match, but Python does not predict `Dependencies`/`Ports`, or Python adds `Scheduling`/`Divider`.

Python behavior in `runSimulation`:

```python
uopsForRound = []
...
while retireQueue:
    fusedUop = retireQueue.popleft()
    for uop in fusedUop.getUnfusedUops():
        instr = uop.prop.instr
        rnd = uop.instrI.rnd
        if rnd >= len(uopsForRound):
            uopsForRound.append({instr: [] for instr in instructions})
        uopsForRound[rnd][instr].append(fusedUop)
        break
...
lastApplicableInstr = next(instr for instr in instructions if instr.isLastDecodedInstr())
...
TP = round((uopsForRelRound[-1][lastApplicableInstr][-1].retired
          - uopsForRelRound[0][lastApplicableInstr][-1].retired)
          / (len(uopsForRelRound) - 1), 2)
```

Python behavior in `getBottlenecks`:

- `TP` is simulated TP from `uopsForRound`, not analytical prediction.
- `relevantInstrInstancesForInstr` is built from instruction instances whose `rnd` is in the relevant round window.
- `Scheduling` uses actual `uop.actualPort` counts from relevant retired uops.
- `Divider` uses actual `uop.prop.divCycles` from relevant retired uops.

Rust fix shape:

- Add/keep `ReorderBuffer.retire_queue` mirroring Python `retireQueue`.
- In the Rust run-simulation loop, drain `retire_queue` and build Python-shaped `uops_for_round[rnd][instr_id] -> fused_uop_ids`.
- Compute simulated TP from `uops_for_round`, `lastApplicableInstr`, `retireIdx`, and retired cycles.
- Compute `Scheduling`/`Divider` from relevant instruction instances/uops, not from analytical limits alone.

Do not:

- Scan all laminated uops after simulation to approximate `uopsForRound` if a retire queue can be mirrored directly.
- Gate the fix by case/profile/schema to avoid test fallout.
- Keep analytical throughput as summary TP after successful simulation in loop mode.

### Pattern E: Latency-start operands and flag domains must mirror `instructions.py`

Symptom:

- `$.cycles[*].addedToRS[*].dependsOn...` mismatch for CMOV/SETCC/shift cases.
- `$.summary.limits.dependencies` mismatch after dependencies in JSON trace look closer.
- A fix for CMOV creates regressions in shift/rotate or high-8 byte forms.

Python behavior in `instructions.py`:

```python
instrInputRegOperands = [... and (('R' in instrD['rw'][n])
   or any(n==k[0] for k in latData.keys()))]
...
flagsR = instrData.get('flagsR', '')
flagsW = instrData.get('flagsW', '')
```

Key implications:

- A register operand is an input if XED marks it read **or** any latency row starts at that XML operand.
- Latency-start write operands map to their own XML operand/register, not to the next decoded input register. SETCC `REG0` is both output and latency-start input; MOVZX destination can be latency-start while source is another register.
- Flag inputs come from `flagsR`; flag outputs come from `flagsW`. Do not convert all mentioned flags into both reads and writes.
- Existing `.uipack` records may lack `flags_read` for some latency-start flag operands used by Python's shift/rotate latency-class split. Input-only fallback from latency rows may be needed, but never create output flags without `flagsW`/`flags_write`.
- XED attribute matching distinguishes `R8h` from `R8l`; Rust matcher needs high-8 operand state (`AH/BH/CH/DH`) even when a larger destination makes `max_op_size_bytes` > 1.

Rust fix shape:

- Keep this predicate in operand mapping and uop expansion, near the record/decoded operand owner.
- Add decoder/matcher state for high-8 explicit operands; do not infer from max operand size alone.
- Preserve Python flag domain split in both summary latency mapping and simulated uop expansion.
- Add tests for CMOV latency-class pseudo-uop order, CMOV/SETCC dependency limits, high-8 record selection, and shift/rotate pseudo-flag dependencies.

Do not:

- Treat `operand.flags` as both read and write just because it is present in the datapack.
- Map latency-start write-only operands through positional decoded inputs before trying their decoded output.
- Fix CMOV by special-casing `cmov_setcc_001` or only HSW; ICL SETCC/high-8 selection can reveal same root cause.

### Pattern F: Decoder mnemonic aliases must match Python/XED names before datapack lookup

Symptom:

- First mismatch is only analytical summary (`$.summary.limits.dependencies`, `issue`, or `lsd`) while cycle trace already matches.
- Runtime simulator handles a branch with fallback/uop-shape logic, but summary `LoopInstrFacts` stayed `matched = false`, so loop analytical model is skipped.
- Iced/Rust disassembly says `je`/`jne`, while Python/XED/uops.info data uses `JZ`/`JNZ`.

Python behavior:

- `uiCA.py` receives XED `iclass`/`iform` names (`JZ_RELBRb`, `JNZ_RELBRb`) and `getInstructions()` matches those names directly against uops.info records.

Rust fix shape:

- Add mnemonic aliases at datapack index lookup and matcher normalization (`JE -> JZ`, `JNE -> JNZ`, etc.).
- Keep aliases generic and mnemonic-level; do not special-case one verification case.
- Add matcher tests and verify affected branch cases across HSW/SKL/ICL.

Do not:

- Force `matched = true` for missing candidates.
- Patch summary limits after simulation to look like Python while leaving lookup unmatched.
- Change verification canonicalization to treat `JE` and `JZ` as equal.

### Pattern G: LEA matching needs XED `agen` and address-latency fields

Symptom:

- `LEA` cases show extra laminated uops or missing LEA dependencies.
- Rust selects first `LEA_GPRv_AGEN` row such as `LEA_B (R16)` instead of Python's exact `LEA_B_IS_D8 (R64)` row.
- After row selection is fixed, HSW/SKL complex LEA dispatches one cycle early because old UIPacks preserved only generic `cycles`, not Python's `cycles_addr` / `cycles_addr_index` latency.

Python behavior:

- XED exposes `agen` (`B_IS_D8`, `B_I_D32`, etc.) and `getInstructions()` matches it against uops.info rows.
- `instructions.py` expands AGEN operands to base/index register operands and uses `latData[(AGEN, REG0, 'addr')]` / `latData[(AGEN, REG0, 'addrI')]`.

Rust fix shape:

- Decode explicit LEA memory operand into address registers even though LEA is not a memory read.
- Carry `agen` through decoder → `InstrInstance` / `NormalizedInstr` → matcher.
- Treat `MEM` iform signatures as `AGEN` only for LEA matching, then filter by exact `LEA_{agen}` string and operand size.
- Preserve `cycles_addr` / `cycles_addr_index` in UIPack generation/decoding; use compatibility normalization for older packs only when Python behavior is unambiguous.
- Expand AGEN uop inputs to base/index registers in simulator and analytical latency maps.

Do not:

- Pick LEA rows by operand size alone.
- Treat LEA as a load or set `has_memory_read` to true.
- Commit regenerated `rust/uica-data/generated/` packs as part of the fix.

## Regression Checklist

Always run, in this order:

```bash
cargo fmt --all
cargo check -q
cargo build -q -p uica-cli
```

Then focused case:

```bash
python3 verification/tools/verify.py \
  --case curated/CASE \
  --arch ARCH \
  --engine rust \
  --rust-bin target/debug/uica-cli \
  --golden-root "$TMP" \
  --golden-tag "$TAG" \
  --dump-diff /tmp/uica-focus.diff
```

Then known previous failures from this campaign:

```bash
python3 verification/tools/verify.py --case curated/add_loop_001 --arch HSW \
  --engine rust --rust-bin target/debug/uica-cli \
  --golden-root "$TMP" --golden-tag "$TAG"

python3 verification/tools/verify.py --case curated/alu_dep_001 --arch HSW \
  --engine rust --rust-bin target/debug/uica-cli \
  --golden-root "$TMP" --golden-tag "$TAG"

python3 verification/tools/verify.py --case curated/cmov_setcc_001 --arch HSW \
  --engine rust --rust-bin target/debug/uica-cli \
  --golden-root "$TMP" --golden-tag "$TAG"

python3 verification/tools/verify.py --case curated/cmov_setcc_001 --arch SKL \
  --engine rust --rust-bin target/debug/uica-cli \
  --golden-root "$TMP" --golden-tag "$TAG"

python3 verification/tools/verify.py --case curated/cmov_setcc_001 --arch ICL \
  --engine rust --rust-bin target/debug/uica-cli \
  --golden-root "$TMP" --golden-tag "$TAG"

python3 verification/tools/verify.py --case curated/shift_rotate_001 --arch HSW \
  --engine rust --rust-bin target/debug/uica-cli \
  --golden-root "$TMP" --golden-tag "$TAG"
```

Then quick profile:

```bash
python3 verification/tools/verify.py \
  --profile quick \
  --engine rust \
  --rust-bin target/debug/uica-cli \
  --golden-root "$TMP" \
  --golden-tag "$TAG" \
  --jobs 4 \
  --dump-diff /tmp/uica-quick.diff
```

Optional wider check:

```bash
python3 verification/tools/verify.py \
  --profile curated12 \
  --engine rust \
  --rust-bin target/debug/uica-cli \
  --golden-root "$TMP" \
  --golden-tag "$TAG" \
  --jobs 4 \
  --dump-diff /tmp/uica-curated12.diff || true
```

Report match count before/after. Previous fixed cases must stay matched.

## Completion and Commit Gate

Before final response after a fix:

```bash
git status --short
```

If code changes are present and user did not say "do not commit":

1. Review staged diff scope; never include generated datapacks or local artifacts.
2. Perform a discrete code review phase of the changes; use a review skill for this.
3. Stage only intended source/test/skill files.
4. Commit with a Python-behavior message.
5. Re-run `git status --short` and report commit hash plus remaining untracked/modified files.

Do not leave verified fixes uncommitted silently.

## Commit Message Pattern

Use concise message naming Python behavior mirrored:

```text
fix: align frontend summary limits with sources
fix: persist pseudo operands across rename cycles
fix: mirror python macro-fused port usage
```

## Common Mistakes

- Stopping at JSON diff without reading Python code.
- Treating event trace mismatch as ordering-only when dependency graph differs.
- Recomputing summary before simulation source facts are known.
- Keeping pseudo operands local to one rename cycle; Python keeps `curInstrPseudoOpDict` until last uop.
- Running only focused test; quick profile can catch regression.
- Saying "no regression" without showing verify output.
- Ending up with a fix that technically works but doesn't follow the original uica style! This is a port, it should be readable for review/comparison.
- Applying a fix that reduces similarity with the original Python in terms of what happens when and where.
- Reconstructing a Python-maintained state from low-level storage instead of porting the Python state itself.
- Adding broad guards (`schema_version`, profile, case, arch) to protect tests instead of matching Python behavior for all applicable paths.
- Computing summary from analytical prediction when Python uses simulated retirement state.
- Mapping operands from decoded input/output order only; Python predicates often come from XML operand names plus latency rows.
- Treating all flag metadata as read/write; Python uses separate `flagsR` and `flagsW` domains.
