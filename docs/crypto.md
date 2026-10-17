# Encryption of sensitive items

Code: `crates/snipdeck-store/src/crypto.rs` and `crypto/`. Pro feature (SPEC §5.7).

## Hierarchy

```text
item content --XChaCha20-Poly1305--> SNENC1 envelope          key: data key (DEK, 32 random bytes)
DEK --[optional master password: Argon2id + XChaCha20-Poly1305]--> --KeyProvider (DPAPI)--> SNKEY1 record
```

* `SNENC1` envelope: `"SNENC1" | nonce (24, random per message) | ciphertext | tag (16)`.
  Associated data is the 16 raw item-id bytes plus the format byte `0x01`, so an envelope
  copied to another item, or relabelled as another version, fails authentication.
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
local user unwrap). `MemoryKeyProvider` is for tests only: its key dies with the process.

## Limits and non-goals

* No recovery if the OS binding is lost (new Windows profile, DPAPI master key reset) and no
  recovery key. A "recovery key" export would be a separate feature.
* Encrypts item content only. Titles, tags and search text are not covered; the FTS index
  must not receive plaintext of sensitive items (architecture.md).
* Process memory can still hold plaintext while an item is shown.
