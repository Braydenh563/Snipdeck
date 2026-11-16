# fix-audit-privacy (branch claude/fix-audit-privacy; coordinator = session worker)

## Rules for every agent (from the owner's order)
- Per finding: write a failing test first that shows the leak (fixed clock, never the real one), then fix the whole class, then make it pass. Replace, do not layer; no weakened tests.
- Existing libraries must keep opening with the right secret and not with a wrong one: migrate key records safely and test both.
- New user strings go in docs-i18n .pot and all six .po files (find them: `git ls-files | grep -E "\.pot?$"`).
- Windows-only code: write it, check `cargo check -p snipdeck-platform --all-targets --target x86_64-pc-windows-gnu --locked`, mark it UNVERIFIED in comments/docs.
- No exploit steps in commits or docs beyond what a user needs.
- Targeted tests only (`cargo test -p <crate> --locked --test <name>`); run `cargo fmt --all`; commit every <=20 min with trailers:
  Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
  Claude-Session: https://claude.ai/code/session_01NKjURuyvhApCqnss7v9Php
- Grep docs/CODEMAP.md, then read line ranges. Do not push. Do not touch other agents' files; update the "Done" lines below for your own findings.

## Done
- PRIVACY-1/2/3: KeyRing::rotate, Store::rotate_key_ring (reseals items+recent, previous=NULL, migration V10); app finish_master uses Opened enum. Tests: store tests/crypto.rs (no_earlier_key_record_is_kept_after_a_change, a_change_re_seals_everything_under_a_new_key, an_older_library_loses_its_previous_record_and_still_opens). Commit ad88c4e.
- PRIVACY-4 slint half done (text_field.slint swallows Ctrl+C/X/Insert/Shift+Del and hides Cut/Copy for password); TEST STILL NEEDED (ui crate, UnlockWindow + testing backend clipboard).
- PRIVACY-6: win_formats::remove_listed_paste_dirs keeps (root, dir) and keeps failed dirs; tests in win_formats.rs. Windows side edited, cargo check for windows target still to run.
- PRIVACY-7: SealedKey stores a random 32-byte per-seal challenge (HELLO_CHALLENGE removed; the copy is in memory only so there is nothing old to migrate). Test: store crypto/hello.rs every_seal_signs_its_own_random_challenge_and_open_replays_it.
- PRIVACY-8: HelloAuthenticator::delete_credential (+FakeHello::deletions, windows impl UNVERIFIED: KeyCredentialManager::DeleteAsync; the windows-gnu target is not installed here so `cargo check --target` could not run). Called on Hello off, erase, remove/set password, turn_hello_off. Test: app tests/master_password.rs hello::the_hello_credential_is_deleted_when_hello_goes_off_or_the_key_goes.
- PRIVACY-16: NFC in KeyRing seal/unseal, fallback raw then NFD on WrongPassword. Tests: keyring.rs the_same_visible_password_opens_whatever_its_unicode_form, a_record_made_from_a_non_nfc_password_before_normalising_still_opens.
- PRIVACY-17: MasterProblem::Wait (2,4,8,16,30 s from the 3rd wrong in a row, injected clock; unlock/change/remove); PasswordField in master_password.slint (Caps Lock guess from keys typed, Show/Hide button); 4 strings in .pot + 6 .po. Tests: master_password.rs wrong_passwords_slow_down_from_the_third_and_clear_on_a_right_one, the_wait_covers_changing_and_removing_the_password_too. NOT verified in the running UI (no UI test for Caps Lock/Show; Slint capture-key-pressed heuristic untested).
- PRIVACY-19: temp_store=MEMORY in secure_delete() (all open paths). Test: store.rs temp_store_is_memory_on_every_open_path. Plain metadata documented in docs/crypto.md (also Hello, NFC, wait notes). PRIVACY-21 not done.

## Findings still open
- 4 test only. 5: copies from Snipdeck's own text fields (sensitive item editor, fill form) go through Slint's TextInput clipboard (arboard) with no exclusion markers, no auto-clear, NULL owner; route them through the platform clipboard writer with markers + the clipboard auto-clear. Design hint: TextField gets `private-copy: bool`; copy/cut/Ctrl+C/X call a callback; Rust writes via the platform clipboard with markers; cut removes the selection.
- 7: Hello wrapping key from a signature over a fixed challenge (crates/snipdeck-store/src/crypto/hello.rs HELLO_CHALLENGE, platform hello.rs): use a per-seal random challenge (stored with the sealed copy) so each seal has its own secret; old sealed copies: drop and ask for the password once.
- 8: Hello credential "Snipdeck" never deleted when Hello is turned off or library erased.
- 9: lock closes an open sensitive editor and drops unsaved text with no notice. Keep it encrypted in memory (restore after unlock) or ask, and tell the user.
- 10: switching Recent off keeps all rows and shows them again on switch-on. Offer to clear; clearing is the default.
- 11: "Clipboard Viewer Ignore" is read (clipboard_watch.rs VIEWER_IGNORE_FORMAT) but never written: Formats::markers() in platform windows/clipboard.rs lacks it.
- 12: app exclusion uses the clipboard owner exe only; NULL owner bypasses it; default exclusion list is empty. Add known password managers by default (1Password, Bitwarden, KeePass(XC), Dashlane, LastPass, NordPass, Proton Pass, Enpass, RoboForm, Keeper); a NULL owner with no owner exe should be treated per a setting/safe default.
- 13: Recent limits (200 entries, 7 days) and 15-minute idle lock are constants: make them settings (with bounds, in Settings UI).
- 14: "Pause history" resumes silently after restart: persist it (or say so in UI).
- 15: age pruning only on a new copy or every 6 h and the list does not filter by age on read: prune on open/unlock and filter on read.
- 16: master password not NFC-normalised: normalise; keep fallback for existing passwords (try the raw bytes if NFC fails, then re-seal normalised on next rotation).
- 17: no delay after wrong passwords, no Caps Lock warning, no show-password button.
- 18: quick capture ignores exclusion markers; "Save to..." from Recent always makes a plain item: offer Sensitive.
- 19: PRAGMA temp_store is not MEMORY (set it); metadata of sealed rows is plain (document in docs/crypto.md or minimise).
- 20: clipboard_clear_secs may be 0 (never): make sensitive copies always clear (min bound); a sensitive copy stays on the clipboard through a lock: clear on lock (lock_ctl.rs lock_now -> clear_pending_clipboard_now).
- 21: docs/crypto.md says the old key record is overwritten (false) and misstates the paste files folder; correct after fixes (also mention rotation).
- Extras (audit and fix): masking in tooltips, accessible names and undo labels on Panel and Main windows for sensitive items; the first-run Recent question window.
