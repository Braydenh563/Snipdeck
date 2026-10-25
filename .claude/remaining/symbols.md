# symbols agent: remaining work

## Done
- Slice 1 (text pipeline): scripts/symbols.py, symbols.lock, test_symbols.py, kaomoji.tsv, generated data, manifest.json, THIRD_PARTY_NOTICES.md.

## Next
- Slice 2 (emoji art 48 px), slice 3 (snipdeck-symbols crate).

## Blocked / for the owner
- CI line (a hook refuses .github/workflows edits): add to the lint job
  `python3 -I scripts/test_symbols.py && python3 -I scripts/symbols.py --check`
  (scripts/gate.sh already runs both).
- Deviation: icons.bin.z is 213 KB (budget raised 200 -> 220 KB; zopfli would give 202 KB).
- Deviation: Fluent art comes from the npm package @iconify-json/fluent-emoji-flat 1.2.6 (MIT, SVG bodies of microsoft/fluentui-emoji), pinned by hash, because the GitHub API is blocked and the repo is huge.
- emoji-test.txt is pinned from the `latest` URL (the versioned 18.0 path 404s); the sha256 fixes the content.
