#!/usr/bin/env python3
"""Fast local checks for the packaging files, so CI does not find what a minute here would.

Run before pushing any change to snap/, packaging/, scripts/package-*.py, docs/brand or the
workflows:  python3 scripts/lint-packaging.py
Each rule names the CI failure that taught it. Exit 1 on any failure; warnings do not fail.
"""
import re
import shutil
import struct
import subprocess
import sys
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parent.parent
failures, warnings = [], []


def fail(rule, detail):
    failures.append(f"FAIL {rule}: {detail}")


def warn(rule, detail):
    warnings.append(f"warn {rule}: {detail}")


def run(command):
    return subprocess.run(command, cwd=ROOT, capture_output=True, text=True)


def check_scripts_compile():
    for path in sorted((ROOT / "scripts").glob("*.py")):
        try:
            compile(path.read_text(), str(path), "exec")
        except SyntaxError as error:
            fail("python-syntax", f"{path.name}:{error.lineno} {error.msg}")


def check_snap():
    path = ROOT / "snap/snapcraft.yaml"
    data = yaml.safe_load(path.read_text())
    for key in ("name", "base", "confinement", "grade", "summary", "description", "parts", "apps"):
        if key not in data:
            fail("snap-key", f"snapcraft.yaml lacks '{key}'")
    icon = data.get("icon")
    if icon and not (ROOT / icon).is_file():
        fail("snap-icon", f"icon {icon} does not exist")
    for app, spec in data.get("apps", {}).items():
        if not (ROOT / f"snap/gui/{app}.desktop").is_file():
            fail("snap-desktop", f"snap/gui/{app}.desktop is missing")
        if spec.get("desktop"):
            warn("snap-desktop", f"app {app} sets 'desktop:' beside snap/gui/{app}.desktop; keep one")
    # 3 Nov CI: "'rustup' not found and part does not depend on a part named 'rust-deps'".
    for name, part in data.get("parts", {}).items():
        if part.get("plugin") == "rust" and "rust-deps" not in data.get("parts", {}):
            fail("snap-rust-plugin", f"part {name} uses plugin: rust without a rust-deps part; "
                 "the snapcraft rust plugin cannot find rustup. Install the toolchain in override-build instead.")
        if "rust-version" in (ROOT / "Cargo.toml").read_text() and name == "rustidian":
            wanted = re.search(r'rust-version\s*=\s*"([\d.]+)"', (ROOT / "Cargo.toml").read_text()).group(1)
            if wanted not in (part.get("override-build") or ""):
                fail("snap-rust-version", f"Cargo.toml needs Rust {wanted}; override-build does not install it")
    for key in ("override-build", "override-pull"):
        for name, part in data.get("parts", {}).items():
            if key in part:
                result = subprocess.run(["bash", "-n"], input=part[key], capture_output=True, text=True)
                if result.returncode:
                    fail("snap-shell", f"part {name} {key}: {result.stderr.strip()}")
    for name, part in data.get("parts", {}).items():
        for line in (part.get("override-build") or "").splitlines():
            match = re.search(r"(?:install -Dm\d+|cp -r)\s+(\S+)\s", line)
            if match and "$" not in match.group(1) and not match.group(1).startswith(("target/", "-")):
                if not any((ROOT / match.group(1)).exists() for _ in [0]) and "*" not in match.group(1):
                    fail("snap-source-file", f"override-build copies {match.group(1)}, which is not in the repo")
    snapcraft = shutil.which("snapcraft")
    if snapcraft:
        result = run([snapcraft, "expand-extensions"])
        if result.returncode:
            fail("snap-schema", (result.stderr or result.stdout).strip().splitlines()[-1])
    else:
        warn("snap-schema", "snapcraft not installed; schema check skipped")


def check_workflows():
    for path in sorted((ROOT / ".github/workflows").glob("*.yml")):
        yaml.safe_load(path.read_text())
        for number, line in enumerate(path.read_text().splitlines(), 1):
            match = re.search(r"uses:\s*([\w./-]+)@(\S+)", line)
            if match and not re.fullmatch(r"[0-9a-f]{40}", match.group(2)):
                fail("workflow-pin", f"{path.name}:{number} {match.group(1)}@{match.group(2)} is not pinned to a commit SHA")


def check_desktop_and_metainfo():
    desktop = ROOT / "snap/gui/rustidian.desktop"
    text = desktop.read_text().replace("${SNAP}/meta/gui/rustidian.png", "rustidian")  # snapcraft expands this
    scratch = ROOT / ".lint-rustidian.desktop"
    scratch.write_text(text)
    try:
        result = run(["desktop-file-validate", str(scratch)])
        errors = [line for line in result.stdout.splitlines() + result.stderr.splitlines() if "error" in line]
        if errors:
            fail("desktop-file", "; ".join(errors))
    except FileNotFoundError:
        warn("desktop-file", "desktop-file-validate missing")
    finally:
        scratch.unlink(missing_ok=True)
    if shutil.which("appstreamcli"):
        result = run(["appstreamcli", "validate", "--no-net", "packaging/rustidian.metainfo.xml"])
        if result.returncode:
            fail("appstream", (result.stdout + result.stderr).strip().splitlines()[-1])
    # 3 Nov CI: appimagetool validates the AppStream file strictly (metainfo-filename-cid-mismatch,
    # desktop-file-not-found). Build the same two files the script writes and run appstreamcli on them.
    appimage = (ROOT / "scripts/package-appimage.py").read_text()
    match = re.search(r"APP_ID = '([^']+)'", appimage)
    if not match:
        fail("appimage-metainfo", "package-appimage.py must define APP_ID")
    else:
        component = re.search(r"<id>([^<]+)</id>", (ROOT / "packaging/rustidian.metainfo.xml").read_text()).group(1)
        if component != match.group(1):
            fail("appimage-metainfo", f"metainfo <id> {component} must equal APP_ID {match.group(1)} (file name = component id)")
        for needle, why in (("{APP_ID}.appdata.xml", "metainfo file named after the component id"),
                            ("usr/share/applications/{APP_ID}.desktop", "desktop file in usr/share/applications named after the id")):
            if needle not in appimage:
                fail("appimage-metainfo", f"package-appimage.py must write the {why} (CI: metainfo-filename-cid-mismatch, desktop-file-not-found)")
    if match and shutil.which("appstreamcli"):
        import tempfile
        with tempfile.TemporaryDirectory() as scratch:
            tree = Path(scratch)
            for folder in ("usr/share/metainfo", "usr/share/applications"):
                (tree / folder).mkdir(parents=True)
            metainfo = (ROOT / "packaging/rustidian.metainfo.xml").read_text().replace(">rustidian.desktop<", f">{match.group(1)}.desktop<")
            (tree / f"usr/share/metainfo/{match.group(1)}.appdata.xml").write_text(metainfo)
            (tree / f"usr/share/applications/{match.group(1)}.desktop").write_text((ROOT / "snap/gui/rustidian.desktop").read_text().replace("${SNAP}/meta/gui/rustidian.png", "rustidian"))
            result = run(["appstreamcli", "validate", "--pedantic", "--strict", "--no-net", str(tree / f"usr/share/metainfo/{match.group(1)}.appdata.xml")])
            if result.returncode:
                fail("appimage-appstream", " | ".join(line.strip() for line in result.stdout.splitlines() if line.strip().startswith(("W:", "E:", "P:"))) or result.stdout.strip())
    pkgbuild = ROOT / "packaging/aur-bin/PKGBUILD"
    if run(["bash", "-n", str(pkgbuild)]).returncode:
        fail("pkgbuild-shell", "bash -n rejects PKGBUILD")
    if "'SKIP'" in pkgbuild.read_text():
        warn("pkgbuild-checksum", "sha256sums is SKIP; fill it from the release before publishing")


def check_files_and_text():
    notices = (ROOT / "THIRD_PARTY_NOTICES.txt").read_text()
    for phrase in ("SIL OPEN FONT LICENSE", "Ubuntu Font Licence", "Hack", "emoji-icon-font"):
        if phrase.lower() not in notices.lower():
            fail("notices", f"THIRD_PARTY_NOTICES.txt lacks the text for: {phrase}")
    for needed in ("LICENSE", "demo-vault", "assets/fonts"):
        if not (ROOT / needed).exists():
            fail("package-input", f"{needed} is missing but the package scripts copy it")
    if not list((ROOT / "assets/fonts").glob("LICENSE-*.txt")):
        fail("package-input", "assets/fonts has no LICENSE-*.txt files")
    for name in ("store-icon.png", "rustidian.png", "themes/love-space.png", "themes/elpis-gold.png", "themes/signature.png"):
        png = ROOT / "docs/brand" / name
        if not png.is_file():
            fail("brand", f"{png.relative_to(ROOT)} is missing")
            continue
        data = png.read_bytes()
        width, height, depth, color = struct.unpack(">IIBB", data[16:26])
        if width != height or width < 256:
            fail("brand", f"{name} must be square and at least 256 px, is {width}x{height}")
        if color != 6:
            fail("brand", f"{name} must be RGBA PNG (colour type 6), is {color}")
    if (ROOT / "snap/gui/rustidian.png").read_bytes() != (ROOT / "docs/brand/store-icon.png").read_bytes():
        fail("brand", "snap/gui/rustidian.png differs from docs/brand/store-icon.png; copy it again")
    home = "/home/" + "mas" + "ih"  # built in parts so this file does not match itself
    tracked = run(["git", "grep", "-l", home, "--", ".", ":!THIRD_PARTY_NOTICES.txt"]).stdout.split()
    if tracked:
        fail("personal-path", "tracked files name a personal home path: " + ", ".join(tracked))


def clippy_command(path):
    text = (ROOT / path).read_text()
    match = re.search(r"cargo(?: \+[\d.]+)? clippy ([^\n|]*?)(?: 2>&1)?(?:\s*\|.*)?$", text, re.M)
    return match.group(1).strip() if match else None


def check_ci_and_release_process():
    # 3 Nov: the quick check passed, then the full run failed clippy on examples and benches it never linted.
    quick, full = clippy_command(".github/workflows/native.yml"), clippy_command(".github/workflows/ux.yml")
    if not quick or quick != full:
        fail("ci-same-clippy", f"native.yml runs clippy '{quick}' but ux.yml runs '{full}'; the quick check must lint what the full run lints")
    ux = (ROOT / ".github/workflows/ux.yml").read_text()
    if not re.search(r"rustidian-verified-\$\{\{ github.sha \}\}", ux) or "package_after_checks" not in ux:
        fail("release-artifact", "ux.yml no longer creates rustidian-verified-<sha> via package_after_checks; docs/RELEASING.md and the preflight are stale")
    runbook = ROOT / "docs/RELEASING.md"
    if not runbook.is_file():
        fail("release-runbook", "docs/RELEASING.md is missing")
    else:
        text = runbook.read_text()
        for needle in ("gui_only=true", "package_after_checks=true", "publish_run", "preflight-release.py", "lint-packaging.py"):
            if needle not in text:
                fail("release-runbook", f"docs/RELEASING.md does not mention {needle}")
    # 4 Nov: release notes are hard-coded per release; the 0.1.3 text must never ship again.
    if "fixes everyday vault workflows" in (ROOT / "scripts/publish-checked-release.py").read_text():
        fail("release-notes", "publish-checked-release.py still has the v0.1.3 release notes; rewrite them for this release")
    # 3 Nov: Masih's rule, no download over the VPN in expensive mode.
    for path in sorted((ROOT / "scripts").glob("*.sh")) + [ROOT / "scripts/preflight-release.py"]:
        text = path.read_text()
        if re.search(r"\b(gh|curl|snapcraft)\b", text) and not re.search(r"unset HTTPS_PROXY|PROXY = \(", text):
            fail("no-proxy", f"{path.name} calls the network but does not clear the proxy variables")


def main():
    for check in (check_scripts_compile, check_snap, check_workflows, check_desktop_and_metainfo, check_files_and_text, check_ci_and_release_process):
        try:
            check()
        except Exception as error:  # a crashed check is a failed check
            fail(check.__name__, f"{type(error).__name__}: {error}")
    for line in warnings + failures:
        print(line)
    print(f"lint-packaging: {len(failures)} failure(s), {len(warnings)} warning(s)")
    sys.exit(1 if failures else 0)


if __name__ == "__main__":
    main()
