#!/usr/bin/env bash
set -euo pipefail

# Removes a file or folder from entire git history.
# Requires: git-filter-repo (pip install git-filter-repo)
#
# Usage:
#   ./git-purge.sh <path-to-remove> [--force]
#
# WARNING: This rewrites history. All commit hashes will change.
#          Back up your repo or work on a fresh clone.

if [[ $# -lt 1 ]]; then
    echo "Usage: $0 <path> [--force]"
    echo "  path    file or directory to purge from history"
    echo "  --force skip the fresh-clone check"
    exit 1
fi

TARGET="$1"
FORCE="${2:-}"

# Check git-filter-repo is installed
if ! command -v git-filter-repo &>/dev/null; then
    echo "ERROR: git-filter-repo not found."
    echo "Install: pip install git-filter-repo"
    exit 1
fi

# Ensure we're in a git repo
if ! git rev-parse --is-inside-work-tree &>/dev/null; then
    echo "ERROR: not inside a git repository."
    exit 1
fi

# Safety: git-filter-repo wants a fresh clone by default
ARGS=(--invert-paths --path "$TARGET")
if [[ "$FORCE" == "--force" ]]; then
    ARGS+=(--force)
fi

echo "Purging '$TARGET' from all history..."
git filter-repo "${ARGS[@]}"

echo ""
echo "Done. To push the rewritten history:"
echo "  git remote add origin <url>   # re-add remote if needed"
echo "  git push origin --force --all"
echo "  git push origin --force --tags"
