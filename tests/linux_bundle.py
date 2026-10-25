"""Exercise the real Linux package and installer without touching the user profile."""
import argparse
import hashlib
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile


def run(*command, **kwargs):
    return subprocess.run(command, check=True, text=True, capture_output=True, **kwargs)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', type=Path, required=True)
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    with tempfile.TemporaryDirectory(prefix='rustidian-bundle-') as temporary:
        base = Path(temporary)
        archive = base / 'rustidian-linux-x86_64.tar.gz'
        run('python3', str(root / 'scripts/package-linux.py'), '--binary', str(args.binary), '--output', str(archive))
        checksum = archive.with_name(archive.name + '.sha256').read_text().split()[0]
        with archive.open('rb') as incoming:
            assert hashlib.file_digest(incoming, 'sha256').hexdigest() == checksum
        bundle = base / 'extracted bundle'
        with tarfile.open(archive) as package:
            package.extractall(bundle, filter='data')
        assert (bundle / 'demo-vault/Home.md').read_bytes() == (root / 'demo-vault/Home.md').read_bytes()
        prefix = base / 'Local "notes" $safe %app `quoted`'
        vault = base / 'Vault سلام'
        vault.mkdir()
        note = vault / 'Keep.md'
        note.write_text('# Keep\n\nسلام [[Other]]\n')
        original = note.read_bytes()
        environment = dict(os.environ, XDG_DATA_HOME=str(base / 'unrelated XDG data'))
        command = ['python3', str(bundle / 'install.py'), '--prefix', str(prefix), '--default-vault', str(vault)]
        run(*command, env=environment)
        launcher = prefix / 'bin/rustidian'
        assert run(str(launcher), '--version').stdout.startswith('Rustidian ')
        assert 'notes=1' in run(str(launcher), '--check-vault', str(vault)).stdout
        assert note.read_bytes() == original
        demo = prefix / 'lib/rustidian/demo-vault/Home.md'
        demo.write_text('User demo edits stay here\n')
        run(*command, env=environment)
        assert demo.read_text() == 'User demo edits stay here\n'
        entry = prefix / 'share/applications/rustidian.desktop'
        run('desktop-file-validate', str(entry))
        source = base / 'Open with.rs'
        source.write_text('fn main() {}\n')
        detected_type = run('xdg-mime', 'query', 'filetype', str(source)).stdout.strip()
        registered = next(line.removeprefix('MimeType=') for line in entry.read_text().splitlines() if line.startswith('MimeType=')).split(';')
        assert detected_type in registered, ('Rust source is missing from Open With', detected_type, registered)
        # An invalid replacement must fail before changing the installed app.
        protected = (prefix / 'lib/rustidian/rustidian').read_bytes()
        (bundle / 'rustidian').rename(bundle / 'missing-executable')
        failure = subprocess.run(command, env=environment, text=True, capture_output=True)
        assert failure.returncode != 0
        assert (prefix / 'lib/rustidian/rustidian').read_bytes() == protected
        assert note.read_bytes() == original
        assert not (base / 'unrelated XDG data').exists(), '--prefix must isolate the whole installation'
        print('Package checks passed: checksum, exact demo, quoted paths, CLI, reinstall, desktop entry and failed replacement.')


if __name__ == '__main__':
    main()
