# Verification suite

Pipeline overview (modes, Rust-parity usage, baseline policy):
- [`docs/verification-pipeline.md`](../docs/verification-pipeline.md)

## Case structure

Each verification case lives under `verification/cases/<group>/<case_name>/`.

Current curated profile sizes:

- `quick`: 2 sentinel cases
- `curated12`: 12 curated sentinel cases
- `curated24`: 24 curated sentinel cases
- `curated48`: 48 curated sentinel cases (includes AVX2 + AVX512 sampling)

Example case directories:

- `verification/cases/curated/add_loop_001/`
- `verification/cases/curated/fusion_jcc_001/`
- `verification/cases/curated/vector256_001/`

Each case directory contains:

- `case.toml` — case id, description, tags, run parameters, default arch list
- `snippet.s` — assembly snippet used for capture and verification

Case id matches directory path below `verification/cases/`.
Example: `curated/add_loop_001` maps to `verification/cases/curated/add_loop_001/`.

## Assemble case fixture manually

Manual assembly useful for spot checks outside harness.

```bash
mkdir -p /tmp/uica-verification/add_loop_001
as verification/cases/curated/add_loop_001/snippet.s -o /tmp/uica-verification/add_loop_001/snippet.o
python3 uiCA.py /tmp/uica-verification/add_loop_001/snippet.o -arch SKL -json /tmp/uica-verification/add_loop_001/SKL.json -TPonly
```

## Capture goldens

Capture full quick profile:

```bash
python3 verification/tools/capture.py --profile quick --engine python
```

Capture expanded curated profile:

```bash
python3 verification/tools/capture.py --profile curated12 --engine python
```

Capture larger curated batch:

```bash
python3 verification/tools/capture.py --profile curated24 --engine python
```

Capture full curated48 batch:

```bash
python3 verification/tools/capture.py --profile curated48 --engine python --jobs 8
```

Capture one case for one arch into custom tag/root:

```bash
python3 verification/tools/capture.py --case curated/add_loop_001 --arch SKL --engine python --golden-tag local-dev --golden-root verification/golden
```

CLI help:

```bash
python3 verification/tools/capture.py --help
```

## Verify goldens

By default, verify executes engine runs and compares against captured goldens:

```bash
python3 verification/tools/verify.py --profile quick --engine python --golden-tag local-dev
```

Fast sanity check (manifests only, no engine execution):

```bash
python3 verification/tools/verify.py --profile quick --engine python --resolve-only
```

Focused compare for one case and one arch:

```bash
python3 verification/tools/verify.py --case curated/add_loop_001 --arch SKL --engine python --golden-tag local-dev
```

Write mismatch report during focused debug:

```bash
python3 verification/tools/verify.py --case curated/add_loop_001 --arch SKL --engine python --golden-tag local-dev --dump-diff /tmp/uica-verification/add_loop_001.diff
```

Control parallelism (both capture and verify):

```bash
python3 verification/tools/verify.py --profile curated24 --engine python --golden-tag local-dev --jobs 8
python3 verification/tools/verify.py --profile curated48 --engine python --golden-tag local-dev --jobs 8
```

CLI help:

```bash
python3 verification/tools/verify.py --help
```

## Golden directory conventions

Goldens live below `verification/golden/`.

```text
verification/golden/
  python/
    <uica_commit>/
      curated/add_loop_001/
        HSW.json
        SKL.json
        ICL.json
      curated/fusion_jcc_001/
        HSW.json
        SKL.json
        ICL.json
```

Conventions:

- top-level engine directory separates Python and future Rust baselines
- `<uica_commit>` identifies commit used to capture golden set
- one canonical JSON file per case id and architecture
- case directory path mirrors case id from `case.toml`
