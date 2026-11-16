#!/usr/bin/env bash
# The checks that need no Rust build: translations, generated files, installer and packaging
# scripts, and their own tests. `scripts/gate.sh` and CI's "scripts" job both run this one
# file, so a check cannot be in the gate and missing from CI.
#
# A `scripts/test_*.py` that this file does not name fails the run: a new test is wired in
# here or it is not a test.
set -u
cd "$(dirname "${BASH_SOURCE[0]}")/.." || exit 1
failed=()
run() { local name="$1"; shift; echo "== $name"; "$@" || failed+=("$name"); }

run codemap python3 scripts/codemap.py --check
run i18n python3 scripts/i18n.py check --strict
run i18n-tests python3 scripts/test_i18n.py
run symbols python3 -I scripts/symbols.py --check
run symbols-tests python3 -I scripts/test_symbols.py
run installer python3 -I scripts/installer_check.py
run installer-tests python3 -I scripts/test_installer_check.py
run msix-tests python3 -I scripts/test_msix_manifest.py
run bump-tests python3 -I scripts/test_bump_version.py
run notices python3 scripts/third_party_notices.py --check

for t in scripts/test_*.py; do
  grep -q "$t" scripts/checks.sh || { echo "not wired into scripts/checks.sh: $t"; failed+=("unwired $t"); }
done

if [ ${#failed[@]} -ne 0 ]; then echo "checks failed: ${failed[*]}"; exit 1; fi
echo "checks ok"
