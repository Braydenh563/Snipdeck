# Licence keys

Snipdeck is free software (GPL-3.0-only). Free covers everyday use. **Supporter** is for people who want a
few extra conveniences and want to support the project.

## Free and Supporter

| | Free | Supporter |
|---|---|---|
| Boards | Up to 5 | Unlimited |
| Lists, items, Panel, Picker, save from the clipboard | Yes | Yes |
| Templates with date, time and cursor | Yes | Yes |
| Sensitive items, masked, and encryption on disk | Yes | Yes |
| Backups, export, light and dark, 7 languages | Yes | Yes |
| Dock to a screen edge | Yes | Yes |
| Templates that ask for a value or a choice | No | Yes |
| Master password | No | Yes |
| Paste stack and Paste next | No | Yes |

Encryption of sensitive items on disk is free for everyone, because it is security and not a convenience. Only
the master password is a Supporter feature.

## Get Supporter

- **Pay what you want, from A$5.** The suggested price is A$8. You pay once, and every 1.x update is
  included.
- **Open Settings and choose Get Supporter…** It shows the price and an **Email me** button, which opens a
  message to brayden.hoyle@outlook.com in your email app, to ask for a key.
- **Can't afford it?** Ask for a key and say so. No questions.
- **Turn it on.** Paste the key under **Licence** in Settings and press **Activate**. The key is checked on
  your PC. Nothing is sent anywhere, and Snipdeck never goes online to check it.
- **Nothing nags you.** There are no pop-ups, countdowns or trial limits. If you reach the 5-board limit, a
  message says so. Nothing is hidden or deleted.

## If your key stops working

If a key is rejected, the window says why. Check that you pasted the whole line, which starts with `SNIP1.`.
If it still fails, ask for a new one. If you stop using Supporter, your library stays as it is. You keep every
item. Features that need Supporter simply stop working, and you can always unlock, change or remove a master
password.

## For developers

**Supporter** is pay what you want, from A$5 (suggested A$8), one time, with every 1.x update. It
adds more boards, templates that ask for values, a master password for encrypted items and paste next.
Encrypting sensitive items on disk is free for everyone: it is security, not convenience.
Supporter is unlocked by an offline, Ed25519-signed licence key (ADR-0005). There is no
account, no phone-home and no clock check: a key either verifies against the public key
compiled into the build or it does not. If you cannot afford it, ask for a key.

The token's `tier` field holds the string `"pro"`: that is the internal id of the Supporter
tier, kept so that keys already issued stay valid.

Code: `crates/snipdeck-core/src/licence.rs`. Tests: the unit tests in that file and
`crates/snipdeck-core/tests/licence.rs` (test vectors and fuzzing).

## Token format (version 1)

```text
SNIP1.<base64url(payload)>.<base64url(signature)>
```

* `base64url` is RFC 4648 §5, **no padding**, canonical (trailing bits zero).
* `payload` is compact UTF-8 JSON:

  | Field | Type | Required | Meaning |
  |---|---|---|---|
  | `v` | integer | yes | Payload format version. Must be `1`. |
  | `tier` | string | yes | Must be `"pro"`. |
  | `issued` | integer | yes | Unix seconds when the key was issued. Kept for a future revocation list. |
  | `order` | string | yes | Seller's order id, 1 to 128 bytes. |
  | `email_hash` | string | no | Hash of the buyer's email, for support look-ups. Never the address. |
  | `features` | string array | no | Feature keys (`paste-stack`, ...). Informational in v1: Supporter allows all features. Unknown keys are ignored. |

  Unknown extra fields are ignored, so adding one later does not break old builds.
* `signature` is the 64-byte Ed25519 signature over the ASCII bytes
  `SNIP1.<base64url(payload)>`, i.e. the first two segments exactly as written, dot
  included. Signing the encoded text binds the prefix and removes any re-encoding step.
* Whitespace around the whole token (from pasting) is ignored: any Unicode whitespace (NBSP,
  ideographic space, ...) and a byte order mark (U+FEFF). Tokens over 4096 bytes are rejected
  without decoding.

## Verification order and errors

1. Split into three parts; check the prefix. `SNIP<n>` with `n != 1` is
   `UnsupportedVersion(n)`, where `n` is written without leading zeros (`SNIP01` is
   `Malformed`, not another spelling of `SNIP1`); anything else is `Malformed`.
2. Decode both base64url segments (`Malformed` on failure, including a signature that is
   not 64 bytes).
3. Verify the signature with `verify_strict` (also rejects malleable signatures and
   small-order keys). Failure is `BadSignature`. **No JSON has been parsed yet**: unsigned
   input never reaches the JSON parser.
4. Parse the signed payload: `v != 1` is `UnsupportedVersion(v)`, `tier != "pro"` is
   `WrongTier`, other problems are `Malformed`.

Verification never panics on hostile input (covered by property tests), never touches the
network or the clock, and error messages never repeat token contents.

API: `snipdeck_core::licence::{verify, verify_licence, verify_embedded, LicenceError}`.
`verify` returns `Entitlements`; `verify_licence` also exposes `order`, `issued`,
`email_hash` and `features`.

## Test vectors

Signed with the **development** key (seed below). Ed25519 is deterministic, so any signer
must reproduce them exactly. The same strings are in `tests/licence.rs`.

Development seed (ASCII, 32 bytes): `snipdeck development licence key`

Minimal payload `{"v":1,"tier":"pro","issued":1760000000,"order":"ORD-1001"}`:

```text
SNIP1.eyJ2IjoxLCJ0aWVyIjoicHJvIiwiaXNzdWVkIjoxNzYwMDAwMDAwLCJvcmRlciI6Ik9SRC0xMDAxIn0.wvpQZiQQvgfQzNOTVrngZ0ehTw6UaUVb0MiAPH72Tc4KqdsLnT1Kq8rmfdbqXpvuGnaav0wuXAVfg_Xq4yW9AQ
```

## The public key in the build

The key is a compile-time constant (`LICENCE_PUBLIC_KEY` in `snipdeck_core::licence`). It is
**not** read from the environment, a file or the registry at run time: a key the user (or
malware) can redirect is no check. `crates/snipdeck-core/build.rs` chooses it **when the
crate is built**, by this policy (`src/licence/key_selection.rs`, unit tested):

| Build | `SNIPDECK_LICENCE_PUBLIC_KEY` | Result |
|---|---|---|
| any | a valid key (base64 of the 32 raw bytes, standard or URL-safe alphabet, padding optional) | that key; `IS_DEVELOPMENT_KEY = false` |
| debug (`cargo build`, `cargo test`, `clippy`, `doc`) | unset or blank | the **development** key; `IS_DEVELOPMENT_KEY = true` |
| **release** (`--release`, or a profile inheriting from it) | unset or blank | **the build fails**, with this section's name in the message |
| any | the development key, not base64, not 32 bytes, or not a point on the curve | the build fails |

The repository's development key (`DEVELOPMENT_PUBLIC_KEY`) is for debug builds and tests
only. Its seed is in the tests, so the vectors below are reproducible. Release builds refuse
it, and a licence signed with the development seed is rejected by any build that embeds
another key. The release-mode test `release_builds_never_embed_the_development_key` checks
the rule.

## Release procedure

1. The production public key (see the next section) is stored as a CI secret or variable,
   `SNIPDECK_LICENCE_PUBLIC_KEY`. It is public, so it need not be a secret, but keep it in one
   place.
2. The release workflow builds with it set, for example
   `SNIPDECK_LICENCE_PUBLIC_KEY=<base64> cargo build --release --locked`. If the variable is
   missing or empty, the build stops with an error naming it. Nothing in source is edited.
3. Check a token signed by the **production** private key verifies in the built app (the app
   shows no "development licence key" warning), and that a development-seed token (the vectors
   below) is rejected.
4. `tools/check-licence-key-guard.sh [+toolchain]` proves the guard itself (release fails with
   no key, an empty key, the development key and malformed keys; debug builds still work;
   a release builds and passes the licence tests with a throwaway key). Run it when touching
   `build.rs`, `keys.rs` or the release workflow.

Running release-mode tests or timings locally (for example the 10k search timing test) needs
a key too: use any valid public key, such as the throwaway
`SNIPDECK_LICENCE_PUBLIC_KEY=Zr5+Myx6RTMyvZ0Kf32wVfXF7xoGraZtmLOftoEMRzo=` (the key of seed
`[0x0b; 32]`; it is a test value, not a product key). A release build refuses this key (and
the other published test keys) unless `SNIPDECK_ALLOW_TEST_LICENCE_KEY=1` is also set, so
a copy of it in the release secret cannot ship; the official release never sets the opt-in.

## Production key: generation, storage, rotation

The production **private** key is never in this repository, in CI logs, or on a developer
laptop that is not the signing machine.

1. **Generate** on an offline machine (or a hardware token such as a YubiKey/HSM that
   supports Ed25519), for example `openssl genpkey -algorithm ed25519 -out licence-signing.pem`.
   Print the raw 32-byte public key: `openssl pkey -in licence-signing.pem -pubout -outform DER | tail -c 32 | xxd -i`.
2. **Store** the private key in a password manager or hardware token plus an offline,
   encrypted backup held by a second person. The shop's fulfilment service gets a
   **signing capability** (a service that signs on demand), not the key file, where possible.
3. **Install the public key** as `SNIPDECK_LICENCE_PUBLIC_KEY` in the release workflow (see
   "Release procedure"); no source edit is needed. Check a production-signed test token
   verifies in the release build before publishing.
4. **Rotate** if the private key is ever suspected exposed: generate a new pair, ship a build
   for the new public key, and re-issue keys to buyers on request. The token format does not
   change. Licences are perpetual and offline, so there is no online check.

## Keys for friends

`tools/snipdeck-licence-issuer` (binary `snipdeck-licence-issuer`) makes the key pair and signs keys by hand.
It is a workspace tool, not part of the app: the signing code lives in
`snipdeck_core::licence::signing` behind the cargo feature `licence-issuer`, which only this tool
enables.

```sh
# Once: make the key pair. Refuses to overwrite an existing key.
cargo run -p snipdeck-licence-issuer -- genkey --out ~/snipdeck-licence-keys
#   private.key  (mode 0600; secret)   public.key
#   SNIPDECK_LICENCE_PUBLIC_KEY=<base64>   <- the exact value the release build takes

# Per friend: print a key to send them.
cargo run -p snipdeck-licence-issuer -- sign --key ~/snipdeck-licence-keys/private.key --order GIFT-alex

# Check any key against a public key and show what it says.
cargo run -p snipdeck-licence-issuer -- verify --public ~/snipdeck-licence-keys/public.key SNIP1.…
```

* `genkey` writes `private.key` **in clear text** (base64 of the seed; mode 0600 on Unix, and on
  Windows it inherits the folder's permissions, so use a private folder). Keep the folder on an
  **encrypted drive** (BitLocker, FileVault, LUKS) or move it to an offline token. The tool
  refuses any folder inside a git work tree, so the key cannot be committed by accident.
* `--order` must be 1 to 128 characters of `A-Z a-z 0-9 . _ -`; anything else is refused.
  Use **`GIFT-<name>`** (`GIFT-alex`, `GIFT-sam-2`) for every key given away, one per
  person. The order id is signed into the key.
* `--email-hash HEX` (lower-case hex) is optional and never an address; `--issued UNIX`
  fixes the issue time (default: now), for reproducible keys.
* There is no `--expires`: format v1 has no expiry field, so every key is perpetual.
* The key goes to the friend as the printed `SNIP1.…` text, one line.
* **Where to keep `private.key`**: the same place as the production key ("Production key:
  generation, storage, rotation"): a password manager or an offline encrypted volume, never in
  this repository (`private.key` and `licence-keys/` are git-ignored as a safety net), CI or a
  shared drive. On Windows the file inherits the folder's permissions, so make the folder
  private.
* Install `public.key`'s value as `SNIPDECK_LICENCE_PUBLIC_KEY` for the release build
  ("Release procedure"). Keys signed by `private.key` verify only in builds that embed the
  matching public key.

## Notes for the UI

* Show a visible warning while `IS_DEVELOPMENT_KEY` is `true` (it is always `false` in a
  release build).
* Store the token text as the user entered it; re-verify at every start (it is a few
  microseconds of work).
* A token that fails verification means Free: nothing is hidden or deleted (SPEC §6).
