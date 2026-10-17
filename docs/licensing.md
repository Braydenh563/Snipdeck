# Licence keys

Snipdeck Pro is unlocked by an offline, Ed25519-signed licence key (ADR-0005). There is no
account, no phone-home and no clock check: a key either verifies against the public key
compiled into the build or it does not. Anyone can rebuild Snipdeck without the check; the
key exists so buyers can unlock the official build.

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
  | `features` | string array | no | Feature keys (`paste-stack`, ...). Informational in v1: Pro allows all features. Unknown keys are ignored. |

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

The repository's development key (`DEVELOPMENT_PUBLIC_KEY`) belongs to a seed that is
public, deliberately: it is in the tests and in this file, so anyone can mint a key that a
development build accepts. It protects nothing and must never ship. A second guard in
`keys.rs` is independent of the profile detection: a build with debug assertions off that
still embeds the development key does not compile either. A release-mode `cargo test` checks
the same rule (`release_builds_never_embed_the_development_key`), and a licence signed with
the development seed is rejected by any build that embeds another key.

Why an environment variable and not a cargo feature (the design choice): a feature such as
`development-licence-key` has to be turned *off* for release, and "off by default" fails open
the moment a CI file or a dependent crate turns it on. Here the only way to get a release is
to supply a key, and no setting turns the development key on in a release.

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
`[0x0b; 32]`; it is a test value, not a product key).

Until a production key exists, nobody can produce a release build. That is the intended
state.

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
4. **Rotate** if the private key is ever suspected exposed, or on a schedule you choose:
   generate a new pair, ship a build that trusts **both** public keys (extend the verifier to
   try each; the token format does not change), re-issue keys to buyers on request, and drop
   the old key in a later release. Because licences are perpetual and offline, a leaked key
   cannot be revoked in already-shipped builds; that is the accepted cost of no phone-home.
5. **Revocation** (not in v1): a signed list of revoked `order` ids or an `issued` cut-off,
   fetched opportunistically and verified against the same key. `verify` already takes `now`
   and every licence carries `issued` and `order` for this.

## Notes for the UI

* Show a visible warning while `IS_DEVELOPMENT_KEY` is `true` (it is always `false` in a
  release build).
* Store the token text as the user entered it; re-verify at every start (it is a few
  microseconds of work). Never store "is Pro" as a flag.
* A token that fails verification means Free: nothing is hidden or deleted (SPEC §6).
