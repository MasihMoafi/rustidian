"""Build an amd64 Debian package without changing the verified executable."""
import argparse
import hashlib
import os
from pathlib import Path
import re
import runpy
import shutil
import subprocess
import tempfile


LAUNCHER = '''#!/usr/bin/python3
import os
from pathlib import Path
import shutil
import sys

prefix = Path(__file__).resolve().parents[1]
if len(sys.argv) == 1 and not any(name in os.environ for name in ("RUSTIDIAN_DEFAULT_VAULT", "RUST_NOTES_DEFAULT_VAULT")):
    data = Path(os.environ.get("XDG_DATA_HOME", str(Path.home() / ".local/share"))) / "rustidian"
    demo = data / "demo-vault"
    try:
        if not demo.exists():
            data.mkdir(parents=True, exist_ok=True)
            shutil.copytree(prefix / "share/rustidian/demo-vault", demo)
    except OSError as error:
        print("Cannot prepare the writable demo vault: " + str(error), file=sys.stderr)
        sys.exit(1)
    os.environ["RUSTIDIAN_DEFAULT_VAULT"] = str(demo)
os.execv(prefix / "lib/rustidian/rustidian", ["rustidian", *sys.argv[1:]])
'''


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--notices', type=Path, required=True)
    parser.add_argument('--revision', type=int, default=1)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    mime_types = runpy.run_path(str(root / 'scripts/install-linux.py'))['MIME_TYPES']
    binary = args.binary.resolve()
    with binary.open('rb') as incoming:
        header = incoming.read(20)
    if header[:6] != b'\x7fELF\x02\x01' or header[18:20] != b'\x3e\x00':
        parser.error('Expected the verified Linux amd64 ELF executable')
    identity = subprocess.check_output([str(binary), '--version'], text=True).strip()
    if not identity.startswith('Rustidian ') or args.revision < 1:
        parser.error('Expected a Rustidian executable and positive package revision')
    versions = re.findall(r'\bGLIBC_(\d+(?:\.\d+)+)\b', subprocess.check_output(
        ['readelf', '--version-info', '--wide', str(binary)], text=True))
    if not versions:
        parser.error('Cannot determine the executable\'s glibc requirement')
    glibc = '.'.join(map(str, max([(2, 39), *[tuple(map(int, version.split('.'))) for version in versions]])))
    upstream = identity.split(' ', 1)[1].replace('-alpha.', '~alpha.')
    if not re.fullmatch(r'[0-9][A-Za-z0-9.+~]*', upstream):
        parser.error('Unsupported version string: ' + upstream)
    if not args.notices.is_file():
        parser.error('Third-party notices are required')
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='rustidian-deb-') as temporary:
        package = Path(temporary)
        for directory in ('DEBIAN', 'usr/bin', 'usr/lib/rustidian', 'usr/share/rustidian', 'usr/share/applications', 'usr/share/icons/hicolor/256x256/apps', 'usr/share/doc/rustidian'):
            (package / directory).mkdir(parents=True, exist_ok=True)
        shutil.copy2(binary, package / 'usr/lib/rustidian/rustidian')
        (package / 'usr/lib/rustidian/rustidian').chmod(0o755)
        launcher = package / 'usr/bin/rustidian'
        launcher.write_text(LAUNCHER)
        launcher.chmod(0o755)
        shutil.copytree(root / 'demo-vault', package / 'usr/share/rustidian/demo-vault')
        shutil.copy2(root / 'docs/brand/rustidian.png', package / 'usr/share/icons/hicolor/256x256/apps/rustidian.png')
        documents = package / 'usr/share/doc/rustidian'
        shutil.copy2(root / 'LICENSE', documents / 'copyright')
        shutil.copy2(args.notices, documents / 'THIRD_PARTY_NOTICES.txt')
        for license in (root / 'assets/fonts').glob('LICENSE-*.txt'):
            shutil.copy2(license, documents / license.name)
        desktop = package / 'usr/share/applications/rustidian.desktop'
        desktop.write_text('[Desktop Entry]\nType=Application\nName=Rustidian\nComment=Local Markdown notes and source code\nExec=rustidian %f\nIcon=rustidian\nTerminal=false\nCategories=Office;TextEditor;\nKeywords=Markdown;Notes;Code;\nStartupWMClass=rustidian\nMimeType=' + mime_types + '\n')
        subprocess.run(['desktop-file-validate', str(desktop)], check=True)
        size = sum(path.stat().st_size for path in (package / 'usr').rglob('*') if path.is_file())
        dependencies = 'libc6 (>= ' + glibc + '), libstdc++6, libgcc-s1, libx11-6, libx11-xcb1, libxcursor1, libxi6, libxrandr2, libxinerama1, libxkbcommon0, libxkbcommon-x11-0, libwayland-client0, libwayland-egl1, libegl1, libgl1, libglib2.0-bin, gsettings-desktop-schemas, python3, zenity'
        (package / 'DEBIAN/control').write_text('Package: rustidian\nVersion: ' + upstream + '-' + str(args.revision) + '\nArchitecture: amd64\nSection: editors\nPriority: optional\nMaintainer: Rustidian contributors\nInstalled-Size: ' + str((size + 1023) // 1024) + '\nDepends: ' + dependencies + '\nHomepage: https://github.com/MasihMoafi/rustidian\nDescription: Native Markdown notebook and source text editor\n Local vaults, linked notes and native editing.\n Obsidian community plugins are not supported.\n')
        environment = dict(os.environ, SOURCE_DATE_EPOCH=os.environ.get('SOURCE_DATE_EPOCH', '0'))
        subprocess.run(['dpkg-deb', '--root-owner-group', '-Zgzip', '-z3', '--build', str(package), str(args.output.resolve())], check=True, env=environment)
    with args.output.open('rb') as incoming:
        digest = hashlib.file_digest(incoming, 'sha256').hexdigest()
    args.output.with_suffix(args.output.suffix + '.sha256').write_text(digest + '  ' + args.output.name + '\n')
    print(str(args.output))


if __name__ == '__main__':
    main()
