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
- `bhive_smoke`: 50 sampled raw-hex cases from BHive SKL throughput data
- `bhive_skl_1k`: 1000 sampled raw-hex cases from BHive SKL throughput data
- `bhive_hsw_1k`: 1000 sampled raw-hex cases from BHive HSW throughput data
- `bhive_ivb_1k`: 1000 sampled raw-hex cases from BHive IVB throughput data

Example case directories:

- `verification/cases/curated/add_loop_001/`
- `verification/cases/curated/fusion_jcc_001/`
- `verification/cases/curated/vector256_001/`

Each curated case directory contains:

- `case.toml` — case id, description, tags, run parameters, default arch list
- `snippet.s` — assembly snippet used for capture and verification

Corpus cases may live as TOML-only manifests under `verification/corpora/<corpus>/cases/`.
BHive cases use `[input] format = "hex"`; capture/verify writes temporary raw bytes and runs uiCA with `-raw` / `--raw`.

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

## BHive corpus import

Import a deterministic 50-case SKL smoke sample from upstream BHive:

```bash
python3 verification/tools/import_bhive.py --arch SKL --limit 50 --profile bhive_smoke
```

Generate larger pinned profiles:

```bash
python3 verification/tools/import_bhive.py --arch SKL --limit 1000 --profile bhive_skl_1k
python3 verification/tools/import_bhive.py --arch HSW --limit 1000 --profile bhive_hsw_1k
python3 verification/tools/import_bhive.py --arch IVB --limit 1000 --profile bhive_ivb_1k
```

Use a local CSV instead of downloading:

```bash
python3 verification/tools/import_bhive.py --arch SKL --source /path/to/skl.csv --limit 1000 --profile bhive_skl_1k
```

BHive throughput CSV values are `cycles_per_100_iterations`; imported manifests also store `measuredCyclesPerIteration` for evaluation metadata.

Resolve the generated profile:

```bash
python3 verification/tools/verify.py --profile bhive_smoke --engine python --resolve-only
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

Emit serial command script without running verification. This prepares persistent input fixtures first, then writes one engine command per case/arch:

```bash
python3 verification/tools/verify.py \
  --profile bhive_skl_1k \
  --engine python \
  --golden-tag timing-tag \
  --emit-command-script /tmp/python-bhive-1k.sh \
  --fixture-root /tmp/uica-bhive-python-fixtures

python3 verification/tools/verify.py \
  --profile bhive_skl_1k \
  --engine rust \
  --rust-bin "$PWD/target/release/uica-cli" \
  --golden-tag timing-tag \
  --emit-command-script /tmp/rust-bhive-1k.sh \
  --fixture-root /tmp/uica-bhive-rust-fixtures
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

## Rust CLI usage

Build Rust CLI:

```bash
cargo build -p uica-cli
```

Run Rust CLI on assembled object:

```bash
target/debug/uica-cli /tmp/uica-verification/add_loop_001/snippet.o --arch SKL --tp-only
target/debug/uica-cli /tmp/uica-verification/add_loop_001/snippet.o --arch SKL --json /tmp/uica-verification/add_loop_001/SKL.rust.json --tp-only
```

## Rust parity commands

Compare Rust engine against frozen Python baseline tag.
Current verifier resolves goldens under engine-scoped roots, so mirror approved Python baseline tag under `rust/<tag>/` before running parity check.

```bash
python3 verification/tools/verify.py --profile quick --engine rust --rust-bin target/debug/uica-cli --golden-root verification/golden --golden-tag py-baseline-001
```

Focused Rust parity debug:

```bash
python3 verification/tools/verify.py --case curated/add_loop_001 --arch SKL --engine rust --rust-bin target/debug/uica-cli --golden-root verification/golden --golden-tag py-baseline-001 --dump-diff /tmp/uica-verification/add_loop_001.rust.diff
```

## CI parity gate commands

Minimal local replay of CI gate:

```bash
TMP_GOLDEN_DIR=$(mktemp -d)
python3 verification/tools/capture.py --profile quick --engine python --golden-root "$TMP_GOLDEN_DIR" --golden-tag py-ci-baseline
python3 verification/tools/capture.py --profile quick --engine rust --rust-bin target/debug/uica-cli --golden-root "$TMP_GOLDEN_DIR" --golden-tag rust-ci-smoke
python3 verification/tools/verify.py --profile quick --engine rust --rust-bin target/debug/uica-cli --golden-root "$TMP_GOLDEN_DIR" --golden-tag rust-ci-smoke
./scripts/build-web.sh
```

`./scripts/build-web.sh` requires `wasm-pack` and writes bundle to `dist/`.

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
