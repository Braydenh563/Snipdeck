# Going public

Owner checklist for making the Snipdeck repository public. Do it in this order. Section 1 has been checked by an agent (results below); sections 2 to 7 are still open.

## 1. Before the switch (repo still private)

- [x] Scan the full history, not only the current files. Run `gitleaks detect --source . --log-opts="--all"` and
      `trufflehog git file://. --only-verified`. Check every hit by hand.
      Result (2026-10-19): neither tool was installed, so a scripted scan of the whole history (454 commits, all
      branches, every added line) looked for private-key blocks, `ghp_`, `github_pat_`, `sk-`, `xox`, `AKIA` and
      `AIza` tokens, JWT-like strings, password and token assignments and 32-byte base64 keys. No secrets. The
      only hits are test values: the development licence vectors (signed with the public development seed) and
      the throwaway test public key. The production public key never appears. Optional: run gitleaks yourself
      before the switch.
- [x] Search history for key files: `git log --all --name-only | grep -iE '\.(pem|key|env|pfx|p12)$|private'`.
      No hits is the goal.
      Result: no hits for those extensions or names. The only database file is the test fixture
      `crates/snipdeck-store/tests/fixtures/v1.db` (schema and sample rows only).
- [x] If anything secret was ever committed, treat it as leaked. Replace it, then rewrite history or start a
      fresh repo. Deleting the file in a new commit is not enough.
      Result: nothing secret was found, so no rewrite is needed.
- [ ] Confirm the private signing key is not in the repo, the working folder, backups synced to the cloud or
      CI logs. Keep it only with the owner, in a password manager, plus one offline copy.
- [x] Read `docs/licensing.md` and `SECURITY.md` once as a stranger. Remove anything private (names, order
      lists, personal paths).
      Result: both, plus README and CONTRIBUTING, updated for the Supporter wording and stale facts; licence
      internals and bypass-useful remarks trimmed to what `docs/licensing.md` publishes.
- [x] Check `docs/`, `.claude/` and `spikes-results/` for notes or test output you do not want public.
      Result: the owner's chat messages in `.claude/now.md` and a few docs are replaced by `[owner message
      omitted]` plus the triage; licence-gating details removed from `docs/reviews/M1-review.md`,
      `docs/HANDOVER.md` and `docs/architecture.md`; a stale clone URL and a local path removed.
      `spikes-results/` is ignored and was never committed.
- [ ] Decide the account name and email that commits show. Old commits keep the author address.
      Note: 6 commits (GitHub web merges) carry your personal email address; the rest use
      noreply addresses. Commit messages also link four `claude.ai/code/session_...` URLs. Neither is secret;
      only a history rewrite would remove them, so decide before the switch whether you mind.

## 2. Repository secret

- [ ] Settings, Secrets and variables, Actions: add the repository secret `SNIPDECK_LICENCE_PUBLIC_KEY`. It
      holds the public half of the production key only.
- [ ] Never add the private key as a secret. Release builds fail without this secret, on purpose.
- [ ] Keep the secret out of fork pull requests: leave "Send secrets to workflows from fork pull requests" off.
- [ ] Set Actions permissions to read-only by default. `release-build.yml` raises to `contents: write` itself.

## 3. GitHub settings after the switch

- [ ] Security, Code security: turn on secret scanning and push protection.
- [ ] Turn on Dependabot alerts and Dependabot security updates. `.github/dependabot.yml` covers cargo and actions.
- [ ] Turn on private vulnerability reporting (security advisories) and make sure `SECURITY.md` points to it.
- [ ] Branch protection on `main`: require a pull request, require the CI checks (fmt, clippy, tests, docs,
      `cargo deny`, Windows check), block force pushes and deletion, require up-to-date branches.
- [ ] Protect `v*` tags with a tag ruleset so only the owner can create them.
- [ ] Add `.github/CODEOWNERS` with the owner for everything.
- [ ] Require approval for workflows from first-time contributors.
- [ ] Check the issue templates and the pull request template render.
- [ ] Set the description, topics and licence (GPL-3.0-only) on the repo page.

## 4. Release v1.0.0

- [ ] Gate passes: `scripts/gate.sh --full`, `cargo deny check`, and the Windows job is green.
- [ ] The owner has run the Windows checks listed in `docs/HANDOVER.md` and `tools/windows-test/`. Unverified
      behaviour stays marked "unverified" until then.
- [ ] Move the `[Unreleased] (1.0.0)` changelog heading to `[1.0.0] - date`. Bump every version site.
- [ ] Tag `v1.0.0` on a merged commit of `main`. The Windows build workflow makes the release zip with its
      SHA-256 and attaches it to the GitHub Release.
- [ ] Download the zip from the release page, check the SHA-256, and run it on a clean Windows PC.
- [ ] Add the new hash to the launcher's pinned list if the launcher needs one, then tag a fix release if so.

## 5. Microsoft Store (MSIX)

- [ ] Note: `docs/store.md` is the storage-layer page (data folder, schema), not a Microsoft Store guide. No
      Store packaging notes exist in the repo yet. Write them (MSIX identity, signing, Partner Center listing)
      in step 21 and link them here.
- [ ] Register a Partner Center account first. The Store signs the package itself.
- [ ] Once listed, put the Store link in the About window constant and the README.

## 6. GitHub Pages (later)

- [ ] Build the site from a documented template or theme, not hand-rolled HTML.
- [ ] Publish from a `docs` folder or a Pages workflow with read-only permissions. Do not publish `.claude/`.
- [ ] Reuse the README screenshots from `docs/images/`. Keep the privacy wording the same as the app.
- [ ] Link to the Releases page for downloads and to the Store listing when it exists.

## 7. After going public

- [ ] Look at the first Dependabot and `cargo audit` results.
- [ ] Watch the Security tab for alerts for the first week.
- [ ] If a secret ever leaks: revoke it, rotate it, and note it in a security advisory.
