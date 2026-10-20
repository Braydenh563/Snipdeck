# Security pass for v1

Branch `claude/v1-security`, cut from release tip 10e8471, merged with 398d5d8. Source: a CodeQL-style sweep and a read-only deep review (grep and line reads, nothing run). No High or Critical finding. 3 Medium, 9 Low, 9 Info.

Everything under `cfg(windows)` is **unverified at runtime**: it passes `cargo clippy --target x86_64-pc-windows-gnu` only. The owner must run `tools/windows-test/` for the items marked W.

## Fixed in this branch

| ID | Finding | Fix | Tests |
|---|---|---|---|
| M1 | An import could strip Sensitive: an `SNENC2` payload on an item marked `sensitive:false` was decrypted with the library key and stored as plain text (`import.rs`). | Such an item is refused with `Error::Malformed` naming it, also in a dry run. | `sensitive::an_encrypted_payload_on_an_item_marked_not_sensitive_is_refused` |
| M2 | A crafted CopyQ JSON file could exhaust memory (whole file parsed into a `serde_json::Value`; preview hit it too). | One streaming pass. Stops with `Warning::Truncated` at `max_items` kept, 4 x `max_items` looked at, and the byte budget. Deep nesting is skipped without recursion. | `copyq_a_huge_array_of_junk_is_cut_off_at_the_scan_cap`, `copyq_stops_reading_at_max_items_without_a_tree`, `copyq_a_deeply_nested_array_is_skipped_not_recursed_into`, `copyq_a_huge_tags_array_in_one_item_keeps_only_the_cap` |
| L1 | Recent capture checked `ExcludeClipboardContentFromMonitorProcessing` before opening the clipboard (race), and `Clipboard Viewer Ignore` was ignored. | All three markers are read after the clipboard is open, before any text is copied. Pure `excluded_by_markers(Markers)`. W | `excluded_by_markers` tests |
| L2 | A sensitive paste whose snapshot or restore failed left the secret on the clipboard with no clear timer; the timer only covered text. | New `UseOutcome::PastedNotRestored` arms the timer. The timer keeps a text digest and the clipboard sequence number and empties the clipboard when the content is no longer ours-as-text and the sequence number is unchanged. W | 4 controller tests, 3 pipeline tests |
| L3 | Pasted files stayed in `%TEMP%\Snipdeck\paste-N`. | Random folder name (`paste-<32 hex>`). Each paste keeps the previous folder (the clipboard or a pending snapshot may still point into it) and removes older ones; nothing is removed on exit (a persistent CF_HDROP may point into the folder), all are removed at the next start. Files are deleted, not overwritten. W | 4 `win_formats` tests (the newest-folder choice, name clash, cleanup keeping one, name check) |
| L4 | The documented throwaway licence key was not refused by the release guard. | `select(.., allow_test_key)` refuses the public keys of seeds `[0x0b;32]` and `[7;32]` (hard-coded deny-list) in release. Opt-in `SNIPDECK_ALLOW_TEST_LICENCE_KEY=1` for local release test runs only; the official release never sets it. | key_selection tests |
| L5 | The store `testing` feature lowers the Argon2 floor. | `compile_error!` for `testing` without debug assertions outside `cfg(test)`; `tests/release_guard.rs` checks no shipped manifest enables it. Side effect: release-profile runs of store and app integration tests no longer compile. | `release_guard` |
| L7 | A draft written before Sensitive was ticked was removed without scrubbing. | `Drafts::clear` overwrites with zeros, syncs, removes. | hard-link test |
| L8 | Ditto import: no defensive config; unbounded group read. | `SQLITE_DBCONFIG_DEFENSIVE`, `cell_size_check=ON`, `mmap_size=0`; at most 10,000 groups, then `Warning::Truncated`. | `ditto_reads_at_most_a_capped_number_of_groups` |
| L9 | Folder importer followed a link swapped in after listing. | `open_without_links`: `symlink_metadata` first (refuses a symbolic link, on Windows a mount point or junction, and a non-regular file; OneDrive and dedup reparse points are ordinary files and import), then open and, on Unix, require the same dev/ino. Skips with the existing `SymlinkSkipped` warning. `libc` is not a dependency, so no `O_NOFOLLOW`. The Windows branch is W. | 4 unit tests (incl. the reparse-tag decision: symlink `0xA000000C` and mount point `0xA0000003` refused; OneDrive `0x9000001A`... and dedup `0x80000013` not) plus the existing folder test |
| B | Licence issuer wrote the Ed25519 private seed in clear. | `genkey` refuses a folder inside a git work tree (symlinks resolved, folder need not exist). `docs/licensing.md` says: clear text, keep on an encrypted drive, inherits folder permissions. A Windows ACL was skipped as not cheap. | `genkey_refuses_a_folder_inside_a_git_work_tree_and_writes_nothing`, `genkey_outside_a_repository_still_works` |
| B | `main_binding.rs:297` logged a tag name. | Logs `<N chars>`. | n/a |
| D | Master password in Rust memory. | Already `Zeroizing<String>` in `master_job.rs`; the store only borrows `&str`. No change. The Slint `SharedString` is out of scope (`docs/crypto.md`). | n/a |
| Recent | Recent inbox entries were plaintext in `snipdeck.db` (hub request). | See "Recent entries at rest" below. | see below |

## Not changed, with reason

- `licence_store.rs:36` writes the licence token in clear. A licence is signed, not secret from its owner. Dismiss in CodeQL.
- L6 Sensitive items keep title, source app, file names, tags and timestamps in plain text. Intended; document in the user guide.
- M3 and the other workflow items: the owner rule is no agent edits to workflows. See "Owner: apply".

## CodeQL alerts (28 critical, 35 after PR #13)

Production crypto is clean: OS CSPRNG, a fresh random 24-byte nonce per seal, random Argon2 salts, no MD5/SHA1/ECB, no TLS code. The alerts are test values.

Done: test seeds, keys, nonces, salts and passwords are built at run time (blake3 derivation via `pw(label)`, computed arrays, a joined string). `licence/testing.rs` is `#![cfg(test)]` file-wide. Covered files: `snipdeck-core/tests/licence.rs`, `snipdeck-app/tests/{settings_window,master_password,unlock_window}.rs`, store `tests/crypto.rs` (including the PR #13 rekey test), `envelope.rs`, `key.rs`, `provider.rs`, `keyring.rs` and `licence_store.rs` test modules. A grep found no password or key literals in the PR #13 lock_ctl job tests or the guarded panic test.

Expected to disappear: `rust/hard-coded-cryptographic-value` in the files above, and the production-sink alerts reached from them (`keyring.rs` Argon2 call, `aead.rs`).

Dismiss as "used in tests" or "by design" (not changed):

| Location | Why |
|---|---|
| `crates/snipdeck-core/src/licence/development_key.in` | Public verifying key of the published development seed. Release builds refuse to embed it. |
| `crates/snipdeck-app/src/key_provider.rs` `ENTROPY` | Documented non-secret DPAPI application entropy (`docs/crypto.md`). |
| `crates/snipdeck-store/src/crypto/provider.rs` `AAD` | Associated data, not a key or nonce; only built with `testing`. |
| `key_selection.rs` seed `[7;32]` test | Cannot be file-level `cfg(test)` because `build.rs` compiles that file. It is inside a `#[cfg(test)]` module. |
| Argon2 known-answer test | Fixed password assembled at run time, kept on purpose. |
| `licence_store.rs:36` token write | Signed licence, not a secret. |

## Residual risks (Info, documented only)

- I1 AAD has no revision counter: anyone who can write the database can roll an item or slot back to an older envelope of itself.
- I2 A same-user process can delete or replace the `key_ring` row; `ensure_key(true)` then makes a new OS-only key.
- I3 Off Windows there is no key provider, so Sensitive items are stored as plain text.
- I4 Predictable temp names opened with `File::create` (`blobs.rs`, `licence_store.rs`) in the private data folder; prefer `create_new`.
- I5 `licence_store::read` is an unbounded `read_to_string`; cap at `MAX_TOKEN_BYTES`.
- I6 Launcher backups go to `Documents\Snipdeck-backups`; with OneDrive folder redirection the database roams. Encrypted items stay safe (key is DPAPI-bound).
- I7 Any process on the desktop can post `WM_CLOSE` to hidden windows or create `Local\Snipdeck-<id>` first (denial of service).
- Clear timer reads the clipboard sequence when it is armed, not at our own write. If another app wrote in between, the timer would later empty that app's non-text content (the window is small; the digest path behaved the same). Whether `SetClipboardData` during `WM_RENDERFORMAT` bumps the sequence is unconfirmed; if it does, a pasted sensitive image or file is never cleared, which fails safe. Fix when taken up: have `write_*` return the post-write sequence and arm the timer with it. Residual; also in the Windows opt-in list below.

- I8 Licences are perpetual: no revocation, no device binding.
- I9 Decrypted bytes are copied into plain `Vec`s in `plan.rs`, `items.rs`, `export.rs`; the Slint password string is not zeroized.
- The importer `items.json` (64 MiB cap, typed DTOs) can still grow about 8 to 16 times through huge `tags`/`formats`/`files` arrays; per-item count caps were not added.

## Recent entries at rest

Hub request: the Recent inbox kept copied text as plain text in `snipdeck.db`.

- **Design.** Each entry's text and source app are sealed with the library data key (the key Sensitive items use), each in its own slot and bound to the row's duplicate hash, so a ciphertext moved to another row does not open. The duplicate hash is keyed with the data key once one exists, so it does not reveal equal copies. Schema v5 rebuilds `recent_entries` with BLOB columns and an `encrypted` flag.
- **Key and lock.** The first copy kept creates the key ring (OS-protected, no password asked); with no key provider nothing is kept. With a master password, a locked library hides the entries, collects nothing, and has no key until unlock. A copy during a running master-password step is dropped, so it cannot create a ring under that step.
- **Migration.** v4 to v5 copies every row as plain with `encrypted = 0`. Sealing needs the key, so it runs inside `seal_and_unlock` (creating or unlocking the key ring), in one transaction with the Sensitive sealing, and is idempotent. With a master password that is the first unlock. Rows therefore stay plain only until the first key or unlock.
- **Bad rows.** A row that cannot be sealed (damaged text) is deleted rather than failing the unlock. A row that cannot be opened is skipped when listing.
- **Rollback copies.** The `pre-vN.bak` files written by upgrades hold the old plain text. The Recent entries in `pre-v1` to `pre-v4` copies are emptied (journal mode `MEMORY`, `secure_delete` on, then `VACUUM`) at the first start of this version and at every later start; a copy that cannot be emptied is deleted, never a reason for the library not to open.
- **Tests.** `snipdeck-store` `tests/recent.rs` (`at_rest`: round trip, no plaintext in any database file, locked refusals, moved ciphertext, old-row migration and one-time sealing, backup scrub, damaged rows), `migrations` unit tests (older backups, failed scrub), `snipdeck-app` `tests/recent_at_rest.rs` and the controller Recent tests (locked, paused, promote).
- **Not covered.** Anyone who can run as the user while the app is unlocked can use the key (as for Sensitive items). Plain until first key or unlock is visible to a disk reader before then.

## Reviewer pass

An independent read-only review of 398d5d8..f1f7818 found 2 Medium and 7 Low. Disposition:

| ID | Finding | Disposition |
|---|---|---|
| MED-1 | L3 removed paste folders the clipboard still pointed at (pasting a second file item broke the restored CF_HDROP; exit deleted the files of a persistent CF_HDROP). | Fixed: the previous folder is kept, older ones go, exit removes nothing, the next start removes all. W |
| MED-2 | L9 refused every Windows reparse point, so OneDrive and dedup files never imported. | Fixed: only name-surrogate reparse points (symlink, mount point, via `FileType::is_symlink`, which tests the `0x20000000` bit) are refused. The tag rule is a pure function with a unit test over the symlink, mount point, OneDrive and dedup tag values; the code uses std's check, so that test documents the rule rather than calling the std path. W |
| LOW-1 | A copy during "Set master password" created an OS-only key ring and made the set fail. | Fixed: a copy while a master step is running is dropped. Controller test. |
| LOW-2 | One unsealable or unreadable Recent row failed the unlock or hid the inbox. | Fixed: unsealable rows are deleted, unreadable rows skipped. Two store tests; the moved-ciphertext test now expects an empty list instead of an error (neither row opens). |
| LOW-3 | Clear timer reads the sequence late; delayed rendering unconfirmed. | Not changed (see "Not changed, with reason"). |
| LOW-4 | A docked Panel did not collapse after `PastedNotRestored`. | Fixed, with a test. |
| LOW-5 | `pre-v3.bak` kept plain Recent; `privacy.md` named the wrong file. | Fixed: all `pre-v1` to `pre-v4` copies are scrubbed; doc corrected. Test. |
| LOW-6 | Scrub could leave plaintext in a journal, and a failed scrub blocked opening. | Fixed: `journal_mode=MEMORY`; a failed scrub deletes the copy. Tests. |
| LOW-7 | Doc comment of `read_file` sat above `OpenRefusal`. | Fixed. |

## What was checked and found clean

Import parsers (no zip-slip, entry-name validation, bomb ratios, declared-size checks, no nested archives, re-validation between preview and import). Dry run (real transaction, then rolled back, blob files included). Licence verification (Ed25519 strict, signature before parse, fails closed to Free, no seed in git). Crypto (XChaCha20-Poly1305, Argon2id bounds, `Zeroizing` keys, redacted `Debug`). DPAPI, clipboard handling, Windows `unsafe` blocks, supply chain (`deny.toml`, no git dependencies, no `pull_request_target`, top-level `contents: read`).

## Owner: apply (M3, `.github/workflows/release.yml`)

Agents must not edit workflows. M3: job `windows-portable` (lines 103 to 161) builds with `cargo build --release` (every dependency's `build.rs` and proc-macros run) while holding `contents: write`, `id-token: write`, `attestations: write`; `actions/checkout@v4` keeps the token in `.git/config`; the zip is attested and uploaded by the same job; every `uses:` is a mutable tag. A compromised crate or re-pointed action tag can read the token, edit the release, or swap the zip before attestation so provenance vouches for the tampered file.

Patch:

1. Split the job in two. The build job has read-only permissions and no persisted token:

```yaml
  windows-build:
    name: Windows portable zip (build)
    needs: [resolve-version, github-release]
    runs-on: windows-latest
    timeout-minutes: 60
    permissions:
      contents: read
    steps:
      - uses: actions/checkout@<full-sha>   # v4
        with:
          ref: ${{ needs.resolve-version.outputs.tag }}
          persist-credentials: false
      # ... unchanged: rustup, Build, Smoke test, Pack the zip and its SHA-256 ...
      - uses: actions/upload-artifact@<full-sha>   # v4
        with:
          name: windows-portable
          path: |
            ${{ env.ZIP }}
            ${{ env.SUM }}
          if-no-files-found: error
```

2. The publish job runs no repository code (no checkout, no cargo):

```yaml
  windows-publish:
    name: Windows portable zip (attest and upload)
    needs: [resolve-version, windows-build]
    runs-on: ubuntu-latest
    timeout-minutes: 15
    permissions:
      contents: write
      id-token: write
      attestations: write
    steps:
      - uses: actions/download-artifact@<full-sha>   # v4
        with:
          name: windows-portable
          path: dist
      - uses: actions/attest-build-provenance@<full-sha>   # v2
        with:
          subject-path: dist/*.zip
      - uses: softprops/action-gh-release@<full-sha>   # v2
        with:
          tag_name: ${{ needs.resolve-version.outputs.tag }}
          files: |
            dist/*.zip
            dist/*.sha256
          fail_on_unmatched_files: true
```

3. Pin every other `uses:` in all workflows to a full commit SHA with the tag in a trailing comment (21 occurrences: checkout, Swatinem/rust-cache, EmbarkStudios/cargo-deny-action, rustsec/audit-check, gitleaks-action, softprops/action-gh-release, attest-build-provenance, upload and download-artifact). Add `.github/dependabot.yml` with `package-ecosystem: github-actions` (weekly) so the pins stay current.
4. Add `zizmor` or `actionlint` as a CI step; both flag unpinned actions and persisted credentials.
5. Keep the `SNIPDECK_LICENCE_PUBLIC_KEY` secret in the build job only, and never set `SNIPDECK_ALLOW_TEST_LICENCE_KEY` in any workflow.

## Unverified

All Windows behaviour (L1 marker race, L2 clipboard sequence number after delayed rendering and the late sequence read of the clear timer, L3 folder cleanup (keep the previous folder, remove at the next start), L9 reparse-point handling of OneDrive and mount-point files, the `snipdeck-app` Windows impls in `host.rs`, which could not be checked without a mingw gcc). `tools/check-licence-key-guard.sh` was not run end to end. `snipdeck-ui` golden failures are the known screenshot diffs; goldens were not regenerated.
