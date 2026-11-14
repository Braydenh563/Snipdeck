#!/usr/bin/env python3
"""Static check of the Windows installer and MSIX template (stdlib only, runs on Linux).

  python3 scripts/installer_check.py

Fails on: AppId not the fixed GUID, PrivilegesRequired not lowest, AppMutex / [Code] mutex not matching
the app's single-instance name, a [Files] source missing from the repo (the exe may be absent), a
Run value not named Snipdeck, MinVersion that differs between the .iss and the MSIX template, and an
MSIX template whose StartupTask / Executable / Assets paths disagree with the app and the asset script.
"""
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
    unplated = re.search(r'UNPLATED\s*=\s*\("([^"]+)"', text).group(1)
    names = {f"{s}.png" for s in stems} | {f"{s}.scale-{p}.png" for s in stems for p in scales}
    names.add(unplated + ".png")
    return names


def check_iss(errors, iss_text=None, root=ROOT, iss_dir=None, mutex=None):
    text = iss_text if iss_text is not None else ISS.read_text(encoding="utf-8")
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
    known = asset_files()
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
    except (OSError, ValueError, AttributeError) as err:
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
