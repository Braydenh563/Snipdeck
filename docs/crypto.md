# Encryption of sensitive items

Code: `crates/snipdeck-store/src/crypto.rs` and `crypto/`. Supporter feature (SPEC §5.7).

## Hierarchy

```text
item payload --XChaCha20-Poly1305--> SNENC2 envelope          key: data key (DEK, 32 random bytes)
DEK --[optional master password: Argon2id + XChaCha20-Poly1305]--> --KeyProvider (DPAPI)--> SNKEY1 record
```

* `SNENC2` envelope: `"SNENC2" | nonce (24, random per message) | ciphertext | tag (16)`.
  Associated data is `0x02 | item id (16) | slot`, where the slot is `0x00` (detached, e.g. an
  export), `0x01 | key length | format key` (`text`, `html`, `rtf`, `image/png`) or
  `0x02 | file index (u32 LE)`. An envelope copied to another item, to another format or file
  of the same item, or between a stored and a detached use fails authentication (review L8).
  `encrypt_payload`/`decrypt_payload` take the slot; `encrypt_item`/`decrypt_item` are the
  detached form. Version 1 (`SNENC1`, item id only) never shipped, so there is no reader for
  it: such bytes are not an envelope.
  Random 192-bit nonces make collisions negligible without a counter (a test draws 100 000
  and checks uniqueness and bit balance).
* `SNKEY1` record: the DEK wrapped by the OS (`KeyProvider`) and, if the user set a master
  password, sealed under an Argon2id-derived key **inside** the OS wrapper. An attacker with a
  stolen copy of the record cannot even start guessing passwords without the user's OS
  session; an attacker inside the user's session still needs the password. The Argon2
  parameters, salt and mode byte are inside the OS-authenticated blob and are also AAD of the
  password seal. The AEAD tag doubles as the password verifier, so a wrong password is a clean
  `WrongPassword` and no separate hash exists to attack.
* Changing or removing the password re-seals the same DEK under a fresh salt and nonce
  (`KeyRing::change_password`); no item is re-encrypted. This is not key rotation: an old
  record plus the old password still opens the old record.

## Argon2id parameters

Default: **64 MiB (65 536 KiB), 3 passes, 1 lane**, 16-byte random salt, 32-byte output.

* RFC 9106 §4's second recommendation (t=3, m=64 MiB), with one lane because the crate runs
  without its `parallel` feature and the unlock path should not depend on core count. It
  exceeds OWASP's minimum (19 MiB, t=2, p=1) with a wide margin.
* Unlock happens once per session or after an idle timeout, so a quarter second is invisible.
* **Floor** (review L6): `KdfParams::new` refuses anything below OWASP's minimum
  (`MIN_M_COST_KIB` = 19 MiB, `MIN_T_COST` = 2), and `unlock` refuses a stored record that asks
  for less, in every build without the `testing` feature. Cheaper settings
  (`KdfParams::insecure_for_tests`) and `MemoryKeyProvider` exist only with that feature, which
  the crate's own tests turn on through a dev-dependency on itself.
* The 64 MiB is a transient allocation (zeroized on drop) during unlock; it is outside the
  40 MB idle-RAM budget of SPEC §9 only while unlocking.
* **Measured** (release build, `cargo test -p snipdeck-store --release --lib -- --ignored
  --nocapture default_params`), 7 runs on a Linux cloud VM: min 238 ms, median 250 ms,
  max 253 ms. Re-measure on the reference Windows machine in milestone 2; adjust
  `KdfParams::DEFAULT` there if it is much outside 0.2 to 0.6 s. Parameters are stored in each
  record, so changing the default never locks anyone out; `change_password` adopts it.
* Stored parameters are bounds-checked before any hashing (at most 1 GiB, 16 passes,
  16 lanes), so a damaged record cannot exhaust memory.
* Argon2id output is checked against libargon2 (`argon2-cffi`) in a unit test.
* Passwords are hashed as the exact UTF-8 bytes given: no Unicode normalisation. The UI
  should normalise (NFC) before calling if it wants IME-independent passwords.

## KeyProvider contract (milestone 2: Windows DPAPI)

See the trait docs for the full list. In short: authenticated wrap (`CryptProtectData`
blobs are), no UI (`CRYPTPROTECT_UI_FORBIDDEN`), `unwrap_key` fails with `Rejected` for any
foreign or modified blob, errors carry no secrets, intermediate copies zeroized, round trip
holds across restarts and updates. Do not use `CRYPTPROTECT_LOCAL_MACHINE` (that lets any
local user unwrap). `MemoryKeyProvider` is for tests only (`testing` feature): its key dies
with the process.

## Storage contract (review M6)

The store does the encryption; the app never handles envelopes.

* **Key record.** `Store::create_key_ring(&KeyRing, password)` makes the DEK, stores the
  `SNKEY1` record in the single-row `key_ring` table (schema v2), unlocks, and encrypts every
  sensitive item still in plain text (made sensitive on Free). `unlock_key_ring`,
  `change_master_password` (re-wraps; the old record is overwritten, but a backup still opens
  with the old password) and `lock_key_ring` (drops the DEK and every decrypted payload held in
  memory) complete it. The DEK is held in the `Store` only while unlocked. `KeyRing` brings the
  `KeyProvider`, so DPAPI plugs in at milestone 2 without store changes.
* **Which items.** Without a key ring, sensitive items are kept in plain text (Free: masking
  only, SPEC §6). With one, every write of a sensitive item stores each payload as an
  `SNENC2` envelope bound to its slot, and sets `items.encrypted = 1`. That column, not the
  bytes, says what is encrypted, so plaintext that looks like an envelope is never mistaken
  for one; `check_integrity` checks that encrypted items hold envelopes only.
* **Reads.** `ItemRepo::formats` decrypts. Payloads up to `INLINE_LIMIT` come back inline;
  larger ones are held in memory (zeroized when dropped) and read through `BlobStore::read`,
  never written to disk in plain text. `Store::put_sensitive` does the same for a new capture,
  so a large secret never reaches the blob folder; a plain `put` for a sensitive item works but
  writes the plaintext first (the store then zeroes and deletes that file).
* **Locked.** Metadata of encrypted items stays readable and editable (rename, pin, tags). Reading
  their content, writing new content of a sensitive item, marking an item sensitive, and
  clearing the flag of an encrypted item fail with `Error::EncryptionLocked`, so no new
  plaintext of a secret is stored and nothing is decrypted without the key.
* **Clearing `sensitive`** decrypts the content first (with the key) and rewrites it in plain
  text, then indexes its body. **Setting it** encrypts the stored content and removes the
  body from the search index.
* **Search.** A sensitive item is indexed by its title only, encrypted or not.

## What is left on disk (review H1)

Making an existing item sensitive, purging the trash, or replacing a sensitive item's
content deletes plaintext. The store takes these steps so it does not survive in the data folder:

| Where plaintext could linger | What the store does | Guarantee |
|---|---|---|
| Freed space in database pages and on the freelist | `PRAGMA secure_delete = ON` on every connection (before migrating), so SQLite overwrites deleted content with zeros. `ON`, not `FAST`: `FAST` skips freelist pages. | SQLite guarantees the zeroing of content it deletes in the database file. |
| The full-text index | FTS5 `secure-delete` option (schema v2): deleting a row removes its tokens from the index segments instead of adding a tombstone. One `optimize` during the v1 to v2 migration merges what v1 left behind. | Same as above, for the index's own tables. |
| `snipdeck.db-wal` | After a write that removes plaintext, `PRAGMA wal_checkpoint(TRUNCATE)`: the WAL is copied into the database and truncated to zero bytes. | Best effort: it runs after the commit. A crash between the commit and the checkpoint leaves the WAL as it was; the next open replays and later checkpoints it. If another connection were reading (there is none: single writer, one connection), the checkpoint could not complete until it finished. |
| `snipdeck.db-shm` | Nothing needed: it holds the WAL index (page numbers and checksums), not content. | |
| Blob files | Files that held plaintext are overwritten with zeros, flushed, then deleted, after the commit. A file that cannot be deleted is left for `gc_blobs`. | Best effort. |
| Versions before v2 | Opening a v1 database runs `VACUUM` once after migrating (v1 freed pages without zeroing), then a `TRUNCATE` checkpoint. | Best effort (it runs after the migration has committed). |

A regression test (`tests/remanence.rs`) marks an existing item (inline text and a large blob)
sensitive and encrypted, then scans `snipdeck.db`, `-wal`, `-shm` and the blob folder for the
plaintext and its lower-cased search token, after a clean close and after a simulated crash
(`mem::forget`), with 0, 50 and 500 other items. Switching any one of `secure_delete`, the
FTS5 option or the checkpoint off makes it fail.

What is **not** guaranteed:

* **Copies made earlier keep the plaintext:** the `snipdeck.db.pre-vN.bak` migration backups,
  the daily `.snipdeck` backups (SPEC §5.7a), OS file history, cloud sync of a relocated data
  folder, and file-system snapshots. The app should tell the user when they mark an existing
  item sensitive.
* **Below the file system.** Zeroing a page or a file rewrites it through the file system; a
  copy-on-write file system, SSD wear levelling or a journaling layer may keep the old blocks.
  Truncating the WAL frees its blocks without overwriting them. Only full-disk encryption
  (BitLocker) covers this.
* **SQLite temporary files** (sorting, large transactions) may be written to the OS temp
  folder and are deleted, not zeroed. Snipdeck's queries over sensitive content are small, but
  this is outside the store's control.
* **Process memory** still holds plaintext while an item is shown or decrypted (below).

## Limits and non-goals

* No recovery if the OS binding is lost (new Windows profile, DPAPI master key reset) and no
  recovery key. A "recovery key" export would be a separate feature.
* Encrypts item content only. Titles, tags and search text are not covered; the FTS index
  never receives the content of sensitive items (title-only indexing, architecture.md).
* Process memory can still hold plaintext while an item is shown.
