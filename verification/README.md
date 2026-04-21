# Verification suite

## Case structure

Each verification case lives under `verification/cases/<group>/<case_name>/`.

Current curated examples:

- `verification/cases/curated/add_loop_001/`
- `verification/cases/curated/fusion_jcc_001/`

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

Capture CLI currently scaffold placeholder. `--help` works. Non-help invocation prints placeholder message and exits non-zero until full implementation task lands.

Planned command shape:

```bash
python3 verification/tools/capture.py --profile quick --engine python
```

Planned focused form:

```bash
python3 verification/tools/capture.py --case curated/add_loop_001 --arch SKL --engine python
```

CLI help:

```bash
python3 verification/tools/capture.py --help
```

## Verify goldens

Verify CLI currently scaffold placeholder. `--help` works. Non-help invocation prints placeholder message and exits non-zero until full implementation task lands.

Planned quick-profile verify:

```bash
python3 verification/tools/verify.py --profile quick --engine python
```

Planned focused verify for one case and one arch:

```bash
python3 verification/tools/verify.py --case curated/add_loop_001 --arch SKL --engine python
```

Planned diff report flow during focused debug:

```bash
python3 verification/tools/verify.py --case curated/add_loop_001 --arch SKL --engine python --dump-diff /tmp/uica-verification/add_loop_001.diff
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
