#!/usr/bin/env python3
"""Check a release before the publish job is dispatched, so a mistake costs seconds, not a CI hour.

Run it from the branch you release, after `git push`:
    python3 scripts/preflight-release.py VERIFIED_RUN_ID
VERIFIED_RUN_ID is the "Rustidian UX candidate" run dispatched with gui_only=true and package_after_checks=true
(see docs/RELEASING.md). Every rule below names the mistake that taught it. Exit 1 on any failure.
Network calls run with the proxy variables removed (expensive mode: downloads never use the VPN).
"""
import json
import os
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
REPO = "MasihMoafi/rustidian"
PROXY = ("HTTPS_PROXY", "HTTP_PROXY", "ALL_PROXY", "https_proxy", "http_proxy", "all_proxy")
ENV = {k: v for k, v in os.environ.items() if k not in PROXY}
failures = []


def run(*args, check=False):
    return subprocess.run(args, cwd=ROOT, capture_output=True, text=True, env=ENV, check=check)


def fail(rule, detail):
    failures.append(f"FAIL {rule}: {detail}")


def gh_json(endpoint):
    return json.loads(run("gh", "api", endpoint, check=True).stdout)


def main():
    if len(sys.argv) != 2 or not sys.argv[1].isdigit():
        sys.exit(__doc__)
    run_id = sys.argv[1]
    run("git", "fetch", "-q", "origin")
    head = run("git", "rev-parse", "HEAD").stdout.strip()
    branch = run("git", "rev-parse", "--abbrev-ref", "HEAD").stdout.strip()
    info = gh_json(f"repos/{REPO}/actions/runs/{run_id}")
    if info["name"] != "Rustidian UX candidate":
        fail("run-workflow", f"run {run_id} is '{info['name']}', not 'Rustidian UX candidate'")
    if info["status"] != "completed" or info["conclusion"] != "success":
        fail("run-result", f"run {run_id} is {info['status']}/{info['conclusion']}; publish needs a successful run")
    source = info["head_sha"]
    # 4 Nov: the first publish failed, "no artifact matches rustidian-verified-<sha>": the full run does not make it.
    artifacts = gh_json(f"repos/{REPO}/actions/runs/{run_id}/artifacts")["artifacts"]
    verified = [a for a in artifacts if a["name"] == f"rustidian-verified-{source}"]
    if not verified:
        fail("verified-artifact", f"run {run_id} has no rustidian-verified-{source[:7]} artifact. Dispatch ux.yml with "
             "-f gui_only=true -f package_after_checks=true and use THAT run id (docs/RELEASING.md).")
    elif verified[0]["expired"]:
        fail("verified-artifact", "the verified artifact expired (1-day retention); dispatch the run again")
    # The publish job refuses a tree whose runtime sources differ from the verified commit; read its path list.
    publish = (ROOT / "scripts/publish-checked-release.py").read_text()
    guard = re.search(r'"git", "diff", "--exit-code", source, "HEAD", "--", ([^\]]+)\]', publish)
    if not guard:
        fail("publish-guard", "cannot read the path list from publish-checked-release.py")
    elif run("git", "cat-file", "-e", source).returncode:
        fail("verified-commit", f"commit {source[:7]} is not in this clone; fetch it")
    else:
        paths = re.findall(r'"([^"]+)"', guard.group(1))
        if run("git", "diff", "--quiet", source, "HEAD", "--", *paths).returncode:
            fail("source-drift", f"{', '.join(paths)} differ between the verified {source[:7]} and HEAD {head[:7]}")
    if run("git", "rev-parse", f"origin/{branch}").stdout.strip() != head:
        fail("pushed", f"HEAD {head[:7]} is not what origin/{branch} has; push first")
    # The publish job fast-forwards main and fails after publishing if main moved.
    if run("git", "merge-base", "--is-ancestor", "origin/main", "HEAD").returncode:
        fail("main-ancestor", "origin/main is not an ancestor of HEAD; cherry-pick onto the current main first")
    version = re.search(r'^version = "([^"]+)"', (ROOT / "Cargo.toml").read_text(), re.M).group(1)
    if run("gh", "release", "view", f"v{version}", "--repo", REPO).returncode == 0:
        fail("version", f"release v{version} exists already; bump Cargo.toml and Cargo.lock")
    lock = re.search(r'name = "rustidian"\nversion = "([^"]+)"', (ROOT / "Cargo.lock").read_text())
    if not lock or lock.group(1) != version:
        fail("version", f"Cargo.lock has {lock and lock.group(1)} but Cargo.toml has {version}")
    for needle in (version,):
        for name in ("packaging/aur-bin/PKGBUILD", "packaging/rustidian.metainfo.xml"):
            if needle not in (ROOT / name).read_text():
                fail("version", f"{name} does not mention {version}")
    lint = run("python3", "scripts/lint-packaging.py")
    if lint.returncode:
        fail("lint", "scripts/lint-packaging.py fails:\n" + lint.stdout.strip())
    for line in failures:
        print(line)
    print(f"preflight-release: {len(failures)} failure(s) for run {run_id}, HEAD {head[:7]}, version {version}")
    sys.exit(1 if failures else 0)


if __name__ == "__main__":
    main()
