"""Check the installed launcher's PATH, which a dock-launched Agent tab searches for agents."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
NODE = '/usr/local/node/bin'
DOCK_PATH = '/usr/local/bin:/usr/bin:/bin'


def main():
    with tempfile.TemporaryDirectory(prefix='rustidian-launcher-') as temporary:
        base = Path(temporary)
        bundle = base / 'bundle'
        (bundle / 'demo-vault').mkdir(parents=True)
        shutil.copy2(ROOT / 'scripts/install-linux.py', bundle / 'install.py')
        for name in ['rustidian.png', 'LICENSE']:
            (bundle / name).write_text('stand-in\n')
        # A stand-in app: answers the installer's identity check, otherwise reports its PATH.
        app = bundle / 'rustidian'
        app.write_text('#!/bin/sh\nif [ "$1" = --version ]; then echo "Rustidian 0.0.0"; else printf %s "$PATH"; fi\n')
        app.chmod(0o755)
        prefix = base / 'prefix'
        subprocess.run(['python3', str(bundle / 'install.py'), '--prefix', str(prefix)], check=True, capture_output=True)

        def launched_path(home, path):
            environment = {'HOME': str(home), 'PATH': path}
            return subprocess.run([str(prefix / 'bin/rustidian')], env=environment, check=True,
                                  capture_output=True, text=True).stdout

        home = base / 'home'
        local_bin = home / '.local/bin'
        local_bin.mkdir(parents=True)
        node = [NODE] if os.path.isdir(NODE) else []
        # The shell's order: ~/.local/bin, then node, then the dock's own folders.
        expected = ':'.join([str(local_bin), *node, DOCK_PATH])
        assert launched_path(home, DOCK_PATH) == expected, launched_path(home, DOCK_PATH)
        # Folders already on PATH stay where they are, once.
        present = ':'.join([DOCK_PATH, str(local_bin), NODE])
        assert launched_path(home, present) == present
        # A folder that does not exist is not added.
        assert launched_path(base / 'no home', DOCK_PATH) == ':'.join([*node, DOCK_PATH])
        print('Launcher PATH checks passed: ~/.local/bin and ' + NODE + ' prepended in shell order, '
              'present folders kept once, missing folders skipped' + ('' if node else ' (' + NODE + ' absent here)') + '.')


if __name__ == '__main__':
    main()
