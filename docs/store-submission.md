# Microsoft Store submission checklist

Read on 2026-11-14 from Microsoft Learn (hub, account, submission, certification, policies v7.20 effective
2026-10-22). Compared with `docs/packaging.md`, `docs/going-public.md`, `docs/privacy.md`, ADR-0006,
`packaging/` and the code. Nothing here was run on Windows. Anything marked **Check before relying on it** was
not confirmed from a page read today.

Source base `B` = `https://learn.microsoft.com/en-us/windows/apps/publish`. Each row ends with a path under `B`
unless it is a full URL.

Status key: **done** (with a file), **gap** (repo work or a decision is missing), **owner** (only the owner can do
it, in Partner Center or on Windows).

## 0. Choose the route

| Requirement | Snipdeck status | Source |
|---|---|---|
| MSIX is Microsoft's recommended format. The Store signs it for free, hosts it, and supports flighting and Store commerce. | done: MSIX is the Store route (`docs/packaging.md` section 1, `packaging/msix/AppxManifest.xml`). | `B`/get-started |
| The MSI/EXE route needs an HTTPS URL to an offline installer that never changes. | done in design: `Snipdeck-Setup-<ver>.exe` is offline and per-user. Second route only. | `B`/publish-your-app/msi/app-certification-process |
| MSI/EXE installers are not re-signed by the Store. Policy 10.2.9 says the installer and all its PE files must be signed with a CA in the Microsoft Trusted Root Program. | gap: nothing is signed yet (SignPath not applied for). The certification page only says "highly recommended", but the policy says "must". Treat signing as required. | `B`/store-policies (10.2.9); `B`/publish-your-app/msi/app-certification-process |
| If you ship both an unpackaged web version and a Store MSIX, plan how to stop double installs or move users across. | gap: no plan. The Store build keeps data per package, not in `%LOCALAPPDATA%\Snipdeck`, so a user who switches loses nothing but also sees an empty library. Document an export/import step. The Learn page on this topic could not be fetched. **Check before relying on it.** | `B`/get-started (note under MSIX) |

## 1. Developer account (Partner Center)

| Requirement | Snipdeck status | Source |
|---|---|---|
| Open a developer account at `https://storedeveloper.microsoft.com` (the only entry for the current flow). Individual needs a government ID and a selfie. | owner: not started. | `B`/partner-center/open-a-developer-account |
| There is no registration fee on the new flow. | gap: `docs/packaging.md` step 1 still says "US$19 ... check". Replace with "no fee via storedeveloper.microsoft.com". | `B`/partner-center/open-a-developer-account |
| Individual accounts are for people whose Store work is not part of a business, trade or profession. A reasonable-consumer test also applies to the publisher name (policy 10.14). Individual cannot be changed to Company later. | owner: decide before registering. Supporter sells keys for A$5 and up, which may count as trade. **Check before relying on it** (Microsoft does not define the line). | `B`/store-policies (10.14); `B`/prepare-developer-account |
| Policy 10.8.3: individual accounts cannot ship apps that need financial account data for main function. | done: Snipdeck asks for none. | `B`/store-policies |
| Start early. Manual verification can take 2 to 5 business days. | owner. | `B`/prepare-developer-account |
| Publishing from the command line (`msstore` CLI, GitHub Actions) needs an Entra ID tenant on the account. | owner: only if you want automated submissions. Not needed for a first manual submission. | `B`/prepare-developer-account |
| Verify the email in Action Center (My Preferences) so certification mail arrives. | owner. | `B`/publish-your-app/msix/manage-submission-options |

## 2. Reserve the name

| Requirement | Snipdeck status | Source |
|---|---|---|
| New product, MSIX or PWA app, check availability, Reserve. | owner. | `B`/publish-your-app/msix/reserve-your-apps-name |
| A reservation is removed if unused for three months. | gap: `docs/packaging.md` says "limited time". Say "three months". Reserve close to submission. | same |
| Name: up to 256 characters, unique, no trademarks, no version or date suffix, no emoji or special characters. Policy 10.1.1: no marketing or keyword text in the title. | done: "Snipdeck". Owner should search the Store and trademark registers for clashes. | same; `B`/store-policies (10.1.1) |
| Copy `Identity Name`, `Publisher`, `PublisherDisplayName` from Product identity into the package. A mismatch fails upload. | done: three CI variables in `docs/packaging.md` section 1, filled by `scripts/msix_manifest.py`. owner: set the variables. | `B`/store-readiness-checklist |

## 3. Package (MSIX)

| Requirement | Snipdeck status | Source |
|---|---|---|
| Accepted files: `.msix`, `.msixbundle`, `.msixupload`, `.appx`, `.appxbundle`, `.appxupload`. `.msixupload` is recommended and carries symbols for crash reports. | done: `Snipdeck-<ver>-x64.msix` built in `release.yml` when variables are set. gap: no `.msixupload`, so Partner Center health/crash data will have no symbols. Optional. **Check before relying on it** (the page's advice names UWP). | `B`/publish-your-app/msix/app-package-requirements; `B`/publish-your-app/msix/upload-app-packages |
| Package under 25 GB. Block map hashes SHA-256. File names ANSI. | done: small package from MakeAppx. | `B`/publish-your-app/msix/app-package-requirements |
| Version has four parts; the fourth is reserved for the Store and must be `0`. Raise it for every upload. | done: manifest template and `docs/packaging.md`. | same |
| Build in Release, not Debug. | done: release build. | `B`/store-readiness-checklist |
| Signing: the Store re-signs MSIX. No certificate needed for Store upload. | done: the MSIX is left unsigned in CI. | `B`/publish-your-app/msix/app-certification-process |
| `TargetDeviceFamily` `MinVersion` and `MaxVersionTested` must name builds you tested. | gap: `MaxVersionTested="10.0.22621.0"` is a placeholder (the README says so). owner: set after a Windows run. | `B`/publish-your-app/msix/app-package-requirements |
| Manifest images must exist, be valid PNG, be under 200 KB each, define either Scale or TargetSize (not both), and have one variant without TargetSize. They must not be template defaults. | done in design: `scripts/make_msix_assets.py` writes 16 PNGs from the Snipdeck icon. owner: run WACK to confirm. | `B` / `https://learn.microsoft.com/en-us/windows/uwp/debug-test-perf/windows-desktop-bridge-app-tests` |
| No `.pfx` or `.snk` in the package. No services or drivers (registry check). No UAC elevation or UIAccess. No `DesktopApplicationPath` IPC. No file-type association spam. | done: manifest has none of these; the Inno `.iss` is not in the MSIX. | same Desktop Bridge test page |
| Only one application in the package. | done: one `Application Id="Snipdeck"`. | same |
| Optional WACK warning: all PE files signed. Informational for MSIX. | gap (low): the packaged `snipdeck.exe` is unsigned; Store re-signing covers it. | same |
| Manifest declares only languages you ship. Policy 10.7: localise the product for every language it supports and localise the Store text for each language you declare. | gap: the manifest says only `en-us`, but the app ships 7 UI locales (`@tr`). Either add `Resource` entries and listings per locale, or state English-only listing. Decide. **Check before relying on it** (Store behaviour when the app has more locales than the manifest). | `B`/store-policies (10.7); `B`/publish-your-app/msix/app-package-requirements |
| Run the Windows App Certification Kit before every submission. | done in docs: `docs/packaging.md` step 4b. owner: run it on Windows. | `B`/store-readiness-checklist |
| Test on a clean device or VM, with keyboard and a screen reader. | owner: HANDOVER lists Narrator steps 78-86 as unverified. | `B`/store-readiness-checklist |

## 4. Restricted capability: runFullTrust

| Requirement | Snipdeck status | Source |
|---|---|---|
| A packaged Win32 (medium integrity) app must declare `runFullTrust` with the `rescap` namespace. Declare only what you use (policy 10.6). | done: the manifest has `rescap:Capability Name="runFullTrust"` and nothing else. | `https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/app-capability-declarations` |
| Partner Center asks, on Submission options, why each restricted capability is declared and how it is used. Testers review it. It can add time. Approval is usually kept for later updates. | gap: no justification text drafted. See "Certification notes wording" below. | `B`/publish-your-app/msix/manage-submission-options |
| You can pre-request approval before submitting. | owner: optional. | app-capability-declarations page (Restricted capability approval process) |

## 5. Submission: Pricing and availability

| Requirement | Snipdeck status | Source |
|---|---|---|
| Choose Free or a price tier, markets, visibility, schedule, free trial, sale price, organisational licensing. | owner: decide price. The Store build and Supporter are an open product decision (`docs/packaging.md` last paragraph of section 1). | `B`/publish-your-app/msix/price-and-availability |
| Commerce choice: Microsoft's platform (15% on apps) or your own platform (100% kept for non-game apps). | owner: tied to the Supporter decision in Policy risk 5. | `B`/get-started |
| Policy 10.8.4/10.8.7: say what is paid, the price range, and the terms of any trial. Do not price irrationally high. | gap: no Store wording yet. | `B`/store-policies |

## 6. Submission: Properties

| Requirement | Snipdeck status | Source |
|---|---|---|
| Category (and optional secondary). | owner: pick one, for example Productivity. | `B`/publish-your-app/msix/enter-app-properties |
| Privacy policy URL. Win32 and Desktop Bridge apps always need one, even if they collect nothing (policy 10.5.1; readiness checklist). Keep it current. | gap: `docs/privacy.md` exists, but the repo is private and `site/index.html` only has a Privacy section. A public URL is needed (the going-public repo URL, or a Pages page). | `B`/publish-your-app/msix/support-info; `B`/store-readiness-checklist |
| Support contact URL or email (optional but recommended). Website URL (own site, not the Store listing). | gap: choose the GitHub issues URL or an email. Do not use the Store page as the website. | `B`/publish-your-app/msix/support-info |
| Phone and address are for regulators and not shown. Optional for individuals. | owner. | same |
| Product declarations: three boxes are ticked by default (see below). | owner. | `B`/publish-your-app/msix/product-declarations |
| Declaration "tested to meet accessibility guidelines": tick only after keyboard, 4.5:1 contrast, Accessibility Insights/Inspect, Narrator, Magnifier, High Contrast and High DPI checks. | gap: Narrator and High Contrast runs are unverified on Windows. Leave unticked until done. | same |
| Declaration "does not use the Microsoft Store commerce system" | owner: tick only if Supporter keys are bought outside the Store (Policy risk 5). | same |
| Declaration "Windows can include this product's data in automatic backups to OneDrive" (ticked by default). | gap: decide. Sensitive items are DPAPI-keyed to one PC (`docs/crypto.md`), so a restored copy on another PC would not open them, and `docs/privacy.md` says the data does not roam. Suggest unticking. **Check before relying on it** (how Windows backs up packaged app data). | same |
| Declaration "install to alternate drives" (ticked by default). | owner: leave ticked unless there is a reason. | same |
| Generative AI declaration. | done: Snipdeck has no generative AI. Leave unticked. | same; `B`/store-policies (11.16) |
| System requirements (hardware). | owner: none special; Windows 10 1903 or newer, 64-bit. | `B`/publish-your-app/msix/enter-app-properties |

## 7. Submission: Age ratings

| Requirement | Snipdeck status | Source |
|---|---|---|
| Complete the IARC questionnaire in Partner Center. Publisher display name and email are shared with IARC. Keep it accurate for updates (policy 11.11). | owner. Expected answers: utility, no violence, no online user content, no purchases through the Store unless Supporter goes through it. | `B`/publish-your-app/msix/age-ratings |

## 8. Submission: Store listing

| Requirement | Snipdeck status | Source |
|---|---|---|
| Minimum to complete the step: one language, a description, and one screenshot. | gap: no listing text written. | `B`/publish-your-app/msix/add-and-edit-store-listing-info |
| Description: up to 10,000 plain-text characters. No HTML, code or URLs; use the support and privacy fields. | gap: write it. Avoid the URL-heavy README wording. | same |
| Product features: up to 20 lines, 200 characters each, no bullets typed. Short description: up to 1,000 characters (270 shown). What's new: 1,500. | gap: write them. | same |
| Search terms: at most 7, relevant, no price words, no other products' names (policy 10.1.3). | gap. | `B`/store-policies |
| Metadata must say what the app does and its limits (10.1.1): Supporter-only features, opt-in typed expansion, Windows 10 1903+ x64, and that the Recent inbox watches the clipboard. | gap. | same |
| Screenshots: PNG, up to 10 for desktop, 50 MB max, **1366x768 or larger**. Keep key content in the top two-thirds. No added logos or marketing text. Caption up to 200 characters. | partly done: `docs/images/` has light and dark PNGs. Only `hero`, `main`, `main-tile` and `settings` are 1366 wide or more. `panel`, `editor`, `fill` and `picker` are narrower and do not meet the minimum (the page's wording on width vs height is ambiguous). **Check before relying on it.** Also check the hero image for text overlays. | `B`/publish-your-app/msix/screenshots-and-images |
| Store logo 1:1 app tile icon 300x300 PNG: recommended; it is used over the package icon. The 2:3 poster, 1:1 box art and 16:9 super-hero images are mainly for games. The page and its FAQ disagree on whether a logo is mandatory. | gap: no 300x300 PNG. Make one from `assets/logo/snipdeck-icon.svg`. Also prepare a 1920x1080 hero without text or app UI as an optional extra. | same |
| Trailers are optional: MP4 or MOV, 1920x1080, under 2 GB, 60 s or less recommended, with a 1920x1080 PNG thumbnail and a title; no age rating in the video. A 16:9 hero is needed for the trailer to appear on top. | done: none planned. | same |
| Screenshots must be real and current. Do not show third-party logos or content you may not use (policy 11.2). | gap: `demo.rs` contains `figma.com` sample links; avoid showing them in screenshots. | `B`/store-readiness-checklist; `B`/store-policies |
| Text and images must fit PEGI 12 or lower (11.1). | done: no conflict seen. | `B`/store-policies |

## 9. Submission options and certification notes

| Requirement | Snipdeck status | Source |
|---|---|---|
| Notes for certification: test accounts, steps to reach hidden or locked features, expected differences, anything testers need. A person reads them. Keep them short. | gap: draft below. | `B`/publish-your-app/msix/manage-submission-options |
| Restricted capability justification goes on this page too. | gap: draft below. | same |
| Publishing hold: default is publish as soon as certified. A manual hold or a date at least 24 hours ahead is available. | owner: choose "do not publish until I select Publish now" for the first release. | same |
| Notification audience. | owner. | same |

## 10. Certification and WACK

| Requirement | Snipdeck status | Source |
|---|---|---|
| Certification takes up to three business days: security scan, technical compliance (WACK tests), content compliance. After publishing, listing is visible in about 15 minutes. Spot checks continue after release. | owner. | `B`/publish-your-app/msix/app-certification-process |
| Avoid common failures: finished app, offline behaviour (no crash without network), privacy policy URL, accurate description, honest age rating, no false accessibility claim. | done: the app makes no network request (`docs/privacy.md`; `help_binding.rs` opens a page, fetches nothing). Others: gap or owner as above. | `B`/publish-your-app/msix/resolve-submission-errors |
| WACK on the real `.msix`, with every FAILED fixed and WARNINGs explained. | owner: step 4b in `docs/packaging.md`. | `https://learn.microsoft.com/en-us/windows/uwp/debug-test-perf/windows-app-certification-kit` |
| A failed submission returns a report naming the test or policy; fix and resubmit. Questions: `reportapp@microsoft.com`. | owner. | `B`/publish-your-app/msix/resolve-submission-errors |

## 11. Updates, analytics, flighting (brief)

| Requirement | Snipdeck status | Source |
|---|---|---|
| Each update is a new submission with a higher version. Metadata-only changes need no new package but still go through certification. | gap: `release.yml` keeps the MSIX only as a workflow artifact for 30 days; upload by hand. Optional later: `msstore` CLI. | `B`/publish-your-app/msix/publish-update-to-your-app-on-store |
| Gradual rollout and package flights are available for updates. | owner: optional. | `B`/publish-your-app/msix/upload-app-packages |
| Partner Center reports (acquisitions, health, usage, reviews) are Microsoft-side data. | gap: `docs/privacy.md` says "no telemetry". True for Snipdeck, but add a sentence that the Store itself reports installs and crashes to the publisher. | `B`/get-started |
| Store updates the app itself. The in-app "Check for updates" must not point to a rival path. | done: packaged builds open `ms-windows-store://downloadsandupdates` (`startup_task.rs:12`, `help_binding.rs:49`); other builds open the Releases page. No download in the app. | repo |

## 12. MSI or EXE route (second)

| Requirement | Snipdeck status | Source |
|---|---|---|
| Versioned HTTPS URL to a `.exe` or `.msi`. The file at the URL must never change. | done in docs: use the versioned GitHub Release asset URL (`docs/packaging.md`). GitHub release assets are mutable by the owner; do not re-upload. | `B`/publish-your-app/msi/upload-app-packages |
| Offline installer, no bundled software, no drivers or services. | done: Inno `.exe` is standalone. | `B`/publish-your-app/msi/app-certification-process |
| Silent install with standard-user account; EXE needs installer parameters (`/VERYSILENT`); optional return-code table. | done in design (`packaging/inno/snipdeck.iss`); gap: return codes not listed; `iscc` never run. owner. | `B`/publish-your-app/msi/upload-app-packages |
| Start menu entry and correct name, publisher, version in Installed apps. | done in script; owner to verify on Windows. | `B`/publish-your-app/msi/app-certification-process |
| Clean uninstall "without remnants". | gap: the library is kept on purpose after a silent uninstall. Say so in the notes (already in `docs/packaging.md`). | same |
| Sign the installer and every PE file with a trusted CA certificate (policy 10.2.9). | gap: not signed. SignPath Foundation or Azure Artifact Signing. | `B`/store-policies |
| Works offline; privacy policy URL; no third-party promotion in the installer. | done: no network use; no promotion. | `B`/publish-your-app/msi/app-certification-process |
| Same pricing, properties, age rating, listing steps; no restricted-capability step. | see sections 5 to 8. | `B`/get-started (MSI/EXE tab) |

## Policy risks

1. **10.2.3 and 10.2.8, typed-trigger expansion hook.** `typing_hook.rs` installs `WH_KEYBOARD_LL` and `WH_MOUSE_LL` and reads keys with `ToUnicodeEx` in any app. That looks like a keylogger to a malware scan or a tester. Why it is defensible: it is off by default, installed only while "Expand abbreviations as I type" is on, ignores injected keys, never logs or prints typed text, and stores nothing. Fix: keep it off by default in the Store build; say so in the notes and the listing; show a one-line consent dialog the first time the user turns it on. Notes wording below. Policy 10.2.8 (do not change Windows settings by unsupported methods) is not tripped: startup uses the supported `StartupTask`, user-toggled.
2. **10.5.1, 10.5.5, clipboard capture ("Recent inbox").** Snipdeck listens with `AddClipboardFormatListener` and keeps up to 200 copies and 7 days of text copied in other programs. It is on for a new install. Clipboard text can hold passwords or health or financial data. Nothing leaves the PC and entries are encrypted with DPAPI, but a tester can read "on by default" as collection without express consent. Fix: for the Store build either default it to off, or make the welcome tour an explicit yes/no step with plain wording; list it first in the privacy policy and the listing. Password-manager exclusions are already in `docs/privacy.md`.
3. **10.5.1, privacy policy.** A policy URL is required for any Win32 app. `docs/privacy.md` is good content but has no public URL, does not mention Windows Hello (it uses `KeyCredentialManager`, local only), the Store's own install and crash reporting, or the Store build's per-package data. Fix: publish a privacy page, add those three points, link it in Partner Center.
4. **10.1.1, accurate description and limits.** Supporter-only features, opt-in typing features, the clipboard watcher and x64-only support must be in the description. The README says "free covers everyday use" and "pay what you want". Fix: copy the same limits into the listing.
5. **10.8.1, 10.8.2, 10.8.4, Supporter outside the Store.** "Get Supporter" opens a dialog with a price and an "Email me" button; buyers get an offline licence key (`docs/licensing.md`). Policy 10.8.2 says a donation that gives features in return must use the Store in-product purchase API. Third-party purchase is allowed for non-game PC apps only with transaction-provider identification, authentication and confirmation, and it must be noted in Partner Center. An email button probably fails those. Fix, pick one before submitting: (a) Store build is Free only and hides "Get Supporter" and licence entry (the licence entry then needs a note); (b) sell Supporter as a Store add-on through `Windows.Services.Store` (new work); (c) keep the email flow and justify it in the notes. Option (a) is lowest risk. **Check before relying on it** (policy wording on voluntary payment is ambiguous). The "does not use Store commerce" declaration must match.
6. **10.14, account type.** Selling Supporter keys may count as trade. Individual cannot be converted. Decide with the owner before registering.
7. **10.2.9, unsigned Setup .exe (MSI/EXE route only).** Cannot pass as is. Fix: sign every PE with SignPath or Azure signing first. ADR-0006 also claims "no SmartScreen warning" after signing; `docs/packaging.md` says that is not guaranteed.
8. **10.6, 10.2.1 (Chromium rule is not relevant), runFullTrust.** Must be explained and may add review time. See notes below.
9. **11.2, third-party content.** Fonts, icons and sample data in screenshots must be licensed (`THIRD_PARTY_NOTICES.md` exists). Remove the `figma.com` demo link from screenshots.
10. **Accessibility claim.** Do not tick the accessibility declaration until a Narrator, keyboard and High Contrast pass is done on Windows.
11. **GPL-3.0 and Store terms.** Not covered by the pages read. **Check before relying on it** with the App Developer Agreement before publishing.

### Certification notes wording (draft, for the owner to adjust)

> Snipdeck is a local snippet deck. It needs no account, no server and no network. Notes:
> 1. Capability runFullTrust: Snipdeck is a Win32 app. It registers global hotkeys (RegisterHotKey) to show a panel, keeps a tray icon, reads and writes the clipboard, and sends Ctrl+V (SendInput) to the app the user was typing in, so a chosen snippet is pasted there. None of that works inside an AppContainer.
> 2. Optional features, off by default or opt-in: "Expand abbreviations as I type" and "Suggest snippets as I type" use a low-level keyboard hook only while turned on, never log or store typed text, and ignore other keys. "Recent inbox" keeps recent copied text on this PC only, encrypted, and can be turned off in Settings.
> 3. Start with Windows uses the StartupTask SnipdeckStartup and is off until the user turns it on.
> 4. Nothing is sent anywhere. Data is stored per package.
> 5. Locked features: Supporter features need a licence key. [Test key or note, owner to decide.]
> Date entered: [date].

## Code or asset work needed

Do not do these here. Each is a concrete repo change.

1. `docs/packaging.md`: replace "US$19" with "no fee via storedeveloper.microsoft.com"; say the name reservation lasts three months; add the restricted-capability justification and OneDrive backup decision; point to this file.
2. `docs/adr/0006-distribution.md`: update. It says "Paid Store listing", "MSI/MSIX on GitHub" and "in-app update check against GitHub Releases"; the code opens a page and fetches nothing, and the GitHub download is an Inno `.exe`. Also remove "no SmartScreen warning".
3. `docs/privacy.md`: add Windows Hello (local `KeyCredentialManager`), the Store's own install and crash reports, and per-package data in the Store build. Publish it at a public URL (Pages page under `site/` or the public repo blob URL).
4. Assets: a 300x300 `PNG` store logo from `assets/logo/snipdeck-icon.svg` (extend `scripts/make_msix_assets.py` or add a sibling script); check which `docs/images/*.png` are 1366x768 or larger; retake `panel`, `editor`, `fill` and `picker` at 1366x768 or larger if they should be Store screenshots; remove `figma.com` text from the demo data used in screenshots.
5. Listing text file (a new doc or CSV for Partner Center import): name, description (no URLs), 20 features, short description, 7 search terms, what's new, support and website URLs.
6. Typed-expansion hook (`crates/snipdeck-platform/src/windows/typing_hook.rs`, `crates/snipdeck-app/src/controller/trigger_ctl.rs`): add a first-turn-on consent line; confirm the default stays off in the packaged build.
7. Recent inbox: decide the default for the packaged build (`settings.rs`, welcome tour) so the first run asks explicitly.
8. Supporter in the Store build: pick option (a), (b) or (c) above; if (a), hide "Get Supporter" and licence entry when `package::is_packaged()` is true (`settings_binding.rs:224` already branches on it); if (b), a new Store add-on module.
9. `packaging/msix/AppxManifest.xml`: set `MaxVersionTested` from a real test; decide the `Resource` language list against the shipped locales; no other change seen.
10. `release.yml`: optional step to build `.msixupload` and attach the MSIX as an artifact for longer than 30 days; optional `msstore` publish step later (needs an Entra tenant).
11. Installer route: SignPath or Azure signing for `snipdeck.exe`, the installer and every other PE; list installer exit codes for Partner Center; confirm `/VERYSILENT` works as a standard user (owner on Windows).
12. Accessibility: run Narrator, keyboard, Magnifier and High Contrast checks before ticking the accessibility box; record them in `docs/HANDOVER.md`.
