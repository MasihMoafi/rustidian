"""Extend verified v0.1.0 notices for checksum-pinned Linux dependencies.

Offline and stdlib-only. The two-face 0.5.1 acknowledgement dump is zlib-compressed
bincode 1: two vectors of (u32 license enum, UTF-8 text, UTF-8 path), with u64
lengths. Its syntax/theme schema lives in src/acknowledgement/core_types.rs.
"""
import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import struct
import subprocess
import tarfile
import tomllib
import zlib

ROOT = Path(__file__).resolve().parents[1]
BASELINE = "v0.1.0"
BASELINE_NOTICES_SHA256 = "f63a6a19f1cca09c4b21dd8276953cd1a91ec979e1c67f3e98cab93ee1fde969"
CRATE_NAME = "two-face"
CRATE_VERSION = "0.5.1"
CRATE_SHA256 = "b285c51f8a6ade109ed4566d33ac4fb289fb5d6cf87ed70908a5eaf65e948e34"
REGISTRY = "registry+https://github.com/rust-lang/crates.io-index"


def external_packages(lock):
    return {
        (package["name"], package["version"], package["source"], package.get("checksum"))
        for package in lock["package"] if "source" in package
    }


def fetched_crate(name=CRATE_NAME, version=CRATE_VERSION, checksum=CRATE_SHA256):
    cargo_home = Path(os.environ.get("CARGO_HOME", Path.home() / ".cargo"))
    for path in sorted((cargo_home / "registry/cache").glob(f"*/{name}-{version}.crate")):
        data = path.read_bytes()
        if hashlib.sha256(data).hexdigest() == checksum:
            return data
    raise ValueError(f"Checksum-pinned {name} {version} missing from Cargo cache; fetch locked dependencies first")


class Acknowledgements:
    def __init__(self, data):
        self.data = data
        self.offset = 0

    def integer(self, fmt):
        value = struct.unpack_from(fmt, self.data, self.offset)[0]
        self.offset += struct.calcsize(fmt)
        return value

    def string(self):
        length = self.integer("<Q")
        end = self.offset + length
        if end > len(self.data):
            raise ValueError("Truncated acknowledgement string")
        value = self.data[self.offset:end].decode("utf-8")
        self.offset = end
        return value

    def licenses(self, expected):
        if self.integer("<Q") != expected:
            raise ValueError("Pinned acknowledgement schema changed")
        licenses = []
        for _ in range(expected):
            kind, text, path = self.integer("<I"), self.string(), self.string()
            if kind > 7 or not text or not path:
                raise ValueError("Invalid acknowledgement entry")
            licenses.append((path, text))
        return licenses


def crate_licenses(package):
    name, version, source, checksum = package
    if source != REGISTRY or not checksum:
        raise ValueError(f"Unsupported unpinned dependency: {name} {version}")
    prefix = f"{name}-{version}/"
    with tarfile.open(fileobj=io.BytesIO(fetched_crate(name, version, checksum)), mode="r:gz") as archive:
        manifest = archive.extractfile(prefix + "Cargo.toml")
        if manifest is None:
            raise ValueError(f"Missing crate manifest: {name} {version}")
        metadata = tomllib.loads(manifest.read().decode("utf-8"))["package"]
        declared = metadata.get("license") or metadata.get("license-file")
        if not declared:
            raise ValueError(f"Missing declared license: {name} {version}")
        texts = []
        for member in archive.getmembers():
            path = Path(member.name)
            label = path.name.upper()
            explicit = member.name == prefix + metadata.get("license-file", "")
            directory = any(part.upper() in {"LICENSE", "LICENSES", "LICENCE", "LICENCES"} for part in path.parts[1:-1])
            if member.isfile() and (explicit or directory or "NOTICE" in label or label.startswith(("LICENSE", "LICENCE", "COPYING", "NOTICE", "COPYRIGHT", "OFL", "UNLICENSE"))):
                texts.append((member.name, archive.extractfile(member).read().decode("utf-8")))
        if metadata.get("repository") == "https://github.com/erweixin/RaTeX" and version == "0.1.15" and declared == "MIT":
            vcs = json.loads(archive.extractfile(prefix + ".cargo_vcs_info.json").read())
            if vcs["git"]["sha1"] != "7d9f9a39218914bead8ffdf39b79028a01d12c22":
                raise ValueError("RaTeX workspace license source changed")
            license_path = ROOT / "assets/fonts/LICENSE-RaTeX.txt"
            license_bytes = license_path.read_bytes()
            if hashlib.sha256(license_bytes).hexdigest() != "f4263a34979bd9343d4686a1923729e8bc03af0c9badb621f8c6c1f3bbc20f78":
                raise ValueError("Verified RaTeX workspace/font license changed")
            texts.append(("assets/fonts/LICENSE-RaTeX.txt", license_bytes.decode("utf-8")))
        if not texts:
            raise ValueError(f"Missing packaged license text: {name} {version} ({declared})")
        return declared, texts


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline-notices", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    baseline = args.baseline_notices.read_bytes()
    if hashlib.sha256(baseline).hexdigest() != BASELINE_NOTICES_SHA256:
        parser.error("Baseline notices do not match verified v0.1.0")
    baseline_lock = tomllib.loads(subprocess.check_output(
        ["git", "show", f"{BASELINE}:Cargo.lock"], cwd=ROOT, text=True,
    ))
    current_lock = tomllib.loads((ROOT / "Cargo.lock").read_text())
    before, after = external_packages(baseline_lock), external_packages(current_lock)
    expected = (CRATE_NAME, CRATE_VERSION, REGISTRY, CRATE_SHA256)
    if expected not in after - before:
        parser.error("Expected the checksum-pinned two-face grammar dependency")
    tree = subprocess.check_output([
        "cargo", "tree", "--locked", "--offline", "--target", "x86_64-unknown-linux-gnu",
        "--edges", "normal,build", "--prefix", "none", "--format", "{p}", "--color", "never",
    ], cwd=ROOT, text=True)
    linux = {(parts[0], parts[1].removeprefix("v")) for line in tree.splitlines()
             if len(parts := line.split()) >= 2 and parts[1].startswith("v")}
    additions = []
    for package in sorted((after - before) - {expected}):
        if package[:2] in linux:
            declared, texts = crate_licenses(package)
            additions.append((package, declared, texts))
    # Read only the pinned archive's needed members; never unpack into the tree.
    with tarfile.open(fileobj=io.BytesIO(fetched_crate()), mode="r:gz") as archive:
        def member(name):
            source = archive.extractfile(f"{CRATE_NAME}-{CRATE_VERSION}/{name}")
            if source is None:
                raise ValueError(f"Missing crate member: {name}")
            return source.read()
        mit = member("LICENSE-MIT").decode("utf-8")
        reader = Acknowledgements(zlib.decompress(member("generated/acknowledgements_full.bin")))
    syntaxes = reader.licenses(66)
    reader.licenses(12)  # Themes are unused: src/syntax.rs builds Rustidian's own palette.
    if reader.offset != len(reader.data):
        parser.error("Unexpected bytes after pinned acknowledgement vectors")
    sections = [
        "=" * 72 + "\ntwo-face 0.5.1 — selected MIT license\n\n" + mit.rstrip(),
        "Most of the code for generating theme and syntax dumps and curating them is taken from the bat project (https://github.com/sharkdp/bat).",
        "Embedded grammar acknowledgements from two-face 0.5.1\nhttps://github.com/CosmicHorrorDev/two-face/blob/v0.5.1/generated/acknowledgements_full.md",
    ]
    sections.extend("-" * 72 + "\n" + path + "\n\n" + text.rstrip() for path, text in syntaxes)
    included = set()
    for package, declared, texts in additions:
        sections.append("=" * 72 + f"\n{package[0]} {package[1]} — {declared}")
        for path, text in texts:
            if path in included:
                sections.append("Shared license text listed above: " + path)
            else:
                sections.append("-" * 72 + "\n" + path + "\n\n" + text.rstrip())
                included.add(path)
    output = baseline + ("\n\n" + "\n\n".join(sections) + "\n").encode("utf-8")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_bytes(output)
    print(json.dumps({
        "baseline": BASELINE, "baseline_sha256": BASELINE_NOTICES_SHA256,
        "added_registry_package": list(expected), "grammar_licenses": len(syntaxes),
        "unused_theme_licenses_omitted": 12,
        "added_locked_linux_packages": [list(package) for package, _, _ in additions],
        "notices_sha256": hashlib.sha256(output).hexdigest(),
    }))


if __name__ == "__main__":
    main()
