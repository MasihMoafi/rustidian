"""Exercise actual CEF paint/DOM/native-click behavior on a disposable X11 display."""
from pathlib import Path
import argparse
import hashlib
import json
import os
import signal
import shutil
import subprocess
import tempfile
import time

from PIL import Image, ImageChops

HERE = Path(__file__).resolve().parent
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--binary', type=Path, required=True)
parser.add_argument('--cef-path', type=Path, required=True)
parser.add_argument('--report', type=Path, required=True)
args = parser.parse_args()
assert os.getpgrp() == os.getpid(), 'Run through the owned thermal-run process group'
binary = args.binary.resolve()
report = args.report.resolve()
report.mkdir(parents=True, exist_ok=True)
fixture_hashes = {str(p.relative_to(HERE)): hashlib.sha256(p.read_bytes()).hexdigest()
                  for p in (HERE / 'fixture').rglob('*') if p.is_file()}
results = {'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(), 'cases': {},
           'scope': 'Isolated browser engine only, not an embedded Rustidian tab or plugin API'}
with tempfile.TemporaryDirectory(prefix='rustidian-html-proof-') as directory:
    base = Path(directory)
    shutil.copytree(HERE / 'fixture', base / 'positive')
    shutil.copytree(HERE / 'fixture', base / 'disabled')
    negative = base / 'disabled/index.html'
    negative.write_text(negative.read_text().replace('<button id="increment"', '<button disabled id="increment"'))
    read_fd, write_fd = os.pipe()
    with (report / 'xvfb.log').open('w') as log:
        server = subprocess.Popen(['Xvfb', '-displayfd', str(write_fd), '-screen', '0',
                                   '960x640x24', '-nolisten', 'tcp', '-ac', '-noreset'],
                                  pass_fds=(write_fd,), stdout=subprocess.DEVNULL, stderr=log)
        os.close(write_fd)
        try:
            with os.fdopen(read_fd) as display_file:
                display = ':' + display_file.readline().strip()
            assert display[1:].isdigit() and server.poll() is None
            env = dict(os.environ, DISPLAY=display, WAYLAND_DISPLAY='', XDG_SESSION_TYPE='x11', GDK_BACKEND='x11',
                       LD_LIBRARY_PATH=str(args.cef_path.resolve()),
                       XDG_CONFIG_HOME=str(base / 'config'), XDG_CACHE_HOME=str(base / 'cache'),
                       XDG_STATE_HOME=str(base / 'state'), XDG_DATA_HOME=str(base / 'data'),
                       DBUS_SESSION_BUS_ADDRESS='unix:path=' + str(base / 'no-bus'))
            for name, count in [('positive', '1'), ('disabled', '0')]:
                before = report / (name + '-before.png')
                after = report / (name + '-after.png')
                started = time.monotonic()
                # Keep every CEF child inside the thermal runner's owned group.
                # Regular logs avoid communicate() hanging on inherited pipes.
                with (report / (name + '.stdout')).open('w') as stdout, (report / (name + '.stderr')).open('w') as stderr:
                    process = subprocess.Popen([str(binary), '--file', str(base / name / 'index.html'),
                                                '--profile', str(base / ('profile-' + name)),
                                                '--width', '960', '--height', '640', '--output', str(before),
                                                '--click-id', 'increment', '--after-output', str(after)],
                                               env=env, stdout=stdout, stderr=stderr)
                    try:
                        process.wait(timeout=45)
                    except subprocess.TimeoutExpired:
                        # Stop the whole owned group; the thermal supervisor
                        # also kills/reaps remaining group members on exit.
                        signal.signal(signal.SIGTERM, signal.SIG_IGN)
                        os.killpg(os.getpgrp(), signal.SIGTERM)
                        try:
                            process.wait(timeout=2)
                        except subprocess.TimeoutExpired:
                            process.kill()
                            process.wait()
                        raise AssertionError('CEF helper timed out; owned group terminated')
                assert process.returncode == 0, f'{name}: host exit{process.returncode}; see stderr'
                observations = [json.loads(line) for line in (report / (name + '.stdout')).read_text().splitlines() if line.startswith('{')]
                loaded = next(item for item in observations if item['phase'] == 'loaded')
                clicked = next(item for item in observations if item['phase'] == 'clicked')
                assert loaded['title'] == 'Rustidian local HTML fixture'
                assert loaded['count'] == '0' and clicked['count'] == count
                assert loaded['background'] == 'rgb(23, 33, 38)', loaded['background']
                assert loaded['images'][0]['width'] > 0 and loaded['images'][0]['height'] > 0
                assert clicked['status'] == ('Clicked' if name == 'positive' else 'Ready')
                first, last = Image.open(before).convert('RGB'), Image.open(after).convert('RGB')
                assert first.size == last.size == (960, 640)
                # Stay inside the page, clear of the browser's right scrollbar.
                assert first.getpixel((900, 620)) == (23, 33, 38)
                rect = next(control['rect'] for control in loaded['controls'] if control['id'] == 'status')
                box = tuple(round(value) for value in (rect['x'], rect['y'], rect['x'] + rect['width'], rect['y'] + rect['height']))
                changed_status = ImageChops.difference(first.crop(box), last.crop(box)).getbbox() is not None
                assert changed_status == (name == 'positive'), 'Painted status text did not match actual click behavior'
                results['cases'][name] = {'status': 'passed', 'seconds': round(time.monotonic() - started, 3),
                                          'loaded': loaded, 'clicked': clicked,
                                          'captures': {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in [before, after]}}
                (report / 'result.json').write_text(json.dumps(results, indent=2) + '\n')
        finally:
            server.terminate()
            try:
                server.wait(timeout=3)
            except subprocess.TimeoutExpired:
                server.kill()
                server.wait()
assert all(hashlib.sha256((HERE / name).read_bytes()).hexdigest() == expected for name, expected in fixture_hashes.items())
results.update(status='passed', source_fixture_unchanged=True)
(report / 'result.json').write_text(json.dumps(results, indent=2) + '\n')
print(json.dumps({'status': results['status'], 'cases': list(results['cases']), 'binary_sha256': results['binary_sha256']}))
