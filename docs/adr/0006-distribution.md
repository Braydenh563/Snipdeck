# ADR-0006: Microsoft Store + signed GitHub Releases / winget

- Status: Accepted (2026-10-17)

## Decision
- Paid **Microsoft Store** listing (MSIX).
- Free **GitHub Releases** (MSI/MSIX) + **winget** manifest, code-signed via **SignPath Foundation** (free for OSS).
- In-app update check against GitHub Releases; user confirms; Store builds disable it.

## Consequences
Two release pipelines in CI; no SmartScreen "unknown publisher" warning on the free channel.
