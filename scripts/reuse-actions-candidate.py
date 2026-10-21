"""Restore an exact Actions release only when runtime/build sources are unchanged."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess

ALLOWED = {
    "tests/gui_smoke.py", "scripts/obsidian-reference.py",
    "scripts/reuse-actions-candidate.py", ".github/workflows/native.yml",
    "scripts/compare-editors.py", "assets/icons/README.md",
    "scripts/record-keyboard.py",
    "ES.md", "VISION.md", "README.md",
}


def unchanged(commit):
    assert re.fullmatch("[0-9a-f]{40}", commit), "Expected a full source commit SHA"
    subprocess.run(["git", "merge-base", "--is-ancestor", commit, "HEAD"], check=True)
    changes = subprocess.check_output(["git", "diff", "--name-only", commit, "HEAD"], text=True).splitlines()
    rejected = [path for path in changes if path not in ALLOWED and not path.startswith("docs/")]
    assert not rejected, f"Candidate reuse refused: runtime/build sources changed: {rejected}"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-commit", required=True)
    parser.add_argument("--check-source", action="store_true")
    parser.add_argument("--artifact-dir", type=Path)
    parser.add_argument("--run-metadata", type=Path)
    args = parser.parse_args()
    unchanged(args.source_commit)
    if args.check_source:
        print("Runtime/build sources unchanged; harness-only verification is permitted.")
        return
    assert args.artifact_dir and args.run_metadata
    source = args.artifact_dir
    run = json.loads(args.run_metadata.read_text())
    assert run["headSha"] == args.source_commit, "Source run and requested commit differ"
    marker = source / "binary-source-commit.txt"
    built = marker.read_text().strip() if marker.exists() else args.source_commit
    unchanged(built)
    build_log = (source / "candidate-build.log").read_text()
    assert "[optimized]" in build_log and "release" in build_log, "Only optimized Actions candidates may be reused"
    reports = Path("reports")
    shutil.copytree(source, reports, dirs_exist_ok=True)
    (reports / "binary-source-commit.txt").write_text(built+"\n")
    (reports / "reused-run.json").write_text(json.dumps(run, indent=2)+"\n")
    output = Path("target/release")
    bundle = source.parent / "reused-app"
    import tarfile
    with tarfile.open(reports / "rust-notes-candidate-linux-x86_64.tar.gz") as archive:
        for member in archive.getmembers():
            path = Path(member.name)
            assert not path.is_absolute() and ".." not in path.parts
            assert member.isfile() or member.isdir()
            assert path.parts[0] in ("rust-notes", "demo-vault")
        archive.extractall(bundle, filter="data")
    binary = bundle / "rust-notes"
    assert hashlib.sha256(binary.read_bytes()).hexdigest() == (reports / "candidate.sha256").read_text().split()[0]
    for file in (bundle / "demo-vault").rglob("*"):
        if file.is_file():
            assert file.read_bytes() == (Path("demo-vault") / file.relative_to(bundle / "demo-vault")).read_bytes()
    output.mkdir(parents=True, exist_ok=True)
    shutil.copy2(binary, output / "rust-notes")
    (output / "rust-notes").chmod(0o755)
    benchmark = reports / "editor-benchmark-candidate.tar.gz"
    if benchmark.exists():
        unchanged((reports / "benchmark-source-commit.txt").read_text().strip())
        directory = output / "examples"
        directory.mkdir(exist_ok=True)
        with tarfile.open(benchmark) as archive:
            members = archive.getmembers()
            assert len(members) == 1 and members[0].name == "editor_benchmark" and members[0].isfile()
            archive.extractall(directory, filter="data")
        binary = directory / "editor_benchmark"
        assert hashlib.sha256(binary.read_bytes()).hexdigest() == (reports / "editor-benchmark.sha256").read_text().split()[0]
        binary.chmod(0o755)
    else:
        for name in ("benchmark-source-commit.txt", "benchmark-toolchain.txt", "editor-benchmark.sha256"):
            (reports / name).unlink(missing_ok=True)
    print(f"Restored exact optimized binary from {built}; runtime sources and sample bytes match.")


if __name__ == "__main__":
    main()
