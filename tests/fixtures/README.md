# Test fixtures

Object files are **not tracked in git** — they are generated from the
assembly sources in `verification/cases/`.

Rebuild all fixtures:

```bash
# from repo root
for s in verification/cases/curated/*/snippet.s; do
  case=$(dirname "$s" | xargs basename)
  llvm-mc -filetype=obj -triple=x86_64-pc-linux-gnu "$s" \
          -o "tests/fixtures/${case}.o" 2>/dev/null \
    && echo "ok: $case" || echo "skip: $case"
done
```

Or use the VSCode task **"assemble fixtures"**.
