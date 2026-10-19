# CI proposals (owner applies)

Unattended runs may not edit `.github/`, so these files wait here for the owner.

1. `release.yml` -> `.github/workflows/release.yml` (new). Tag or manual release: checks the tag
   against `[workspace.package] version`, notes from CHANGELOG.md, builds the zip with the
   production key, smoke-tests `--check-db` on a fresh folder, uploads zip + SHA-256 + provenance.
2. `release-build.yml`: in the existing `.github/workflows/release-build.yml`, delete
   `tags: ["v*"]` from `on.push`, the "Attach to the release" step, and the `if ($release)` branch
   (keep the dev build). release.yml now owns releases, so nothing is built twice.
3. `secrets.yml` -> `.github/workflows/secrets.yml` (new). gitleaks over full history.

After applying, delete this folder. Before tagging v1.0.0: bump the workspace version to 1.0.0
and rename the CHANGELOG header (the hub does both in the ship step).
