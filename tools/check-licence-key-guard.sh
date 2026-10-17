#!/usr/bin/env bash
# Proves the release guard on the licence public key (docs/licensing.md, "Release procedure"):
# a release build can never embed the public development key.
#
#   tools/check-licence-key-guard.sh [cargo-toolchain-arg, e.g. +1.99.1]
#
# Builds snipdeck-core several times; uses its own target dir (CARGO_TARGET_DIR to override).
set -u
cd "$(dirname "$0")/.."
TOOLCHAIN="${1:-}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-${TMPDIR:-/tmp}/snipdeck-licence-guard}"
VAR=SNIPDECK_LICENCE_PUBLIC_KEY
# A throwaway, valid Ed25519 public key (seed: 32 bytes of 0x0b). TEST ONLY, not a real key.
THROWAWAY=Zr5+Myx6RTMyvZ0Kf32wVfXF7xoGraZtmLOftoEMRzo=
DEV_B64=$(grep -o '0x[0-9a-f][0-9a-f]' crates/snipdeck-core/src/licence/development_key.in |
  while read -r h; do printf "\\x${h#0x}"; done | base64 -w0)

failures=0
cargo_() { cargo $TOOLCHAIN "$@"; }

expect_fail() { # name, expected message fragment, env assignments..., -- cargo args
  local name=$1 fragment=$2; shift 2
  local out
  out=$(env "$@" 2>&1) && { echo "FAIL  $name: build succeeded but must not"; failures=$((failures + 1)); return; }
  if grep -qF -- "$fragment" <<<"$out"; then echo "ok    $name"; else
    echo "FAIL  $name: failed without the expected message ($fragment)"; echo "$out" | tail -5; failures=$((failures + 1)); fi
}
expect_ok() { # name, command...
  local name=$1; shift
  if out=$("$@" 2>&1); then echo "ok    $name"; else echo "FAIL  $name"; echo "$out" | tail -15; failures=$((failures + 1)); fi
}

build=(cargo $TOOLCHAIN build -p snipdeck-core --locked --quiet)
expect_fail "release, no key"            "$VAR" env -u $VAR "${build[@]}" --release
expect_fail "release, empty key"         "$VAR" env $VAR= "${build[@]}" --release
expect_fail "release, development key"   "development key" env $VAR="$DEV_B64" "${build[@]}" --release
expect_fail "release, not base64"        "not base64" env $VAR='%%%' "${build[@]}" --release
expect_fail "release, wrong length"      "32 bytes" env $VAR=AAAA "${build[@]}" --release
expect_fail "debug, development key set" "development key" env $VAR="$DEV_B64" "${build[@]}"
expect_ok   "debug, no key (dev key)"    env -u $VAR "${build[@]}"
expect_ok   "release, production key"    env $VAR="$THROWAWAY" "${build[@]}" --release
# The release-mode test run proves the other half: the embedded key is not the development
# key, so a licence signed with the public development seed unlocks nothing.
expect_ok   "release tests, production key" env $VAR="$THROWAWAY" cargo $TOOLCHAIN test -p snipdeck-core --locked --release --quiet --lib --test licence

if [ "$failures" -eq 0 ]; then echo "licence key guard: all checks passed"; else echo "licence key guard: $failures check(s) FAILED"; exit 1; fi
