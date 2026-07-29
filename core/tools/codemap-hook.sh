#!/usr/bin/env bash
# Claude Code PostToolUse hook: keep CODEMAP.md in step with the code.
#
# Wired from .claude/settings.local.json. Reads the hook payload on stdin and
# regenerates the index only when a file that the index is derived from changed
# — so ordinary doc/config edits cost nothing.
#
# Why a hook and not just the CI gate: CI catches staleness at merge time, which
# is too late to stop a duplicate implementation that was written five minutes
# earlier against a stale index. This keeps the index true *during* the work.
#
# Always exits 0: a bookkeeping step must never block the edit that triggered it.
# The CI gate (core/tools/codemap.py --check) is the real enforcement.
set -uo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

payload="$(cat)"

read -r -d '' filter <<'PY' || true
import json, os, sys

try:
    data = json.load(sys.stdin)
except Exception:
    sys.exit(0)

tool_input = data.get("tool_input") or {}
paths = [tool_input.get("file_path") or ""]
# MultiEdit-style payloads carry their targets in a list instead.
for edit in tool_input.get("edits") or []:
    if isinstance(edit, dict):
        paths.append(edit.get("file_path") or "")

SOURCES = (".rs", ".kt", ".feature")
NAMED = ("Cargo.toml", "settings.gradle.kts")
for p in paths:
    if not p:
        continue
    if p.endswith(SOURCES) or os.path.basename(p) in NAMED:
        print("yes")
        break
PY

changed="$(printf '%s' "$payload" | python3 -c "$filter" 2>/dev/null || true)"

[ "$changed" = "yes" ] || exit 0

python3 "$REPO/core/tools/codemap.py" --quiet || true
exit 0
