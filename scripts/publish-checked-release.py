"""Publish an explicitly requested, successful UX run from an Actions runner."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tarfile


def command(*args):
    return subprocess.check_output(args, text=True).strip()


def api(endpoint, payload=None):
    args = ["gh", "api", endpoint]
    if payload is not None:
        args += ["--method", "PATCH", "--input", "-"]
    result = subprocess.run(args, input=None if payload is None else json.dumps(payload), check=True, capture_output=True, text=True)
    return json.loads(result.stdout)


def digest(path):
    with path.open("rb") as incoming:
        return hashlib.file_digest(incoming, "sha256").hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--run", required=True, type=int)
    args = parser.parse_args()
    assert os.environ.get("GITHUB_ACTIONS") == "true", "Use the explicitly dispatched Actions publication job"
    repo = os.environ["GITHUB_REPOSITORY"]
    assert repo == "MasihMoafi/rustidian"
    run = json.loads(command("gh", "run", "view", str(args.run), "--repo", repo, "--json", "status,conclusion,headSha,workflowName,url"))
    assert run["status"] == "completed" and run["conclusion"] == "success", run
    assert run["workflowName"] == "Rustidian UX candidate", run
    source = run["headSha"]
    assets = Path("release-assets")
    subprocess.run(["gh", "run", "download", str(args.run), "--repo", repo, "--name", "rustidian-verified-" + source, "--dir", str(assets)], check=True)
    for line in (assets / "SHA256SUMS").read_text().splitlines():
        expected, name = line.split(None, 1)
        name = name.strip()
        assert Path(name).name == name, "Unexpected checksum path"
        assert digest(assets / name) == expected, name
    info = json.loads((assets / "BUILDINFO.json").read_text())
    assert info["source_commit"] == source
    version = info["version"].removeprefix("Rustidian ")
    assert re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", version), version
    checks = info["gui_checks"]
    assert len(checks) >= 16 and {"table-resize", "undo", "palette-queued", "recovery"} <= checks.keys()
    assert all(check["status"] == "passed" for check in checks.values())
    package = info["debian_verification"]
    assert package["status"] == "passed" and package["system_install_upgrade_remove_purge"]
    assert package["exact_binary_sha256"] == info["binary_sha256"]
    assert digest(assets / "rustidian-linux-amd64.deb") == package["package_sha256"]
    with tarfile.open(assets / "rustidian-linux-x86_64.tar.gz") as archive:
        member = archive.extractfile("rustidian")
        assert member is not None and hashlib.file_digest(member, "sha256").hexdigest() == info["binary_sha256"]
    # Runtime and portable installer sources must match the checked revision.
    # The Debian metadata correction is verified separately below.
    subprocess.run(["git", "diff", "--exit-code", source, "HEAD", "--", "Cargo.toml", "Cargo.lock", "src", "vendor", "assets", "demo-vault", "docs/brand", "scripts/install-linux.py", "scripts/package-linux.py"], check=True)
    # Repackage the already tested executable with corrected native runtime
    # dependencies, then check the exact delivery artifact and installed launcher.
    bundle = Path("verified-bundle")
    with tarfile.open(assets / "rustidian-linux-x86_64.tar.gz") as archive:
        archive.extractall(bundle, filter="data")
    binary = bundle / "rustidian"
    assert digest(binary) == info["binary_sha256"]
    subprocess.run(["python3", "scripts/package-deb.py", "--binary", str(binary), "--notices", str(assets / "THIRD_PARTY_NOTICES.txt"), "--output", str(assets / "rustidian-linux-amd64.deb")], check=True)
    subprocess.run(["sudo", "apt-get", "install", "-y", "--no-install-recommends", "./" + str(assets / "rustidian-linux-amd64.deb")], check=True)
    assert digest(Path("/usr/lib/rustidian/rustidian")) == info["binary_sha256"]
    subprocess.run(["python3", "tests/gui_smoke.py", "--binary", "/usr/bin/rustidian", "--section", "undo"], check=True)
    smoke = json.loads(Path("reports/gui-results.json").read_text())
    assert smoke["status"] == "passed"
    subprocess.run(["python3", "tests/gui_smoke.py", "--binary", "/usr/bin/rustidian", "--section", "palette-queued"], check=True)
    palette = json.loads(Path("reports/gui-results.json").read_text())
    assert palette["status"] == "passed"
    package = json.loads(command("python3", "tests/debian_package.py", "--binary", str(binary), "--package", str(assets / "rustidian-linux-amd64.deb"), "--notices", str(assets / "THIRD_PARTY_NOTICES.txt"), "--system"))
    assert package["status"] == "passed" and package["exact_binary_sha256"] == info["binary_sha256"]
    assert package["package_sha256"] == digest(assets / "rustidian-linux-amd64.deb")
    info["debian_verification"] = package
    info["installed_package_gui"] = smoke
    info["installed_package_palette"] = palette
    info["packaging_source_commit"] = os.environ["GITHUB_SHA"]
    info["depends"] = command("dpkg-deb", "--field", str(assets / "rustidian-linux-amd64.deb"), "Depends")
    (assets / "BUILDINFO.json").write_text(json.dumps(info, indent=2) + "\n")
    (assets / "SHA256SUMS").write_text("".join(digest(assets / name) + "  " + name + "\n" for name in ("rustidian-linux-amd64.deb", "rustidian-linux-x86_64.tar.gz", "BUILDINFO.json")))
    publication_source = os.environ["GITHUB_SHA"]
    tag = "v" + version
    base = "https://github.com/" + repo + "/releases/download/" + tag + "/"
    total = sum(len(value["checks"]) for value in checks.values())
    notes = assets / "release-notes.md"
    notes.write_text(
        "**[Download the Debian/Ubuntu installer](" + base + "rustidian-linux-amd64.deb)** · [Portable Linux archive](" + base + "rustidian-linux-x86_64.tar.gz) · [Checksums](" + base + "SHA256SUMS)\n\n"
        "Linux x86_64, Ubuntu 24.04 or newer, glibc 2.39 or newer. Download the `.deb` and `SHA256SUMS`, then run:\n\n"
        "```sh\nsha256sum --check --ignore-missing SHA256SUMS\nsudo apt install ./rustidian-linux-amd64.deb\n```\n\n"
        "This update bounds extreme table-column dragging, adds chronological text/layout Undo and Redo, fixes queued dialog dismissal, makes graph relationship colors visible, and unifies compact dialogs and fonts. Theme icons follow the selected palette.\n\n"
        "Verified: " + str(total) + " native checks across " + str(len(checks)) + " groups; core tests; exact Debian install/upgrade/remove/purge and portable installer checks. Existing notes, recovery data and user demo edits survive. [Verification run](" + run["url"] + "). `BUILDINFO.json` binds the source and executable.\n\n"
        "Obsidian community plugins and browser-style HTML rendering remain unfinished. Full Obsidian parity and a current overall performance advantage are unproven.\n"
    )
    existing = subprocess.run(["gh", "release", "view", tag, "--repo", repo, "--json", "isDraft,targetCommitish"], capture_output=True, text=True)
    if existing.returncode:
        subprocess.run(["gh", "release", "create", tag, "--repo", repo, "--target", publication_source, "--draft", "--title", "Rustidian " + version + " — Linux installers", "--notes-file", str(notes)], check=True)
    else:
        release = json.loads(existing.stdout)
        assert release["targetCommitish"] == publication_source, "Existing release targets another revision"
    release = next(value for value in api("repos/" + repo + "/releases") if value["tag_name"] == tag)
    current = {value["name"]: value for value in release["assets"]}
    names = ["rustidian-linux-amd64.deb", "rustidian-linux-x86_64.tar.gz", "BUILDINFO.json", "SHA256SUMS", "THIRD_PARTY_NOTICES.txt"]
    for name in names:
        expected = "sha256:" + digest(assets / name)
        if name in current:
            assert current[name].get("digest") == expected, "Existing asset differs: " + name
        else:
            assert release["draft"], "Only an owned draft may receive new release assets"
            subprocess.run(["gh", "release", "upload", tag, str(assets / name), "--repo", repo], check=True)
    release = next(value for value in api("repos/" + repo + "/releases") if value["tag_name"] == tag)
    current = {value["name"]: value for value in release["assets"]}
    assert all(current[name].get("digest") == "sha256:" + digest(assets / name) for name in names)
    if release["draft"]:
        subprocess.run(["gh", "release", "edit", tag, "--repo", repo, "--draft=false", "--latest", "--notes-file", str(notes)], check=True)
    # A concurrent public-main change must never be overwritten.
    main_ref = api("repos/" + repo + "/git/ref/heads/main")["object"]["sha"]
    subprocess.run(["git", "merge-base", "--is-ancestor", main_ref, publication_source], check=True)
    api("repos/" + repo + "/git/refs/heads/main", {"sha": publication_source, "force": False})
    receipt = {"status": "published", "tag": tag, "source_commit": source, "packaging_source_commit": publication_source, "verification_run": run["url"], "url": "https://github.com/" + repo + "/releases/tag/" + tag, "assets": {name: {"sha256": digest(assets / name), "bytes": (assets / name).stat().st_size, "url": base + name} for name in names}, "native_checks": total, "native_groups": len(checks), "network": "GitHub runner uploaded directly; no local VPN transfer"}
    Path("publication.json").write_text(json.dumps(receipt, indent=2) + "\n")
    print(json.dumps(receipt, indent=2))
    with Path(os.environ["GITHUB_STEP_SUMMARY"]).open("a") as summary:
        summary.write("[Download Rustidian .deb](" + base + "rustidian-linux-amd64.deb) · [Release](" + receipt["url"] + ")\n")


if __name__ == "__main__":
    main()
