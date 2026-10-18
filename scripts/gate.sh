#!/usr/bin/env bash
# The gate: run before every push (and, with --staged, before every commit).
#
#   scripts/gate.sh            # fast checks (lint, typecheck, format)
#   scripts/gate.sh --changed  # plus the tests that name files changed since GATE_BASE
#   scripts/gate.sh --staged   # fast checks against what is STAGED, not the working tree
#   scripts/gate.sh --full     # plus the whole test suite (once, at the end)
#
# Written by the lean-workflow plugin's lean-setup skill from this repo's CI
# config. Edit the three marked blocks if CI changes; keep them identical to CI.
set -u
cd "$(dirname "${BASH_SOURCE[0]}")/.." || exit 1
ROOT="$PWD"
MODE_CHANGED=0; MODE_STAGED=0; MODE_FULL=0
for a in "$@"; do case "$a" in
  --changed) MODE_CHANGED=1 ;; --staged) MODE_STAGED=1 ;; --full) MODE_FULL=1 ;;
esac; done

# --- FAST CHECKS (from CI: fill one command per line, run from the repo root) ---
fast_checks() {
  cargo fmt --all --check &&
    cargo clippy --workspace --all-targets --locked -- -D warnings &&
    RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
}
# --- ALL TESTS (from CI) ---
all_tests() {
  cargo test --workspace --locked
}
# --- SOME TESTS: "$@" is the list of test files to run ---
some_tests() {
  # Rust tests are inline (#[cfg(test)]) or in crates/*/tests/*.rs; run the owning crates.
  local crates="" f c
  for f in "$@"; do
    c="$(echo "$f" | sed -n 's#^crates/\([^/]*\)/.*#\1#p')"
    [ -n "$c" ] && case " $crates " in *" $c "*) ;; *) crates="$crates $c" ;; esac
  done
  local args=""; for c in $crates; do args="$args -p $c"; done
  [ -n "$args" ] && cargo test --locked $args
}
# --- test file globs to search for a changed file's stem ---
TEST_GLOB=(':(glob)crates/**/*.rs' ':(glob)**/test_*' ':(glob)**/*_test.*' ':(glob)**/*.test.*' ':(glob)**/*.spec.*' ':(glob)tests/**')

ran=(); failed=(); skipped=()
step() { local name="$1"; shift; if "$@"; then ran+=("$name ok"); else ran+=("$name FAILED"); failed+=("$name"); fi; }

TARGET="$ROOT"
if [ "$MODE_STAGED" = 1 ]; then
  if git diff --cached --quiet; then
    skipped+=("staged (nothing staged; the working tree is checked)")
  else
    # Check the index, not the folder: a commit that splits a pair passes on the
    # working tree and breaks in CI. Heavy untracked dirs are linked in.
    TARGET="$(mktemp -d -t gate.XXXXXX)"
    trap 'rm -rf "$TARGET"' EXIT
    git checkout-index -a --prefix="$TARGET/" || exit 1
    for d in node_modules .venv venv; do [ -e "$ROOT/$d" ] && ln -s "$ROOT/$d" "$TARGET/$d"; done
  fi
fi
step fast-checks bash -c "cd '$TARGET' && $(declare -f fast_checks); fast_checks"

if [ "$MODE_CHANGED" = 1 ]; then
  base="${GATE_BASE:-$(git rev-parse --abbrev-ref '@{upstream}' 2>/dev/null || echo HEAD~1)}"
  changed="$( { git diff --name-only "$base" 2>/dev/null; git diff --name-only; git ls-files -o --exclude-standard; } | sort -u)"
  picked=""
  for f in $changed; do
    stem="$(basename "${f%.*}")"
    case "$f" in */test_*|*_test.*|*.test.*|*.spec.*|tests/*) picked="$picked $f"; continue ;; esac
    picked="$picked $(git grep -l -F -e "$stem" -- "${TEST_GLOB[@]}" 2>/dev/null | tr '\n' ' ')"
  done
  picked="$(echo $picked | tr ' ' '\n' | sort -u | grep -v '^$' | tr '\n' ' ')"
  echo "changed since $base -> tests: ${picked:-none}"
  if [ -n "$picked" ]; then
    # shellcheck disable=SC2086
    step changed-tests bash -c "$(declare -f some_tests); some_tests $picked"
  else
    skipped+=("changed-tests (no test names a changed file)")
  fi
fi
[ "$MODE_FULL" = 1 ] && step full-suite bash -c "$(declare -f all_tests); all_tests"

if [ -f scripts/codemap.py ]; then step codemap python3 scripts/codemap.py --check; fi
if [ -f scripts/i18n.py ]; then step i18n python3 scripts/i18n.py check; fi
echo "ran: ${ran[*]:-none}"
echo "failed: ${failed[*]:-none}"
echo "skipped: ${skipped[*]:-none}"
[ ${#failed[@]} -eq 0 ]
