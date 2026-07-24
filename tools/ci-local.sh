#!/usr/bin/env bash
# Local CI-equivalent: runs the exact gate sequence from .github/workflows/ci.yml
# so the pipeline is reproducible without pushing. Exits non-zero on first
# failure. Tools that may be absent locally (cargo-llvm-cov, cargo-deny) are
# skipped with a clear notice rather than silently passing.
set -uo pipefail
cd "$(dirname "$0")/.."

fail=0
step() { echo; echo "==> $1"; }
run()  { if "$@"; then echo "   OK"; else echo "   FAIL"; fail=1; fi; }

step "rustfmt (formatting)"
run cargo fmt --all --check

step "clippy — library/bins (strict: no-panic invariant)"
run cargo clippy --workspace --lib --bins -- -D warnings

step "clippy — tests (restriction lints allowed)"
run cargo clippy --workspace --tests -- -D warnings \
    -A clippy::unwrap_used -A clippy::expect_used -A clippy::panic

step "tests"
run cargo test --workspace

step "architectural fitness functions"
run python3 tools/fitness/check.py

step "coverage gate (line >= 98%)"
if cargo llvm-cov --version >/dev/null 2>&1; then
    run cargo llvm-cov --workspace --fail-under-lines 98 --summary-only
else
    echo "   SKIPPED (cargo-llvm-cov not installed) — CI installs it"
fi

step "supply-chain (cargo-deny)"
if cargo deny --version >/dev/null 2>&1; then
    cargo generate-lockfile >/dev/null 2>&1
    run cargo deny check
    rm -f Cargo.lock
else
    echo "   SKIPPED (cargo-deny not installed) — CI installs it"
fi

echo
if [ "$fail" -eq 0 ]; then
    echo "ci-local: ALL GATES PASSED"
else
    echo "ci-local: FAILURES ABOVE"
fi
exit "$fail"
