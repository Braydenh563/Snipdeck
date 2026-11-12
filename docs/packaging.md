# Packaging and code signing

Owner steps for Windows. Nothing here has been run yet: see [What is unverified](#what-is-unverified-until-run-on-windows).
The Microsoft Store page you may have been looking for is this one. [`store.md`](store.md) is the storage layer.

## Releasing a version

1. Push everything to `main` and wait for CI to go green.
2. On GitHub: Actions > Release > Run workflow, branch `main`, type the version without the `v` (for
   example `1.1.0`), Run.

The workflow writes the version into `Cargo.toml` and `Cargo.lock`, renames the changelog's `## [Unreleased]`
section to `## [1.1.0] - <today>` (`scripts/bump_version.py`), commits `Release v1.1.0` to `main`, tags it,
and publishes the GitHub Release with that changelog section as the notes. It then builds, smoke-tests and
attaches `Snipdeck-Setup-1.1.0.exe`, the portable zip, their `.sha256` files and build attestations. The
Store MSIX is kept as a workflow artifact when the MSIX variables are set (section 1).

It refuses a version lower than the current one, an existing tag, and a run from any branch but `main`.
If `main` is protected against direct pushes, let `github-actions` bypass the rule, or bump the version
in a PR first (the workflow then has nothing to commit). Pushing a tag `v1.1.0` yourself still works when
`Cargo.toml` already says `1.1.0`.

## Installer (Setup .exe)

The normal download for a home user is `Snipdeck-Setup-<version>.exe`, built with Inno Setup 6 from
[`packaging/inno/snipdeck.iss`](../packaging/inno/snipdeck.iss). The portable zip stays available.

- **No admin.** It installs for the current user only, into `%LOCALAPPDATA%\Programs\Snipdeck`
  (the wizard lets you pick another folder). It adds a Start menu shortcut, an optional desktop shortcut
  and an entry in Settings > Apps. Windows 10 version 1903 or newer, 64-bit.
- **Start with Windows** is not an installer option. The app owns that setting (first run or Settings).
- **Your library is never moved.** Cards, settings and backups live in `%LOCALAPPDATA%\Snipdeck`
  whatever the install folder is.
- **Upgrade.** Run a newer Setup over the old one. It asks a running Snipdeck to quit (`snipdeck.exe --quit`,
  the same as tray > Quit) and waits up to 15 seconds. If it is still running, Setup stops with a message
  telling you to quit it from the tray icon. A silent upgrade (`/VERYSILENT`) starts Snipdeck again afterwards
  if it was running. An old version that does not know `--quit` just times out the same way.
- **Uninstall** (Settings > Apps) removes the program and the "Start with Windows" entry. It then asks whether
  to also erase the library in `%LOCALAPPDATA%\Snipdeck` (default No). A silent uninstall never erases it.
- **SmartScreen.** Until the installer is signed (section 2, SignPath is the plan), Windows shows "Windows
  protected your PC": choose More info > Run anyway. Check the download against the `.sha256` file.
- **Build locally** (Windows, Inno Setup 6 installed): build the release exe, then
  `iscc /DAppVersion=1.0.0 /DSourceExe=C:\path\to\target\release\snipdeck-app.exe packaging\inno\snipdeck.iss`.
  The result is `dist\Snipdeck-Setup-1.0.0.exe`. Optional: `/DOutputSuffix=-dev-abc1234`, and `/DSignTool=<name>`
  with a matching `/S<name>=...` to sign. `python3 scripts/installer_check.py` checks the script on any OS.
- **CI.** `release.yml` builds it after the exe smoke test, installs it silently into a temp folder, runs
  `--check-db` from there, uninstalls and checks the exe is gone. `release-build.yml` builds a dev installer.

**The portable zip stays available.** It is built by `.github/workflows/release.yml` (dev builds by
`release-build.yml`) and attached to the GitHub Release with its SHA-256 and a build attestation, like the
Setup .exe. MSIX and signing add a second channel and fewer warnings. They do not replace the zip.

## Facts to check before relying on them

Prices, programme rules and tool names change. Everything marked *check before relying on it* below
comes from memory or from `docs/HANDOVER.md`, not from a page read today.

## 1. MSIX

The manifest template and the logo script are in [`packaging/msix/`](../packaging/msix/README.md).

### Inputs from the repo

| What | Value |
|---|---|
| Executable | `target\release\snipdeck-app.exe`, shipped as `snipdeck.exe` (the workflow renames it) |
| Version | `1.0.0` in `Cargo.toml`. MSIX needs four parts: `1.0.0.0` |
| Icon sources | `assets/logo/snipdeck-icon.svg` (vector), `assets/logo/snipdeck.ico` (embedded in the exe) |
| Start with Windows | Settings toggle, runs `snipdeck.exe --hidden` (`settings_binding.rs`); in the Store build the WinRT `StartupTask` `SnipdeckStartup` |
| Licence | GPL-3.0-only, repository `https://github.com/Braydenh563/Snipdeck` |

### AppxManifest.xml fields

| Field | Value | Notes |
|---|---|---|
| `Identity Name` | the name Partner Center reserves, for example `YourPublisher.Snipdeck` | Copy it from Partner Center (Product identity). Do not invent it. |
| `Identity Publisher` | `CN=...` from Partner Center | Must match exactly. For a sideload test build, use the subject of your test certificate. |
| `Identity Version` | `1.0.0.0` | Raise it for every upload. The last part is reserved for the Store: keep it `0`. |
| `Identity ProcessorArchitecture` | `x64` | The only target built today. |
| `Properties DisplayName` | `Snipdeck` | |
| `Properties PublisherDisplayName` | your Partner Center publisher name | |
| `Properties Logo` | `Assets\StoreLogo.png` (50x50) | |
| `Dependencies TargetDeviceFamily` | `Windows.Desktop`, `MinVersion="10.0.18362.0"` (Windows 10 1903, equal to the installer), `MaxVersionTested` a current build | Set `MinVersion` to what the app was tested on. |
| `Application Executable` | `snipdeck.exe` | |
| `Application EntryPoint` | `Windows.FullTrustApplication` | |
| `uap:VisualElements` | `DisplayName="Snipdeck"`, `Description`, `BackgroundColor="transparent"`, `Square44x44Logo`, `Square150x150Logo` | |
| `Capabilities` | `<rescap:Capability Name="runFullTrust"/>` | Needed for a Win32 app. Needs the `rescap` namespace. |
| `desktop:Extension Category="windows.startupTask"` | `TaskId="SnipdeckStartup"`, `Executable="snipdeck.exe"`, `Enabled="false"`, `DisplayName="Snipdeck"` | The packaged equivalent of "Start with Windows". The app passes no arguments, so see the note below. |

Declare the namespaces `uap`, `desktop` (for `desktop:`), `rescap` and list them in
`IgnorableNamespaces`. Use the schema reference on Microsoft Learn for the exact namespace URIs.

**Start with Windows in the Store build.** The manifest declares the `StartupTask` (`SnipdeckStartup`,
`Enabled="false"`: off until the user turns it on). A packaged app cannot use the HKCU Run value (registry
writes are redirected), so the app calls the WinRT `StartupTask` API for it. `scripts/installer_check.py`
keeps the id in the manifest and the name `snipdeck.exe` honest.

Packaged-app differences to expect (unverified):

- The data folder. AppData virtualisation stays ON (the manifest has no `unvirtualizedResources`). On
  Windows 10 1903 and newer, new files a packaged desktop app writes under `%APPDATA%` and `%LOCALAPPDATA%`
  go into the package's own folder, so **the Store build's data is per package and is removed when the app is
  uninstalled**. It is not shared with the Setup or portable install. Check where Snipdeck keeps `snipdeck.db`
  and update [`user-guide.md`](user-guide.md) ("Open data folder") if the path differs.
- The in-app update check. `docs/adr/0006-distribution.md` says Store builds disable it. The Store
  updates the app itself.

### Logos

The repo has an `.ico` and an SVG, not the PNG sizes MSIX wants. Render PNGs from
`snipdeck-icon.svg` into a `packaging\msix\Assets\` folder (do not commit generated output
unless the owner asks):

| File | Size |
|---|---|
| `StoreLogo.png` | 50x50 |
| `Square44x44Logo.png` | 44x44 (also add `Square44x44Logo.targetsize-24_altform-unplated.png` at 24x24 so the taskbar icon is not boxed) |
| `Square150x150Logo.png` | 150x150 |
| Scale variants | `.scale-125`, `.scale-150`, `.scale-200`, `.scale-400` of each, if you want sharp icons on high-DPI screens |

The Store listing also asks for its own images (screenshots, and a 300x300 logo at the time of
writing). *Check before relying on it.* Use `docs/images/` for screenshots.

### Build an unsigned or test-signed package

**In CI.** `release.yml` builds `Snipdeck-<version>-x64.msix` when all three repository variables are set
(Settings > Secrets and variables > Actions > Variables), with the values from Partner Center > Product identity:

| Variable | Value |
|---|---|
| `MSIX_IDENTITY_NAME` | `Package/Identity/Name` |
| `MSIX_PUBLISHER` | `Publisher` (`CN=...`) |
| `MSIX_PUBLISHER_DISPLAY_NAME` | `PublisherDisplayName` |

If any is empty the step prints a notice and skips. The logos are rendered on Linux (`cairosvg` is hard to
install on the Windows runner) and handed to the Windows job. The MSIX is unsigned (the Store signs it) and is
kept as the workflow artifact `windows-msix` for 30 days. It is **not** attached to the GitHub Release.

**By hand,** on a Windows PC with the Windows SDK (it provides `MakeAppx.exe` and `SignTool.exe` under
`C:\Program Files (x86)\Windows Kits\10\bin\<version>\x64\`):

1. Build the release exe: `cargo build --release -p snipdeck-app --locked` (needs `SNIPDECK_LICENCE_PUBLIC_KEY`,
   see [`licensing.md`](licensing.md)).
2. Stage a folder `dist\msix\` with `snipdeck.exe` (the renamed exe), `Assets\*.png`
   (`python3 scripts/make_msix_assets.py`) and the filled manifest:
   `python3 scripts/msix_manifest.py --identity-name ... --publisher "CN=..." --publisher-display-name ... --version 1.0.0 --out dist\msix\AppxManifest.xml`.
3. `MakeAppx.exe pack /o /d dist\msix /p dist\Snipdeck.msix`.

### Sideload test with a self-signed certificate

Use a **test** certificate whose subject equals the manifest's `Publisher`. Its private key stays on your PC.

```powershell
$cert = New-SelfSignedCertificate -Type Custom -Subject "CN=SnipdeckTest" -KeyUsage DigitalSignature `
  -FriendlyName "Snipdeck test" -CertStoreLocation "Cert:\CurrentUser\My" `
  -TextExtension @("2.5.29.37={text}1.3.6.1.5.5.7.3.3", "2.5.29.19={text}")
# Build the package with --publisher "CN=SnipdeckTest", then sign it:
signtool sign /fd SHA256 /a /sha1 $cert.Thumbprint dist\Snipdeck.msix
# Trust the certificate (administrator PowerShell), then install:
Export-Certificate -Cert $cert -FilePath dist\SnipdeckTest.cer
Import-Certificate -FilePath dist\SnipdeckTest.cer -CertStoreLocation Cert:\LocalMachine\TrustedPeople
Add-AppxPackage dist\Snipdeck.msix
```

Then check: Start menu entry and icon, tray, the Panel hotkey, single instance, paste into another app, the
Start with Windows toggle (Settings > Apps > Startup shows Snipdeck), and where the data folder is. Remove it with
`Get-AppxPackage *Snipdeck* | Remove-AppxPackage` and remove the test certificate from Trusted People afterwards.

For the Store you upload the package unsigned or with any signature: the Store signs it with its own
certificate on publication. *Check before relying on it* (Partner Center may ask for a `.msixupload`
or `.msixbundle`, which Visual Studio or `MakeAppx bundle` creates).

### Microsoft Store route

| Step | Detail |
|---|---|
| 1. Account | Partner Center, **individual** developer account. One-off registration fee: HANDOVER notes US$19. *Check before relying on it* (fee, identity checks, how long verification takes). |
| 2. Reserve the name | Apps and games > New product > MSIX or PWA app. Reserve `Snipdeck`. The name is held for a limited time if you do not submit. *Check before relying on it.* |
| 3. Copy the identity | Product management > Product identity. Put `Package/Identity/Name`, `Publisher` and `PublisherDisplayName` into the manifest. |
| 4. Build | As above, with the real identity values and a version higher than any earlier upload. |
| 5. Submit | Start a submission, upload the package, set price (paid or free), age rating, Store listing text and screenshots, privacy policy link ([`privacy.md`](privacy.md)), and notes for certification. |
| 6. Certification | Describe `runFullTrust` plainly in the notes: the app reads and writes the clipboard, registers global hotkeys and pastes into other apps. |
| 7. After it is live | Put the Store link in the About window constant and the README (see [`going-public.md`](going-public.md) section 5). |

Supporter pricing for the Store build is a product decision, not a packaging one. Do not add a
second licence path for it here.

## 2. Code signing

Windows shows SmartScreen and "Unknown publisher" warnings for unsigned downloads. Signing names
the publisher. It does not remove the warning on day one.

| | SignPath Foundation | Azure Trusted Signing |
|---|---|---|
| Cost | Free for open-source projects | Paid per month (*check before relying on it*) |
| Fit | Snipdeck is GPL-3.0 and public | Works for any publisher; had country and identity limits at launch (*check before relying on it*) |
| Who holds the key | SignPath, in an HSM. You never hold it. | Microsoft, in a managed service. You never hold it. |
| Certificate shown to users | "SignPath Foundation" as the publisher, not your name | Your verified name or organisation |
| Signing from CI | GitHub Action sends the artifact, waits for the signed file | Azure sign step in the workflow |
| Choice | **First choice** | Alternative if SignPath declines or the name matters |

The product was renamed since launch (it may now be called Artifact Signing). Search for both names.

### SignPath Foundation: steps

1. Read the programme terms at signpath.org and check Snipdeck meets them (open-source licence,
   public repository, a release process in CI, a project page that states the signing policy).
   *Check before relying on it.*
2. Apply as the owner. Expect a wait for review.
3. When accepted, SignPath creates an organisation, a project, an artifact configuration and a
   signing policy. Install their GitHub App, and add the API token as a repository secret.
   The private key never leaves SignPath. (For anything of ours, the rule stays: the private key
   never leaves the owner's machine.)
4. Add a short signing statement to the README and the site: who signs, and that only builds from
   this repository's workflow are signed.

### What changes in the release workflow

Describe only. Do not edit `.github/` from this task; make these changes in a separate, reviewed PR.

| Today | With signing |
|---|---|
| One job builds the exe, zips it, hashes it, attests it, attaches it | Split into build, sign, package |
| The exe goes straight into the zip | Build job uploads `snipdeck.exe` as an artifact. The sign step sends it to SignPath and downloads the signed exe. The zip is then made from the **signed** exe. |
| Zip hash made after the build | Hash and attestation made after signing, so they cover the signed bytes |
| `release-sha256.txt` for the launcher | Update it from the signed zip, not the unsigned one (see `going-public.md` section 4) |
| No MSIX | An extra step runs `MakeAppx pack` on the signed exe and attaches `Snipdeck.msix` to the release. For the GitHub copy, sign the `.msix` too, with the publisher in `Identity Publisher` set to the certificate subject (SignPath's, for the Foundation route). The Store copy keeps the Partner Center publisher. |
| Runs on tags only for releases | Signing runs on `v*` tags only. Never sign dev builds. |

Two manifests are needed because the publisher differs: the Store build uses the Partner Center
identity, the GitHub MSIX uses the signing certificate's subject. A sideloaded MSIX also needs the
certificate chain to be trusted on the user's PC, which is why the zip stays the main download.

Signing and the `SNIPDECK_LICENCE_PUBLIC_KEY` check are independent. Keep the existing "a release
cannot embed the development key" guard in the build job.

### Timestamping

Always timestamp. A timestamp keeps the signature valid after the certificate expires. SignPath
and Azure add one by default (*check before relying on it*). If you ever sign by hand with
`SignTool`, add `/tr <RFC 3161 server> /td sha256 /fd sha256`.

### SmartScreen expectations

- A signed file with no history can still trigger a SmartScreen warning at first. Reputation builds
  as people download and run it. Do not promise users it goes away at a given number.
- Reputation attaches to the signing certificate and the file. A shared SignPath certificate may
  start with more reputation than a new one, but this is not guaranteed. *Check before relying on it.*
- A new release of the same exe may restart the file's reputation. Release sparingly and tell
  users in the README how to verify the download (SHA-256, `gh attestation verify`).
- MSIX installed from the Store does not show SmartScreen's download warning.
- Antivirus false positives can still happen for a small unsigned or newly signed exe. Submit
  false positives to the vendor; do not ask users to disable protection.

## What is unverified until run on Windows

Nothing in this page has been run. Tick each when the owner has done it.

- [ ] `MakeAppx pack` accepts the manifest and produces `Snipdeck.msix`.
- [ ] The test-signed package installs, starts, and shows the right name and icon in Start, the taskbar and Settings > Apps.
- [ ] Tray icon, global hotkeys, single-instance lock and paste into other apps work inside the package (full trust).
- [ ] Where the packaged app keeps its data folder, and what survives uninstall and update.
- [ ] Start with Windows in the package: the `StartupTask` `SnipdeckStartup` toggles from the app and Settings > Apps > Startup.
- [ ] Setup .exe: installs without a prompt, upgrades over an older version (and over one that ignores `--quit`), restarts Snipdeck after a silent upgrade, uninstalls, and the erase question deletes only `%LOCALAPPDATA%\Snipdeck`. `iscc` has never been run on this script.
- [ ] The update check is off in the packaged build.
- [ ] Partner Center: account verification, name reservation, upload format, certification of `runFullTrust`, current fee.
- [ ] SignPath Foundation: acceptance, required wording, the artifact configuration, signed exe passes `signtool verify /pa`, publisher name shown in file properties.
- [ ] Signed zip: SmartScreen behaviour on a clean PC, and the hash in `release-sha256.txt` matches.
- [ ] Azure Trusted Signing (if used): eligibility, price, region, the Actions step.
