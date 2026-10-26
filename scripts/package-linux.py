#!/usr/bin/env python3
"""Package the exact verified executable with the Linux installer and demo."""
import argparse
import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--notices', type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    version = subprocess.check_output([str(args.binary.resolve()), '--version'], text=True).strip()
    if not version.startswith('Rustidian '):
        parser.error('Expected a verified Rustidian executable')
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='rustidian-package-') as temporary:
        bundle = Path(temporary)
        shutil.copy2(args.binary, bundle / 'rustidian')
        shutil.copy2(root / 'scripts/install-linux.py', bundle / 'install.py')
        shutil.copy2(root / 'docs/brand/rustidian.png', bundle / 'rustidian.png')
        shutil.copy2(root / 'LICENSE', bundle / 'LICENSE')
        if args.notices:
            shutil.copy2(args.notices, bundle / 'THIRD_PARTY_NOTICES.txt')
        for license in (root / 'assets/fonts').glob('LICENSE-*.txt'):
            shutil.copy2(license, bundle / license.name)
        shutil.copytree(root / 'demo-vault', bundle / 'demo-vault')
        (bundle / 'START_HERE.txt').write_text(
            version + '\n\nRun ./rustidian to try the included demo.\n'
            'Install to your applications menu: python3 install.py\n'
            'Open a copied vault: ./rustidian "/path/to/vault copy"\n\n'
            'Linux desktop notebook. Existing Obsidian community plugins are unsupported.\n'
            'Report a problem: https://github.com/MasihMoafi/rustidian/issues/new/choose\n')
        def public_metadata(info):
            info.uid = info.gid = 0
            info.uname = info.gname = 'root'
            info.mtime = int(os.environ.get('SOURCE_DATE_EPOCH', '0'))
            return info

        with tarfile.open(args.output, 'w:gz') as archive:
            for path in sorted(bundle.iterdir()):
                archive.add(path, arcname=path.name, filter=public_metadata)
    digest = hashlib.file_digest(args.output.open('rb'), 'sha256').hexdigest()
    args.output.with_name(args.output.name + '.sha256').write_text(digest + '  ' + args.output.name + '\n')
    print(str(args.output))


if __name__ == '__main__':
    main()
