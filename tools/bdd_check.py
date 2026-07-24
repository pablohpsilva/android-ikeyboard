#!/usr/bin/env python3
"""BDD traceability gate.

The Gherkin specs under features/ are only meaningful if they stay wired to real
requirements. This makes them a *verified build artifact* rather than free-
floating documentation: every scenario must carry at least one `@BR-<n>` tag, and
every such tag must reference a BR that actually exists in the source of truth
(BUSINESS_REQUIREMENTS.md).

It does NOT execute the scenarios — the executable verification is the Rust test
suite (each test module cites its BR). This gate guarantees the specs cannot
drift from the requirements. Run in CI and via tools/ci-local.sh.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
FEATURES = REPO / "features"
BRD = REPO / "BUSINESS_REQUIREMENTS.md"

BR_TAG = re.compile(r"@(BR-\d+[ab]?)")
BR_DEF = re.compile(r"\bBR-\d+[ab]?\b")


def known_brs() -> set[str]:
    return set(BR_DEF.findall(BRD.read_text(encoding="utf-8")))


def main() -> int:
    valid = known_brs()
    problems: list[str] = []
    features = sorted(FEATURES.glob("*.feature"))
    if not features:
        print("bdd: no feature files found", file=sys.stderr)
        return 1

    for f in features:
        rel = f.relative_to(REPO).as_posix()
        lines = f.read_text(encoding="utf-8").splitlines()
        file_tags = {m for ln in lines for m in BR_TAG.findall(ln)}

        # Every tag must reference a real BR.
        for tag in sorted(file_tags):
            if tag not in valid:
                problems.append(f"{rel}: tag @{tag} is not a requirement in the BRD")

        # The feature must trace to at least one BR.
        if not file_tags:
            problems.append(f"{rel}: no @BR-<n> tag — feature is untraceable")

        # Every Scenario must be individually tagged (tags appear on the line(s)
        # immediately above the Scenario keyword).
        for i, ln in enumerate(lines):
            if re.match(r"\s*Scenario\b", ln):
                above = []
                j = i - 1
                while j >= 0 and lines[j].strip().startswith("@"):
                    above.append(lines[j])
                    j -= 1
                if not any(BR_TAG.search(a) for a in above):
                    problems.append(
                        f"{rel}:{i + 1}: scenario has no @BR tag on the lines above it")

    if not problems:
        print(f"bdd: {len(features)} feature files traceable — "
              "all scenarios @BR-tagged, all tags reference real requirements")
        return 0

    print(f"bdd: {len(problems)} traceability problem(s):", file=sys.stderr)
    for p in problems:
        print(f"  {p}", file=sys.stderr)
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
