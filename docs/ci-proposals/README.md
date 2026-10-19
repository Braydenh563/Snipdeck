# CI proposals (owner copies these into the repository's workflow folder)

Agents may not edit workflow files during unattended runs, so the changes live here. Copy each
file over the one with the same name (Dependabot's file sits next to the `workflows` folder),
in one commit on GitHub, then delete this folder.

| File | Change | Why |
|---|---|---|
| `dependabot.yml` | Groups the Slint crates into one PR; groups all action bumps; labels and commit prefixes | Slint crates share a version and a bump changes the goldens, so they must move together; fewer, tidier PRs |
| `ci.yml` (two edits, below) | Windows job runs clippy instead of `cargo check`; both jobs get `timeout-minutes: 45` | The `cfg(windows)` code (Win32 clipboard, keyboard, hotkeys, DPAPI, placement) is never linted by the Linux job; a hung job stops after 45 min instead of 6 h |
| `release-build.yml` (two edits, below) | Build provenance attestation; a size report in the job summary; `timeout-minutes: 60` | Anyone can verify a download came from this workflow (`gh attestation verify <zip> --repo Braydenh563/Snipdeck`); ship-list B12 asks for the release size |

CodeQL: the default setup you enabled already covers Rust and the workflows; no file is needed.

## `ci.yml`

1. Under `runs-on: ubuntu-latest` and under `runs-on: windows-latest`, add a line:
   `    timeout-minutes: 45`
2. In the Windows job, replace `- run: cargo check --workspace --all-targets --locked` with:

```yaml
      # Clippy rather than a plain check: the cfg(windows) code (Win32 clipboard, keyboard,
      # hotkeys, DPAPI, window placement) is never linted by the Linux job.
      - run: cargo clippy --workspace --all-targets --locked -- -D warnings
```

The first Windows clippy run may report lints in Windows-only code; I fix those.

## `release-build.yml`

1. Replace the job's `permissions:` block with:

```yaml
    timeout-minutes: 60
    permissions:
      contents: write
      # Build provenance: anyone can check a download came from this workflow with
      # `gh attestation verify <zip> --repo Braydenh563/Snipdeck`.
      id-token: write
      attestations: write
```

2. Insert before `- uses: actions/upload-artifact@v4`:

```yaml
      - name: Size report
        shell: pwsh
        run: |
          $zip = Get-Item $env:ZIP
          $exe = Get-ChildItem dist -Recurse -Filter snipdeck.exe | Select-Object -First 1
          "### Build size`n| File | MB |`n|---|---|`n| $($exe.Name) | $([math]::Round($exe.Length / 1MB, 2)) |`n| $($zip.Name) | $([math]::Round($zip.Length / 1MB, 2)) |" >> $env:GITHUB_STEP_SUMMARY
      - uses: actions/attest-build-provenance@v2
        with:
          subject-path: ${{ env.ZIP }}
```
