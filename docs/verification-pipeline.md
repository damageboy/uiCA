# Verification Pipeline

Purpose: lock Python behavior as oracle, then measure Rust parity against that oracle.

## 1) Pipeline at glance

```text
Cases/Profile -> Capture (engine run -> JSON -> canonicalize -> golden)
             -> Verify  (engine run -> JSON -> canonicalize -> compare vs golden)
```

Core tools:
- `verification/tools/capture.py`
- `verification/tools/verify.py`
- `verification/tools/canonicalize.py`

Core data:
- cases: `verification/cases/...`
- profiles: `verification/profiles/...`
- goldens: `verification/golden/<engine>/<tag>/<case>/<arch>.json`

## 2) Modes and what they mean

| Mode | Command shape | What it does | Typical use |
|---|---|---|---|
| Harness tests | `python3 -m unittest ...` | Validates pipeline code itself | Dev confidence after edits |
| Verify sanity | `verify.py --profile quick --engine python` | Resolves profile/cases/manifests only (fast preflight) | Quick CI guard |
| Full execute+compare | `verify.py --profile quick --engine <engine> --execute ...` | Runs engine and compares canonical JSON vs golden | Real parity gate |
| Focused execute+compare | `verify.py --case ... --arch ... --execute ...` | Single case/arch parity check | Fast mismatch debugging |

## 3) Current corpus profiles

### `quick`

Profile: `verification/profiles/quick.toml`

Cases:
- `curated/add_loop_001`
- `curated/fusion_jcc_001`

Arches (from case manifests):
- `HSW`, `SKL`, `ICL`

Quick matrix size: `2 cases x 3 arches = 6 results`.

### `curated12`

Profile: `verification/profiles/curated12.toml`

Contains 12 curated sentinel cases covering ALU deps, flags, load/store, divider, shifts, SIMD (128/256), cmov/setcc, and fence mix.

Matrix size: `12 cases x 3 arches = 36 results`.

### `curated24`

Profile: `verification/profiles/curated24.toml`

Contains the `curated12` set plus 12 additional curated cases for branch forms, LEA/address-generation, move-elimination style chains, partial/high8 register behavior, stack push/pop, pointer-chasing, logic mix, indexed memory addressing, and NOP-heavy decode pattern.

Matrix size: `24 cases x 3 arches = 72 results`.

## 4) Capture flow

Capture writes canonical goldens.

Example:

```bash
python3 verification/tools/capture.py \
  --profile quick \
  --engine python \
  --golden-root verification/golden \
  --golden-tag py-baseline-001
```

Focused capture:

```bash
python3 verification/tools/capture.py \
  --case curated/add_loop_001 \
  --arch SKL \
  --engine python \
  --golden-root verification/golden \
  --golden-tag local-dev
```

## 5) Verify flow

Sanity only (no engine run):

```bash
python3 verification/tools/verify.py --profile quick --engine python
```

Full execute+compare:

```bash
python3 verification/tools/verify.py \
  --profile quick \
  --engine python \
  --execute \
  --golden-root verification/golden \
  --golden-tag py-baseline-001
```

Focused execute+compare + diff report:

```bash
python3 verification/tools/verify.py \
  --case curated/add_loop_001 \
  --arch SKL \
  --engine python \
  --execute \
  --golden-root verification/golden \
  --golden-tag py-baseline-001 \
  --dump-diff /tmp/uica.diff
```

Exit codes:
- `0`: success
- `1`: failure (missing inputs, engine error, or mismatch)

## 6) Rust-port usage model

### Before Rust parity loop
1. Capture Python baseline once per approved tag:
   - `--engine python --golden-tag <tag>`
2. Freeze that baseline for Rust comparison.

### During Rust development
1. Run Rust engine in verify execute mode (`--engine rust --execute`) against Python baseline tag.
2. Fix mismatches until clean pass.
3. Use focused case+arch mode for rapid debugging.

## 7) Canonicalization and compare

`canonicalize.py` normalizes JSON so ordering noise does not cause false diffs.

Comparator reports first mismatch path (via `first_mismatch_path`) and optional diff report file when `--dump-diff` used.

## 8) Suggested baseline policy

- Recapture goldens only when Python oracle behavior intentionally changes.
- Tag goldens with stable names (`py-baseline-###` or commit-based tags).
- Keep quick profile small for CI speed; expand profiles separately for deeper parity runs.

## 9) Related docs

- Operational commands/details: `verification/README.md`
- Design rationale: `docs/plans/2026-04-21-rust-port-verification-suite-design.md`
- Build history/tasks: `docs/plans/2026-04-21-rust-port-verification-suite-implementation.md`
