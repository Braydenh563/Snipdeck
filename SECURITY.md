# Security policy

## Reporting a vulnerability

Please do not open a public issue for a security problem.

Report it privately with a GitHub private security advisory:
<https://github.com/Braydenh563/Snipdeck/security/advisories/new>
(repository **Security** tab, **Report a vulnerability**). Include what you found, the steps or a file
that reproduces it, the version or commit, and what an attacker gains. A proof of concept helps; please
do not use real personal data in it.

Snipdeck is maintained by one person, so replies are best effort. Reports are read, confirmed or
declined with a reason, fixed, and credited in the changelog unless you prefer not to be named.
Please give a fix reasonable time before you publish details.

## Supported versions

Snipdeck has not had a release. Only the `main` branch is supported; fixes land there. When releases
exist, this section will list which are supported.

## Scope

In scope:

- **Reading untrusted files:** `.snipdeck` archives and the Ditto, CopyQ and folder importers
  (`snipdeck-portable`): zip tricks, decompression bombs, path traversal, memory or time exhaustion,
  data written outside the library.
- **Sensitive items and encryption** (`snipdeck-store`, [`docs/crypto.md`](docs/crypto.md)): plaintext of
  a sensitive item stored or logged where the design says it is not, key handling, authentication of
  envelopes.
- **Licence verification** (`snipdeck-core`, [`docs/licensing.md`](docs/licensing.md)): accepting a key that
  was not signed by the build's public key.
- **The clipboard and paste path** (`snipdeck-platform`, `snipdeck-app`): content leaking to history
  features that should exclude it, pasting into a window other than the Target, handling of other
  programs' clipboard data.
- **Dependencies:** a known advisory in a crate that Snipdeck ships (`cargo deny check` runs in CI).

Out of scope:

- Anyone who can already run code as you, or read your user profile, can read your library. The data
  folder is protected by your operating system account and, for sensitive items in the Pro tier, by
  encryption (below); that is the whole model.
- Rebuilding Snipdeck without the licence check. The source is GPL-3.0-only, so this is allowed
  ([ADR-0005](docs/adr/0005-licensing-and-pro.md)).
- Findings that need a modified build, a debugger, or administrator rights on the same machine.
- Features that do not exist yet (see the README status section).
- Denial of service from a library you created yourself.

## Design summary

- **Local only.** No account, no telemetry, no listening network port. Nothing is sent unless you ask
  (the site-icon setting is off by default). The planned crash log stays on disk.
- **Storage:** SQLite with `secure_delete`, plus a blob folder; a single writer holds a lock file
  ([`docs/store.md`](docs/store.md)).
- **Sensitive items:** masked in the UI and excluded from history; the clipboard is cleared after use (the
  masking and clearing are specified in SPEC §5.7 and not all built yet). In the Pro tier, item payloads are encrypted with XChaCha20-Poly1305 under a
  random data key, wrapped by the Windows account (DPAPI) and optionally sealed by a master password
  (Argon2id). The envelope is bound to the item and format. Titles, tags and search text are not
  encrypted.
- **Untrusted input** (archives, importers) is read with hard size and count limits, never extracted to
  paths taken from the file, and checked before anything is written; imports are transactional. Hostile-input
  and property tests cover the parsers ([`docs/format.md`](docs/format.md)).
- **Licence keys:** Ed25519 signatures verified offline.
- **Memory safety:** Rust. `unsafe` is confined to the Windows platform layer, each block commented.

Known limits, stated in [`docs/crypto.md`](docs/crypto.md): no recovery if the Windows key binding is lost;
plaintext can remain in process memory while an item is shown, and in disk blocks that file systems or
SSDs do not overwrite (use BitLocker); a program running as you can read what you can read.

Status: the crypto and parsers are covered by automated tests and written reviews
([`docs/reviews/`](docs/reviews/)). The Windows-specific parts (DPAPI, clipboard handling, paste) have
not been tested on Windows, and nothing has had an external audit.
