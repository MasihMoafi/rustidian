#!/usr/bin/env python3
"""Install an extracted Rustidian bundle for the current user, without root."""
import argparse
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import tempfile


MIME_TYPES = ('text/markdown;text/x-markdown;text/plain;text/rust;'
              'text/x-c;text/x-csrc;text/x-chdr;text/x-c++;text/x-c++src;text/x-c++hdr;'
              'text/x-python;text/x-python3;text/javascript;application/javascript;'
              'text/x-java;text/x-go;application/x-ruby;application/x-shellscript;'
              'text/x-shellscript;application/json;application/toml;application/yaml;'
              'text/yaml;application/xml;text/xml;text/css;text/html;application/sql;')


def atomic_copy(source, destination, mode):
    destination.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(dir=destination.parent, delete=False) as output:
        temporary = Path(output.name)
        with source.open('rb') as incoming:
            shutil.copyfileobj(incoming, output)
        os.fchmod(output.fileno(), mode)
    os.replace(temporary, destination)


def desktop_string(value):
    return str(value).replace('\\', '\\\\').replace('\n', '\\n').replace('\r', '\\r').replace('\t', '\\t')


def desktop_exec(value):
    # Desktop Entry quoting has two escaping layers and treats % as a field code.
    value = str(value).replace('%', '%%')
    for character in ['\\', '"', '`', '$']:
        value = value.replace(character, '\\' + character)
    return desktop_string('"' + value + '"')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--prefix', type=Path, help='Install prefix (default: ~/.local)')
    parser.add_argument('--default-vault', type=Path, help='Open this folder when no file is passed')
    args = parser.parse_args()
    bundle = Path(__file__).resolve().parent
    binary = bundle / 'rustidian'
    for name in ['rustidian', 'rustidian.png', 'demo-vault', 'LICENSE']:
        if not (bundle / name).exists():
            parser.error('Incomplete bundle: missing ' + name)
    if args.default_vault and not args.default_vault.is_dir():
        parser.error('--default-vault must be an existing folder')
    # Validate before changing an existing installation.
    version = subprocess.run([str(binary), '--version'], check=True, capture_output=True, text=True).stdout.strip()
    if not version.startswith('Rustidian '):
        parser.error('This bundle does not contain a Rustidian executable')
    prefix = (args.prefix or Path.home() / '.local').expanduser().resolve()
    data = prefix / 'share' if args.prefix else Path(os.environ.get('XDG_DATA_HOME', prefix / 'share'))
    target = prefix / 'lib/rustidian'
    launcher = prefix / 'bin/rustidian'
    atomic_copy(binary, target / 'rustidian', 0o755)
    for name in ['LICENSE', 'THIRD_PARTY_NOTICES.txt']:
        if (bundle / name).is_file():
            atomic_copy(bundle / name, target / name, 0o644)
    # Existing demo edits belong to the user after the first installation.
    if not (target / 'demo-vault').exists():
        shutil.copytree(bundle / 'demo-vault', target / 'demo-vault')
    launcher.parent.mkdir(parents=True, exist_ok=True)
    default = ''
    if args.default_vault:
        default = ('if [ -z "${RUSTIDIAN_DEFAULT_VAULT+x}" ]; then\n'
                   '    export RUSTIDIAN_DEFAULT_VAULT=' + shlex.quote(str(args.default_vault.resolve())) + '\nfi\n')
    script = '#!/bin/sh\n' + default + 'exec ' + shlex.quote(str(target / 'rustidian')) + ' "$@"\n'
    with tempfile.TemporaryDirectory(prefix='rustidian-install-') as scratch:
        staging = Path(scratch) / 'launcher'
        staging.write_text(script)
        atomic_copy(staging, launcher, 0o755)
        icon = data / 'icons/hicolor/256x256/apps/rustidian.png'
        atomic_copy(bundle / 'rustidian.png', icon, 0o644)
        os.utime(data / 'icons/hicolor')  # icon themes rescan on the folder's time
        entry = ('[Desktop Entry]\nType=Application\nName=Rustidian\n'
                 'Comment=Write and connect local Markdown notes\n'
                 'Exec=' + desktop_exec(launcher) + ' %f\n'
                 'Icon=rustidian\nTerminal=false\n'
                 'Categories=Office;TextEditor;\nKeywords=Markdown;Notes;Vault;\n'
                 'StartupWMClass=rustidian\nMimeType=' + MIME_TYPES + '\n')
        staging.write_text(entry)
        applications = data / 'applications'
        atomic_copy(staging, applications / 'rustidian.desktop', 0o644)
    update = shutil.which('update-desktop-database')
    if update:
        subprocess.run([update, str(applications)], check=True)
    print(version + ' installed. Open Rustidian from your applications menu.')
    print('Command: ' + str(launcher))


if __name__ == '__main__':
    main()
