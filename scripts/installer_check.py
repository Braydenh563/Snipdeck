#!/usr/bin/env python3
"""Static check of the Windows installer and MSIX template (stdlib only, runs on Linux).

  python3 scripts/installer_check.py

Fails on: AppId not the fixed GUID, PrivilegesRequired not lowest, AppMutex / [Code] mutex not matching
the app's single-instance name, a [Files] source missing from the repo (the exe may be absent), a
Run value not named Snipdeck, MinVersion that differs between the .iss and the MSIX template, and an
MSIX template whose StartupTask / Executable / Assets paths disagree with the app and the asset script.
It also checks: packaging/registration.json (the one table for the file association, the URL scheme and the
registry leftovers) against the .iss, the manifest and the launcher; a line for every Setup language on every
[CustomMessages] entry; the .iss encoding (UTF-8 with a byte-order mark); the downgrade guard, the hidden
relaunch and the guarded DelTree calls in [Code]; the ARM64 switch; the makepri step in the docs.
"""
import json
import pathlib
import re
import sys
import xml.etree.ElementTree as ET

ROOT = pathlib.Path(__file__).resolve().parent.parent
ISS = ROOT / "packaging/inno/snipdeck.iss"
MANIFEST = ROOT / "packaging/msix/AppxManifest.xml"
INSTANCE_NAMES = ROOT / "crates/snipdeck-platform/src/instance_names.rs"
PLATFORM = ROOT / "crates/snipdeck-app/src/platform.rs"
ASSETS_SCRIPT = ROOT / "scripts/make_msix_assets.py"
TABLE = ROOT / "packaging/registration.json"
LAUNCHER_DATA = ROOT / "tools/launcher/data.ps1"
APP_MANIFEST = ROOT / "crates/snipdeck-app/snipdeck.manifest"
APP_BUILD_RS = ROOT / "crates/snipdeck-app/build.rs"
# What the exe's own manifest must carry (PLATFORM-15), as (needle, why).
APP_MANIFEST_PARTS = (
    ("Microsoft.Windows.Common-Controls", "Common Controls v6"),
    ('version="6.0.0.0"', "Common Controls v6"),
    ("<longPathAware", "long paths"),
    (">true</longPathAware>", "long paths turned on"),
    ("8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a", "supportedOS Windows 10/11"),
    (">PerMonitorV2", "per-monitor v2 DPI awareness"),
    ('level="asInvoker"', "asInvoker"),
    (">UTF-8</activeCodePage>", "the UTF-8 code page"),
)
PACKAGING_DOCS = (ROOT / "docs/packaging.md", ROOT / "packaging/msix/README.md")
# Custom messages that Inno itself defines in every language file.
BUILTIN_MESSAGES = {"AdditionalIcons", "LaunchProgram"}

APP_ID_GUID = "5ACCE38D-D2C8-4BD3-88BD-9E76E5D0D402"
RUN_VALUE = "Snipdeck"
TASK_ID = "SnipdeckStartup"
EXE_NAME = "snipdeck.exe"


def sections(text):
    """{'Setup': [lines], ...}; comment and blank lines dropped; the preprocessor lines under ''."""
    out, cur = {"": []}, ""
    for raw in text.splitlines():
        line = raw.strip()
        if not line or line.startswith(";"):
            continue
        m = re.match(r"^\[(\w+)\]$", line)
        if m:
            cur = m.group(1)
            out.setdefault(cur, [])
        else:
            out.setdefault(cur, []).append(line)
    return out


def defines(text):
    """#define Name "value" or Name expr, with simple "A + "b"" concatenation, resolved in order."""
    values = {}
    for m in re.finditer(r'^\s*#define\s+(\w+)\s+(.+?)\s*$', text, re.M):
        name, expr = m.groups()
        parts = [p.strip() for p in expr.split("+")]
        built = ""
        for p in parts:
            if p.startswith('"') and p.endswith('"'):
                built += p[1:-1]
            elif p in values:
                built += values[p]
            else:
                built = None
                break
        if built is not None:
            values.setdefault(name, built)  # first (default) value wins
    return values


def expand(s, values):
    return re.sub(r"\{#(\w+)\}", lambda m: values.get(m.group(1), m.group(0)), s)


def setup_dict(lines):
    d = {}
    for line in lines:
        if line.startswith("#"):
            continue
        k, _, v = line.partition("=")
        d[k.strip()] = v.strip()
    return d


def entry_fields(line):
    """Parse an Inno parameter line: 'Key: "value"; Key: value; Flags: a b'."""
    fields = {}
    for m in re.finditer(r'(\w+):\s*("(?:[^"]|"")*"|[^;]*)', line):
        v = m.group(2).strip()
        if v.startswith('"') and v.endswith('"'):
            v = v[1:-1].replace('""', '"')
        fields[m.group(1)] = v
    return fields


def rust_mutex():
    """Expected mutex name: 'Snipdeck-' (instance_names.rs) + APP_ID (platform.rs)."""
    names = INSTANCE_NAMES.read_text(encoding="utf-8")
    plat = PLATFORM.read_text(encoding="utf-8")
    if not re.search(r'format!\(r"Local\\Snipdeck-\{app_id\}"\)', names):
        raise ValueError("instance_names.rs no longer builds Local\\Snipdeck-{app_id}")
    m = re.search(r'pub const APP_ID: &str = "([^"]+)";', plat)
    if not m:
        raise ValueError("APP_ID not found in platform.rs")
    return "Snipdeck-" + m.group(1)


def asset_files():
    """The PNG names scripts/make_msix_assets.py writes (read from its constants, no import)."""
    text = ASSETS_SCRIPT.read_text(encoding="utf-8")
    base = re.search(r"BASE\s*=\s*\((.*?)\n\)", text, re.S)
    stems = re.findall(r'\("(\w+)",\s*\d+\)', base.group(1))
    scales = re.findall(r"\d+", re.search(r"SCALES\s*=\s*\(([^)]*)\)", text).group(1))
    sizes = [int(n) for n in re.findall(r"\d+", re.search(r"TARGET_SIZES\s*=\s*\(([^)]*)\)", text).group(1))]
    target = re.search(r'TARGET_STEM\s*=\s*"(\w+)"', text).group(1)
    names = {f"{s}.png" for s in stems} | {f"{s}.scale-{p}.png" for s in stems for p in scales}
    for n in sizes:
        names.add(f"{target}.targetsize-{n}.png")
        names.add(f"{target}.targetsize-{n}_altform-unplated.png")
    return names, sizes


def check_encoding(errors, raw=None):
    """Inno reads a file with no byte-order mark as ANSI: non-ASCII text (the translations) needs the BOM."""
    raw = raw if raw is not None else ISS.read_bytes()
    if any(b > 127 for b in raw) and not raw.startswith(b"\xef\xbb\xbf"):
        errors.append("snipdeck.iss has non-ASCII text but no UTF-8 byte-order mark (Inno would read it as ANSI)")


def languages(text):
    """The Setup language names in [Languages]."""
    return [entry_fields(line).get("Name") for line in sections(text).get("Languages", [])]


def check_custom_messages(errors, text):
    """Every [CustomMessages] key has a non-empty line for every language, and every key used is defined."""
    langs = languages(text)
    if not langs:
        errors.append("no [Languages] entries found")
    found = {}
    for line in sections(text).get("CustomMessages", []):
        m = re.match(r"^(\w+)\.(\w+)=(.*)$", line)
        if not m:
            errors.append(f"[CustomMessages] line has no language prefix: {line[:60]}")
            continue
        found.setdefault(m.group(2), {})[m.group(1)] = m.group(3)
    for key, per in sorted(found.items()):
        for lang in langs:
            if not per.get(lang, "").strip():
                errors.append(f"[CustomMessages] {key} has no line for language {lang}")
        for lang in per:
            if lang not in langs:
                errors.append(f"[CustomMessages] {key} has a line for {lang}, which is not in [Languages]")
        # The same placeholders in every language (a lost %1 would show the wrong folder).
        shapes = {tuple(sorted(re.findall(r"%\d", v))) for v in per.values()}
        if len(shapes) > 1:
            errors.append(f"[CustomMessages] {key} uses different %1/%2 placeholders in different languages")
    used = set(re.findall(r"CustomMessage\('(\w+)'\)", text)) | set(re.findall(r"\{cm:(\w+)", text))
    for key in sorted(used - set(found) - BUILTIN_MESSAGES):
        errors.append(f"custom message {key} is used but not defined in [CustomMessages]")


def check_registration(errors, iss_text, table, manifest_text, launcher_text):
    """The .iss [Registry], the manifest and the launcher all follow packaging/registration.json."""
    entries = [entry_fields(line) for line in sections(iss_text).get("Registry", [])]
    ext, prog, scheme, exe = table["file_extension"], table["prog_id"], table["url_scheme"], table["exe"]
    command = table["open_command"].replace("{exe}", "{app}\\" + exe)

    def need(subkey, value_name, data, flag=None):
        hits = [e for e in entries if e.get("Subkey", "").lower() == subkey.lower() and e.get("ValueName", "") == value_name]
        if not hits:
            errors.append(f"snipdeck.iss [Registry] has no entry for {subkey} {value_name!r}")
            return
        e = hits[0]
        if data is not None and e.get("ValueData") != data:
            errors.append(f"snipdeck.iss [Registry] {subkey}: ValueData must be {data!r}; got {e.get('ValueData')!r}")
        if flag and flag not in e.get("Flags", ""):
            errors.append(f"snipdeck.iss [Registry] {subkey} needs Flags: {flag}")

    classes = "Software\\Classes\\"
    need(classes + ext, "", prog, "uninsdeletekey")
    need(classes + prog, "", table["file_type_name"], "uninsdeletekey")
    need(classes + prog + "\\shell\\open\\command", "", command)
    need(classes + scheme, "URL Protocol", "")
    need(classes + scheme, "", None, "uninsdeletekey")
    need(classes + scheme + "\\shell\\open\\command", "", command)
    if "ChangesAssociations=yes" not in iss_text:
        errors.append("snipdeck.iss [Setup] needs ChangesAssociations=yes (it registers a file association)")

    # Leftovers: exactly the table's, in Setup and in the launcher.
    want_values, want_keys = set(), set(table["setup_only_keys"])
    for item in table["registry_leftovers"]:
        if item["kind"] == "value":
            want_values.add((item["subkey"].lower(), item["name"]))
        else:
            want_keys.add(item["subkey"])
    got_values = {(e.get("Subkey", "").lower(), e.get("ValueName", "")) for e in entries if "uninsdeletevalue" in e.get("Flags", "")}
    got_keys = {e.get("Subkey", "") for e in entries if "uninsdeletekey" in e.get("Flags", "")}
    for v in sorted(want_values - got_values):
        errors.append(f"snipdeck.iss does not remove the leftover value {v[0]}\\{v[1]} on uninstall (registration.json lists it)")
    for v in sorted(got_values - want_values):
        errors.append(f"snipdeck.iss removes the value {v[0]}\\{v[1]}, which registration.json does not list")
    for k in sorted({k.lower() for k in want_keys} - {k.lower() for k in got_keys}):
        errors.append(f"snipdeck.iss does not remove the key {k} on uninstall (registration.json lists it)")
    for k in sorted({k.lower() for k in got_keys} - {k.lower() for k in want_keys}):
        errors.append(f"snipdeck.iss removes the key {k}, which registration.json does not list")
    launcher = set(re.findall(r"Kind = '(\w+)'; SubKey = '([^']+)'; Name = '([^']*)'", launcher_text))
    listed = {(i["kind"], i["subkey"], i.get("name", "")) for i in table["registry_leftovers"]}
    if launcher != listed:
        errors.append(f"tools/launcher/data.ps1 Get-RegistryLeftovers differs from registration.json: {sorted(launcher ^ listed)}")
    if table["app_execution_alias"] != exe:
        errors.append("registration.json app_execution_alias must equal the exe name")

    # The manifest.
    try:
        root = ET.fromstring(manifest_text.encode("utf-8"))
    except ET.ParseError:
        return  # check_manifest reports it
    types = [e.text for e in root.iter() if e.tag.endswith("}FileType")]
    protos = [e.get("Name") for e in root.iter() if e.tag.endswith("}Protocol")]
    aliases = [e.get("Alias") for e in root.iter() if e.tag.endswith("}ExecutionAlias")]
    if types != [ext]:
        errors.append(f"AppxManifest.xml FileType must be [{ext!r}]; got {types}")
    if protos != [scheme]:
        errors.append(f"AppxManifest.xml Protocol must be [{scheme!r}]; got {protos}")
    if aliases != [table["app_execution_alias"]]:
        errors.append(f"AppxManifest.xml ExecutionAlias must be [{table['app_execution_alias']!r}]; got {aliases}")


def check_code(errors, text):
    """[Code] rules from the audit: downgrade guard, hidden relaunch, DelTree never on a bare environment path."""
    code = text[text.find("\n[Code]\n"):]
    guid = re.search(r"UninstallKey\s*=\s*'[^']*\\\{([0-9A-Fa-f-]+)\}_is1'", code)
    if not guid or guid.group(1).upper() != APP_ID_GUID:
        errors.append("[Code] UninstallKey must name the AppId GUID (it is how Setup finds the installed version)")
    if "IsDowngrade" not in code or "DowngradeWarning" not in code or "ComparePackedVersion" not in code:
        errors.append("[Code] must compare versions in InitializeSetup and show DowngradeWarning for an older Setup")
    if '"{code:LaunchParams}"' not in text or "'--hidden'" not in code:
        errors.append("the [Run] entry must pass {code:LaunchParams} (--hidden when Snipdeck was running)")
    if re.search(r"DelTree\(\s*GetEnv", code):
        errors.append("[Code] DelTree must not take GetEnv(...) directly: an empty variable gives a path at the drive root")
    for call in re.findall(r"DelTree\(([^,]+),", code):
        if call.strip() not in ("Data", "RecordedData") and not re.match(r"(Profile|Tmp) \+ '", call.strip()):
            errors.append(f"[Code] DelTree({call.strip()}) is not one of the guarded paths")
    for var in ("Profile", "Tmp"):
        if not re.search(rf"if {var} <> '' then\s*\n\s*DelTree\({var} \+", code):
            errors.append(f"[Code] DelTree on {var} must be guarded by 'if {var} <> '''")
    if "SafeToErase" not in code or "DataKey" not in code:
        errors.append("[Code] must erase the recorded data folder (DataKey) through SafeToErase")


def check_arch(errors, text, setup):
    if not re.search(r'#ifdef Arm64\s*\n\s*#define Arch "arm64"\s*\n\s*#else\s*\n\s*#define Arch "x64compatible"', text):
        errors.append("the .iss must pick Arch arm64 with /DArm64 and x64compatible otherwise")
    for key in ("ArchitecturesAllowed", "ArchitecturesInstallIn64BitMode"):
        if setup.get(key) != "{#Arch}":
            errors.append(f"{key} must be {{#Arch}}; got {setup.get(key)!r}")


def check_app_manifest(errors, manifest_text=None, build_text=None):
    """The exe's embedded manifest must carry every part we rely on, and build.rs must embed it."""
    text = manifest_text if manifest_text is not None else APP_MANIFEST.read_text(encoding="utf-8")
    build = build_text if build_text is not None else APP_BUILD_RS.read_text(encoding="utf-8")
    try:
        ET.fromstring(text.encode("utf-8"))
    except ET.ParseError as err:
        errors.append(f"snipdeck.manifest is not well-formed: {err}")
        return
    for needle, why in APP_MANIFEST_PARTS:
        if needle not in text:
            errors.append(f"snipdeck.manifest lacks {why} ({needle!r})")
    if "set_manifest_file(MANIFEST)" not in build or 'MANIFEST: &str = "snipdeck.manifest"' not in build:
        errors.append("crates/snipdeck-app/build.rs must embed snipdeck.manifest with set_manifest_file")


def check_docs(errors, texts=None):
    """The packaging recipe must build resources.pri with makepri (the target-size icons need it)."""
    texts = texts if texts is not None else [p.read_text(encoding="utf-8") for p in PACKAGING_DOCS]
    for path, text in zip(PACKAGING_DOCS, texts):
        if "makepri new" not in text or "resources.pri" not in text:
            errors.append(f"{path.relative_to(ROOT)} must describe the makepri step that builds resources.pri")


def check_iss(errors, iss_text=None, root=ROOT, iss_dir=None, mutex=None):
    text = iss_text if iss_text is not None else ISS.read_text(encoding="utf-8-sig")
    iss_dir = iss_dir or ISS.parent
    sec = sections(text)
    setup = setup_dict(sec.get("Setup", []))
    vals = defines(text)

    if setup.get("AppId") != "{{" + APP_ID_GUID + "}":
        errors.append(f"AppId must be exactly {{{{{APP_ID_GUID}}}}} (it is the install identity, never change it); got {setup.get('AppId')!r}")
    if setup.get("PrivilegesRequired") != "lowest":
        errors.append("PrivilegesRequired must be lowest (per-user, no admin)")
    expected = mutex if mutex is not None else rust_mutex()
    if setup.get("AppMutex") != expected:
        errors.append(f"AppMutex must be {expected!r} (the app's single-instance mutex); got {setup.get('AppMutex')!r}")
    code_mutexes = set(re.findall(r"MutexName\s*=\s*'([^']*)'", text)) | set(re.findall(r"CheckForMutexes\('([^']*)'\)", text))
    if code_mutexes != {expected}:
        errors.append(f"[Code] mutex name must be {expected!r}; found {sorted(code_mutexes)}")
    if not re.search(r"#ifndef AppVersion\s*\n\s*#error", text):
        errors.append("the script must #error when /DAppVersion is missing")

    # [Files]
    exe_default = vals.get("SourceExe")
    if exe_default is None:
        errors.append("no default #define SourceExe")
    for line in sec.get("Files", []):
        src = entry_fields(line).get("Source", "")
        resolved = expand(src, vals)
        if "{#" in resolved:
            errors.append(f"[Files] Source uses an unknown define: {src}")
            continue
        path = (iss_dir / resolved.replace("\\", "/")).resolve()
        if src == "{#SourceExe}":
            continue  # the build output; absent until cargo has run
        if not path.exists():
            errors.append(f"[Files] Source does not exist: {src} -> {path}")
    if not any("DestName" in entry_fields(l) and entry_fields(l)["DestName"] == EXE_NAME for l in sec.get("Files", [])):
        errors.append(f"the exe must be installed as {EXE_NAME} (DestName)")

    # Setup paths that must exist
    for key in ("SetupIconFile",):
        v = expand(setup.get(key, ""), vals)
        if v and not (iss_dir / v.replace("\\", "/")).exists():
            errors.append(f"{key} does not exist: {v}")

    # [Registry] Run value
    run_ok = False
    for line in sec.get("Registry", []):
        f = entry_fields(line)
        if f.get("Subkey", "").lower().endswith(r"currentversion\run"):
            run_ok = True
            if f.get("ValueName") != RUN_VALUE:
                errors.append(f"the Run value must be named {RUN_VALUE!r}; got {f.get('ValueName')!r}")
            if "uninsdeletevalue" not in f.get("Flags", ""):
                errors.append("the Run value needs Flags: uninsdeletevalue")
            if f.get("ValueType") != "none":
                errors.append("the Run value entry must be ValueType: none (Setup writes nothing)")
    if not run_ok:
        errors.append("no [Registry] entry removes the HKCU Run value on uninstall")
    return setup


def check_manifest(errors, manifest_text=None, iss_setup=None):
    text = manifest_text if manifest_text is not None else MANIFEST.read_text(encoding="utf-8")
    try:
        root = ET.fromstring(text.encode("utf-8"))
    except ET.ParseError as err:
        errors.append(f"AppxManifest.xml is not well-formed: {err}")
        return
    tasks = [e for e in root.iter() if e.tag.endswith("}StartupTask")]
    if len(tasks) != 1 or tasks[0].get("TaskId") != TASK_ID:
        errors.append(f"AppxManifest.xml needs exactly one StartupTask with TaskId {TASK_ID!r}")
    for e in root.iter():
        if e.tag.endswith("}Extension") and e.get("Category") == "windows.startupTask" and e.get("Executable") != EXE_NAME:
            errors.append(f"the startupTask Executable must be {EXE_NAME}")
        if e.tag.endswith("}Application") and e.get("Executable") != EXE_NAME:
            errors.append(f"the Application Executable must be {EXE_NAME}")
    known, sizes = asset_files()
    if sorted(sizes) != [16, 24, 32, 48, 256]:
        errors.append(f"make_msix_assets.py TARGET_SIZES must be 16, 24, 32, 48, 256 (what Windows asks for); got {sizes}")
    used = set()
    for e in root.iter():
        for v in list(e.attrib.values()) + [e.text or ""]:
            v = v.strip()
            if v.startswith("Assets\\"):
                used.add(v[len("Assets\\"):])
    if not used:
        errors.append("AppxManifest.xml references no Assets\\ files")
    for name in sorted(used - known):
        errors.append(f"AppxManifest.xml uses Assets\\{name}, which make_msix_assets.py does not write")
    # MinVersion must agree with the installer.
    m = re.search(r'<TargetDeviceFamily[^>]*\bMinVersion="([^"]+)"', text)
    iss_min = (iss_setup or {}).get("MinVersion")
    if not m:
        errors.append("AppxManifest.xml has no TargetDeviceFamily MinVersion")
    elif iss_min is not None and m.group(1) != iss_min + ".0":
        errors.append(f"MinVersion differs: MSIX {m.group(1)!r} vs installer {iss_min!r} (+ '.0')")


def run():
    errors = []
    try:
        setup = check_iss(errors)
        check_manifest(errors, iss_setup=setup)
        iss_text = ISS.read_text(encoding="utf-8-sig")
        check_encoding(errors)
        check_custom_messages(errors, iss_text)
        check_code(errors, iss_text)
        check_arch(errors, iss_text, setup)
        check_registration(errors, iss_text, json.loads(TABLE.read_text(encoding="utf-8")),
                           MANIFEST.read_text(encoding="utf-8"), LAUNCHER_DATA.read_text(encoding="utf-8"))
        check_docs(errors)
        check_app_manifest(errors)
    except (OSError, ValueError, AttributeError, KeyError) as err:
        errors.append(f"could not check: {err}")
    return errors


def main():
    errors = run()
    for e in errors:
        print(f"installer_check: {e}")
    if errors:
        return 1
    print("installer_check: ok")
    return 0


if __name__ == "__main__":
    sys.exit(main())
