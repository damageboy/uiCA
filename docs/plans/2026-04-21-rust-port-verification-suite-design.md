# uiCA Rust Port Verification Suite Design

Date: 2026-04-21  
Status: Brainstormed, ready for implementation planning  
Scope: Add verification harness and golden corpus to Python repo **before** Rust port

## 1. Goals

Primary goal: freeze Python behavior as machine-checkable oracle, so Rust port can prove parity.

Secondary goals:
- Enable future `uica` Rust crate/library validation.
- Enable future wasm/browser version validation against same corpus.
- Detect regressions fast via CI quick profile.

Explicit decisions from brainstorm:
- Fidelity target: **full JSON parity**.
- Arch coverage for full JSON goldens: **HSW + SKL + ICL**.
- Corpus strategy: **hybrid** (curated + generated).
- Comparator style: **semantic canonical diff** (not raw byte diff).

## 2. Current repo facts

- `uiCA.py` already supports `-json` output with:
  - `parameters`
  - `instructions`
  - `cycles`
- Existing JSON is rich but lacks explicit top-level throughput summary.
- No existing verification corpus/test harness in repo.
- `-json` cannot be combined with `-arch all` / `-alignmentOffset all` (per current CLI constraints).

## 3. High-level architecture

Create `verification/` subsystem with one harness and pluggable engines.

- Engine `python`: current `uiCA.py`.
- Engine `rust`: future binary/library adapter with same output schema.

Flow:
1. Select case set (profile/tags/case id).
2. Build/locate machine-code artifact (`-raw` preferred for stable input path).
3. Run engine with deterministic parameters.
4. Canonicalize JSON output.
5. Capture mode: write golden.
6. Verify mode: compare against golden and report structured diff.

## 4. Proposed repository layout

```text
verification/
  cases/
    curated/
      <case_id>/
        snippet.s
        case.toml
        snippet.bin          # optional committed artifact for reproducibility
    generated/
      <case_id>/
        snippet.s
        case.toml
        snippet.bin
  golden/
    python/
      <uica_commit>/
        <case_id>/
          HSW.json
          SKL.json
          ICL.json
  schemas/
    uica-result-v1.json
  tools/
    capture.py
    verify.py
    canonicalize.py
    generate_cases.py
    report.py
  README.md
```

## 5. JSON contract (v1)

Retain current payload fields and add stable metadata + summary.

Top-level schema (v1):
- `schema_version` = `"uica-result-v1"`
- `engine` (`python` / `rust`)
- `engine_version`
- `uica_commit`
- `invocation` (case id, flags, arch, profile)
- `summary`
- `parameters` (existing)
- `instructions` (existing)
- `cycles` (existing)

`summary` fields:
- `throughput_cycles_per_iteration`
- `iterations_simulated`
- `cycles_simulated`
- `mode` (`loop`/`unroll`)
- per-bottleneck throughput limits:
  - `predecoder`, `decoder`, `dsb`, `lsd`, `issue`, `ports`, `dependencies`
- `bottlenecks_predicted` (sorted list)

Compatibility rule:
- Keep existing field semantics unchanged.
- Additive fields only for v1 rollout.

## 6. Canonicalization + compare rules

Need semantic compare to avoid false failures from ordering noise.

Canonicalization rules:
1. Sort all object keys recursively.
2. Sort event lists by deterministic tuple:
   - `(rnd, instrID, lamUopID, fUopID, uopID, source, regMergeUop, stackSyncUop)`
3. Normalize `dispatched` map key ordering (`Port0`, `Port1`, ...).
4. Remove non-semantic volatile fields (timestamps, temp paths) if introduced later.
5. Emit minified canonical JSON string for storage/compare.

Compare behavior:
- Exact tree equality after canonicalization.
- Failure output:
  - first mismatch path
  - left/right compact values
  - optional markdown diff artifact per case+arch

## 7. Corpus plan

Target size: ~360 cases.

### 7.1 Curated set (~60)

Hand-authored, readable, intent-driven.

Coverage tags must include:
- front-end: predecode/decoder pressure, DSB, LSD
- fusion: macro-fusion/micro-fusion opportunities
- backend: ports/divider stress
- dependencies: reg/flag/mem dependency chains
- operand hazards: high8/partial-reg, move-elimination
- memory addressing variants (base/index/scale/disp)
- control flow + JCC erratum-sensitive placement
- SIMD width interactions (128/256/512)
- fences/serializing instructions

### 7.2 Generated set (~300)

Auto-generated from templates + instruction metadata.

Generation constraints:
- produce valid assembly only
- dedupe by canonical disasm/opcode signature
- enforce tag balance (no single category dominance)
- include deterministic seed and manifest

## 8. Case metadata format (`case.toml`)

Each case carries run matrix and invariants.

Example fields:
- `id`
- `description`
- `tags = [ ... ]`
- `[run]`
  - `arches = ["HSW", "SKL", "ICL"]`
  - `alignmentOffset = 0`
  - `initPolicy = "diff"`
  - `noMicroFusion = false`
  - `noMacroFusion = false`
  - `simpleFrontEnd = false`
  - `minIterations = 10`
  - `minCycles = 500`
- `[expectations]` (optional non-numeric checks)

## 9. Tooling commands

Capture baseline:

```bash
python3 verification/tools/capture.py --profile full --engine python
```

Verify baseline (smoke):

```bash
python3 verification/tools/verify.py --profile quick --engine python
```

Future Rust parity:

```bash
python3 verification/tools/verify.py --profile quick --engine rust --rust-bin target/release/uica-rs
```

Targeted debug:

```bash
python3 verification/tools/verify.py --case <case_id> --arch SKL --engine python --dump-diff out.md
```

## 10. CI strategy

Profiles:
- `quick`: ~40 sentinel cases × 3 arches (PR required check)
- `full`: all cases × 3 arches (nightly/manual)

Jobs:
1. `verification-quick` (required)
2. `verification-full` (nightly + manual dispatch)

CI artifacts on failure:
- canonical output JSON for failing cases
- diff report markdown
- summary by arch/tag

## 11. Determinism controls

- Keep fixed RNG seed (`random.seed(0)` already set in `uiCA.py`).
- Pin Python version in CI for baseline captures/verifications.
- Fix all run knobs per case (`minIterations`, `minCycles`, flags).
- Prefer single-process compare path initially; parallelize after deterministic proof.
- Record `uica_commit` in every golden file.

## 12. Rollout phases

### Phase 0: Harness bootstrap
- Add verification folder, schema, canonicalizer, capture/verify scripts.
- Add initial ~12 curated cases.
- Prove capture + verify loop.

### Phase 1: Python oracle stabilization
- Extend JSON output with `summary` + metadata.
- Freeze schema v1.
- Expand curated set to ~60.
- Enable CI quick profile.

### Phase 2: Corpus scaling
- Implement generator and dedupe pipeline.
- Reach ~300 generated cases.
- Capture full goldens for HSW/SKL/ICL.
- Enable nightly full profile.

### Phase 3: Rust parity campaign
- Rust emitter targets schema v1 exactly.
- Verify rust engine against Python goldens.
- Track pass-rate by tag and arch.

### Phase 4: wasm readiness
- Export browser-friendly subset packs.
- Reuse canonical compare logic in wasm harness.
- Add perf/size budgets for browser execution.

## 13. Acceptance gates

Before starting major Rust feature work:
- Python quick profile: 100% pass.
- Python full profile: stable on 3 consecutive nightly runs.

Rust parity progression gates:
- Gate A: 100% curated pass on all 3 arches.
- Gate B: >=98% generated pass.
- Gate C: 100% full corpus pass.

## 14. Risks and mitigations

1. Golden churn from legitimate Python fixes  
   - Mitigation: explicit recapture command + changelog note + review requirement.

2. Runtime/storage growth  
   - Mitigation: compressed golden storage (`.zst`), slim quick profile.

3. False diffs from ordering/noise  
   - Mitigation: canonicalizer unit tests + schema validation in CI.

4. Long-tail edge cases slowing Rust port  
   - Mitigation: tag-scoped tracking, visible waiver list with expiry.

## 15. Next step (implementation planning)

Use this design as input for implementation plan with concrete tasks, owners, and sequencing:
1. Introduce schema v1 + minimal `uiCA.py` JSON extensions.
2. Build canonicalizer + comparator.
3. Add curated seed corpus and quick CI.
4. Add generator and scale to full corpus.
5. Start Rust parity loop.
