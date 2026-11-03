#!/usr/bin/env python3
"""Package the exact verified executable as an AppImage (x86_64).

Needs `appimagetool` on PATH or in APPIMAGETOOL. The executable is dynamically linked, so
build it on the oldest glibc you support; graphics libraries come from the host.
"""
import argparse
import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

APPRUN = '''#!/bin/sh
HERE="$(dirname "$(readlink -f "$0")")"
for folder in /usr/local/node/bin "$HOME/.local/bin"; do
    case ":$PATH:" in
        *":$folder:"*) ;;
        *) if [ -d "$folder" ]; then PATH="$folder${PATH:+:$PATH}"; fi ;;
    esac
done
export PATH
exec "$HERE/usr/bin/rustidian" "$@"
'''

DESKTOP = ('[Desktop Entry]\nType=Application\nName=Rustidian\nComment=Local Markdown notes and source code\n'
           'Exec=rustidian %f\nIcon=rustidian\nTerminal=false\nCategories=Office;TextEditor;\n'
           'Keywords=Markdown;Notes;Code;\nStartupWMClass=rustidian\n')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--notices', type=Path, required=True)
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    binary = args.binary.resolve()
    identity = subprocess.check_output([str(binary), '--version'], text=True).strip()
    if not identity.startswith('Rustidian '):
        parser.error('Expected a Rustidian executable')
    tool = os.environ.get('APPIMAGETOOL') or shutil.which('appimagetool')
    if not tool:
        parser.error('appimagetool not found')
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='rustidian-appdir-') as temporary:
        appdir = Path(temporary) / 'Rustidian.AppDir'
        for directory in ('usr/bin', 'usr/share/rustidian', 'usr/share/doc/rustidian', 'usr/share/metainfo'):
            (appdir / directory).mkdir(parents=True)
        shutil.copy2(binary, appdir / 'usr/bin/rustidian')
        shutil.copytree(root / 'demo-vault', appdir / 'usr/share/rustidian/demo-vault')
        documents = appdir / 'usr/share/doc/rustidian'
        shutil.copy2(root / 'LICENSE', documents / 'copyright')
        shutil.copy2(args.notices, documents / 'THIRD_PARTY_NOTICES.txt')
        for license in (root / 'assets/fonts').glob('LICENSE-*.txt'):
            shutil.copy2(license, documents / license.name)
        shutil.copy2(root / 'packaging/rustidian.metainfo.xml', appdir / 'usr/share/metainfo/io.github.masihmoafi.rustidian.appdata.xml')
        shutil.copy2(root / 'docs/brand/rustidian.png', appdir / 'rustidian.png')
        (appdir / 'rustidian.desktop').write_text(DESKTOP)
        (appdir / 'AppRun').write_text(APPRUN)
        (appdir / 'AppRun').chmod(0o755)
        subprocess.run(['desktop-file-validate', str(appdir / 'rustidian.desktop')], check=True)
        subprocess.run([tool, str(appdir), str(args.output.resolve())],
                       check=True, env=dict(os.environ, ARCH='x86_64', APPIMAGE_EXTRACT_AND_RUN='1'))
    digest = hashlib.file_digest(args.output.open('rb'), 'sha256').hexdigest()
    args.output.with_suffix(args.output.suffix + '.sha256').write_text(digest + '  ' + args.output.name + '\n')
    print(args.output)


if __name__ == '__main__':
    main()
