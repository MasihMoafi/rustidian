"""Add license notices for every crate in the Linux build that the baseline notices lack.

The baseline is a verified THIRD_PARTY_NOTICES.txt; a crate counts as covered when a
line there reads "<name> <version> — <license>". The build's crates come from
`cargo tree --frozen --edges no-dev` for x86_64 Linux: normal and build dependencies,
since build scripts can generate shipped code. Each added crate is read from the Cargo
cache and checked against its Cargo.lock checksum: its root license files, the
name-table notices of fonts it embeds and, for two-face 0.5.1, its grammar
acknowledgements. The RaTeX crates ship no license file; assets/fonts/LICENSE-RaTeX.txt
holds upstream's MIT text and the KaTeX font license.

Offline after `cargo fetch --locked`; stdlib-only. The two-face 0.5.1 acknowledgement
dump is zlib-compressed bincode 1: two vectors of (u32 license enum, UTF-8 text, UTF-8
path), with u64 lengths. Its syntax/theme schema lives in src/acknowledgement/core_types.rs.
"""
import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import re
import struct
import subprocess
import sys
import tarfile
import tomllib
import zlib

ROOT = Path(__file__).resolve().parents[1]
TARGET = "x86_64-unknown-linux-gnu"
COVERED = re.compile(r"^([\w.-]+) ([0-9][^\s]*) — ", re.M)
LICENSE_FILE = re.compile(r"(licen[cs]e|copying|copyright|notice|unlicense)", re.I)
FALLBACK = {"ratex-": ROOT / "assets/fonts/LICENSE-RaTeX.txt"}
GRAMMARS = ("two-face", "0.5.1")
SEPARATOR = "=" * 70


def build_crates():
    """(name, version) of every crate compiled into the Linux build, except Rustidian."""
    tree = subprocess.check_output(
        ["cargo", "tree", "--frozen", "--edges", "no-dev", "--target", TARGET,
         "--prefix", "none", "--format", "{p}"], cwd=ROOT, text=True)
    crates = {tuple(line.split()[:2]) for line in tree.splitlines() if line.strip()}
    package = tomllib.loads((ROOT / "Cargo.toml").read_text())["package"]
    return {(name, version.removeprefix("v")) for name, version in crates} - {(package["name"], package["version"])}


def crate_archive(name, version, checksum):
    cargo_home = Path(os.environ.get("CARGO_HOME", Path.home() / ".cargo"))
    for path in sorted((cargo_home / "registry/cache").glob(f"*/{name}-{version}.crate")):
        data = path.read_bytes()
        if hashlib.sha256(data).hexdigest() == checksum:
            return data
    raise ValueError(f"{name} {version} with its locked checksum is not in the Cargo cache; run cargo fetch --locked")


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


def grammar_licenses(dump):
    reader = Acknowledgements(zlib.decompress(dump))
    syntaxes = reader.licenses(66)
    reader.licenses(12)  # Themes are unused: src/syntax.rs builds Rustidian's own palette.
    if reader.offset != len(reader.data):
        raise ValueError("Unexpected bytes after pinned acknowledgement vectors")
    return syntaxes


def font_notices(data):
    """Copyright (name 0) and license (name 13) strings from a font's name table."""
    notices = []
    for index in range(struct.unpack_from(">H", data, 4)[0]):
        tag, _, table, _ = struct.unpack_from(">4sIII", data, 12 + index * 16)
        if tag != b"name":
            continue
        _, count, storage = struct.unpack_from(">HHH", data, table)
        for record in range(count):
            platform, _, _, name, length, offset = struct.unpack_from(">HHHHHH", data, table + 6 + record * 12)
            if name in (0, 13):
                raw = data[table + storage + offset:table + storage + offset + length]
                text = raw.decode("utf-16-be" if platform in (0, 3) else "mac_roman")
                if text not in notices:
                    notices.append(text)
    return notices


def crate_notice(name, version, data):
    """One section with the crate's license files, then its fonts' notices and grammars."""
    prefix = f"{name}-{version}/"
    with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as archive:
        files = {member.name.removeprefix(prefix): member for member in archive.getmembers()
                 if member.isfile() and member.name.startswith(prefix)}

        def read(path):
            return archive.extractfile(files[path]).read()

        manifest = tomllib.loads(read("Cargo.toml").decode("utf-8"))["package"]
        names = sorted(path for path in files if "/" not in path and LICENSE_FILE.match(path))
        if manifest.get("license-file") in files and manifest["license-file"] not in names:
            names.append(manifest["license-file"])
        texts = [(path, read(path).decode("utf-8").rstrip()) for path in names]
        fonts = {}
        for path in sorted(files):
            if path.lower().endswith((".ttf", ".otf")):
                fonts.setdefault(tuple(font_notices(read(path))), []).append(Path(path).name)
        grammars = grammar_licenses(read("generated/acknowledgements_full.bin")) if (name, version) == GRAMMARS else []
    fallback = next((path for start, path in FALLBACK.items() if name.startswith(start)), None)
    section = SEPARATOR + f"\n{name} {version} — {manifest.get('license', 'see license file')}\n"
    for path, text in texts:
        section += f"\n--- {path} ---\n{text}\n"
    if not texts:
        section += f"\nThe package ships no license file{'; see ' + fallback.name + ' below' if fallback else ''}.\n"
    for notices, paths in fonts.items():
        section += "\n> Font name-table notices: " + ", ".join(paths) + "\n\n" + "\n\n".join(notices) + "\n"
    if grammars:
        section += ("\nMost of the code for generating theme and syntax dumps and curating them is taken from the bat project (https://github.com/sharkdp/bat).\n"
                    "\nEmbedded grammar acknowledgements from two-face 0.5.1\nhttps://github.com/CosmicHorrorDev/two-face/blob/v0.5.1/generated/acknowledgements_full.md\n"
                    + "".join("\n" + "-" * 72 + "\n" + path + "\n\n" + text.rstrip() + "\n" for path, text in grammars))
    return section, len(grammars), bool(texts), fallback


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline-notices", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    baseline = args.baseline_notices.read_bytes()
    text = baseline.decode("utf-8")
    if text.startswith("Rustidian alpha: "):
        text = "Rustidian: " + text.removeprefix("Rustidian alpha: ")
    covered = set(COVERED.findall(text))
    locked = {(package["name"], package["version"]): package.get("checksum")
              for package in tomllib.loads((ROOT / "Cargo.lock").read_text())["package"]}
    crates = build_crates()
    if any(name == GRAMMARS[0] and (name, version) != GRAMMARS for name, version in crates - covered):
        parser.error("two-face changed version; update the pinned grammar acknowledgement reader")
    sections, fallbacks, added, without_file, grammar_count = [], {}, [], [], 0
    for name, version in sorted(crates - covered):
        checksum = locked.get((name, version))
        if not checksum:
            continue  # A path crate the baseline lacks is reported missing below.
        section, grammars, has_file, fallback = crate_notice(name, version, crate_archive(name, version, checksum))
        sections.append(section)
        grammar_count += grammars
        added.append(f"{name} {version}")
        if not has_file and fallback:
            fallbacks.setdefault(fallback, []).append(name)
        elif not has_file:
            without_file.append(f"{name} {version}")
    for path, names in fallbacks.items():
        sections.append(SEPARATOR + f"\n{path.name}, the license text for {', '.join(names)}\n\n"
                        + path.read_text(encoding="utf-8").rstrip() + "\n")
    if sections:
        text = text.rstrip("\n") + "\n\n\nCrates added since the baseline notices\n\n" + "\n".join(sections)
    output = text.encode("utf-8")
    missing = sorted(f"{name} {version}" for name, version in crates - set(COVERED.findall(text)))
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_bytes(output)
    print(json.dumps({
        "baseline_sha256": hashlib.sha256(baseline).hexdigest(), "build_crates": len(crates),
        "added": added, "added_without_license_file": without_file, "grammar_licenses": grammar_count,
        "baseline_crates_not_in_build": sorted(f"{name} {version}" for name, version in covered - crates),
        "missing_notices": missing, "notices_sha256": hashlib.sha256(output).hexdigest(),
    }))
    if missing:
        sys.exit("Crates without a notice: " + ", ".join(missing))


if __name__ == "__main__":
    main()
