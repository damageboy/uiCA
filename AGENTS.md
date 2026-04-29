# Repository Guidelines

## Project Structure & Module Organization

This repository is a compact Python command-line analyzer. The main entry point is `uiCA.py`, with supporting analysis logic in `facile.py`, ISA helpers in `x64_lib.py`, shared utilities in `utils.py`, instruction data in `instructions.py`, and microarchitecture definitions in `microArchConfigs.py`. `convertXML.py` regenerates instruction data from uops.info XML. `traceTemplate.html` is used for generated HTML traces and graphs. `setup.sh` and `setup.cmd` prepare platform-specific dependencies and generated files. The bundled paper is kept at the repository root as both Markdown and PDF. There is currently no dedicated `tests/` directory.

## Build, Test, and Development Commands

- `./setup.sh`: initializes the XED submodule, builds the Python XED module, downloads `instructions.xml`, and regenerates `instructions.py`.
- `setup.cmd`: Windows equivalent of the setup flow.
- `echo ".intel_syntax noprefix; l: add rax, rbx; add rbx, rax; dec r15; jnz l" > test.asm`: creates a minimal assembly input.
- `as test.asm -o test.o`: assembles the sample input on Linux.
- `./uiCA.py test.o -arch SKL`: runs the analyzer for Skylake.
- `./uiCA.py test.o -arch all -TPonly`: quick throughput-only smoke check across supported architectures.

## Coding Style & Naming Conventions

Use Python 3 and preserve the existing style: 3-space indentation, compact helper functions, and explicit class names such as `MicroArchConfig`, `UopProperties`, and `LaminatedUop`. Prefer descriptive camel-case names for architecture concepts already modeled that way, and keep module-level helper names consistent with nearby code. Avoid broad refactors when updating generated or data-heavy files.

## Testing Guidelines

No formal test framework is checked in. Validate changes with focused smoke tests using small assembled basic blocks and representative architectures, especially `SKL` plus any architecture touched in `microArchConfigs.py`. For output changes, compare `-TPonly` results before and after where possible. If modifying trace or graph behavior, generate an HTML output with `-trace out.html` or `-graph out.html` and inspect it manually.

## Commit & Pull Request Guidelines

Recent commits use short imperative summaries such as `Fixes k0 register dependencies #23`, `Facile front end`, and `simulation parameters`. Keep commit subjects concise and reference issues when relevant. Pull requests should describe the affected analyzer behavior, list architectures impacted, include reproduction commands, and attach screenshots or generated HTML only when UI trace or graph output changes.

## Rust Data-Pack Invariant

Rust must have one instruction-data path: manifest-selected `.uipack` files. Do not add `instructions.json` or `instructions_full.json` runtime fallbacks. If trace/simulation needs operands, latencies, or other XML-derived fields, extend UIPack generation and decoding so the `.uipack` exposes complete data.

## Security & Configuration Tips

`setup.sh` downloads data from uops.info and builds the XED Python module. Review generated changes before committing, and avoid checking in local sample objects, temporary HTML reports, or downloaded intermediate XML files.
