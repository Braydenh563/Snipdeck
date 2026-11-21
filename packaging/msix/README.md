# MSIX packaging: manifest template and logos

Owner steps for the Store or a test package. Background and the Partner Center route are in
[`docs/packaging.md`](../../docs/packaging.md), section 1.

Files here:

- `AppxManifest.xml`: the manifest template. It has `{{PLACEHOLDER}}` tokens and does not build as it is.
- `.gitignore`: ignores the generated `Assets/` folder.
- `scripts/make_msix_assets.py` (in the repo's `scripts/`): renders the logo PNGs into `Assets/`.

## Fill the template

Fill a **copy**, for example `dist/msix/AppxManifest.xml`. Do not edit the template in git.

| Token | Value |
|---|---|
| `{{IDENTITY_NAME}}` | Partner Center, Product management > Product identity > `Package/Identity/Name`. For a test build, any name such as `YourPublisher.Snipdeck`. |
| `{{PUBLISHER}}` | Partner Center `Publisher` (`CN=...`). For a test build, the subject of your test certificate. It must match the signing certificate exactly. |
| `{{PUBLISHER_DISPLAY_NAME}}` | Partner Center `PublisherDisplayName`. |
| `{{VERSION}}` | Four parts, from `Cargo.toml` plus `.0`, for example `0.1.0.0`. Raise it for every upload. Keep the last part `0`. |

`MaxVersionTested` in `Dependencies` is `10.0.26300.0`, the owner's Windows 11 PC on the first Windows run
(`docs/spikes/windows-results-20261017.md`). Raise it only after a run on a newer build. `MinVersion` is
Windows 10 1903, the same as the installer; no Windows 10 run exists yet.

The manifest declares the `StartupTask` (`TaskId="SnipdeckStartup"`, disabled until the user turns on
"Start with Windows"). The app drives it through the WinRT `StartupTask` API. Keep the id in step with the
app; `scripts/installer_check.py` checks it. AppData virtualisation stays on: the Store build's data is per
package and goes when the app is uninstalled.

`scripts/msix_manifest.py` fills the tokens for you (CI uses it):

```
python3 scripts/msix_manifest.py --identity-name Pub.Snipdeck --publisher "CN=..." \
  --publisher-display-name "Name" --version 1.0.0 --out dist/msix/AppxManifest.xml
```

## Make the logos

```
pip install cairosvg
python3 scripts/make_logo.py            # only if assets/logo/snipdeck-icon.svg is missing
python3 scripts/make_msix_assets.py
```

The script reads `assets/logo/snipdeck-icon.svg` and writes 16 PNGs into `packaging/msix/Assets/`:
`StoreLogo` (50 px), `Square44x44Logo` (44 px), `Square150x150Logo` (150 px), a 24 px unplated
taskbar icon, and `.scale-125`, `.scale-150`, `.scale-200`, `.scale-400` for the three base logos.

Plus the target-size set of the 44 px logo (`Square44x44Logo.targetsize-16`, `-24`, `-32`, `-48`, `-256`, each
also as `_altform-unplated`). That is why it writes more than 16 files; the exact list is in the script.

It needs cairosvg and the native cairo library. If either is missing, it prints what to install and
writes nothing. On Windows, cairo is the hard part, so running it on Linux or macOS is simpler.

Do not commit the PNGs. Copy `Assets\` into the staging folder with the filled manifest and the exe.

## Build `resources.pri` (makepri), then pack

Windows picks the scale and target-size files only through a resource index. Build it from the staging folder
after the manifest and `Assets\` are there, and before `MakeAppx pack`. Keep `priconfig.xml` outside the folder:

```powershell
makepri createconfig /cf dist\priconfig.xml /dq en-US /o
makepri new /pr dist\msix-stage /cf dist\priconfig.xml /mn dist\msix-stage\AppxManifest.xml /of dist\msix-stage\resources.pri /o
makeappx pack /o /d dist\msix-stage /p dist\Snipdeck.msix
```

`makepri.exe` and `makeappx.exe` sit side by side in the Windows SDK (`...\Windows Kits\10\bin\<version>\x64\`).
Never run it on the template: a leftover `{{TOKEN}}` stops it. For an ARM64 package fill the manifest with
`--arch arm64` and stage the ARM64 exe.

## File type, link and alias

The manifest registers the `.snipdeck` file type, the `snipdeck:` scheme and the `snipdeck` command-line alias,
and `packaging/inno/snipdeck.iss` the first two. Both come from `packaging/registration.json`; change the table
first, then the two files, and `scripts/installer_check.py` tells you if one is behind.

## Check the XML

```
python3 -c "import xml.dom.minidom,sys;xml.dom.minidom.parse('packaging/msix/AppxManifest.xml')"
```

This only checks that the XML is well formed. It does not check the schema or the tokens.
`MakeAppx pack` on Windows is the real check. It has not been run yet: see the list at the end of
`docs/packaging.md`.
