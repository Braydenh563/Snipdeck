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

Also check `MinVersion` and `MaxVersionTested` in `Dependencies`. They should name the Windows builds
you actually tested on. The values in the template are placeholders, not a test result.

Do not add a `startupTask` extension yet. The manifest comment says why.

## Make the logos

```
pip install cairosvg
python3 scripts/make_logo.py            # only if assets/logo/snipdeck-icon.svg is missing
python3 scripts/make_msix_assets.py
```

The script reads `assets/logo/snipdeck-icon.svg` and writes 16 PNGs into `packaging/msix/Assets/`:
`StoreLogo` (50 px), `Square44x44Logo` (44 px), `Square150x150Logo` (150 px), a 24 px unplated
taskbar icon, and `.scale-125`, `.scale-150`, `.scale-200`, `.scale-400` for the three base logos.

It needs cairosvg and the native cairo library. If either is missing, it prints what to install and
writes nothing. On Windows, cairo is the hard part, so running it on Linux or macOS is simpler.

Do not commit the PNGs. Copy `Assets\` into the staging folder with the filled manifest and the exe.

## Check the XML

```
python3 -c "import xml.dom.minidom,sys;xml.dom.minidom.parse('packaging/msix/AppxManifest.xml')"
```

This only checks that the XML is well formed. It does not check the schema or the tokens.
`MakeAppx pack` on Windows is the real check. It has not been run yet: see the list at the end of
`docs/packaging.md`.
