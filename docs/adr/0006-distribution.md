# ADR-0006: Microsoft Store + signed GitHub Releases / winget

- Status: Accepted (2026-10-17)

## Decision
- Paid **Microsoft Store** listing (MSIX).
- Free **GitHub Releases** (MSI/MSIX) + **winget** manifest, code-signed via **SignPath Foundation** (free for OSS).
- In-app update check against GitHub Releases; user confirms; Store builds disable it.

## Consequences
Two release pipelines in CI; no SmartScreen "unknown publisher" warning on the free channel.

## Status note (2026-11-14)
Two parts of this decision are not what shipped:
- **Update check.** v1.0.0 has no in-app update check against GitHub Releases. "Check for updates" opens the
  releases page, and in a packaged build the Store's updates page. The app makes no network requests.
- **SmartScreen.** Signing does not remove the warning at once: SmartScreen reputation builds over time for a
  new certificate (see [`packaging.md`](../packaging.md), SmartScreen expectations).
