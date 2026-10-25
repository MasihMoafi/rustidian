"""Check real package contents and lifecycle; --system is for the disposable Actions runner."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile


def run(*command, **kwargs):
    return subprocess.run(command, check=True, capture_output=True, text=True, **kwargs)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--notices', type=Path, required=True)
    parser.add_argument('--system', action='store_true')
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    original = hashlib.sha256(args.binary.read_bytes()).hexdigest()
    with tempfile.TemporaryDirectory(prefix='rustidian-debian-check-') as temporary:
        base = Path(temporary)
        package = base / 'rustidian.deb'
        run('python3', str(root / 'scripts/package-deb.py'), '--binary', str(args.binary), '--notices', str(args.notices), '--output', str(package))
        assert run('dpkg-deb', '--field', str(package), 'Package').stdout.strip() == 'rustidian'
        assert run('dpkg-deb', '--field', str(package), 'Architecture').stdout.strip() == 'amd64'
        extracted = base / 'extracted package'
        run('dpkg-deb', '--extract', str(package), str(extracted))
        binary = extracted / 'usr/lib/rustidian/rustidian'
        assert hashlib.sha256(binary.read_bytes()).hexdigest() == original
        launcher = extracted / 'usr/bin/rustidian'
        assert run(str(launcher), '--version').stdout.startswith('Rustidian ')
        home = base / 'isolated home'; home.mkdir()
        runtime = base / 'runtime'; runtime.mkdir(mode=0o700)
        environment = dict(os.environ, HOME=str(home), XDG_DATA_HOME=str(home/'data'), XDG_STATE_HOME=str(home/'state'), XDG_CONFIG_HOME=str(home/'config'), XDG_RUNTIME_DIR=str(runtime), DISPLAY='', WAYLAND_DISPLAY='', DBUS_SESSION_BUS_ADDRESS='unix:path='+str(base/'no-bus'))
        environment.pop('RUSTIDIAN_DEFAULT_VAULT', None)
        environment.pop('RUST_NOTES_DEFAULT_VAULT', None)
        # Real first launch prepares a user-owned demo before the GUI correctly fails without a display.
        first = subprocess.run([str(launcher)], env=environment, capture_output=True, text=True)
        assert first.returncode != 0, 'A display is deliberately unavailable in this packaging check'
        demo = home / 'data/rustidian/demo-vault/Home.md'
        assert demo.read_bytes() == (root/'demo-vault/Home.md').read_bytes(), first.stderr
        demo.write_text('User demo edits survive upgrades\n')
        subprocess.run([str(launcher)], env=environment, capture_output=True, text=True)
        assert demo.read_text() == 'User demo edits survive upgrades\n'
        vault = home / 'My vault'; vault.mkdir(); note = vault / 'Keep.md'; note.write_text('# Keep\n\nسلام\n')
        recovery = home/'state/rustidian/recovery.json'; recovery.parent.mkdir(parents=True, exist_ok=True); recovery.write_text('{"saved":true}\n')
        protected = {path: path.read_bytes() for path in (note, demo, recovery)}
        assert 'notes=1' in run(str(launcher), '--check-vault', str(vault), env=environment).stdout
        if args.system:
            try:
                run('sudo', 'dpkg', '--install', str(package))
                assert run('/usr/bin/rustidian', '--version').stdout.startswith('Rustidian ')
                run('python3', str(root/'scripts/package-deb.py'), '--binary', str(args.binary), '--notices', str(args.notices), '--output', str(package), '--revision', '2')
                run('sudo', 'dpkg', '--install', str(package))
                assert run('dpkg-query', '--show', '--showformat=${Version}', 'rustidian').stdout.endswith('-2')
                run('sudo', 'dpkg', '--remove', 'rustidian')
                assert not Path('/usr/bin/rustidian').exists()
                run('sudo', 'dpkg', '--purge', 'rustidian')
            finally:
                subprocess.run(['sudo', 'dpkg', '--purge', 'rustidian'], capture_output=True)
        assert all(path.read_bytes() == expected for path, expected in protected.items())
        print(json.dumps({'status':'passed','system_install_upgrade_remove_purge':args.system,'exact_binary_sha256':original,'user_notes_recovery_and_demo_retained':True}))


if __name__ == '__main__':
    main()
