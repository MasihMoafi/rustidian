"""Extend verified v0.1.0 notices using the checksum-pinned fetched grammar crate.

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


def fetched_crate():
    cargo_home = Path(os.environ.get("CARGO_HOME", Path.home() / ".cargo"))
    for path in sorted((cargo_home / "registry/cache").glob(f"*/{CRATE_NAME}-{CRATE_VERSION}.crate")):
        data = path.read_bytes()
        if hashlib.sha256(data).hexdigest() == CRATE_SHA256:
            return data
    raise ValueError("Checksum-pinned two-face crate missing from Cargo cache; fetch locked dependencies first")


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
    if before - after or after - before != {expected}:
        parser.error("External dependency delta must be only checksum-pinned two-face 0.5.1")
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
    output = baseline + ("\n\n" + "\n\n".join(sections) + "\n").encode("utf-8")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_bytes(output)
    print(json.dumps({
        "baseline": BASELINE, "baseline_sha256": BASELINE_NOTICES_SHA256,
        "added_registry_package": list(expected), "grammar_licenses": len(syntaxes),
        "unused_theme_licenses_omitted": 12,
        "notices_sha256": hashlib.sha256(output).hexdigest(),
    }))


if __name__ == "__main__":
    main()
