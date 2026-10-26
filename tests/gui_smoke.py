#!/usr/bin/env python3
"""Exercise the real native window on a disposable vault; needs Xvfb, xdotool, Pillow."""
import argparse
from datetime import datetime
import fcntl
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import traceback

from PIL import Image, ImageChops, ImageGrab

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
from native_input import UnicodeKeyboard, modified_shortcut, save_shortcut, wait_autosave


def pixel_data(picture):
    """Support the Pillow version supplied by the declared Ubuntu runner."""
    modern = getattr(picture, "get_flattened_data", None)
    return modern() if modern else picture.getdata()


def luminance(rgb):
    channels = [value / 255 for value in rgb[:3]]
    linear = [value / 12.92 if value <= .04045 else ((value + .055) / 1.055) ** 2.4 for value in channels]
    return sum(value * weight for value, weight in zip(linear, (.2126, .7152, .0722)))


def pixel_contrast(first, second):
    first, second = luminance(first), luminance(second)
    return (max(first, second) + .05) / (min(first, second) + .05)


def run_checks(checks, on_failure, *, count_checks=None, on_success=None):
    failures = []
    for check in checks:
        before=count_checks() if count_checks is not None else 0
        try:
            check()
            completed=count_checks()-before if count_checks is not None else 0
            if count_checks is not None:
                assert completed>0,f"No behavioral checks completed: {check.__name__}"
            if on_success is not None:
                on_success(check.__name__,completed)
        except Exception as error:
            failures.append(check.__name__)
            on_failure(check.__name__, error)
    assert not failures, f"Native workflow failures: {', '.join(failures)}"


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", default="target/debug/rustidian")
    parser.add_argument("--default-vault", action="store_true", help="Test an extracted executable beside its sample vault, without a vault argument")
    parser.add_argument("--open-file", choices=["md", "txt", "rs", "py", "html", "json", "toml"], help="Check opening and saving an individual document")
    parser.add_argument("--scroll-document", type=Path, help="Read-only source copied into the private scrolling fixture")
    parser.add_argument("--section", choices=["all", "system-open", "graph", "graph-state", "emoji", "properties", "images", "tasks", "task-clipping", "large-search", "folders", "explorer", "vault-switch", "vaults", "tab-state", "file-toolbar", "folding", "typography", "font-controls", "visual-controls", "settings", "code", "tab-overflow", "tab-drag", "tab-keys", "note-dialogs", "syntax", "keyboard-focus", "reading-code", "tables", "inline-styles", "table-edit", "table-links", "table-input", "table-resize", "table-edges", "trash-options", "palettes", "callouts", "persian", "attachments", "bookmarks", "essential", "colors", "native-highlights", "page-preview", "quick-ranking", "navigation", "hover", "title", "scrolling", "diagrams", "chrome", "note-aids", "note-creators", "tab-actions", "tab-restore", "file-copy", "file-move", "workspaces", "recovery", "palette-queued", "unicode-queued", "undo"], default="all")
    args = parser.parse_args()
    reports = ROOT / "reports"
    reports.mkdir(exist_ok=True)
    run_lock = (reports / ".gui-smoke.lock").open("w")
    try:
        fcntl.flock(run_lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except BlockingIOError:
        raise SystemExit("A GUI run is already active; wait for its result.")
    with tempfile.TemporaryDirectory(prefix="rustidian-gui-") as temporary:
        base = Path(temporary)
        fixture = Path(temporary) / ("demo-vault" if args.default_vault else "Test Vault")
        shutil.copytree(ROOT / "demo-vault", fixture)
        (fixture / "Assets").mkdir(exist_ok=True)
        attachment = fixture / "Assets/Attachment.png"
        image_color = (63, 163, 122)
        Image.new("RGB", (600, 240), image_color).save(attachment)
        read_fd, write_fd = os.pipe()
        xvfb_log = (reports / "gui-xvfb.log").open("w")
        server = subprocess.Popen(
            ["Xvfb", "-displayfd", str(write_fd), "-screen", "0", "1440x960x24", "-nolisten", "tcp", "-ac", "-noreset"],
            pass_fds=(write_fd,), stdout=subprocess.DEVNULL, stderr=xvfb_log,
        )
        os.close(write_fd)
        with os.fdopen(read_fd) as display_file:
            display = ":" + display_file.readline().strip()
        assert display[1:].isdigit() and server.poll() is None, (reports / "gui-xvfb.log").read_text()
        environment = dict(os.environ, DISPLAY=display, WAYLAND_DISPLAY="", WINIT_UNIX_BACKEND="x11", XDG_CONFIG_HOME=str(base / "config"), XDG_STATE_HOME=str(base / "state"), XDG_DATA_HOME=str(base / "data"), GSETTINGS_BACKEND="memory", RUSTIDIAN_UI_PROBE=str(base / "ui-probe.json"))
        if args.section in ("palettes", "all"):
            launcher = base / "data/applications/rustidian.desktop"
            environment["XDG_DATA_DIRS"] = str(base / "system data") + os.pathsep + os.environ.get("XDG_DATA_DIRS", "/usr/local/share:/usr/share")
            system_launcher = base / "system data/applications/rustidian.desktop"
            system_launcher.parent.mkdir(parents=True, exist_ok=True)
            original_launcher = "[Desktop Entry]\nType=Application\nName=Rustidian\nExec=/usr/bin/true %f\nIcon=old-teal\nStartupWMClass=rustidian\nActions=Example;\n\n[Desktop Action Example]\nName=Keep this action\nExec=/usr/bin/true\nIcon=keep-action-icon\n"
            system_launcher.write_text(original_launcher)
        # Keep native GTK pickers on this test display instead of the user's portal.
        environment.update(GDK_BACKEND="x11", GTK_USE_PORTAL="0",
            DBUS_SESSION_BUS_ADDRESS=f"unix:path=/tmp/rustidian-gui-no-bus-{os.getpid()}")
        display_ready = False
        for _ in range(30):
            geometry = subprocess.run(
                ["xdotool", "getdisplaygeometry"], env=environment,
                text=True, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
            )
            if geometry.returncode == 0:
                display_ready = True
                xvfb_log.write(f"Allocated {display}, geometry {geometry.stdout.strip()}\n")
                xvfb_log.flush()
                break
            assert server.poll() is None, (reports / "gui-xvfb.log").read_text()
            time.sleep(.1)
        assert display_ready, f"Private X server {display} did not accept a connection"
        # Thermal SIGSTOP can span a key release. Keep the private display from
        # inventing held-key repeats when the owned process group resumes.
        subprocess.run(["xset", "r", "off"], env=environment, check=True)
        log = (reports / "gui-smoke.log").open("w")
        if args.section in ("navigation", "all"):
            browser = base / "browser-stub"
            browser.write_text("#!/usr/bin/python3\nimport sys\nfrom pathlib import Path\nwith Path(" + repr(str(reports / "browser-requests.txt")) + ").open('a') as f: f.write(repr(sys.argv[1:])+'\\n')\n")
            browser.chmod(0o700)
            environment["BROWSER"] = str(browser)
        if args.section == "chrome":
            for path in ["__pycache__", ".git", "Notes/__pycache__"]:
                folder = fixture / path
                folder.mkdir(parents=True, exist_ok=True)
                (folder / "Cache.md").write_text("Never index this generated file.\n")
            (fixture / ".gitignore").write_text("__pycache__/\n")
            (fixture / ".gitignore.md").write_text("Hidden file.\n")
            (fixture / "Cache notes.md").write_text("Keep this ordinary note.\n")
        window_manager = None
        unicode_keyboard = None
        try:
            if os.environ.get("RUSTIDIAN_STABLE_UNICODE", "1") == "1":
                unicode_keyboard = UnicodeKeyboard(environment, Path(__file__).read_text())
        except Exception:
            server.terminate()
            server.wait(timeout=5)
            log.close()
            xvfb_log.close()
            raise
        environment.pop("RUSTIDIAN_DEFAULT_VAULT",None)
        if args.section == "chrome" and shutil.which("openbox"):
            window_manager = subprocess.Popen(["openbox", "--sm-disable"], env=environment,
                stdout=log, stderr=log)
            time.sleep(.4)
        launch_command = [str(ROOT / args.binary), str(fixture)]
        if args.open_file:
            startup_path = fixture / f"Open me سلام.{args.open_file}"
            startup_text = "fn main() {\n    // Original text سلام\n    let value = 42;\n    println!(\"Hello\");\n}\n" if args.open_file == "rs" else "# File opening\n\nOriginal text سلام\n"
            startup_path.write_text(startup_text)
            launch_command = [str(ROOT / args.binary), str(startup_path)]
        if args.default_vault:
            bundled_binary = base / "rustidian"
            shutil.copy2(ROOT / args.binary, bundled_binary)
            launch_command = [str(bundled_binary)]
        def launch_app(command):
            (base / "ui-probe.json").unlink(missing_ok=True)
            return subprocess.Popen(command, cwd=base, env=environment, stdout=log, stderr=log)

        app = launch_app(launch_command)
        results = []
        (reports / "gui-results.json").write_text(json.dumps({"status": "running", "checks": []}, indent=2)+"\n")

        def xdo(*arguments):
            return subprocess.check_output(["xdotool", *arguments], env=environment, text=True, stderr=subprocess.DEVNULL).strip()

        def pause():
            time.sleep(0.25)
            assert app.poll() is None, "The native app exited unexpectedly"

        def key(value):
            if value == "ctrl+s":
                save_shortcut(xdo)
            else:
                xdo("key", "--clearmodifiers", value)
            pause()

        def type_text(value, delay=1):
            for index, line in enumerate(value.split("\n")):
                if index: xdo("key", "--clearmodifiers", "Return")
                if line: xdo("type", "--clearmodifiers", "--delay", str(delay), "--", line)
            pause()

        def paste_text(value, *, held=False):
            # xdotool temporarily remaps scratch keys for Unicode. Exercise the
            # real UTF-8 clipboard path without racing those keymap changes.
            owner = subprocess.Popen(
                ["xclip", "-selection", "clipboard", "-in", "-quiet"],
                stdin=subprocess.PIPE, stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL, env=environment,
            )
            try:
                owner.stdin.write(value.encode())
                owner.stdin.close()
                pause()
                actual = subprocess.check_output(
                    ["xclip", "-selection", "clipboard", "-out", "-target", "UTF8_STRING"],
                    env=environment, timeout=5,
                )
                assert actual == value.encode(), (actual, value)
                if held:
                    modified_shortcut(xdo, "v")
                    pause()
                else:
                    key("ctrl+v")
            finally:
                owner.terminate()
                owner.wait(timeout=5)

        def queued_steps(*steps):
            # Deliver a known event sequence in one UI frame, before autosave.
            app.send_signal(signal.SIGSTOP)
            try:
                for kind, value in steps:
                    if kind == "text": xdo("type", "--clearmodifiers", "--delay", "0", "--", value)
                    elif kind == "click": xdo("mousemove", "--sync", *map(str, value), "click", "1")
                    else: xdo("key", "--clearmodifiers", value)
            finally:
                app.send_signal(signal.SIGCONT)
            pause()

        def queued_text(text, *keys):
            queued_steps(("text", text), *(("key", value) for value in keys))

        def capture(name, size=(1440, 960)):
            ImageGrab.grab(xdisplay=display).crop((0, 0, *size)).save(reports / name)
            probe = base / "ui-probe.json"
            if probe.exists():
                shutil.copyfile(probe, reports / f"{Path(name).stem}-controls.json")

        def control_bounds(name, required=True):
            deadline = time.monotonic() + (3 if required else .1)
            while time.monotonic() < deadline:
                try:
                    controls = json.loads((base / "ui-probe.json").read_text())
                    if name in controls:
                        return controls[name]
                except (FileNotFoundError, json.JSONDecodeError):
                    pass
                time.sleep(.05)
            if required:
                capture("missing-control.png")
                try:
                    controls = json.loads((base / "ui-probe.json").read_text())
                except (FileNotFoundError, json.JSONDecodeError):
                    controls = {}
                focus = {key: value for key, value in controls.items() if "focus" in key}
                raise AssertionError(f"No visible control: {name}; X focus={xdo('getwindowfocus')}; focus={focus}; visible={list(controls)}")
            return None

        def click_control(name, button="1", *, hold=0):
            if name.startswith("settings-") and not name.startswith("settings-page:") and not control_bounds(name, required=False):
                page = ("Editor" if name == "settings-autosave" else
                        "Templates" if name.startswith("settings-template-") else
                        "Daily notes" if name.startswith("settings-daily-") else
                        "Unique note creator" if name.startswith("settings-unique-") else
                        "File recovery" if name.startswith("settings-recovery-") else None)
                if page and control_bounds("settings-page:"+page, required=False):
                    click_control("settings-page:"+page)
            left, top, right, bottom = control_bounds(name)
            x, y = round((left + right) / 2), round((top + bottom) / 2)
            pointer = ("mousemove", "--sync", str(x), str(y))
            if hold:
                xdo(*pointer)
                time.sleep(.1)
                current = control_bounds(name)
                assert current[0] <= x <= current[2] and current[1] <= y <= current[3], f"{name} moved away from the pointer before clicking: {current}"
                xdo("mousedown", button)
                try:
                    time.sleep(hold)
                finally:
                    xdo("mouseup", button)
            else:
                xdo(*pointer, "click", button)
            pause()

        def slider_pixels(name, label, dark):
            """Inspect the real thumb, including its idle, hover and held-drag states."""
            left, top, right, bottom = control_bounds(name)
            target = round(left + (right - left) * .35)
            middle = round((top + bottom) / 2)
            xdo("mousemove", "--sync", str(target), str(middle), "click", "1")
            pause()
            observations = []

            def inspect(state, expected_x):
                capture(f"native-slider-{label}-{'dark' if dark else 'light'}-{state}.png")
                picture = ImageGrab.grab(xdisplay=display).convert("RGB")
                background = picture.getpixel((round(left) - 3, middle))
                columns = []
                for x in range(max(0, expected_x - 16), min(picture.width, expected_x + 17)):
                    ys = [y for y in range(max(0, round(top) - 2), min(picture.height, round(bottom) + 3))
                          if pixel_contrast(picture.getpixel((x, y)), background) >= 3]
                    if ys:
                        columns.append((max(ys) - min(ys) + 1, x, min(ys), max(ys)))
                assert columns, ("A visible slider thumb must be painted", name, state)
                diameter, _, first, last = max(columns)
                centers = [x for height, x, _, _ in columns if height >= diameter - 1]
                center = round((min(centers) + max(centers)) / 2)
                fill = picture.getpixel((center, round((first + last) / 2)))
                observations.append({"state": state, "diameter": diameter, "center": center, "fill": fill,
                                     "fill_contrast": pixel_contrast(fill, background)})
                (reports / f"slider-{label}-{'dark' if dark else 'light'}.json").write_text(json.dumps(observations, indent=2) + "\n")
                assert 8 <= diameter <= 15, ("Slider thumbs must remain compact at 100% scale", name, observations[-1])
                assert pixel_contrast(fill, background) >= 1.5, ("The thumb needs a visible filled center as well as an outline", name, observations[-1])
                return center, round((first + last) / 2)

            xdo("mousemove", "--sync", "100", "20");pause()
            center, y = inspect("idle", target)
            xdo("mousemove", "--sync", str(center), str(y));pause()
            inspect("hover", center)
            xdo("mousedown", "1")
            try:
                xdo("mousemove", "--sync", str(center + 12), str(y));pause()
                moved, _ = inspect("drag", center + 12)
                assert moved >= center + 6, ("Dragging the visible thumb must actually move it", name, center, moved)
            finally:
                xdo("mouseup", "1")
            xdo("mousemove", "--sync", "100", "20");pause()

        def scrollbar_pixels(path, source):
            # Inspect the actual pixels, then drag that visible thumb.
            for dark in (True, False):
                set_theme(dark)
                xdo("mousemove", "100", "20")
                time.sleep(.3)
                viewport = control_bounds("document-viewport")
                image = ImageGrab.grab(xdisplay=display).convert("RGB")
                left, top, right, bottom = map(round, viewport)
                observations = []
                def thumb_pixels(picture, state):
                    background = picture.getpixel((right-24, (top+bottom)//2))
                    rows = []
                    for y in range(top+4, bottom-4):
                        xs = [x for x in range(right-2, min(picture.width, right+14))
                              if pixel_contrast(picture.getpixel((x,y)), background) >= 3]
                        if len(xs) >= 4:
                            rows.append((y, xs))
                    assert len(rows) >= 24, ("Scrollbar must remain visibly distinct from its page", dark, state, viewport, len(rows))
                    width = max(len(xs) for _, xs in rows)
                    assert width <= 6, ("Keep the original six-pixel thumb width in every state", dark, state, width)
                    y, xs = rows[len(rows)//2]
                    color = picture.getpixel((xs[len(xs)//2], y))
                    observations.append({"state": state, "width": width, "color": color})
                    (reports / f"scrollbar-{'dark' if dark else 'light'}-states.json").write_text(json.dumps(observations, indent=2) + "\n")
                    if dark:
                        assert max(color) <= 135, ("Hovering or dragging the dark scrollbar must keep it gray instead of white", observations[-1])
                    return rows
                rows = thumb_pixels(image, "idle")
                y, xs = rows[len(rows)//2]
                x = xs[len(xs)//2]
                capture("native-scrollbar-" + ("dark" if dark else "light") + ".png")
                before = control_bounds("document-scroll-y")[0]
                xdo("mousemove", "--sync", str(x), str(y))
                pause()
                capture("native-scrollbar-hover-" + ("dark" if dark else "light") + ".png")
                thumb_pixels(ImageGrab.grab(xdisplay=display).convert("RGB"), "hover")
                xdo("mousedown", "1")
                try:
                    for step in range(1, 11):
                        xdo("mousemove", "--sync", str(x), str(min(bottom-12, y+step*12)))
                        time.sleep(.025)
                        if step == 3:
                            pause()
                            capture("native-scrollbar-drag-" + ("dark" if dark else "light") + ".png")
                            thumb_pixels(ImageGrab.grab(xdisplay=display).convert("RGB"), "drag")
                finally:
                    xdo("mouseup", "1")
                pause()
                assert control_bounds("document-scroll-y")[0] > before+100, "Dragging the visible scrollbar must move the document"
                assert path.read_text() == source
            results.append("The six-pixel scrollbar stays readable in idle, hover and real drag states in both themes; dark thumbs stay gray and dragging moves the document without edits")

        def title_contains(value):
            deadline = time.monotonic() + 5
            while time.monotonic() < deadline:
                assert app.poll() is None, "The native app exited while opening a note"
                title = xdo("getwindowname", window)
                if value in title:
                    return
                time.sleep(.05)
            raise AssertionError((value, title))

        def open_note(value, *, title=None):
            key("ctrl+o")
            type_text(value)
            key("Return")
            title_contains(value if title is None else title)

        def ready_window():
            window = None
            # The thermal guard can pause both this controller and the app.
            for _ in range(200):
                assert app.poll() is None, (reports / "gui-smoke.log").read_text()
                try:
                    window = xdo("search", "--onlyvisible", "--name", "Rustidian$").splitlines()[0]
                    break
                except (subprocess.CalledProcessError, IndexError):
                    time.sleep(0.1)
            assert window, "No native window appeared"
            xdo("windowsize", window, "1440", "960")
            xdo("windowmove", window, "0", "0")
            # A mapped GL window can precede egui-winit's input state. Activating
            # it then loses FocusIn, since no window manager delivers a later
            # transition in Xvfb. Wait for the first actual UI, then activate once.
            probe = base / "ui-probe.json"
            for _ in range(1000):
                if probe.exists():
                    break
                assert app.poll() is None, "App exited before preparing its first UI"
                time.sleep(.02)
            assert probe.exists(), "The native window never prepared its first UI"
            xdo("windowfocus", "0")
            time.sleep(.1)
            if args.open_file:
                control_bounds("editor")
                unfocused_deadline = time.monotonic() + 3
                while control_bounds("window-focused")[0] != 0 and time.monotonic() < unfocused_deadline:
                    time.sleep(.02)
                assert control_bounds("window-focused")[0] == 0, "The negative startup case must be unfocused"
                assert control_bounds("editor-memory-focus")[0] == 1, "Opening a file must select its editor for input"
                assert control_bounds("editor-focus-request")[0] == 0, "Unfocused startup must finish its focus request without continuously reclaiming input"
            xdo("windowfocus", "--sync", window)
            assert xdo("getwindowfocus") == window, "The private test window must own keyboard input"
            pause()
            return window

        try:
            window = ready_window()
            if args.open_file:
                title_contains(startup_path.stem)
                control_bounds("editor-focused")
                key("ctrl+End")
                type_text("Saved from the editor")
                key("ctrl+s")
                expected = startup_text + "Saved from the editor"
                assert startup_path.read_text() == expected, repr(startup_path.read_text())
                assert (fixture / "Home.md").read_text() == (ROOT / "demo-vault/Home.md").read_text()
                capture(f"file-open-{args.open_file}.png")
                xdo("key", "--clearmodifiers", "ctrl+q")
                app.wait(timeout=5)
                second_path = fixture / f"Another file.{args.open_file}"
                second_text = "A different document\n"
                second_path.write_text(second_text)
                app = launch_app([str(ROOT / args.binary), str(second_path)])
                window = ready_window()
                title_contains(second_path.stem)
                control_bounds("editor-focused")
                key("ctrl+End")
                type_text("Second document saved")
                key("ctrl+s")
                capture(f"file-reopen-{args.open_file}.png")
                actual = second_path.read_text()
                assert actual == second_text + "Second document saved", repr(actual)
                assert startup_path.read_text() == expected
                # The last open note is a separate case from deleting one of many tabs.
                target_tab = "close-tab:" + str(second_path.relative_to(fixture))
                for _ in range(8):
                    controls = json.loads((base / "ui-probe.json").read_text())
                    others = [name for name in controls if name.startswith("tab-full:") and name != "tab-full:" + str(second_path.relative_to(fixture))]
                    if not others:
                        break
                    click_control(others[0])
                    key("ctrl+w")
                controls = json.loads((base / "ui-probe.json").read_text())
                assert len([name for name in controls if name.startswith("tab-full:")]) == 1, "The last-tab deletion case must start with exactly one tab"
                title_contains(second_path.stem)
                last_bytes = second_path.read_bytes()
                key("ctrl+p")
                type_text("Move file to system Trash")
                key("Return")
                key("Return")
                title_contains("New tab")
                assert not second_path.exists()
                assert any(path.read_bytes() == last_bytes for path in (base / "data/Trash/files").iterdir() if path.is_file())
                assert startup_path.read_text() == expected
                results.append(f"Individual .{args.open_file} files opened, saved and reopened; the last note became a blank tab after recoverable deletion")
                (reports / "gui-results.json").write_text(json.dumps({"checks": results, "status": "passed"}, indent=2)+"\n")
                print(json.dumps({"status": "passed", "checks": len(results), "open_file": args.open_file}))
                return
            title_contains("Home")
            capture("native-desktop.png")
            results.append("Native window opened; three-pane home captured")

            def command(value, *, available=True):
                def trace(stage):
                    if os.environ.get("RUSTIDIAN_TRACE_COMMANDS") == "1":
                        with (reports/"command-sequence.jsonl").open("a") as output:
                            controls=json.loads((base/"ui-probe.json").read_text())
                            relevant={name:bounds for name,bounds in controls.items() if name.startswith(("command:","dialog","workspace","pending-","editor","tab-full:"))}
                            output.write(json.dumps({"command":value,"expected_available":available,"stage":stage,"section":args.section,"pid":app.pid,"time":time.monotonic(),"controls":relevant,"x_focus":xdo("getwindowfocus")})+"\n")
                trace("before")
                key("ctrl+p")
                control_bounds("dialog-input-focused")
                trace("palette")
                type_text(value)
                expected={"srcedt":"Source editing","Split source":"Split source and preview"}.get(value,value)
                deadline=time.monotonic()+3
                while time.monotonic()<deadline:
                    controls=json.loads((base/"ui-probe.json").read_text())
                    matches={name:bounds for name,bounds in controls.items() if name.startswith("command:")}
                    wanted=matches.get("command:"+expected)
                    if not available and not matches:
                        break
                    if available and wanted is not None and wanted[1] == min(bounds[1] for bounds in matches.values()):
                        break
                    time.sleep(.05)
                else:
                    trace("query-not-selected")
                    capture("command-query-not-selected.png")
                    actual_query = None
                    if control_bounds("dialog-input-focused", required=False):
                        xdo("key", "--clearmodifiers", "ctrl+a", "ctrl+c")
                        time.sleep(.15)
                        try:
                            actual_query = subprocess.check_output(["xclip", "-selection", "clipboard", "-out", "-target", "UTF8_STRING"], env=environment, text=True, timeout=2)
                        except (subprocess.CalledProcessError, subprocess.TimeoutExpired):
                            pass
                    raise AssertionError(("Requested command availability/result differs",value,available,matches,actual_query))
                trace("query")
                key("Return")
                if not available:
                    assert control_bounds("dialog-input")
                    assert not any(name.startswith("command:") for name in json.loads((base/"ui-probe.json").read_text()))
                trace("result")

            theme_observations=[]
            def set_theme(dark):
                def sample():
                    return ImageGrab.grab(xdisplay=display).getpixel((5,100))[:3]
                before=sample()
                if (max(before)<128)!=dark:command("Switch appearance")
                after=sample()
                deadline=time.monotonic()+5
                while (max(after)<128)!=dark and time.monotonic()<deadline:
                    assert app.poll() is None, "App exited before drawing the requested appearance"
                    time.sleep(.05)
                    after=sample()
                theme_observations.append({"requested_dark":dark,"before_rgb":before,"after_rgb":after})
                (reports/"gui-theme-observations.json").write_text(json.dumps(theme_observations,indent=2)+"\n")
                assert (max(after)<128)==dark,(dark,before,after)

            def create_note_named(name):
                path = Path(name)
                key("ctrl+n")
                control_bounds("note-title-input")
                type_text(path.stem)
                key("Return")
                control_bounds("editor-focused")
                if path.parent != Path("."):
                    command("Rename or move file")
                    key("ctrl+a")
                    type_text(str(path))
                    key("Return")
                title_contains(path.stem)

            def close_tabs():
                for _ in range(32):
                    controls = json.loads((base / "ui-probe.json").read_text())
                    tabs = [name for name in controls if name.startswith("tab-full:")]
                    if not tabs:
                        break
                    click_control(tabs[0])
                    key("ctrl+w")
                controls = json.loads((base / "ui-probe.json").read_text())
                assert not any(name.startswith("tab-full:") for name in controls)

            def editor_end():
                bounds = control_bounds("editor", required=False)
                if bounds:
                    left, top, right, bottom = bounds
                    # A scrolled galley can extend behind the note header.
                    header_bottom = control_bounds("button:More options")[3]
                    visible_top = max(top, header_bottom + 1)
                    visible_bottom = min(bottom, 900)
                    assert visible_bottom > visible_top, bounds
                    xdo("mousemove", str(round((left + right) / 2)), str(round(min(visible_bottom - 1, visible_top + 20))), "click", "1")
                else:
                    xdo("mousemove", "650", "300", "click", "1")
                key("ctrl+End")

            def undo_checks():
                note = fixture / "Home.md"
                original = note.read_text()
                open_note("Home")
                for mode in ("Source editing", "Live preview"):
                    command(mode)
                    editor_end()
                    type_text("FIRST_UNDO_GROUP")
                    time.sleep(.35)  # Together with type_text: >500 ms, <1 second.
                    type_text("SECOND_UNDO_GROUP")
                    key("ctrl+z")
                    key("ctrl+s")
                    assert note.read_text() == original + "FIRST_UNDO_GROUP", (mode, "Undo reverted earlier typing", note.read_text()[-120:])
                    key("ctrl+y")
                    key("ctrl+s")
                    assert note.read_text() == original + "FIRST_UNDO_GROUPSECOND_UNDO_GROUP", (mode, "Redo")
                    key("ctrl+z")
                    key("ctrl+z")
                    key("ctrl+s")
                    assert note.read_text() == original, (mode, "second Undo")
                    results.append(mode + ": paused typing undoes one group at a time, redo restores it and exact original bytes survive")
                capture("native-undo-groups.png")

            def callout_checks():
                source = (ROOT / "tests/fixtures/callouts.md").read_text()
                note = fixture / "Notes/Callout checks.md"
                note.write_text(source)
                seed = fixture / "Notes/Callout state seed.md"
                seed.write_text("> [!tip]+ Previous document\n> PREVIOUS_CALLOUT_BODY\n")
                time.sleep(.6)
                open_note("Callout state seed")
                command("Reading view")
                control_bounds("callout-body:0")
                open_note("Callout checks")
                command("Source editing")
                assert not control_bounds("callout:0", required=False)
                editor_end()
                type_text("PROSE_CONTROL")
                key("ctrl+s")
                assert note.read_text() == source + "PROSE_CONTROL"
                key("ctrl+z")
                key("ctrl+s")
                assert note.read_text() == source
                results.append("Callout Source mode stays literal and ordinary prose editing/undo preserves the file")
                command("Live preview")
                editor_end()
                key("Escape")
                control_bounds("callout:0")
                assert not control_bounds("callout-body:0", required=False)
                click_control("callout:0")
                control_bounds("callout-body:0")
                control_bounds("callout:1")
                control_bounds("callout-body:1")
                click_control("callout:1")
                assert not control_bounds("callout-body:1", required=False)
                click_control("callout:0")
                assert not control_bounds("callout:1", required=False)
                click_control("callout:0")
                assert not control_bounds("callout-body:1", required=False)
                control_bounds("callout:2")
                assert not control_bounds("callout:3", required=False)
                capture("native-callouts-dark.png")
                assert note.read_text() == source
                results.append("Native Live callouts honor fold defaults, retain nested state, preserve title-only callouts and ignore fenced markers")
                left, top, right, bottom = control_bounds("callout-body:0")
                xdo("mousemove", str(round(left + 120)), str(round(top + 12)), "click", "1")
                key("End")
                type_text(" LIVE_BODY")
                key("ctrl+s")
                assert note.read_text() == source.replace("Hidden body", "Hidden body LIVE_BODY", 1)
                key("ctrl+z")
                key("ctrl+s")
                assert note.read_text() == source
                key("Escape")
                results.append("Live callout body editing and undo preserve exact surrounding Markdown and nested content")
                click_control("callout:0", button="3")
                click_control("callout-type:success")
                key("ctrl+s")
                assert note.read_text() == source.replace("[!warning]", "[!success]", 1)
                key("ctrl+z")
                key("ctrl+s")
                assert note.read_text() == source
                key("Escape")
                results.append("The native callout type menu changes only its marker and shares document undo/save")
                command("Reading view")
                control_bounds("callout:0")
                assert not control_bounds("callout-body:0", required=False)
                click_control("callout:0")
                control_bounds("callout-body:0")
                control_bounds("callout-body:1")
                assert not control_bounds("callout:3", required=False)
                key("ctrl+s")
                assert note.read_text() == source
                capture("native-callouts-reading.png")
                results.append("Reading callouts render and fold nested content without altering note bytes")
                command("Switch appearance")
                capture("native-callouts-light.png")
                command("Switch appearance")
                rich = ("# Rich callout\n\n> [!tip]+ Rich content\n> - [ ] Callout task\n>\n"
                        "> | Name | Count |\n> | --- | --- |\n> | [[Home]] | 2 |\n>\n"
                        "> ```rust\n> let value = 7;\n> ```\n>\n> π\n> ![[Assets/Attachment.png|160]]\n\nFollowing prose.\n")
                rich_note = fixture / "Notes/Rich callout.md"
                rich_note.write_text(rich)
                time.sleep(.6)
                open_note("Rich callout")
                command("Reading view")
                control_bounds("callout:0")
                control_bounds("task:0")
                control_bounds("table:0")
                control_bounds("copy-code:0")
                left, top, right, bottom = control_bounds("task:0")
                app.send_signal(signal.SIGSTOP)
                try:
                    xdo("mousemove", str(round((left + right) / 2)), str(round((top + bottom) / 2)), "click", "1")
                    xdo("key", "--clearmodifiers", "ctrl+s")
                finally:
                    app.send_signal(signal.SIGCONT)
                # An explicit save must finish before the 600ms autosave can run.
                deadline = time.monotonic() + .5
                checked = rich.replace("[ ] Callout task", "[x] Callout task")
                while time.monotonic() < deadline and rich_note.read_text() != checked:
                    time.sleep(.01)
                assert rich_note.read_text() == checked, repr(rich_note.read_text())
                capture("native-callouts-rich.png")
                results.append("Reading callouts retain native tasks, tables, code controls and embedded images")
                command("Live preview")
                key("Escape")
                control_bounds("embed:0")
                click_control("callout:0", button="3")
                click_control("callout-type:warning")
                key("ctrl+s")
                checked = rich.replace("[ ] Callout task", "[x] Callout task")
                assert rich_note.read_text() == checked.replace("[!tip]", "[!warning]", 1)
                control_bounds("embed:0")
                key("ctrl+z")
                key("ctrl+s")
                assert rich_note.read_text() == checked
                command("Reading view")
                click_control("table-link:0:1:0:0")
                title_contains("Home")
                results.append("Type changes before a Unicode/image offset preserve rendering and undo; a table link still opens its note")
                end_source = "# End fold\n\n> [!note]- End\n> Last body."
                end_note = fixture / "Notes/End fold.md"
                end_note.write_text(end_source)
                time.sleep(.6)
                open_note("End fold")
                command("Live preview")
                assert not control_bounds("callout-body:0", required=False)
                editor_end()
                control_bounds("callout-body:0")
                type_text(" EOF_EDIT")
                key("ctrl+s")
                assert end_note.read_text() == end_source + " EOF_EDIT"
                key("ctrl+z")
                key("ctrl+s")
                assert end_note.read_text() == end_source
                results.append("Ctrl+End reveals a collapsed callout at EOF and immediate typing/undo preserves every byte")
                wrap_note = fixture / "Notes/Wrap callout.md"
                for theme, size in (("dark", (1440, 960)), ("light", (1040, 800))):
                    if theme == "light":
                        command("Switch appearance")
                        xdo("windowsize", window, *map(str, size))
                        pause()
                    for count in (38, 39):
                        wrap_source = "# Wrap callout\n\n> [!note]+ Outer\n> > [!tip]+ " + "M " * count + "\n> > Nested body.\n\nAfter.\n"
                        wrap_note.write_text(wrap_source)
                        time.sleep(.65)
                        if theme == "dark" and count == 38:
                            open_note("Wrap callout")
                            command("Live preview")
                            key("Escape")
                        header = control_bounds("callout:1")
                        body = control_bounds("callout-body:1")
                        assert header[3] <= body[1], (theme, count, header, body)
                        assert wrap_note.read_text() == wrap_source
                    capture(f"native-callout-wrap-{theme}.png", size)
                command("Switch appearance")
                xdo("windowsize", window, "1440", "960")
                pause()
                results.append("Long nested callout titles reserve their actual wrapped height in both themes and compact width")

            def properties_checks():
                create_note_named('Notes/Properties GUI.md')
                command("Source editing")
                editor_end()
                key("ctrl+a")
                property_source = "---\naliases: [Notebook alias]\ntags: [notes/deep]\ndone: false\nscore: 3\n---\n\n# Properties GUI\nBody must stay exact.\n"
                type_text(property_source)
                key("ctrl+s")
                property_file = fixture / "Notes/Properties GUI.md"
                assert property_file.read_text() == property_source, (repr(property_file.read_text()), repr(property_source))
                command("Live preview")
                capture("native-properties.png")
                key("ctrl+o")
                type_text("Notebook alias")
                key("Return")
                title_contains("Properties GUI")
                command("Add property")
                type_text("status")
                key("Return")
                key("ctrl+s")
                assert "status:" in property_file.read_text()
                assert property_file.read_text().endswith("\n# Properties GUI\nBody must stay exact.\n")
                capture("native-properties-added.png")
                click_control("property:done:value")
                time.sleep(.2)
                click_control("property:score:value")
                key("ctrl+a")
                type_text("42")
                key("ctrl+s")
                time.sleep(.3)
                edited = property_file.read_text()
                assert "done: true" in edited and "score: 42" in edited, edited
                assert edited.endswith("\n# Properties GUI\nBody must stay exact.\n"), edited
                editor_end()
                key("ctrl+End")
                key("ctrl+a")
                type_text("BODY_ONLY_REPLACEMENT")
                key("ctrl+s")
                time.sleep(.3)
                replaced = property_file.read_text()
                assert replaced.endswith("---\nBODY_ONLY_REPLACEMENT"), replaced
                assert "aliases:" in replaced and "done: true" in replaced and "score: 42" in replaced, replaced
                key("ctrl+z")
                key("ctrl+s")
                time.sleep(.3)
                restored = property_file.read_text()
                assert restored == edited, (restored, edited)
                results.append("Live body selection, replacement, and undo preserved properties; checkbox and number controls saved typed values")
                key("ctrl+End")
                type_text("\n- [x] Completed")
                key("Return")
                type_text("Next")
                key("ctrl+s")
                time.sleep(.3)
                assert property_file.read_text().endswith("- [x] Completed\n- [ ] Next"), property_file.read_text()
                key("Return")
                key("Return")
                key("ctrl+s")
                time.sleep(.3)
                assert property_file.read_text().endswith("- [ ] Next\n"), property_file.read_text()
                type_text("```md\n- Code")
                key("Return")
                type_text("literal")
                key("ctrl+s")
                time.sleep(.3)
                assert property_file.read_text().endswith("```md\n- Code\nliteral"), property_file.read_text()
                results.append("Native Enter continued a checked task as unchecked, exited an empty item, and left fenced code literal")
                before_search = property_file.read_text()
                key("ctrl+o")
                type_text("Home")
                key("Return")
                key("ctrl+shift+f")
                type_text("tag:#notes [score: > 40]")
                time.sleep(1)
                assert property_file.read_text() == before_search
                capture("native-search.png")
                click_control("search:Notes/Properties GUI.md")
                time.sleep(.3)
                assert "Properties GUI" in xdo("getwindowname", window)
                results.append("Global search shortcut focused its input and a tag/property query opened its matching note")
                key("ctrl+t")
                title_contains("New tab")
                key("ctrl+shift+Tab")
                title_contains("Properties GUI")
                key("ctrl+Tab")
                title_contains("New tab")
                key("ctrl+w")
                results.append("Ctrl+Shift+Tab moved backward independently from Ctrl+Tab across three tabs")
                results.append("YAML properties rendered natively; alias quick-open found the file; adding a property preserved the exact body")
            def image_checks():
                create_note_named('Notes/Image GUI.md')
                command("Source editing")
                editor_end()
                key("ctrl+a")
                source = "# Image GUI\n\n![[Assets/Attachment.png|180]]\n\n![Relative](../Assets/Attachment.png)\n\n`![[Missing.png]]`\n"
                type_text(source)
                key("ctrl+s")
                image_note = fixture / "Notes/Image GUI.md"
                assert image_note.read_text() == source
                command("Reading view")
                time.sleep(1)
                frame = ImageGrab.grab(xdisplay=display).convert("RGB")
                pixels = list(pixel_data(frame))
                assert sum(pixel == image_color for pixel in pixels) > 140000
                row_counts = [sum(frame.getpixel((x, y)) == image_color for x in range(frame.width)) for y in range(170, 850)]
                assert 180 in row_counts and 600 in row_counts, sorted(set(row_counts))
                capture("native-images.png")
                updated_color = (184, 101, 145)
                Image.new("RGB", (600, 240), updated_color).save(attachment)
                time.sleep(1.3)
                frame = ImageGrab.grab(xdisplay=display).convert("RGB")
                assert sum(pixel == updated_color for pixel in pixel_data(frame)) > 140000
                assert image_note.read_text() == source
                results.append("Native wiki and relative Markdown images rendered at requested width; external image edits refreshed without changing the note")
                command("Live preview")
                editor_end()
                time.sleep(.6)
                frame = ImageGrab.grab(xdisplay=display).convert("RGB")
                assert sum(pixel == updated_color for pixel in pixel_data(frame)) > 140000
                capture("native-live-images.png")
                type_text("\nWRITING_AROUND_IMAGES")
                key("ctrl+s")
                time.sleep(.3)
                assert image_note.read_text() == source + "\nWRITING_AROUND_IMAGES"
                click_control("embed:0")
                time.sleep(.3)
                assert control_bounds("embed:0")
                assert control_bounds("embed-source:0")
                assert control_bounds("embed:1")
                capture("native-live-image-source.png")
                key("ctrl+b")
                key("ctrl+s")
                time.sleep(.3)
                assert "**![[Assets/Attachment.png|180]]**" in image_note.read_text(), image_note.read_text()
                key("ctrl+z")
                key("ctrl+s")
                time.sleep(.3)
                capture("native-live-image-undo.png")
                assert image_note.read_text() == source + "\nWRITING_AROUND_IMAGES", repr(image_note.read_text())
                editor_end()
                time.sleep(.3)
                assert control_bounds("embed:0")
                assert image_note.read_text() == source + "\nWRITING_AROUND_IMAGES"
                results.append("Live images stayed visible while writing; clicking an embed exposed its source and returning to the body restored the image without changing bytes")
            def attachment_tab_checks():
                nonlocal app, window
                home_before = fixture.joinpath("Home.md").read_bytes()
                click_control("button:File explorer")
                pause()
                if not control_bounds("file:Assets/Attachment.png", required=False):
                    click_control("folder:Assets")
                assert control_bounds("file:Assets/Attachment.png")
                click_control("file:Assets/Attachment.png")
                title_contains("Attachment.png")
                assert control_bounds("attachment-image")
                results.append("Explorer image rows opened a native image tab")

                image_path = fixture / "Assets/Viewer[1].png"
                Image.new("RGB", (1920, 1200), (52, 137, 196)).save(image_path)
                original = image_path.read_bytes()
                links_path = fixture / "Notes/Attachment links GUI.md"
                links_path.write_text("# Attachment links GUI\n\n[[Assets/Viewer[1].png]]\n\n![Image](../Assets/Viewer%5B1%5D.png)\n")
                time.sleep(1)
                pause()
                open_note("Viewer[1].png")
                fit = control_bounds("attachment-image")
                assert fit[2] - fit[0] < 1400 and fit[3] - fit[1] < 900, fit
                capture("native-image-tab-fit.png")
                click_control("image-scale:100%")
                actual = control_bounds("attachment-image")
                assert actual[2] - actual[0] > fit[2] - fit[0] + 100, (fit, actual)
                assert actual[3] - actual[1] > fit[3] - fit[1] + 100, (fit, actual)
                xdo("mousemove", "900", "700", "click", "5")
                pause()
                capture("native-image-tab-actual.png")
                click_control("image-scale:Fit")
                for shortcut in ["ctrl+e", "ctrl+f", "ctrl+b", "ctrl+a", "Delete"]:
                    key(shortcut)
                key("ctrl+semicolon")
                for unavailable in ["Source editing", "Save a copy"]:
                    command(unavailable,available=False)
                    key("Escape")
                queued_text("SHOULD_NOT_EDIT_IMAGE", "ctrl+s")
                assert control_bounds("attachment-image")
                assert not control_bounds("editor", required=False)
                assert image_path.read_bytes() == original
                assert not fixture.joinpath("Assets/Viewer[1].png.md").exists()
                results.append("Fit and 100% image zoom worked; editing, find, formatting and save shortcuts left image bytes intact")

                open_note("Attachment links GUI")
                command("Source editing")
                editor_end()
                key("Return")
                queued_text("[[Viewer", "Tab")
                key("ctrl+s")
                assert links_path.read_text().endswith("\n[[Assets/Viewer[1].png]]"), links_path.read_text()
                key("ctrl+Return")
                title_contains("Viewer[1].png")
                assert control_bounds("attachment-image")
                assert control_bounds("tab-full:Notes/Attachment links GUI.md")
                key("ctrl+w")
                title_contains("Attachment links GUI")
                assert control_bounds("editor")
                editor_end()
                type_text("\n[Open image](../Assets/Viewer%5B1%5D.png)")
                key("ctrl+s")
                key("ctrl+Return")
                title_contains("Viewer[1].png")
                assert control_bounds("attachment-image")
                assert control_bounds("tab-full:Notes/Attachment links GUI.md")
                key("ctrl+w")
                title_contains("Attachment links GUI")
                command("Reading view")
                left, top, right, bottom = control_bounds("reading-block:1")
                xdo("mousemove", "--sync", str(round(left + min(25, (right-left)/2))), str(round((top+bottom)/2)), "click", "1")
                pause()
                title_contains("Viewer[1].png")
                key("alt+Left")
                title_contains("Attachment links GUI")
                key("alt+Right")
                title_contains("Viewer[1].png")
                assert control_bounds("attachment-image")
                results.append("Image link completion kept its extension; Ctrl+Enter retained its source tab and ordinary Reading clicks supported Back/Forward between the note and read-only image")

                Image.new("RGB", (1920, 1200), (184, 101, 145)).save(image_path)
                external = image_path.read_bytes()
                time.sleep(1)
                pause()
                screen = ImageGrab.grab(xdisplay=display)
                count = sum(pixels for pixels, pixel in screen.getcolors(screen.width * screen.height) or [] if pixel[:3] == (184, 101, 145))
                assert count > 300000, count
                key("F2")
                key("ctrl+a")
                type_text("Assets/Renamed viewer[1]")
                key("Return")
                renamed = fixture / "Assets/Renamed viewer[1].png"
                title_contains("Renamed viewer[1].png")
                assert renamed.read_bytes() == external and not image_path.exists()
                assert "[[Assets/Renamed viewer[1].png]]" in links_path.read_text()
                assert "../Assets/Renamed%20viewer%5B1%5D.png" in links_path.read_text(), links_path.read_text()
                capture("native-image-tab-renamed.png")
                results.append("An externally replaced image refreshed; image rename retained binary bytes and rewrote wiki and Markdown links")

                xdo("key", "--clearmodifiers", "ctrl+q")
                app.wait(timeout=10)
                state = json.loads(next((base / "state/rustidian").glob("*.json")).read_text())
                assert ["Assets/Renamed viewer[1].png", "Read"] in state["tabs"], state
                app = launch_app(launch_command)
                window = ready_window()
                title_contains("Renamed viewer[1].png")
                assert control_bounds("attachment-image")
                assert renamed.read_bytes() == external
                command("Move file to system Trash")
                key("Return")
                time.sleep(.5)
                pause()
                assert not renamed.exists()
                trashed = [path for path in (base / "data/Trash/files").iterdir() if path.name.startswith("Renamed viewer[1]")]
                assert any(path.read_bytes() == external for path in trashed), trashed
                open_note("Attachment links GUI")
                command("Source editing")
                editor_end()
                type_text("_NOTE_EDIT_STILL_WORKS")
                key("ctrl+s")
                assert links_path.read_text().endswith("_NOTE_EDIT_STILL_WORKS")
                type_text("\n[[Assets/Renamed viewer[1].png]]")
                key("ctrl+s")
                key("ctrl+Return")
                title_contains("Attachment links GUI")
                assert not fixture.joinpath("Assets/Renamed viewer[1].png.md").exists()
                assert fixture.joinpath("Home.md").read_bytes() == home_before
                results.append("Clean restart restored the image tab; image Trash preserved its bytes and ordinary note editing still worked")

            def color_checks():
                colors = [
                    ("Cyan", '<mark style="background: #00bfbc41;">', "</mark>", lambda r, g, b: g > r + 15 and b > r + 15),
                    ("Green", '<mark style="background: #44cf6e41;">', "</mark>", lambda r, g, b: g > r + 15 and g > b + 15),
                    ("Orange", '<mark style="background: #ffaa4041;">', "</mark>", lambda r, g, b: r > g + 16 and g > b + 15),
                    ("Red", '<mark style="background: #fb464c41;">', "</mark>", lambda r, g, b: r > g + 20 and r > b + 20),
                    ("Yellow", "==", "==", lambda r, g, b: r > g > b + 20 and r - g < 45),
                ]
                for name, opening, closing, tinted in colors:
                    source = f"# {name} highlight\n\neditable"
                    note = fixture / f"Notes/Highlight {name}.md"
                    note.write_text(source)
                    time.sleep(.6)
                    open_note(f"Highlight {name}")
                    command("Source editing")
                    editor_end()
                    key("ctrl+shift+Left")
                    key("ctrl+p")
                    type_text(f"Highlight {name}")
                    assert control_bounds(f"command:Highlight {name}"), f"{name} command is missing"
                    key("Return")
                    key("ctrl+s")
                    expected = source[:-len("editable")] + opening + "editable" + closing
                    assert note.read_text() == expected, (name, note.read_text())
                    key("ctrl+z")
                    key("ctrl+s")
                    assert note.read_text() == source, (name, "undo")
                    key("ctrl+y")
                    key("ctrl+s")
                    assert note.read_text() == expected, (name, "redo")
                    for view, control in [("Reading view", "reading-block:1"), ("Live preview", "editor")]:
                        command(view)
                        picture = f"native-highlight-{name.lower()}-{view.split()[0].lower()}.png"
                        capture(picture)
                        box = tuple(round(value) for value in control_bounds(control))
                        count = sum(tinted(r, g, b) for r, g, b in pixel_data(Image.open(ROOT / "reports" / picture).crop(box).convert("RGB")))
                        assert count > 15, (name, view, count)
                    command("Switch appearance")
                    command("Reading view")
                    capture(f"native-highlight-{name.lower()}-light.png")
                    command("Switch appearance")
                    key("ctrl+w")
                    open_note(f"Highlight {name}")
                    command("Source editing")
                    editor_end()
                    type_text(" reopened")
                    key("ctrl+s")
                    assert note.read_text() == expected + " reopened", (name, "reopen")
                results.append("All five highlight colors preserve selected text through Ctrl+P, save/reopen and undo/redo, and render in Live/Reading with light/dark captures")

            def native_highlight_checks():
                close_tabs();key("ctrl+t")
                command("Toggle blue highlight",available=False);key("Escape")
                note=fixture/"Native color toggle.md"
                original="# Native color toggle\n\n`==literal==`\n\nBefore TARGET"
                note.write_text(original);time.sleep(.6)
                open_note("Native color toggle");command("Source editing")
                key("ctrl+p");type_text("Highlight Cyan");control_bounds("command:Highlight Cyan");key("Escape")
                assert note.read_text()==original
                results.append("Native color toggles stayed unavailable on a blank tab; the original Cyan command remained available on a document without editing it")
                colors=[("Blue","🔵"),("Green","🟢"),("Orange","🟠"),("Purple","🟣"),("Red","🔴"),("Yellow","🟡")]
                source_tints={
                    "Blue":lambda r,g,b:b>r+16 and b>g+10,
                    "Green":lambda r,g,b:g>r+12 and g>b+12,
                    "Orange":lambda r,g,b:r>g+12 and g>b+12,
                    "Purple":lambda r,g,b:b>g+12 and r>g+5,
                    "Red":lambda r,g,b:r>g+20 and r>b+20,
                    "Yellow":lambda r,g,b:r>=g and g>b+12,
                }
                for color,marker in colors:
                    editor_end();key("ctrl+shift+Left")
                    command("Toggle "+color.lower()+" highlight");key("ctrl+s")
                    expected=original[:-6]+"=="+marker+"TARGET=="
                    assert note.read_text()==expected,(color,note.read_text())
                    marker_pixels=pixel_data(ImageGrab.grab(xdisplay=display).crop(tuple(round(v) for v in control_bounds("editor"))).convert("RGB"))
                    assert sum(source_tints[color](*pixel) for pixel in marker_pixels)>15,(color,"Native marker must render in color rather than as a missing glyph")
                    capture("native-highlight-marker-"+color.lower()+".png")
                    key("ctrl+z");key("ctrl+s");assert note.read_text()==original,(color,"undo")
                    key("ctrl+y");key("ctrl+s");assert note.read_text()==expected,(color,"redo")
                    command("Toggle "+color.lower()+" highlight");key("ctrl+s")
                    assert note.read_text()==original,(color,"repeat toggle",note.read_text())
                    results.append(color+" used the native emoji marker, preserved its selected text and literal-code neighbor, and reversed exactly through undo/redo and a repeated toggle")
                editor_end();key("ctrl+shift+Left");command("Toggle blue highlight");key("ctrl+s")
                blue=original[:-6]+"==🔵TARGET==";assert note.read_text()==blue
                key("ctrl+End");key("Left");key("Left");key("Left")
                key("shift+Left");key("shift+Left");key("shift+Left")
                command("Toggle purple highlight");key("ctrl+s")
                partial=original[:-6]+"==🔵TA====🟣RGE====🔵T=="
                assert note.read_text()==partial,note.read_text()
                key("ctrl+c")
                copied=subprocess.check_output(["xclip","-selection","clipboard","-out","-target","UTF8_STRING"],env=environment,timeout=5).decode()
                assert copied=="RGE",repr(copied)
                command("Reading view");capture("native-highlight-partial-reading.png")
                key("ctrl+e");command("Source editing");key("ctrl+z");key("ctrl+s");assert note.read_text()==blue
                key("ctrl+y");key("ctrl+s");assert note.read_text()==partial
                key("ctrl+z");key("ctrl+s");assert note.read_text()==blue
                results.append("Recoloring a partial selection split Blue/Purple/Blue with the exact reference bytes, retained the selected RGE clipboard text, rendered, and reversed as a single edit")
                key("ctrl+End");key("Left");key("Left");key("Left")
                command("Toggle blue highlight");key("ctrl+s");assert note.read_text()==original
                type_text("!");key("ctrl+s");assert note.read_text()==original[:-6]+"TARGE!T",note.read_text()
                key("ctrl+z");key("ctrl+s");assert note.read_text()==original
                results.append("Toggling an existing color at an interior caret removed the whole mark while preserving the exact insertion point")
                editor_end();key("Left");key("Left");key("Left")
                command("Toggle blue highlight");key("ctrl+s");assert note.read_text()==blue
                type_text("!");key("ctrl+s");assert note.read_text()==original[:-6]+"==🔵TAR!GET==",note.read_text()
                key("ctrl+z");key("ctrl+s");assert note.read_text()==blue
                key("ctrl+z");key("ctrl+s");assert note.read_text()==original
                results.append("A bare interior caret highlighted the entire word without selecting it; the next typed character remained at the same word position")
                empty_note=fixture/"Native empty color.md"
                empty_original="Before  ==🟣EXISTING== after"
                empty_note.write_text(empty_original);time.sleep(.6);open_note("Native empty color");command("Source editing")
                key("ctrl+Home")
                for _ in range(7):key("Right")
                command("Toggle blue highlight");key("ctrl+s")
                empty="Before ==🔵== ==🟣EXISTING== after";assert empty_note.read_text()==empty
                command("Toggle blue highlight");key("ctrl+s");assert empty_note.read_text()==empty_original
                key("ctrl+z");key("ctrl+s");assert empty_note.read_text()==empty
                key("ctrl+y");key("ctrl+s");assert empty_note.read_text()==empty_original
                key("ctrl+z");key("ctrl+s");assert empty_note.read_text()==empty
                command("Reading view");capture("native-highlight-empty-reading.png")
                bounds=tuple(round(v) for v in control_bounds("reading-block:0"))
                pixels=pixel_data(ImageGrab.grab(xdisplay=display).crop(bounds).convert("RGB"))
                assert sum(b>g+12 and r>g+5 for r,g,b in pixels)>20
                command("Source editing");type_text("NEW");key("ctrl+s")
                assert empty_note.read_text()=="Before ==🔵NEW== ==🟣EXISTING== after",empty_note.read_text()
                key("ctrl+z");key("ctrl+s");assert empty_note.read_text()==empty
                key("ctrl+z");key("ctrl+s");assert empty_note.read_text()==empty_original
                results.append("An empty native mark toggled off and reversed exactly, stayed invisible beside Purple in Reading and accepted new text at the retained caret without swallowing its neighbor")
                rendered=fixture/"Native color rendering.md"
                source="# Native color rendering\n\n"+"\n\n".join(color+" =="+marker+"TARGET_"+color.upper()+"==" for color,marker in colors)+"\n\n`==🔵literal==`\n\n```md\n==🟣fenced==\n```\n\n==🟡[Target link](Native%20color%20toggle.md)==\n"
                rendered.write_text(source);time.sleep(.6);open_note("Native color rendering")
                tints={
                    "Blue":lambda r,g,b:b>r+16 and b>g+10,
                    "Green":lambda r,g,b:g>r+12 and g>b+12,
                    "Orange":lambda r,g,b:r>g+12 and g>b+12,
                    "Purple":lambda r,g,b:b>g+12 and r>g+5,
                    "Red":lambda r,g,b:r>g+20 and r>b+20,
                    "Yellow":lambda r,g,b:r>=g and g>b+12,
                }
                for dark in (True,False):
                    set_theme(dark);command("Reading view")
                    capture("native-core-highlight-"+("dark" if dark else "light")+"-reading.png")
                    image=ImageGrab.grab(xdisplay=display)
                    for index,(color,_) in enumerate(colors,1):
                        bounds=tuple(round(v) for v in control_bounds("reading-block:"+str(index)))
                        count=sum(tints[color](r,g,b) for r,g,b in pixel_data(image.crop(bounds).convert("RGB")))
                        assert count>20,(dark,color,count)
                    command("Toggle blue highlight",available=False);key("Escape")
                    command("Live preview");key("ctrl+Home")
                    capture("native-core-highlight-"+("dark" if dark else "light")+"-live.png")
                    assert rendered.read_text()==source
                set_theme(True)
                assert note.read_text()==original
                results.append("Six native colors rendered distinct theme-specific backgrounds in Reading and Live, retained inline/fenced literal source and a highlighted link, and stayed unavailable in Reading")

            def file_menu(path):
                if control_bounds("file:" + path, required=False) is None:
                    click_control("folder:Notes")
                click_control("file:" + path, "3")
                control_bounds("menu:Open in new tab")

            def chrome_checks():
                assert control_bounds("folder:__pycache__", required=False) is None
                assert control_bounds("folder:.git", required=False) is None
                assert control_bounds("file:.gitignore.md", required=False) is None
                click_control("file:Cache notes.md")
                title_contains("Cache notes")
                open_note("Home")
                results.append("Explorer excludes dotfiles, Git data and Python caches while keeping ordinary cache-related notes")
                originals = {str(p.relative_to(fixture)): p.read_bytes() for p in fixture.rglob("*.md")}
                for name in ["Close window", "Maximize window", "Minimize window"]:
                    xdo("mousemove", "800", "500")
                    time.sleep(.07)
                    bounds = tuple(round(x) for x in control_bounds("window:" + name))
                    before = ImageGrab.grab(xdisplay=display).crop(bounds).convert("RGB")
                    left, top, right, bottom = bounds
                    xdo("mousemove", str((left + right)//2), str((top + bottom)//2))
                    time.sleep(.08)
                    after = ImageGrab.grab(xdisplay=display).crop(bounds).convert("RGB")
                    assert sum(a != b for a, b in zip(pixel_data(before), pixel_data(after))) > 100, name
                capture("native-window-controls-hover.png")
                results.append("All three generated window controls show immediate visible hover feedback")

                file_menu("Notes/First steps.md")
                for label in ["Open in new tab", "Make a copy", "Bookmark…", "Copy path", "Rename…", "D  Delete"]:
                    left, top, right, bottom = control_bounds("menu:" + label)
                    assert 24 <= bottom-top <= 30 and 190 <= right-left <= 240, (label, right-left, bottom-top)
                capture("native-file-context-menu.png")
                key("Escape")
                title_contains("Home")
                assert {str(p.relative_to(fixture)): p.read_bytes() for p in fixture.rglob("*.md")} == originals
                file_menu("Notes/First steps.md")
                click_control("menu:Open in new tab")
                title_contains("First steps")
                assert control_bounds("tab-full:Home.md")
                assert control_bounds("tab-full:Notes/First steps.md")
                stale = "STALE_PATH_π"
                owner = subprocess.Popen(["xclip", "-selection", "clipboard", "-in", "-quiet"],
                    stdin=subprocess.PIPE, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, env=environment)
                samples = []
                try:
                    owner.stdin.write(stale.encode())
                    owner.stdin.close()
                    pause()
                    assert subprocess.check_output(["xclip", "-selection", "clipboard", "-out", "-target", "UTF8_STRING"], env=environment, timeout=2).decode() == stale
                    file_menu("Home.md")
                    started = time.monotonic()
                    click_control("menu:Copy path")
                    expected = str(fixture / "Home.md")
                    clipboard = stale
                    deadline = started + 2
                    while time.monotonic() < deadline:
                        result = subprocess.run(["xclip", "-selection", "clipboard", "-out", "-target", "UTF8_STRING"],
                            env=environment, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=1)
                        clipboard = result.stdout.decode()
                        samples.append({"elapsed_ms": (time.monotonic()-started)*1000, "returncode": result.returncode,
                            "text": clipboard, "error": result.stderr.decode()})
                        if result.returncode == 0 and clipboard == expected:
                            break
                        time.sleep(.02)
                    assert clipboard == expected, (expected, samples)
                finally:
                    (reports / "copy-path-clipboard.json").write_text(json.dumps({"metric": "clipboard correctness; elapsed includes click helper waits, not a latency benchmark", "samples": samples,
                        "pointer": xdo("getmouselocation", "--shell")}, indent=2)+"\n")
                    if owner.poll() is None:
                        owner.terminate()
                    owner.wait(timeout=5)
                title_contains("First steps")
                file_menu("Home.md")
                click_control("menu:Make a copy")
                assert control_bounds("dialog-input",required=False) is None
                created=set(str(p.relative_to(fixture)) for p in fixture.rglob("*.md"))-set(originals)
                assert len(created)==1
                copied=next(iter(created))
                assert Path(copied).name.startswith("Home ")
                title_contains(Path(copied).stem)
                assert (fixture / copied).read_bytes() == originals["Home.md"]
                assert control_bounds("tab-full:Notes/First steps.md")
                file_menu(copied)
                click_control("menu:Rename…")
                control_bounds("dialog-input-focused")
                key("ctrl+a")
                type_text("Menu renamed.md")
                key("Return")
                title_contains("Menu renamed")
                assert not (fixture / copied).exists()
                assert (fixture / "Menu renamed.md").read_bytes() == originals["Home.md"]
                file_menu("Menu renamed.md")
                click_control("menu:D  Delete")
                control_bounds("dialog-input")
                key("Escape")
                assert (fixture / "Menu renamed.md").exists(), "Cancel must preserve the note"
                file_menu("Menu renamed.md")
                click_control("menu:D  Delete")
                control_bounds("dialog-input")
                key("Return")
                title_contains("New tab")
                assert control_bounds("tab-full:Home.md")
                assert not (fixture / "Menu renamed.md").exists()
                assert any(p.read_bytes() == originals["Home.md"] for p in (base / "data/Trash/files").iterdir() if p.is_file())
                assert {str(p.relative_to(fixture)): p.read_bytes() for p in fixture.rglob("*.md")} == originals
                results.append("Compact grouped file menu opens a background note in a new tab, copies its actual path and contents, renames safely, cancels deletion and recovers the confirmed deletion from Trash")
                if window_manager is not None:
                    def drag(start, distance):
                        x, y = start
                        dx, dy = distance
                        xdo("mousemove", "--sync", str(x), str(y), "mousedown", "1")
                        try:
                            for step in range(1, 9):
                                time.sleep(.025)
                                xdo("mousemove", "--sync", str(round(x+dx*step/8)), str(round(y+dy*step/8)))
                        finally:
                            xdo("mouseup", "1")
                        pause()

                    def geometry():
                        return {k:int(v) for k,v in (line.split("=",1) for line in xdo("getwindowgeometry", "--shell", window).splitlines()) if k in ("X", "Y", "WIDTH", "HEIGHT")}
                    xdo("windowsize", window, "1100", "760")
                    xdo("windowmove", window, "0", "0")
                    pause()
                    click_control("window:Maximize window")
                    control_bounds("window:Restore window")
                    assert geometry()["WIDTH"] == 1440 and geometry()["HEIGHT"] == 960, geometry()
                    click_control("window:Restore window")
                    control_bounds("window:Maximize window")
                    assert geometry()["WIDTH"] == 1100 and geometry()["HEIGHT"] == 760, geometry()
                    click_control("window:Minimize window")
                    state = subprocess.check_output(["xprop", "-id", window, "WM_STATE"], env=environment, text=True)
                    assert "Iconic" in state, state
                    xdo("windowmap", window)
                    xdo("windowactivate", "--sync", window)
                    pause()
                    before = geometry()
                    left, top, right, bottom = control_bounds("window-title-drag")
                    x = round((left + right)/2)
                    y = round((top + bottom)/2)
                    drag((x, y), (128, 64))
                    assert geometry()["X"] >= before["X"] + 60 and geometry()["Y"] >= before["Y"] + 20, (before, geometry())
                    xdo("windowmove", window, "0", "0")
                    pause()
                    drag((1098, 758), (82, 62))
                    assert geometry()["WIDTH"] >= 1150 and geometry()["HEIGHT"] >= 800, geometry()
                    capture("native-window-controls.png")
                    (reports / "window-manager-results.json").write_text(json.dumps({"status":"passed","manager":"Openbox","geometry":geometry()},indent=2)+"\n")
                    results.append("Real window manager confirmed maximize/restore, minimize/reopen, title-bar dragging and edge resizing")
                # The close guard is meaningful when the source cannot autosave.
                command("Open settings")
                type_text("autosave")
                assert control_bounds("settings-dark", required=False) is None
                click_control("settings-autosave")
                wait_autosave(base / "state", False)
                key("Escape")
                open_note("First steps")
                command("Source editing")
                control_bounds("editor-focused")
                key("ctrl+End")
                type_text("WINDOW_CLOSE_UNSAVED")
                active = "Notes/First steps.md"
                time.sleep(.75)
                assert (fixture / active).read_bytes() == originals[active]
                title_contains("First steps")
                click_control("window:Close window")
                pause()
                capture("native-window-unsaved-close.png")
                assert app.poll() is None, "Close must preserve an unsaved note"
                assert (fixture / active).read_bytes() == originals[active]
                key("Escape")
                control_bounds("editor-focused")
                key("ctrl+z")
                key("ctrl+s")
                assert (fixture / active).read_bytes() == originals[active]
                results.append("Generated close button respects unsaved-change confirmation and preserves original file bytes")

            def hover_checks():
                key("ctrl+t")
                open_note("Home")
                command("Reading view")
                # Earlier full-suite groups expand many nested fixture folders.
                # Collapse them once so the root Home row is actually onscreen.
                click_control("file-collapse")
                originals = {str(p.relative_to(fixture)): p.read_bytes() for p in fixture.rglob("*.md")}
                original_folders = {str(p.relative_to(fixture)) for p in fixture.rglob("*") if p.is_dir()}
                timings = []
                for name in ["new-tab", "close-tab:Home.md", "new-folder", "file-sort", "file-collapse", "bookmarks-tab", "folder:Notes", "file:Home.md"]:
                    xdo("mousemove", "--sync", "1000", "880")
                    time.sleep(.07)
                    bounds = tuple(round(value) for value in control_bounds(name))
                    baseline = ImageGrab.grab(xdisplay=display).crop(bounds).convert("RGB")
                    left, top, right, bottom = bounds
                    start = time.perf_counter()
                    xdo("mousemove", "--sync", str((left+right)//2), str((top+bottom)//2))
                    changed = False
                    while time.perf_counter() - start < .15:
                        picture = ImageGrab.grab(xdisplay=display).crop(bounds).convert("RGB")
                        changed = sum(abs(r-34) <= 2 and abs(g-41) <= 2 and abs(b-44) <= 2 for r,g,b in pixel_data(picture)) > 50
                        if changed: break
                        time.sleep(.003)
                    elapsed = (time.perf_counter()-start)*1000
                    current_bounds = control_bounds(name)
                    sample = {"control":name,"dispatch_to_visible_ms":elapsed,
                              "bounds_before":list(bounds),"bounds_after":current_bounds,
                              "pointer":xdo("getmouselocation", "--shell")}
                    timings.append(sample)
                    (reports / "hover-samples.json").write_text(json.dumps(timings,indent=2)+"\n")
                    assert changed and picture.tobytes() != baseline.tobytes(), f"No immediate Obsidian hover feedback on {name}: {elapsed:.1f}ms"
                    if name.startswith("close-tab:"): capture("native-hover-close.png")
                    if name.startswith("folder:"): capture("native-hover-folder.png")
                # A disabled history control must neither highlight nor navigate.
                name = "button:Navigate back · Alt+Left"
                left, top, right, bottom = control_bounds(name)
                xdo("mousemove", "1000", "880")
                time.sleep(.07)
                bounds = tuple(round(v) for v in (left,top,right,bottom))
                before = ImageGrab.grab(xdisplay=display).crop(bounds).tobytes()
                xdo("mousemove", str(round((left+right)/2)), str(round((top+bottom)/2)))
                time.sleep(.07)
                assert ImageGrab.grab(xdisplay=display).crop(bounds).tobytes() == before
                xdo("click", "1")
                title_contains("Home")
                assert {str(p.relative_to(fixture)): p.read_bytes() for p in fixture.rglob("*.md")} == originals
                ordered = sorted(item["dispatch_to_visible_ms"] for item in timings)
                (reports / "hover-latency.json").write_text(json.dumps({"boundary":"xdotool dispatch plus polling for changed screen pixels; not internal CPU time", "samples":timings,"median_ms":ordered[len(ordered)//2],"max_ms":ordered[-1]}, indent=2)+"\n")
                results.append("Pointer hover visibly changes icons, tab close, folders and selected files; disabled history stays inert and every note byte is unchanged")

                def icon_background(name):
                    left,top,right,bottom=control_bounds(name)
                    return ImageGrab.grab(xdisplay=display).getpixel((round(left+3),round((top+bottom)/2)))[:3]

                for active in ["Search notes", "Bookmarks", "File explorer"]:
                    click_control("button:"+active,hold=.15)
                    xdo("mousemove","--sync","1000","880");pause()
                    assert icon_background("button:"+active)==(34,41,44), (active,"Selected tab lost its background when the pointer left")
                    for other in {"Search notes","Bookmarks","File explorer"}-{active}:
                        assert icon_background("button:"+other)==(17,21,23), (other,"Inactive tab still appears selected")
                capture("native-selected-sidebar.png")
                click_control("button:New note · Ctrl+N",hold=.15)
                control_bounds("note-title-input")
                key("Escape")
                xdo("mousemove","--sync","1000","880");pause()
                assert icon_background("button:New note · Ctrl+N")== (17,21,23), "An immediate action must not remain selected"
                assert icon_background("ribbon-new-note")== (17,21,23), "The ribbon action must also clear its marker"
                click_control("new-folder",hold=.15)
                control_bounds("dialog-input")
                xdo("mousemove","--sync","1000","880");pause()
                active=icon_background("new-folder")
                inactive=icon_background("button:Open today's daily note")
                assert min(a-b for a,b in zip(active,inactive))>=5, ("New folder must remain visibly marked behind its chooser",active,inactive)
                capture("native-selected-folder-tool.png")
                key("Escape")
                xdo("mousemove","--sync","1000","880");pause()
                assert icon_background("new-folder")== (17,21,23), "A cancelled folder action must not remain selected"
                assert {str(p.relative_to(fixture)): p.read_bytes() for p in fixture.rglob("*.md")} == originals
                assert {str(p.relative_to(fixture)) for p in fixture.rglob("*") if p.is_dir()} == original_folders
                results.append("Files, Search and Bookmarks retained a single selected background away from the pointer; New note creates immediately without a stuck marker, while New folder stays marked behind its chooser and Cancel leaves folders unchanged")

            def title_checks():
                path = fixture / "Notes/Title edit.md"
                source = "# Body heading stays independent\n\nExact body فارسی.\n"
                path.write_text(source)
                linked = fixture / "Notes/Title backlinks.md"
                linked.write_text("[[Title edit]]\n\n[read](Title%20edit.md)\n")
                conflict = fixture / "Notes/Title conflict.md"
                conflict.write_text("Do not replace this note.\n")
                time.sleep(.7)
                open_note("Title edit")
                command("Reading view")
                title_bounds = control_bounds("note-title", required=False)
                if title_bounds:
                    click_control("note-title")
                else:
                    # Before the fix the visible title had no interaction probe.
                    # Click its measured position and prove the rename itself fails.
                    left, top, _, _ = control_bounds("reading-block:0")
                    xdo("mousemove", str(round(left+70)), str(round(top-35)), "click", "1")
                    pause()
                key("ctrl+a")
                type_text("Title renamed")
                key("Return")
                renamed = fixture / "Notes/Title renamed.md"
                assert renamed.exists() and renamed.read_text() == source and not path.exists(), "Clicking the visible title did not rename the file"
                title_contains("Title renamed")
                assert "Title renamed" in linked.read_text() and "Title edit" not in linked.read_text() and "Title%20renamed.md" in linked.read_text()
                open_note("Title backlinks")
                command("Reading view")
                click_control("reading-block:0")
                title_contains("Title renamed")
                key("alt+Left")
                title_contains("Title backlinks")
                click_control("reading-block:1")
                title_contains("Title renamed")
                capture("native-title-renamed.png")
                click_control("note-title")
                key("ctrl+a")
                type_text("Cancelled title")
                key("Escape")
                assert renamed.read_text() == source and not (fixture / "Notes/Cancelled title.md").exists()
                click_control("note-title")
                key("ctrl+a")
                type_text("Title conflict")
                key("Return")
                assert renamed.read_text() == source and conflict.read_text() == "Do not replace this note.\n"
                key("Escape")
                click_control("note-title")
                key("ctrl+a")
                type_text("Title blur")
                click_control("new-folder")
                key("Escape")
                assert (fixture / "Notes/Title blur.md").read_text() == source and not renamed.exists()
                results.append("Clicking the inline title renames on Enter or blur, updates links and tabs, cancels on Escape and refuses to overwrite an existing note")

            def scrolling_checks():
                path = fixture / "Notes/Scrolling check.md"
                if args.scroll_document:
                    source = args.scroll_document.read_text()
                else:
                    paragraph = "Long wrapped text remains responsive while navigating between headings, paragraphs and lists. "
                    source = "# Scrolling check\n\n" + "\n".join(
                        f"## Section {index}\n\n" + paragraph * 5 + "\n\n" +
                        "\n".join(f"- Item {item}: " + paragraph * 2 for item in range(3)) + "\n"
                        for index in range(18)
                    )
                path.write_text(source)
                time.sleep(.7)
                open_note("Scrolling check")
                measurements = {"source_sha256": hashlib.sha256(source.encode()).hexdigest(), "source_bytes": len(source.encode()), "views": {}}
                failures = []
                for view in ("Reading view", "Live preview"):
                    command(view)
                    if view == "Live preview":
                        control_bounds("editor-focused")
                        key("ctrl+Home")
                    def position():
                        return control_bounds("document-scroll-y")[0]
                    xdo("mousemove", "900", "500")
                    start_y = position()
                    assert 0 <= start_y <= 200, (view, start_y)
                    start = time.perf_counter()
                    xdo("click", "--delay", "0", "5")
                    while time.perf_counter()-start < 1 and position()-start_y < 1:
                        time.sleep(.003)
                    immediate = position()
                    elapsed = (time.perf_counter()-start)*1000
                    time.sleep(.2)
                    stopped = position()
                    xdo("click", "--repeat", "4", "--delay", "15", "5")
                    time.sleep(.1)
                    repeated = position()
                    xdo("click", "--delay", "0", "4")
                    time.sleep(.1)
                    reversed_y = position()
                    step = immediate-start_y
                    measurements["views"][view] = {"dispatch_to_offset_ms": elapsed, "pixels_per_notch": step, "tail_pixels": stopped-immediate, "repeated_pixels": repeated-start_y, "reverse_pixels": reversed_y-start_y}
                    (reports / "scroll-latency.json").write_text(json.dumps(measurements, indent=2)+"\n")
                    capture("native-scrolling-" + ("live" if view == "Live preview" else "reading") + ".png")
                    if elapsed >= 150:
                        failures.append((view, "wheel response milliseconds", elapsed))
                    if not (260 <= step <= 340):
                        failures.append((view, "wheel distance", step))
                    if abs(stopped-immediate) >= 1:
                        failures.append((view, "smoothing tail", stopped-immediate))
                    if abs(repeated-start_y-5*step) > 10 or abs(reversed_y-start_y-4*step) > 10:
                        failures.append((view, "distance/direction", step, repeated, reversed_y))
                    assert path.read_text() == source, "Scrolling changed the note"
                scrollbar_pixels(path, source)
                assert not failures, failures
                results.append("Reading and live preview move 300 pixels per wheel notch without a smoothing tail; repeated and reverse input preserve distance/direction and never edit the note")

            def diagram_checks():
                path = fixture / "Notes/Diagram check.md"
                source = "# Diagram check\n\n| Note | State |\n| --- | --- |\n| One | Ready |\n\n```mermaid\nflowchart LR\n A[Write] --> B[Read]\n```\n\nEnd.\n"
                path.write_text(source)
                time.sleep(.7)
                open_note("Diagram check")
                command("Reading view")
                control_bounds("table:0")
                bounds = control_bounds("mermaid-diagram")
                assert 100 < bounds[2]-bounds[0] <= 500 and 30 < bounds[3]-bounds[1] <= 200, "Small diagrams must retain natural size"
                reading_bounds = bounds
                count = control_bounds("diagram-render-count")[0]
                capture("native-table-mermaid-reading.png")
                for _ in range(4):
                    xdo("mousemove", "800", "500")
                    xdo("click", "5")
                    xdo("click", "4")
                time.sleep(.15)
                assert control_bounds("diagram-render-count")[0] == count, "Hover/scroll regenerated a cached diagram"
                command("Live preview")
                key("ctrl+Home")
                control_bounds("edit-table:0")
                bounds = control_bounds("mermaid-live")
                assert 100 < bounds[2]-bounds[0] <= 500 and 30 < bounds[3]-bounds[1] <= 200
                editor_bounds = control_bounds("editor")
                assert bounds[0] >= editor_bounds[0]-1 and bounds[2] <= editor_bounds[2]+1, "The complete Live diagram must fit inside the note"
                assert bounds[1] >= 80 and bounds[3] <= 930, "This diagram must be fully visible"
                assert abs((bounds[2]-bounds[0])-(reading_bounds[2]-reading_bounds[0])) <= 1
                assert abs(bounds[0]-reading_bounds[0]) <= 1, "Live/Reading diagrams must share their note alignment"
                capture("native-table-mermaid-live.png")
                assert control_bounds("diagram-render-count")[0] == count
                assert control_bounds("copy-code:0", required=False) is None
                for queued in (True, False):
                    left, top, right, bottom = control_bounds("mermaid-live")
                    if queued:
                        app.send_signal(signal.SIGSTOP)
                        try:
                            xdo("mousemove", "--sync", str(round((left+right)/2)), str(round((top+bottom)/2)), "click", "1")
                        finally:
                            app.send_signal(signal.SIGCONT)
                    else:
                        xdo("mousemove", "--sync", str(round((left+right)/2)), str(round((top+bottom)/2)), "mousedown", "1")
                        time.sleep(.15)
                        xdo("mouseup", "1")
                    pause()
                    assert control_bounds("mermaid-live", required=False) is None
                    control_bounds("copy-code:0")
                    assert path.read_text() == source
                    key("ctrl+Home")
                    control_bounds("mermaid-live")
                assert control_bounds("diagram-render-count")[0] == count
                command("Source editing")
                assert control_bounds("mermaid-live", required=False) is None
                assert control_bounds("edit-table:0", required=False) is None
                assert path.read_text() == source
                # Invalid syntax remains readable and editable instead of blank output.
                invalid = fixture / "Notes/Invalid diagram.md"
                invalid.write_text("```mermaid\nnot a diagram\n```\n")
                time.sleep(.7)
                open_note("Invalid diagram")
                command("Reading view")
                control_bounds("mermaid-error")
                assert invalid.read_text() == "```mermaid\nnot a diagram\n```\n"
                results.append("Tables and native Mermaid render in Reading and Live, stay literal in Source, reuse the same diagram through hover/scroll, and invalid syntax retains its source")

            def quick_ranking_checks():
                close_tabs()
                originals={}
                for label in ("Closed A","Closed B","Closed C"):
                    path=fixture/(label+".md");originals[path]="# "+label+"\n\nORDERED_QUERY_FIXTURE\n";path.write_text(originals[path])
                alias=fixture/"Quick aliases.md"
                originals[alias]="---\naliases: [Thinking circle, یادداشت فارسی]\n---\n# Alias target\n"
                alias.write_text(originals[alias]);time.sleep(.7);key("ctrl+t")
                def query(value,expected=None):
                    key("ctrl+o");control_bounds("dialog-input-focused")
                    if value:
                        if value.isascii():type_text(value)
                        else:paste_text(value)
                    deadline=time.monotonic()+3
                    while True:
                        actual={name.removeprefix("quick-open:") for name in json.loads((base/"ui-probe.json").read_text()) if name.startswith("quick-open:")}
                        if expected is None or actual==set(expected):return actual
                        if time.monotonic()>=deadline:
                            capture("native-quick-ranking-failure.png")
                            raise AssertionError((value,expected,sorted(actual)))
                        time.sleep(.03)
                for value in ("Closed C","closed c"):
                    query(value,["Closed C.md"]);capture("native-quick-ranking-"+value.replace(" ","-")+".png")
                    key("Return");title_contains("Closed C")
                results.append("Exact and lowercase multiword queries returned only the requested Closed C note rather than reusing the shared prefix")
                for value,title in (("Closed B","Closed B"),("ClsdC","Closed C")):
                    query(value,[title+".md"]);key("Return");title_contains(title)
                results.append("Another exact title and an abbreviated query still opened their intended files through the real chooser")
                for value in ("Thinking circle","یادداشت فارسی"):
                    query(value,["Quick aliases.md"]);key("Return");title_contains("Quick aliases")
                results.append("Multiword and Unicode aliases resolved once to their canonical file without duplicate rows")
                for value in ("Closed Closed","No matching needle qzx"):
                    query(value,[]);key("Return");control_bounds("dialog-input");key("Escape")
                results.append("Repeated-word false matches and a missing filename produced no rows, and Enter created no note")
                rows=query("");assert 1<=len(rows)<=100,rows
                key("Escape");title_contains("Quick aliases")
                assert all(path.read_text()==original for path,original in originals.items())
                results.append("An empty chooser still listed files; cancelling it kept the current document and every fixture byte unchanged")

            def page_preview_checks():
                close_tabs()
                source=fixture/"Preview source.md";target=fixture/"Preview folder/Preview target.md"
                target.parent.mkdir(exist_ok=True)
                original="# Preview source\n\n[[Preview folder/Preview target|Target preview]]\n\n[[Missing preview target|Missing preview]]\n\n`[[Preview target]]`\n\n[External example](https://example.com/)\n\n[[Preview metadata target|Properties preview]]\n"
                target_original="# Preview target\n\nVISIBLE_PREVIEW_DESTINATION\n\n- [ ] Preview task\n\n[Open next](Preview%20next.md)\n"
                metadata=fixture/"Preview metadata target.md"
                metadata_original="---\ntags: [preview]\n---\n# Preview metadata target\n\n- [ ] Metadata task\n"
                metadata.write_text(metadata_original)
                source.write_text(original);target.write_text(target_original)
                (fixture/"Preview folder/Preview next.md").write_text("# Preview next\n\nNEXT_PREVIEW_DESTINATION\n")
                time.sleep(.7);open_note("Preview source");command("Reading view")
                def away(click=False):
                    xdo("keyup","ctrl");xdo("mousemove","--sync","1100","800")
                    if click:xdo("click","1")
                    time.sleep(.5)
                def absent():
                    assert control_bounds("page-preview-card",required=False) is None
                def hover_target(control=False,block=1,editor=False):
                    bounds=control_bounds("editor-link:Preview folder/Preview target|Target preview" if editor else f"reading-block:{block}")
                    x=round((bounds[0]+bounds[2])/2) if editor else round(bounds[0]+25)
                    if control:xdo("keydown","ctrl")
                    xdo("mousemove","--sync",str(x),str(round((bounds[1]+bounds[3])/2)));time.sleep(.8)
                bounds=control_bounds("reading-block:1")
                xdo("mousemove","--sync",str(round(bounds[0]+25)),str(round((bounds[1]+bounds[3])/2)),"click","1");pause();title_contains("Preview target")
                assert target.read_text()==target_original
                open_note("Preview source");command("Reading view")
                results.append("The exact reading-link coordinates actually navigated to the unchanged target as a positive control")
                hover_target();card=control_bounds("page-preview-card")
                title_contains("Preview source");assert source.read_text()==original and target.read_text()==target_original
                assert abs(card[2]-card[0]-452)<=3,card
                capture("native-page-preview-reading.png")
                results.append("Unmodified Reading hover showed the 452px target card without opening a tab or changing notes")
                xdo("mousemove","--sync",str(round((card[0]+card[2])/2)),str(round((card[1]+card[3])/2)));time.sleep(.7)
                control_bounds("page-preview-card");away();absent()
                hover_target(True);control_bounds("page-preview-card");away();absent()
                results.append("Pointer entry kept the card open, unmodified pointer-away dismissed it and Ctrl plus Reading hover also worked")
                for mode in ("Live preview","Source editing"):
                    command(mode);away();hover_target(editor=True);absent()
                    hover_target(True,editor=True);control_bounds("page-preview-card")
                    capture("native-page-preview-"+mode.split()[0].lower()+".png")
                    away();absent()
                results.append("Live and Source required Ctrl for the same preview; plain hover in both modes showed no card")
                command("Reading view");hover_target()
                click_control("page-preview/task:0");title_contains("Preview source")
                assert target.read_text()==target_original.replace("- [ ]","- [x]")
                assert source.read_text()==original
                away();control_bounds("page-preview-card");key("Escape");control_bounds("page-preview-card")
                click_control("page-preview/task:0");assert target.read_text()==target_original
                away(True);absent()
                results.append("Preview task clicks updated only the target, and a focused card survived pointer-away/Escape until an outside click")
                open_note("Preview target");command("Source editing");editor_end()
                type_text("UNDO_FOCUS_PROOF");key("ctrl+z");key("ctrl+s");assert target.read_text()==target_original
                key("ctrl+z");key("ctrl+s");assert target.read_text()==target_original
                results.append("Focused typing and Undo worked, while a newly opened target did not import earlier closed-note preview changes into its editor history")
                open_note("Preview source");command("Reading view");hover_target(block=5)
                click_control("page-preview/task:0");away(True)
                metadata_checked=metadata_original.replace("- [ ]","- [x]");assert metadata.read_text()==metadata_checked
                open_note("Preview metadata target");command("Live preview");editor_end()
                type_text("UNDO_FOCUS_PROOF");key("ctrl+z");key("ctrl+s");assert metadata.read_text()==metadata_checked
                key("ctrl+z");key("ctrl+s");assert metadata.read_text()==metadata_checked
                capture("native-page-preview-properties.png")
                open_note("Preview source");command("Reading view");hover_target(block=5)
                click_control("page-preview/task:0");away(True);assert metadata.read_text()==metadata_original
                open_note("Preview target");command("Source editing")
                results.append("Properties-note preview edits preserved exact frontmatter and the same first-Live-editor history behavior, with metadata and task bytes restored")
                command("Open settings");type_text("autosave")
                click_control("settings-autosave",hold=.15)
                sessions=[path for path in (base/"state/rustidian").glob("*.json") if any(name=="Preview folder/Preview target.md" for name,_ in json.loads(path.read_text()).get("tabs",[]))]
                assert len(sessions)==1,sessions
                session_path=sessions[0]
                wait_autosave(base/"state",False,session_path=session_path)
                key("Escape")
                editor_end();type_text("BUFFER_PREVIEW_MARKER");assert target.read_text()==target_original
                key("ctrl+t");open_note("Preview source");command("Reading view");hover_target()
                card=control_bounds("page-preview-card");task=control_bounds("page-preview/task:0")
                assert task[1]>=card[1]+31 and task[3]<=card[3]-31,("A longer preview inherited a short card's clipped viewport",card,task)
                capture("native-page-preview-open-buffer.png")
                click_control("page-preview/task:0");assert target.read_text()==target_original
                away(True);click_control("tab:Preview folder/Preview target.md");title_contains("Preview target")
                key("ctrl+s");assert target.read_text()==target_original.replace("- [ ]","- [x]")+"BUFFER_PREVIEW_MARKER"
                # Tab selection retains focus for arrow navigation. Re-enter the
                # document before exercising its text Undo, without changing it.
                editor_end()
                assert control_bounds("editor-memory-focus")[0] == 1
                key("ctrl+z");key("ctrl+s");assert target.read_text()==target_original+"BUFFER_PREVIEW_MARKER"
                key("ctrl+z");key("ctrl+s");assert target.read_text()==target_original
                command("Open settings");type_text("autosave")
                click_control("settings-autosave",hold=.15)
                wait_autosave(base/"state",True,session_path=session_path)
                key("Escape")
                results.append("A dirty target in another tab kept its prior text with autosave disabled, shared the task change and undid task then typing separately")
                click_control("tab:Preview source.md");command("Reading view");hover_target()
                changed=target_original.replace("VISIBLE_PREVIEW_DESTINATION","EXTERNAL_PREVIEW_UPDATE")
                target.write_text(changed);time.sleep(1.1);control_bounds("page-preview-card")
                capture("native-page-preview-external-update.png")
                click_control("page-preview/task:0")
                assert target.read_text()==changed.replace("- [ ]","- [x]")
                click_control("page-preview/task:0");assert target.read_text()==changed
                target.write_text(target_original);time.sleep(1.1);away(True)
                results.append("An external target update refreshed the open card before task editing and was preserved byte-for-byte")
                hover_target();click_control("page-preview-open");title_contains("Preview target")
                assert control_bounds("tab-count")[0]==2
                # Reopen the source in this tab, leaving the original target tab available.
                open_note("Preview source");command("Reading view");hover_target()
                link=control_bounds("page-preview/reading-block:3")
                xdo("mousemove","--sync",str(round(link[0]+25)),str(round((link[1]+link[3])/2)),"click","1");pause();title_contains("Preview next")
                results.append("The corner Open link and an internal card link replaced the current view, resolving the latter from the target's folder")
                open_note("Preview source");command("Reading view");hover_target(block=2)
                control_bounds("page-preview-create");assert not (fixture/"Missing preview target.md").exists()
                capture("native-page-preview-missing.png");away();absent()
                hover_target(block=2);click_control("page-preview-create");title_contains("Missing preview target")
                assert (fixture/"Missing preview target.md").is_file()
                results.append("A missing note showed a create card without writing on hover and created the note only after its actual button click")
                open_note("Preview source");command("Reading view")
                for block in (3,4):
                    away();hover_target(block=block);absent()
                results.append("Literal code and an external web link did not open an internal-note preview")
                for dark in (False,True):
                    set_theme(dark);hover_target();control_bounds("page-preview-card")
                    capture("native-page-preview-"+("dark" if dark else "light")+".png");away()
                xdo("windowsize",window,"860","560");pause();hover_target();card=control_bounds("page-preview-card")
                assert 0<=card[0]<card[2]<=860 and 0<=card[1]<card[3]<=560,card
                capture("native-page-preview-compact.png",(860,560));away(True)
                xdo("windowsize",window,"1440","960");pause()
                assert source.read_text()==original and target.read_text()==target_original and metadata.read_text()==metadata_original
                results.append("Both themes and compact-window preview geometry stayed inside the viewport with original source/target bytes restored")
                close_tabs();open_note("Preview target")
                command("Graph view: Open graph view")
                control_bounds("global-graph:canvas")
                key("ctrl+t");open_note("Preview source");command("Reading view")
                hover_target();click_control("page-preview/task:0")
                assert target.read_text()==target_original.replace("- [ ]","- [x]")
                away(True);key("ctrl+w")
                title_contains("Graph view")
                key("ctrl+s");key("ctrl+w")
                open_note("Preview target")
                assert target.read_text()==target_original.replace("- [ ]","- [x]")
                command("Source editing");editor_end();key("ctrl+a");paste_text(target_original);key("ctrl+s")
                assert target.read_text()==target_original and source.read_text()==original
                results.append("Editing a preview of a graph-backed note saved the actual file; Save and closing the graph could not discard that change")

            def navigation_checks():
                note = fixture / "Notes/Navigation UI.md"
                target = fixture / "Notes/Linked target.md"
                file_uri = target.as_uri()
                evidence = base / "Outside evidence/record #1.json"
                evidence.parent.mkdir()
                evidence.write_text('{"proof":"original evidence"}\n')
                attachment = fixture / "Notes/record.json"
                attachment.write_text('{"attachment":"original evidence"}\n')
                source = "# Navigation UI\n\n[[Notes/Linked target|Wiki target]]\n\n[Open target](Linked%20target.md#Section%20two)\n\n[Open file](" + file_uri + ")\n\n[Open relative evidence](../../Outside%20evidence/record%20%231.json)\n\n[Open attachment](record.json)\n\n[Open absolute evidence](" + evidence.as_uri().removeprefix("file://") + ")\n"
                note.write_text(source)
                target_source = "# Linked target\n\n" + "Introduction paragraph.\n\n" * 45 + "## Section two\n\nDestination text.\n"
                target.write_text(target_source)
                time.sleep(.6)
                open_note("Navigation UI")
                command("Reading view")

                def click_link(block, modified=False):
                    left, top, right, bottom = control_bounds(f"reading-block:{block}")
                    if modified:
                        xdo("keydown", "Control_L")
                    try:
                        xdo("mousemove", str(round(left + min(25, (right-left)/2))), str(round((top+bottom)/2)), "click", "1")
                        pause()
                    finally:
                        if modified:
                            xdo("keyup", "Control_L")

                def tab_count():
                    return sum(name.startswith("tab:") for name in json.loads((base / "ui-probe.json").read_text()))

                click_link(1)
                title_contains("Linked target")
                key("alt+Left")
                title_contains("Navigation UI")
                click_link(2)
                title_contains("Linked target")
                assert control_bounds("reading-block:46"), "The encoded heading target must scroll into view"
                key("alt+Left")
                title_contains("Navigation UI")
                key("alt+Right")
                title_contains("Linked target")
                key("alt+Left")
                title_contains("Navigation UI")
                before = tab_count()
                click_link(2, modified=True)
                title_contains("Linked target")
                deadline = time.monotonic() + 3
                while tab_count() != before + 1 and time.monotonic() < deadline:
                    time.sleep(.05)
                assert tab_count() == before + 1, "Ctrl-click must retain the source tab"
                key("ctrl+w")
                title_contains("Navigation UI")
                command("Live preview")
                key("ctrl+Home")
                click_control("editor-link:Linked%20target.md#Section%20two")
                title_contains("Linked target")
                key("alt+Left")
                title_contains("Navigation UI")
                command("Reading view")
                requests = reports / "browser-requests.txt"
                for block, expected_uri in [(3, file_uri), (4, evidence.as_uri()), (5, attachment.as_uri()), (6, evidence.as_uri())]:
                    before_requests = requests.read_text() if requests.exists() else ""
                    click_link(block)
                    deadline = time.monotonic() + 3
                    while (not requests.exists() or requests.read_text() == before_requests) and time.monotonic() < deadline:
                        time.sleep(.05)
                    assert requests.exists() and expected_uri in requests.read_text()[len(before_requests):], f"File link {block} must reach the desktop file handler"
                    title_contains("Navigation UI")
                    time.sleep(.4)
                    assert not control_bounds("page-preview-create", required=False), "An existing file must not offer to create a missing note"
                assert not fixture.joinpath("file:").exists(), "A URI must not create spurious notes"
                assert evidence.read_text() == '{"proof":"original evidence"}\n'
                assert attachment.read_text() == '{"attachment":"original evidence"}\n'
                assert not attachment.with_suffix(".json.md").exists()
                assert note.read_text() == source
                assert target.read_text() == target_source
                capture("native-navigation.png")
                results.append("Actual wiki/Markdown link clicks, encoded spaces, heading targets and Back/Forward work; Ctrl-click retains its source tab, plain Live links open, and all source bytes stay intact")

                plan = fixture / "Notes/Plan UI.md"
                spec = fixture / "Notes/spec.md"
                plan_source = "# Plan\n\n**Status:** Approved · **Date:** 21 October · [Approved spec](spec.md)\n\nTail.\n"
                plan.write_text(plan_source)
                spec.write_text("# Approved specification\n\nLinked document.\n")
                time.sleep(.6)
                open_note("Plan UI")
                command("Live preview")
                key("ctrl+End")
                click_control("editor-link:spec.md")
                title_contains("spec")
                key("alt+Left")
                title_contains("Plan UI")
                assert plan.read_text() == plan_source
                results.append("A plain click on the inline Approved spec Markdown link opens the sibling file and Back returns to the unchanged plan")
                command("Live preview")
                key("ctrl+End")
                left, top, right, bottom = control_bounds("editor-link:spec.md")
                x, y = round((left+right)/2), round((top+bottom)/2)
                xdo("mousemove", "--sync", str(x), str(y), "mousedown", "1")
                time.sleep(.15)
                xdo("mousemove", "--sync", str(x+160), str(y), "mouseup", "1")
                pause()
                title_contains("Plan UI")
                key("ctrl+End")
                left, top, right, bottom = control_bounds("editor-link:spec.md")
                xdo("mousemove", "--sync", str(round((left+right)/2)), str(round((top+bottom)/2)), "mousedown", "1")
                time.sleep(.15)
                xdo("mouseup", "1")
                pause()
                title_contains("spec")
                key("alt+Left")
                title_contains("Plan UI")
                assert plan.read_text() == plan_source
                results.append("A held click follows the original rendered Live link across separate input frames; dragging that link selects text without opening its file")

                origin = fixture / "Notes/Queued origin.md"
                destination = fixture / "Notes/Queued target.md"
                origin_source = "# Queued origin\n\n[[Notes/Queued target]]"
                origin.write_text(origin_source)
                destination.write_text("# Queued target\n\nIndependent destination.\n")
                time.sleep(.6)
                key("ctrl+t")
                open_note("Queued origin")
                command("Source editing")
                key("ctrl+End")
                before = tab_count()
                queued_steps(("key", "ctrl+Return"))
                title_contains("Queued target")
                assert tab_count() == before + 1, "A complete Ctrl+Enter chord in one frame must retain its source in a new tab"
                assert control_bounds("tab-full:Notes/Queued origin.md")
                key("alt+Left")
                title_contains("Queued target")
                key("ctrl+w")
                title_contains("Queued origin")
                assert origin.read_text() == origin_source
                results.append("Queued Ctrl+Enter retains the origin and gives its new destination tab independent history even when Control is released in the same frame")

            def essential_checks():
                nonlocal app, window
                source = "# Highlight check\n\n==yellow **bold**==\n\n`==literal code==`\n\neditable"
                note = fixture / "Notes/Essential UI.md"
                source = source.replace("\n\neditable", "\n\nSee [[Home]].\n\neditable")
                note.write_text(source)
                (fixture / "Notes/Referrer.md").write_text("[[Notes/Essential UI]]\n")
                time.sleep(.6)
                open_note("Essential UI")
                command("Source editing")
                editor_end()
                assert control_bounds("editor-focused"), "Source input must own focus after the palette closes"
                type_text("control")
                key("ctrl+s")
                assert note.read_text() == source + "control", repr(note.read_text())
                key("ctrl+z")
                key("ctrl+s")
                assert note.read_text() == source
                def marked_pixels(name, control):
                    capture(name)
                    path = ROOT / "reports" / name
                    box = tuple(round(value) for value in control_bounds(control))
                    return sum(r > g > b + 20 and r - g < 45 for r, g, b in pixel_data(Image.open(path).crop(box).convert("RGB")))
                command("Reading view")
                type_text("ignored")
                key("ctrl+s")
                assert note.read_text() == source
                assert marked_pixels("native-highlights-reading.png", "reading-block:1") > 15
                command("Live preview")
                assert marked_pixels("native-highlights-live-dark.png", "editor") > 15
                command("Source editing")
                assert marked_pixels("native-highlights-source.png", "editor") == 0
                editor_end()
                key("ctrl+shift+Left")
                command("Highlight selection")
                key("ctrl+s")
                highlighted = source[:-len("editable")] + "==editable=="
                assert note.read_text() == highlighted, repr(note.read_text())
                key("ctrl+z")
                key("ctrl+s")
                assert note.read_text() == source
                key("ctrl+y")
                key("ctrl+s")
                assert note.read_text() == highlighted
                command("Live preview")
                command("Switch appearance")
                assert marked_pixels("native-highlights-live-light.png", "editor") > 15
                command("Switch appearance")
                command("srcedt")
                assert control_bounds("dialog-input", required=False) is None
                assert marked_pixels("native-source-fuzzy.png", "editor") == 0
                key("ctrl+p")
                type_text("save")
                capture("native-command-palette.png")
                assert control_bounds("command:Save note")
                key("Escape")
                results.append("Native highlighting renders in Live/Reading, Source and inline code stay literal, palette insertion preserves selection/undo/redo and fuzzy command discovery works")

                command("Open settings")
                capture("native-settings.png")
                type_text("autosave")
                assert control_bounds("settings-dark", required=False) is None
                assert control_bounds("settings-autosave")
                key("ctrl+a")
                key("BackSpace")
                click_control("settings-page:Appearance")
                control_bounds("settings-dark")
                click_control("settings-autosave")
                state_files = [p for p in (base / "state" / "rustidian").glob("*.json") if any(path == "Notes/Essential UI.md" for path, _ in json.loads(p.read_text()).get("tabs", []))]
                assert len(state_files) == 1
                state_path = state_files[0]
                assert json.loads(state_path.read_text())["autosave_enabled"] is False
                key("Escape")
                editor_end()
                type_text(" offline")
                time.sleep(1)
                assert note.read_text() == highlighted
                key("ctrl+s")
                expected = highlighted + " offline"
                assert note.read_text() == expected
                xdo("key", "--clearmodifiers", "ctrl+q")
                app.wait(timeout=5)
                app = launch_app(launch_command)
                window = ready_window()
                title_contains("Essential UI")
                editor_end()
                type_text(" restored")
                time.sleep(1)
                assert note.read_text() == expected
                key("ctrl+s")
                expected += " restored"
                assert note.read_text() == expected
                key("ctrl+comma")
                click_control("settings-page:Editor")
                assert control_bounds("settings-autosave")
                click_control("settings-autosave")
                assert json.loads(state_path.read_text())["autosave_enabled"] is True
                displaced = base / "held-essential-state.json"
                state_path.rename(displaced)
                state_path.mkdir()
                try:
                    click_control("settings-autosave")
                    capture("native-settings-write-error.png")
                    assert control_bounds("error-save-copy", required=False) is None
                finally:
                    state_path.rmdir()
                    displaced.rename(state_path)
                assert json.loads(state_path.read_text())["autosave_enabled"] is True
                click_control("settings-autosave")
                assert json.loads(state_path.read_text())["autosave_enabled"] is False
                click_control("settings-autosave")
                assert json.loads(state_path.read_text())["autosave_enabled"] is True
                key("Escape")
                assert note.read_text() == expected
                results.append("Searchable settings persist autosave outside the vault across restart; failed writes roll back, retry succeeds, explicit saving and original Markdown stay intact")
                if control_bounds("panel:outline", required=False) is None:
                    command("Toggle right sidebar")
                assert control_bounds("panel:outline")
                click_control("panel:backlinks")
                capture("native-panel-backlinks.png")
                click_control("backlink:Notes/Referrer.md")
                title_contains("Referrer")
                open_note("Essential UI")
                click_control("panel:outgoing")
                click_control("outgoing:Home")
                title_contains("Home")
                open_note("Essential UI")
                click_control("panel:outline")
                assert note.read_text() == expected
                results.append("Outline/backlink/outgoing tabs expose working panels; real link clicks navigate to the correct note without changing Markdown")


            def task_checks():
                key("ctrl+n")
                capture("native-new-note-created.png")
                key("Escape")
                source = "# Task GUI\n\n- [ ] Native task\n- [x] Done فارسی\n  - [ ] Nested\n\n```md\n- [ ] Literal code\n```\n"
                task_note = fixture / "Notes/Task GUI.md"
                task_note.write_text(source)
                time.sleep(.6)
                open_note("Task GUI")
                command("Source editing")
                editor_end()
                type_text("control")
                key("ctrl+s")
                assert task_note.read_text() == source + "control"
                key("ctrl+z")
                key("ctrl+s")
                assert task_note.read_text() == source
                command("Reading view")
                capture("native-tasks.png")
                first_box = control_bounds("task:0")
                first_label = control_bounds("reading-block:1")
                next_label = control_bounds("reading-block:2")
                assert abs((first_box[1]+first_box[3])/2-(first_label[1]+14.5)) <= 1, "Task checkbox must align with its first text line"
                assert 8 <= first_label[0]-first_box[2] <= 10, "Task label gap must match Obsidian"
                assert 2 <= next_label[1]-first_label[3] <= 4, "Tight checklist rows must stay compact"
                box = tuple(round(value) for value in control_bounds("task:1"))
                pixels = pixel_data(Image.open(ROOT / "reports/native-tasks.png").crop(box).convert("RGB"))
                assert sum(abs(r-23) <= 3 and abs(g-127) <= 3 and abs(b-131) <= 3 for r, g, b in pixels) > 80, "Completed tasks must have Obsidian's solid accent fill"
                click_control("task:0")
                time.sleep(1)
                checked = source.replace("- [ ] Native task", "- [x] Native task")
                assert task_note.read_text() == checked
                click_control("task:0")
                time.sleep(1)
                assert task_note.read_text() == source
                assert control_bounds("task:3", required=False) is None
                command("Live preview")
                capture("native-tasks-live-dark.png")
                assert control_bounds("task-live:0")
                assert control_bounds("task-live:1")
                assert control_bounds("task-live:2")
                assert control_bounds("task-live:3", required=False) is None
                click_control("task-live:0")
                key("ctrl+s")
                assert task_note.read_text() == checked
                key("ctrl+z")
                key("ctrl+s")
                assert task_note.read_text() == source
                key("ctrl+y")
                key("ctrl+s")
                assert task_note.read_text() == checked
                command("Source editing")
                assert control_bounds("task-live:0", required=False) is None
                command("Live preview")
                command("Switch appearance")
                capture("native-tasks-live-light.png")
                command("Switch appearance")
                loose = fixture / "Notes/Loose checklist.md"
                loose_source = "# Loose checklist\n\n- [ ] First paragraph\n\n- [x] Second paragraph\n"
                loose.write_text(loose_source)
                time.sleep(.6)
                open_note("Loose checklist")
                command("Reading view")
                first = control_bounds("reading-block:1")
                second = control_bounds("reading-block:2")
                assert second[1]-first[3] >= 14, "Blank lines in loose lists must retain paragraph spacing"
                assert loose.read_text() == loose_source
                results.append("Reading and Live tasks preserve exact Markdown, nested/Unicode tasks and undo/redo; Source and fenced task syntax stay literal")
            def task_clip_checks():
                task_note = fixture / "Notes/Task clipping.md"
                source = "# Task clipping\n\n" + "\n".join(f"- [x] Completed task {index}" for index in range(48)) + "\n\nEnd.\n"
                task_note.write_text(source)
                time.sleep(.6)
                open_note("Task clipping")
                command("Live preview")
                key("ctrl+Home")
                first = control_bounds("task-live:0")
                chrome_bottom = round(control_bounds("button:More options")[3])
                band = (round(first[0])-2, 0, round(first[2])+2, chrome_bottom)
                before = ImageGrab.grab(xdisplay=display).crop(band).convert("RGB")
                xdo("mousemove", "900", "500", "click", "5")
                pause()
                assert control_bounds("document-scroll-y")[0] >= 260
                capture("native-task-clipping.png")
                after = Image.open(reports / "native-task-clipping.png").crop(band).convert("RGB")
                accent = lambda pixel: max(abs(pixel[i] - (23, 127, 131)[i]) for i in range(3)) <= 3
                escaped = sum(accent(new) and not accent(old) for old, new in zip(pixel_data(before), pixel_data(after)))
                (reports / "task-clipping.json").write_text(json.dumps({"chrome_band":band,"escaped_checkbox_pixels":escaped},indent=2)+"\n")
                assert escaped <= 4, ("Checkboxes painted over the tabs or navigation bar", escaped)
                assert control_bounds("task-live:0", required=False) is None, "An offscreen task must not expose a checkbox over the window chrome"
                visible = json.loads((base / "ui-probe.json").read_text())
                assert any(name.startswith("task-live:") and bounds[1] >= chrome_bottom for name,bounds in visible.items())
                assert task_note.read_text() == source
                results.append("Scrolled Live checklist checkboxes stay inside the document; the tabs and navigation bar retain their pixels and the note stays unchanged")

            dark_palette = [(250,153,205), (68,207,110), (168,130,255), (251,70,76), (83,223,221), (104,115,117)]
            light_palette = [(213,57,132), (8,185,78), (120,82,238), (233,49,71), (0,191,188), (171,171,171)]

            def colored_pixels(palette, required=True, control="code-block:0"):
                bounds = tuple(round(value) for value in control_bounds(control))
                pixels = list(pixel_data(ImageGrab.grab(xdisplay=display).crop(bounds)))
                for color in palette:
                    count = sum(max(abs(pixel[channel]-color[channel]) for channel in range(3)) <= 18 for pixel in pixels)
                    assert count >= 2 if required else count == 0, (color, count)

            def syntax_checks():
                source = (ROOT / "tests/fixtures/syntax.md").read_text()
                note = fixture / "Syntax GUI.md"
                note.write_text(source)
                time.sleep(.6)
                open_note("Syntax GUI")
                command("Live preview")
                colored_pixels(dark_palette)
                capture("native-syntax-live.png")
                command("Source editing")
                colored_pixels(dark_palette)
                capture("native-syntax-source.png")
                assert note.read_text() == source
                results.append("Native Rust highlighting matched all six measured dark token colors in Source and Live while preserving the exact Markdown")
                command("Switch appearance")
                colored_pixels(light_palette)
                capture("native-syntax-light.png")
                command("Live preview")
                editor_end()
                key("ctrl+Home")
                key("Down")
                key("Home")
                type_text("pub ")
                key("ctrl+s")
                assert note.read_text() == source.replace("fn main", "pub fn main", 1)
                colored_pixels(light_palette)
                key("ctrl+z")
                key("ctrl+s")
                assert note.read_text() == source
                command("Switch appearance")
                colored_pixels(dark_palette)
                results.append("Measured light token colors survived live code edits, undo and a switch back to dark without stale colors or source loss")
                plain_source = "~~~unknownlang\nfn main() { let value = \"uncoloured\"; }\n~~~\n"
                plain = fixture / "Plain code GUI.md"
                plain.write_text(plain_source)
                time.sleep(.6)
                open_note("Plain code GUI")
                command("Live preview")
                colored_pixels(dark_palette[:5], required=False)
                assert plain.read_text() == plain_source
                results.append("An unknown fence language stayed readable as plain code without inheriting the previous file's syntax colors")

                # Code files use Source; Markdown fences exercise all three views.
                language_cases = [
                    ("ts", '// TypeScript comment\r\nconst greeting: string = "Hello TypeScript";\r\n'),
                    ("tsx", '// TSX comment\r\nconst view = <section title="Hello TSX">text</section>;\r\n'),
                ]
                language_files = []
                for language, body in language_cases:
                    path = fixture / f"Native language.{language}"
                    original = body.encode()
                    path.write_bytes(original)
                    markdown = fixture / f"Native {language} fence.md"
                    fenced = (f"~~~{language}\r\n{body}~~~\r\n").encode()
                    markdown.write_bytes(fenced)
                    language_files.append((language, path, original, markdown, fenced))
                time.sleep(.6)

                def save_round_trip(path, original):
                    editor_end()
                    type_text("SAVE_PROBE")
                    save_shortcut(xdo)
                    pause()
                    assert path.read_bytes() == original + b"SAVE_PROBE", path.name
                    key("ctrl+z")
                    save_shortcut(xdo)
                    pause()
                    assert path.read_bytes() == original, (path.name, "Save/Undo changed original bytes")

                for dark, palette in [(True, dark_palette), (False, light_palette)]:
                    set_theme(dark)
                    appearance = "dark" if dark else "light"
                    # Only keyword, string and comment colors belong to this contract.
                    required = [palette[index] for index in (0, 1, 5)]
                    for language, path, original, markdown, fenced in language_files:
                        open_note(path.name, title=path.name + " —")
                        command("Source editing")
                        colored_pixels(required, control="editor")
                        capture(f"native-syntax-{language}-file-source-{appearance}.png")
                        save_round_trip(path, original)
                        open_note(markdown.stem)
                        for mode, name in [("Source editing", "source"), ("Live preview", "live"), ("Reading view", "reading")]:
                            command(mode)
                            colored_pixels(required)
                            capture(f"native-syntax-{language}-fence-{name}-{appearance}.png")
                            if name != "reading":
                                save_round_trip(markdown, fenced)
                            else:
                                save_shortcut(xdo)
                                pause()
                            assert markdown.read_bytes() == fenced
                        assert path.read_bytes() == original
                set_theme(True)
                results.append("Actual TS/TSX source files and Source/Live/Reading fences rendered keyword/string/comment colors in both themes; real edit/Save/Undo/Save preserved every original CRLF byte")

            def reading_code_checks():
                # The compact-tab check can restore a previously open outline.
                # This fixture measures the 920px reader with that pane closed.
                if control_bounds("panel:outline", required=False) is not None:
                    click_control("button:Toggle right sidebar")
                assert control_bounds("panel:outline", required=False) is None
                source = (ROOT / "tests/fixtures/syntax.md").read_text()
                plain_body = "**literal** [[Never open]] π\n"
                source += "\n~~~text\n" + plain_body + "~~~\n\n~~~\n~~~\n\nAfter code.\n"
                note = fixture / "Reading code GUI.md"
                note.write_text(source)
                time.sleep(.6)
                open_note("Reading code GUI")
                command("Reading view")
                panel = control_bounds("code-block:0")
                assert abs(panel[2] - panel[0] - 920) < 1, panel
                assert abs(panel[3] - panel[1] - 177) < 1, panel
                assert control_bounds("code-source:0", required=False) is None
                colored_pixels(dark_palette + [(224,222,113)])
                colored_pixels(dark_palette[:5], required=False, control="code-block:1")
                empty = control_bounds("code-block:2")
                assert abs(empty[3] - empty[1] - 49.515) < 1, empty
                capture("native-reading-code-dark.png")
                results.append("Reading code used a full-width panel and kept both fences hidden without editing the note")
                second = control_bounds("code-block:1")
                xdo("mousemove", str(round(second[2] - 18)), str(round(second[1] + 12)))
                pause()
                capture("native-reading-code-copy.png")
                click_control("copy-code:1")
                create_note_named('Reading clipboard.md')
                command("Source editing")
                key("ctrl+a")
                key("ctrl+v")
                key("ctrl+s")
                assert (fixture / "Reading clipboard.md").read_text() == plain_body
                assert note.read_text() == source
                results.append("Reading code's generated Copy control pasted the exact Unicode code body without fences or activating a literal wiki link")
                open_note("Reading code GUI")
                command("Reading view")
                command("Switch appearance")
                capture("native-reading-code-light.png")
                colored_pixels(light_palette + [(224,172,0)])
                colored_pixels(light_palette[:5], required=False, control="code-block:1")
                type_text("READING_CODE_MUST_NOT_CHANGE")
                key("ctrl+s")
                command("Switch appearance")
                assert note.read_text() == source
                assert not (fixture / "Never open.md").exists()
                results.append("Reading code matched all seven measured token colors in both themes, kept plain blocks uncolored and empty blocks visible, and rejected text input")

            def trash_options_checks():
                nonlocal app, window
                source = "# Contextual deletion\n\nKeep this text.\n"
                first = fixture / "Notes/Trash first.md"
                second = fixture / "Notes/Trash second.md"
                first.write_text(source)
                second.write_text(source)
                time.sleep(.6)
                open_note("Trash first")
                command("Source editing")
                key("ctrl+End")
                type_text("d")
                key("ctrl+s")
                assert first.read_text() == source + "d", "An ordinary D must type text, not delete a file"
                file_menu("Notes/Trash first.md")
                control_bounds("menu:D  Delete")
                key("d")
                control_bounds("trash-dont-ask")
                capture("native-trash-confirmation.png")
                click_control("trash-dont-ask")
                key("Escape")
                assert first.exists()
                file_menu("Notes/Trash first.md")
                key("d")
                control_bounds("trash-dont-ask")
                click_control("trash-dont-ask")
                key("Return")
                assert not first.exists()
                assert any(path.read_bytes() == (source + "d").encode() for path in (base / "data/Trash/files").iterdir() if path.is_file())
                results.append("D types normally outside menus; its visible menu mnemonic opens recoverable Trash, and cancelling does not save the skip-confirmation choice")
                xdo("key", "--clearmodifiers", "ctrl+q")
                app.wait(timeout=10)
                app = launch_app(launch_command)
                window = ready_window()
                file_menu("Notes/Trash second.md")
                key("d")
                assert not second.exists()
                assert control_bounds("trash-dont-ask", required=False) is None
                command("Open settings")
                click_control("settings-page:Files and links")
                click_control("settings-confirm-trash")
                key("Escape")
                second.write_text(source)
                time.sleep(.6)
                file_menu("Notes/Trash second.md")
                key("d")
                control_bounds("trash-dont-ask")
                key("Escape")
                assert second.read_text() == source
                results.append("Confirmed skip preference survives a real restart, and Settings restores the confirmation without changing the surviving note")

            def palette_ready():
                # Wait for rendered, colored logos rather than capturing loader spinners.
                for _ in range(50):
                    cards = [control_bounds("settings-palette:" + name) for name in ("Original teal", "Love Space", "Elpis Gold", "Signature")]
                    image = ImageGrab.grab(xdisplay=display).convert("RGB")
                    counts = []
                    for left, top, right, bottom in cards:
                        center = (left+right)/2
                        crop = image.crop((round(center-24),round(top+8),round(center+24),round(top+68)))
                        counts.append(sum(max(pixel)-min(pixel)>35 for pixel in pixel_data(crop)))
                    if min(counts) >= 60:
                        return
                    assert app.poll() is None, "App exited before drawing palette logos"
                    time.sleep(.1)
                raise AssertionError(("Palette logos did not finish drawing in their visible cards", counts, cards))

            def note_dialog_checks():
                original = {path: path.read_bytes() for path in fixture.rglob("*.md")}
                key("ctrl+n")
                control_bounds("note-title-input")
                assert control_bounds("dialog-input", required=False) is None, "New note must create immediately, without a naming modal"
                first = fixture / "Untitled.md"
                assert first.exists() and first.read_bytes() == b"", "A new note must start empty, without a duplicate Markdown heading"
                control_bounds("note-title-input")
                capture("native-new-note-immediate.png")
                type_text("Quick thought")
                key("Return")
                renamed = fixture / "Quick thought.md"
                assert renamed.exists() and not first.exists(), "Typing must replace the selected inline title"
                type_text("Write immediately after naming.")
                key("ctrl+s")
                assert renamed.read_text() == "Write immediately after naming.", "Enter from the title must focus the note body"
                capture("native-new-note-written.png")
                results.append("New note creates an empty file immediately; the selected inline title renames it, and Enter moves directly to body typing")
                for index, action in enumerate([
                    lambda: click_control("ribbon-new-note"),
                    lambda: click_control("button:New note · Ctrl+N"),
                    lambda: command("New note"),
                ]):
                    action()
                    control_bounds("note-title-input")
                    name = "Untitled.md" if index == 0 else f"Untitled {index}.md"
                    path = fixture / name
                    assert control_bounds("dialog-input", required=False) is None
                    control_bounds("note-title-input")
                    assert path.exists() and path.read_bytes() == b"", name
                    key("Escape")
                assert renamed.read_text() == "Write immediately after naming."
                results.append("Ribbon, file-explorer and command-palette creation share collision-safe empty Untitled naming without touching existing notes")
                open_note("Quick thought")
                command("Open settings")
                control_bounds("settings-close")
                click_control("settings-close")
                for name in ["New folder", "Rename or move file", "Save a copy", "Add property"]:
                    if name == "New folder":
                        click_control("button:New folder")
                    else:
                        command(name)
                    cancel = control_bounds("dialog-cancel")
                    submit = control_bounds("dialog-submit")
                    assert cancel[2] < submit[0], (name, "Cancel precedes the primary action", cancel, submit)
                    assert min(cancel[3]-cancel[1], submit[3]-submit[1]) >= 32, (name, cancel, submit)
                    bounds = control_bounds("dialog-bounds"); button = control_bounds("dialog-close")
                    assert bounds[3]-bounds[1] <= 260, (name, "A one-input form must size to its content, including after Settings", bounds)
                    assert 22 <= bounds[3]-submit[3] <= 28, (name, "Footer stays at the padded bottom edge", bounds, submit)
                    assert 23 <= bounds[2]-button[2] <= 27 and 23 <= button[1]-bounds[1] <= 27, (name, bounds, button)
                    capture("native-form-" + name.lower().replace(" ", "-") + ".png")
                    click_control("dialog-cancel")
                    assert control_bounds("dialog-input", required=False) is None
                    control_bounds("editor-focused")
                    key("ctrl+End")
                    type_text(" Continues after cancelling.")
                    key("ctrl+s")
                    assert renamed.read_text() == "Write immediately after naming. Continues after cancelling."
                    key("ctrl+z"); key("ctrl+s")
                    assert renamed.read_text() == "Write immediately after naming."
                open_note("Quick thought")
                file_menu("Quick thought.md")
                key("d")
                assert control_bounds("dialog-input", required=False) is None, "Trash must show a filename label, not an input"
                control_bounds("trash-target")
                control_bounds("trash-dont-ask")
                cancel = control_bounds("dialog-cancel")
                submit = control_bounds("dialog-submit")
                assert cancel[2] < submit[0]
                capture("native-trash-dialog-refined.png")
                click_control("dialog-cancel")
                assert renamed.read_text() == "Write immediately after naming."
                assert all(path.read_bytes() == expected for path, expected in original.items())
                for dark in (True, False):
                    set_theme(dark)
                    command("Open settings")
                    palette_ready()
                    bounds = control_bounds("dialog-bounds")
                    button = control_bounds("settings-close")
                    assert 23 <= bounds[2]-button[2] <= 27 and 23 <= button[1]-bounds[1] <= 27, ("Settings close must use the shared 24px header inset", bounds, button)
                    capture("native-settings-header-" + ("dark" if dark else "light") + ".png")
                    click_control("settings-close")
                    assert control_bounds("settings-close", required=False) is None
                results.append("Settings close control stays inside the shared padded header in dark and light, and real clicks close the dialog")
                results.append("Standard form actions share readable button geometry; Trash has a non-editable target and cancellation preserves every original note")

            def palette_checks():
                nonlocal app, window
                def expect_icons(value):
                    assert system_launcher.read_text() == original_launcher, "System launchers must remain unchanged"
                    asset = ROOT / ("docs/brand/rustidian.png" if value == "teal" else f"docs/brand/themes/{value}.png")
                    entry = launcher.read_text()
                    icon_lines = [line[5:] for line in entry.splitlines() if line.startswith("Icon=")]
                    assert len(icon_lines) == 2 and icon_lines[1] == "keep-action-icon", entry
                    icon_path = Path(icon_lines[0])
                    assert icon_path.is_relative_to(base / "data"), ("Use the private XDG data folder", icon_path)
                    assert icon_path.read_bytes() == asset.read_bytes(), ("Launcher must use the selected palette logo", value, icon_lines)
                    assert "Exec=/usr/bin/true %f\n" in entry and "Actions=Example;\n" in entry, entry
                    raw = subprocess.check_output(["xprop", "-id", window, "-notype", "-f", "_NET_WM_ICON", "32c", "_NET_WM_ICON"], env=environment, text=True)
                    pixels = [int(value.strip()) for value in raw.split("=", 1)[1].split(",")]
                    width, height = pixels[:2]
                    assert width > 0 and height > 0 and len(pixels) == width * height + 2, "The native window must expose its selected icon"
                    actual = [((p >> 16) & 255, (p >> 8) & 255, p & 255, p >> 24) for p in pixels[2:]]
                    expected = list(pixel_data(Image.open(asset).convert("RGBA")))
                    def colored_mean(values):
                        colors = [p[:3] for p in values if p[3] >= 128 and max(p[:3]) - min(p[:3]) > 35]
                        assert len(colors) >= 50
                        return [sum(p[i] for p in colors) / len(colors) for i in range(3)]
                    assert max(abs(a-b) for a,b in zip(colored_mean(actual), colored_mean(expected))) <= 18, ("Native window and launcher must match the chosen palette", value)
                expect_icons("teal")
                source = "# Palette sample\n\n[[Home]] and **clear text**.\n\n- [x] Keep contrast\n\n| Palette | Status |\n| --- | --- |\n| Native | Ready for review |\n"
                note = fixture / "Notes/Palette sample.md"
                note.write_text(source)
                command("Refresh vault"); open_note("Palette sample")
                def background_color():
                    header = control_bounds("button:More options")
                    return ImageGrab.grab(xdisplay=display).getpixel((round((header[0] + header[2]) / 2), round(header[3] + 12)))[:3]
                def expect_background(expected):
                    deadline = time.monotonic() + 3
                    while True:
                        actual = background_color()
                        if max(abs(a-b) for a,b in zip(actual,expected)) <= 2:
                            return
                        assert app.poll() is None, "App exited before presenting its restored palette"
                        if time.monotonic() >= deadline:
                            raise AssertionError(("Native background did not present the selected palette", actual, expected))
                        time.sleep(.05)
                palettes = [("Original teal", "teal"), ("Love Space", "love-space"), ("Elpis Gold", "elpis-gold"), ("Signature", "signature")]
                # Start away from the default so the first Teal click is a real change.
                command("Open settings"); click_control("settings-page:Appearance"); palette_ready()
                click_control("settings-palette:Love Space"); key("Escape")
                for dark in (True, False):
                    set_theme(dark)
                    for label, value in palettes:
                        command("Open settings"); click_control("settings-page:Appearance"); palette_ready()
                        click_control("settings-palette:" + label)
                        palette_ready()
                        expect_icons(value)
                        cards = [control_bounds("settings-palette:" + name) for name, _ in palettes]
                        assert max(card[1] for card in cards)-min(card[1] for card in cards) <= 1, cards
                        assert all(left[2] < right[0] for left, right in zip(cards, cards[1:])), cards
                        capture("native-palettes-" + ("dark" if dark else "light") + "-" + value + ".png")
                        key("Escape")
                        state = None
                        for _ in range(40):
                            states = [json.loads(path.read_text()) for path in (base / "state/rustidian").glob("*.json")]
                            state = next((state for state in states if any(path == "Notes/Palette sample.md" for path, _ in state.get("tabs", []))), None)
                            if state and state.get("palette") == value and state.get("dark") == dark:
                                break
                            assert app.poll() is None, "App exited before persisting its palette"
                            time.sleep(.1)
                        assert state and state["palette"] == value and state["dark"] == dark, state
                        expected = {("teal", True):(23,27,29), ("teal", False):(247,246,242), ("love-space", True):(29,23,27), ("elpis-gold", True):(28,27,23), ("signature", True):(31,31,40), ("love-space", False):(251,245,245), ("elpis-gold", False):(251,248,239), ("signature", False):(252,245,242)}[(value, dark)]
                        expect_background(expected)
                        capture("native-palette-document-" + ("dark" if dark else "light") + "-" + value + ".png")
                        assert note.read_text() == source
                    results.append(("Dark" if dark else "Light") + " palettes apply the requested native colors, save the actual selection and preserve Markdown")
                xdo("key", "--clearmodifiers", "ctrl+q"); app.wait(timeout=10)
                app = launch_app(launch_command); window = ready_window()
                expect_background((252,245,242))
                command("Open settings"); click_control("settings-page:Appearance")
                capture("native-palettes-restored.png")
                states = [json.loads(path.read_text()) for path in (base / "state/rustidian").glob("*.json")]
                assert any(state.get("palette") == "signature" and state["dark"] is False for state in states)
                expect_icons("signature")
                click_control("settings-palette:Original teal"); key("Escape"); set_theme(True)
                expect_icons("teal")
                assert note.read_text() == source
                results.append("All four palettes update the real native window and XDG launcher icons; a real restart restores the selection without changing launcher actions or note bytes")

            def table_resize_checks():
                nonlocal app, window
                edge_source = "# Table edge\n\n| Day | DW (h) | AW (h) |\n| --- | --- | --- |\n| Saturday | 0 | 9.5 |\n| Sunday | 0 | 0 |\n"
                edge_note = fixture / "Notes/Table edge.md"
                edge_note.write_text(edge_source)
                time.sleep(.6)
                open_note("Table edge")
                command("Live preview")

                def columns(required=True):
                    return [control_bounds(f"edit-table-cell:0:0:{column}", required=required) for column in range(3)]

                def same_widths(actual, expected):
                    return all(a is not None and b is not None and abs(a[index] - b[index]) < 2 for a, b in zip(actual, expected) for index in (0, 2))

                def drag_divider(column, target, cancel=False):
                    rect = control_bounds(f"table-resize:0:{column}")
                    x, y = round((rect[0] + rect[2]) / 2), round((rect[1] + rect[3]) / 2)
                    xdo("mousemove", "--sync", str(x), str(y))
                    pause()
                    xdo("mousedown", "1")
                    time.sleep(.1)
                    try:
                        for step in range(1, 11):
                            xdo("mousemove", "--sync", str(round(x + (target - x) * step / 10)), str(y))
                            time.sleep(.025)
                        if cancel:
                            key("Escape")
                    finally:
                        xdo("mouseup", "1")
                    pause()

                before_edge = columns()
                capture("native-table-edge-before.png")
                drag_divider(1, 1435)
                after_edge = columns(required=False)
                capture("native-table-edge-far-right.png")
                content_right = before_edge[0][0] + control_bounds("document-content-width")[0]
                bounded = all(rect is not None and rect[2] > rect[0] and rect[0] >= before_edge[0][0] - 2 and rect[2] <= content_right + 2 for rect in after_edge)
                assert edge_note.read_bytes() == edge_source.encode(), "Extreme resize changed note bytes"
                key("ctrl+z")
                undone_edge = columns(required=False)
                capture("native-table-edge-undo.png")
                undo_restored = same_widths(undone_edge, before_edge)
                assert edge_note.read_bytes() == edge_source.encode(), "Layout Undo changed note bytes"
                (reports / "native-table-edge-evidence.json").write_text(json.dumps({
                    "before": before_edge, "far_right": after_edge, "undo": undone_edge,
                    "content_right": content_right, "bounded": bounded,
                    "undo_restored": undo_restored, "note_bytes_unchanged": True,
                }, indent=2) + "\n")
                assert bounded and undo_restored, f"Extreme divider regression: bounded={bounded}, undo_restored={undo_restored}; before={before_edge}, after={after_edge}, undo={undone_edge}"
                key("ctrl+shift+z")
                assert same_widths(columns(), after_edge), "Layout Redo did not restore the committed widths"
                assert edge_note.read_bytes() == edge_source.encode()
                results.append("Three-column far-right divider stays within the document; one Undo and Redo restore exact widths without changing note bytes")

                drag_divider(1, 1435)
                assert same_widths(columns(), after_edge), "Dragging beyond the same bound changed widths"
                key("ctrl+z")
                assert same_widths(columns(), before_edge), "A clamped no-op drag added an Undo entry"
                key("ctrl+y")
                assert same_widths(columns(), after_edge)

                divider = control_bounds("table-resize:0:1")
                drag_divider(1, (divider[0] + divider[2]) / 2 - 90, cancel=True)
                assert same_widths(columns(), after_edge), "Escape did not cancel the entire resize gesture"
                key("ctrl+z")
                assert same_widths(columns(), before_edge), "Cancelled drag added an Undo entry"
                key("ctrl+y")
                assert same_widths(columns(), after_edge)
                click_control("table-resize:0:1")
                key("Left")
                keyboard_edge = columns()
                assert not same_widths(keyboard_edge, after_edge), "Keyboard resize did not change column widths"
                key("ctrl+z")
                assert same_widths(columns(), after_edge), "Keyboard resize Undo did not restore widths"
                key("ctrl+y")
                assert same_widths(columns(), keyboard_edge), "Keyboard resize Redo did not restore widths"
                assert edge_note.read_bytes() == edge_source.encode()
                capture("native-table-edge-keyboard-redo.png")
                results.append("Escape cancels a whole drag without adding history; keyboard resize shares layout Undo and Redo")

                drag_divider(2, 1435)
                outer_edge = columns()
                assert outer_edge[2][2] <= content_right + 2 and outer_edge[2][2] > keyboard_edge[2][2] + 2, "Outer edge must use only the remaining document width"
                drag_divider(2, 0)
                minimum_edge = columns()
                assert abs(minimum_edge[2][2] - minimum_edge[2][0] - 56) < 2, "Outer column must stop at its readable minimum"
                key("ctrl+z"); assert same_widths(columns(), outer_edge)
                key("ctrl+z"); assert same_widths(columns(), keyboard_edge)
                assert edge_note.read_bytes() == edge_source.encode()
                results.append("The outer divider is bounded at both viewport and minimum limits, with one Undo per gesture and unchanged bytes")

                xdo("windowsize", window, "900", "700"); pause()
                narrow = columns()
                narrow_right = narrow[0][0] + control_bounds("document-content-width")[0]
                assert all(rect[2] <= narrow_right + 2 for rect in narrow), "Narrow windows must keep all three columns visible"
                capture("native-table-edge-narrow.png")
                xdo("windowsize", window, "1440", "960"); pause()
                assert same_widths(columns(), keyboard_edge), "Viewport fitting must not overwrite the saved widths"
                assert edge_note.read_bytes() == edge_source.encode()
                results.append("A narrower real window fits all columns; widening restores chosen widths without editing the saved preference or note")

                xdo("key", "--clearmodifiers", "ctrl+q")
                app.wait(timeout=10)
                app = launch_app(launch_command)
                window = ready_window()
                open_note("Table edge")
                command("Live preview")
                assert same_widths(columns(), keyboard_edge), "Committed widths did not survive a real restart"
                key("ctrl+z")
                assert same_widths(columns(), keyboard_edge), "Restart resurrected a previous session's layout Undo"
                assert edge_note.read_bytes() == edge_source.encode()
                capture("native-table-edge-restarted.png")
                results.append("A real restart preserves committed table widths and note bytes, without reviving session-only Undo")

                # Text and geometry must share chronological Undo boundaries.
                click_control("edit-table-cell:0:1:0")
                key("ctrl+a")
                paste_text("Friday")
                key("ctrl+s")
                text_a = edge_source.replace("Saturday", "Friday")
                assert edge_note.read_text() == text_a
                before_ordered_resize = columns()
                divider = control_bounds("table-resize:0:1")
                drag_divider(1, (divider[0] + divider[2]) / 2 - 40)
                ordered_resize = columns()
                assert not same_widths(ordered_resize, before_ordered_resize)
                click_control("edit-table-cell:0:1:2")
                key("ctrl+a")
                paste_text("22")
                key("ctrl+s")
                text_b = text_a.replace("9.5", "22")
                assert edge_note.read_text() == text_b
                key("ctrl+z")
                key("ctrl+s")
                assert edge_note.read_text() == text_a, "First Undo must restore the later text edit"
                assert same_widths(columns(), ordered_resize), "Text Undo also changed table widths"
                key("ctrl+z")
                assert same_widths(columns(), before_ordered_resize), "Second Undo must restore the intervening resize"
                assert edge_note.read_text() == text_a, "Layout Undo rewrote note text"
                key("ctrl+z")
                key("ctrl+s")
                assert edge_note.read_text() == edge_source, "Third Undo must restore the earlier text edit"
                key("ctrl+y")
                key("ctrl+s")
                assert edge_note.read_text() == text_a
                key("ctrl+y")
                assert same_widths(columns(), ordered_resize), "Redo skipped the layout command"
                assert edge_note.read_text() == text_a
                key("ctrl+y")
                key("ctrl+s")
                assert edge_note.read_text() == text_b
                capture("native-table-edge-ordered-redo.png")
                results.append("Text edit, resize, and later text edit Undo and Redo in chronological order, with layout steps preserving exact note bytes")

                # Equal final text must not hide newer text edits behind geometry.
                for value in ("33", "22"):
                    click_control("edit-table-cell:0:1:2"); key("ctrl+a"); paste_text(value); key("ctrl+s")
                key("ctrl+z"); key("ctrl+s")
                assert edge_note.read_text() == text_b.replace("22", "33"), "A→B→A must undo the newest text edit before layout"
                assert same_widths(columns(), ordered_resize)
                key("ctrl+z"); key("ctrl+s")
                assert edge_note.read_text() == text_b
                for _ in range(2): key("ctrl+y"); key("ctrl+s")
                assert edge_note.read_text() == text_b
                click_control("edit-table-cell:0:1:2"); key("End")
                queued_steps(("text", "X"), ("key", "ctrl+z"), ("key", "ctrl+s"))
                time.sleep(.5)
                assert edge_note.read_text() == text_b, "Queued typing then Undo must preserve event order"
                assert same_widths(columns(), ordered_resize), "Queued typing Undo must not consume a layout command"
                results.append("A→B→A text history and queued typing/Undo preserve chronological text/layout ordering")

                before_reading_resize = columns()
                command("Reading view")
                divider = control_bounds("table-resize:0:1")
                drag_divider(1, (divider[0] + divider[2]) / 2 - 30)
                capture("native-table-edge-reading-resize.png")
                key("ctrl+z")
                command("Live preview")
                assert same_widths(columns(), before_reading_resize), "Reading-view resize did not Undo through the shared layout history"
                assert edge_note.read_text() == text_b
                key("ctrl+y")
                assert not same_widths(columns(), before_reading_resize), "Reading-view resize Redo was lost when changing view"
                assert edge_note.read_text() == text_b
                results.append("Reading-view resizing shares Undo/Redo across view switches and leaves Markdown bytes unchanged")

                source = "# Resize tables\n\n| Item | Value |\n| --- | --- |\n| Short | A longer value that must remain readable |\n\n> | Nested | Value |\n> | --- | --- |\n> | Quote | Safe |\n"
                note = fixture / "Notes/Resize tables.md"
                note.write_text(source)
                time.sleep(.6)
                open_note("Resize tables")
                for mode in ("Live preview", "Reading view"):
                    command(mode)
                    capture("native-table-resize-" + mode.split()[0].lower() + "-before.png")
                    for table in (0, 1):
                        name = f"table-resize:{table}:0"
                        original = control_bounds(name)
                        x, y = round((original[0] + original[2]) / 2), round((original[1] + original[3]) / 2)
                        for distance in (100, -60):
                            current = control_bounds(name)
                            x = round((current[0] + current[2]) / 2)
                            xdo("mousemove", "--sync", str(x), str(y))
                            pause()
                            xdo("mousedown", "1")
                            time.sleep(.1)
                            try:
                                for step in range(1, 11):
                                    xdo("mousemove", "--sync", str(round(x + distance * step / 10)), str(y))
                                    time.sleep(.025)
                            finally:
                                xdo("mouseup", "1")
                            pause()
                            updated = control_bounds(name)
                            assert abs((updated[0] + updated[2] - current[0] - current[2]) / 2 - distance) < 12, (mode, table, current, updated, distance)
                            assert note.read_text() == source, "Resizing a table must not rewrite Markdown"
                    capture("native-table-resize-" + mode.split()[0].lower() + "-after.png")
                    results.append(mode + " resizes ordinary and callout table columns in both directions without changing note bytes")
                before = control_bounds("table-resize:0:0")
                xdo("key", "--clearmodifiers", "ctrl+q")
                app.wait(timeout=10)
                app = launch_app(launch_command)
                window = ready_window()
                open_note("Resize tables")
                command("Reading view")
                after = control_bounds("table-resize:0:0")
                assert abs(before[0] - after[0]) < 2, (before, after)
                assert note.read_text() == source
                results.append("Table widths survive a real restart in workspace state while the Markdown remains byte-for-byte unchanged")

            def table_edge_checks():
                observations = []

                def reveal(table, direction, label):
                    # Discover from the visible table edge, not a hidden control's
                    # coordinates: a probe without actual '+' pixels is insufficient.
                    xdo("mousemove", "100", "20"); pause()
                    name = f"edit-table-add-{direction}:{table}"
                    assert control_bounds(name, required=False) is None, ("Edge buttons must hide away from their table", name)
                    left, top, right, bottom = control_bounds(f"edit-table:{table}")
                    idle = ImageGrab.grab(xdisplay=display).convert("RGB")
                    x, y = ((left + right) / 2, bottom + 6) if direction == "row" else (right + 6, (top + bottom) / 2)
                    xdo("mousemove", "--sync", str(round(x)), str(round(y))); pause()
                    bounds = control_bounds(name)
                    a, b, c, d = bounds
                    if direction == "row":
                        assert abs(a - left) <= 2 and abs(c - right) <= 2 and 8 <= d - b <= 20, ("The row control must be a compact full-width bottom strip", bounds, (left, top, right, bottom))
                        assert b >= bottom - 1, ("The row strip must not cover an existing cell", bounds, bottom)
                    else:
                        assert abs(b - top) <= 2 and abs(d - bottom) <= 2 and 8 <= c - a <= 20, ("The column control must be a compact full-height right strip", bounds, (left, top, right, bottom))
                        assert a >= right - 1, ("The column strip must not cover an existing cell", bounds, right)
                    capture(f"native-table-edge-{label}-{direction}-hover.png")
                    picture = ImageGrab.grab(xdisplay=display).convert("RGB")
                    region = tuple(map(round, bounds))
                    changed = sum(max(pixel) >= 12 for pixel in pixel_data(ImageChops.difference(idle.crop(region), picture.crop(region))))
                    cx, cy = round((a + c) / 2), round((b + d) / 2)
                    background = picture.getpixel((cx + 5, cy + 5))
                    plus_pixels = sum(pixel_contrast(picture.getpixel((px, py)), background) >= 1.5
                                      for px in range(cx - 4, cx + 5) for py in range(cy - 4, cy + 5))
                    observations.append({"label": label, "direction": direction, "bounds": bounds, "changed_pixels": changed, "visible_plus_pixels": plus_pixels})
                    (reports / "native-table-edge-controls.json").write_text(json.dumps(observations, indent=2) + "\n")
                    assert changed >= 30 and plus_pixels >= 7, ("Hover must paint a visible '+' inside the edge strip", observations[-1])
                    return name

                def expect_bytes(note, expected):
                    key("ctrl+s")
                    assert note.read_bytes() == expected.encode(), repr(note.read_bytes())

                def normal(dark):
                    label = "dark" if dark else "light"
                    source = "# Table edge controls\n\nBefore π\n\n| Item | Count | State |\n| :--- | ---: | --- |\n| Alpha \\| Beta | 2 | keep |\n| Gamma | 1 | stay |\n\nAfter table سلام\n"
                    note = fixture / f"Notes/Table edges {label}.md"
                    note.write_text(source)
                    time.sleep(.6)
                    open_note(f"Table edges {label}"); command("Live preview"); set_theme(dark)
                    click_control(reveal(0, "row", label))
                    row_blank = source.replace("| Gamma | 1 | stay |\n", "| Gamma | 1 | stay |\n|  |  |  |\n")
                    expect_bytes(note, row_blank)
                    type_text("edge row π")
                    row_filled = row_blank.replace("|  |  |  |\n", "| edge row π |  |  |\n")
                    expect_bytes(note, row_filled)
                    key("ctrl+z"); expect_bytes(note, row_blank)
                    key("ctrl+z"); expect_bytes(note, source)
                    key("ctrl+y"); expect_bytes(note, row_blank)
                    key("ctrl+y"); expect_bytes(note, row_filled)
                    click_control(reveal(0, "column", label))
                    column_blank = row_filled.replace(" |\n", " |  |\n").replace("| :--- | ---: | --- |  |", "| :--- | ---: | --- | --- |")
                    expect_bytes(note, column_blank)
                    type_text("Extra")
                    column_filled = column_blank.replace("| Item | Count | State |  |", "| Item | Count | State | Extra |")
                    expect_bytes(note, column_filled)
                    key("ctrl+z"); expect_bytes(note, column_blank)
                    key("ctrl+z"); expect_bytes(note, row_filled)
                    key("ctrl+y"); expect_bytes(note, column_blank)
                    key("ctrl+y"); expect_bytes(note, column_filled)
                    key("Escape"); key("ctrl+End"); type_text("PROSE"); expect_bytes(note, column_filled + "PROSE")
                    results.append(f"{label.title()} table hover reveals compact bottom/right '+' strips; each click appends exactly one row/column, accepts immediate typing, saves exact bytes and supports Undo/Redo and Escape to prose")

                def normal_dark():
                    normal(True)

                def normal_light():
                    normal(False)

                def queued():
                    queued_source = "# Queued table edges\n\n| A | B |\n| --- | --- |\n| old | 1 |\n\nAfter π\n"
                    queued_note = fixture / "Notes/Queued table edges.md"
                    queued_note.write_text(queued_source)
                    time.sleep(.6)
                    open_note("Queued table edges"); command("Live preview"); set_theme(True)
                    queued_row = queued_source.replace("| old | 1 |\n", "| old row | 1 |\n| queued row |  |\n")
                    queued_column = queued_row.replace("| A | B |", "| A | B | queued column |").replace("| --- | --- |", "| --- | --- | --- |").replace("| old row | 1 |", "| old row column | 1 |  |").replace("| queued row |  |", "| queued row |  |  |")
                    for direction, expected in (("row", queued_row), ("column", queued_column)):
                        click_control("edit-table-cell:0:1:0"); key("End")
                        left, top, right, bottom = control_bounds(reveal(0, direction, "queued"))
                        point = (round((left + right) / 2), round((top + bottom) / 2))
                        queued_steps(("text", f" {direction}"), ("click", point), ("text", f"queued {direction}"), ("key", "ctrl+s"))
                        time.sleep(.5)
                        assert queued_note.read_bytes() == expected.encode(), (direction, repr(queued_note.read_bytes()))
                    results.append("Same-frame pending text, edge click, destination typing and Save preserve the old cell and reach the new row cell or column header")

                def keyboard():
                    source = "# Keyboard table edges\n\n| A | B |\n| --- | --- |\n| old | 1 |\n\nAfter\n"
                    note = fixture / "Notes/Keyboard table edges.md"
                    note.write_text(source)
                    time.sleep(.6)
                    open_note("Keyboard table edges"); command("Live preview"); set_theme(True)
                    for shortcut, direction in (("space", "row"), ("Return", "column")):
                        click_control("table-resize:0:1")
                        xdo("mousemove", "100", "20")
                        key("Tab")
                        if direction == "column": key("Tab")
                        assert control_bounds(f"edit-table-add-{direction}:0")
                        key(shortcut)
                        expected = source.replace("| old | 1 |\n", "| old | 1 |\n|  |  |\n") if direction == "row" else source.replace(" |\n", " |  |\n").replace("| --- | --- |  |", "| --- | --- | --- |")
                        expect_bytes(note, expected)
                        key("ctrl+z"); expect_bytes(note, source)
                    results.append("Keyboard-focused edge strips append with Space/Enter without inserting the activation character; one Undo restores the table")

                def nested():
                    nested_source = "# Multiple table edges\r\n\r\n| First | Safe |\r\n| --- | --- |\r\n| original | value |\r\n\r\n> | Quoted | N |\r\n> | --- | --- |\r\n> | second | 2 |\r\n\r\n```md\r\n| Literal | Table |\r\n| --- | --- |\r\n| unchanged | fenced |\r\n```\r\n"
                    nested = fixture / "Notes/Multiple table edges.md"
                    nested.write_bytes(nested_source.encode())
                    time.sleep(.6)
                    open_note("Multiple table edges"); command("Live preview"); set_theme(True)
                    assert control_bounds("edit-table:2", required=False) is None
                    click_control(reveal(1, "row", "nested"))
                    nested_row = nested_source.replace("> | second | 2 |\r\n", "> | second | 2 |\r\n> |  |  |\r\n")
                    expect_bytes(nested, nested_row)
                    click_control(reveal(1, "column", "nested"))
                    nested_column = nested_row.replace("> | Quoted | N |\r\n> | --- | --- |\r\n> | second | 2 |\r\n> |  |  |\r\n", "> | Quoted | N |  |\r\n> | --- | --- | --- |\r\n> | second | 2 |  |\r\n> |  |  |  |\r\n")
                    expect_bytes(nested, nested_column)
                    key("ctrl+z"); expect_bytes(nested, nested_row)
                    key("ctrl+z"); expect_bytes(nested, nested_source)
                    key("ctrl+y"); expect_bytes(nested, nested_row)
                    key("ctrl+y"); expect_bytes(nested, nested_column)
                    for mode in ("Reading view", "Source editing"):
                        command(mode)
                        assert control_bounds("edit-table-add-row:1", required=False) is None
                        assert control_bounds("edit-table-add-column:1", required=False) is None
                        expect_bytes(nested, nested_column)
                    results.append("Edge insertion targets the second quoted table, preserves CRLF, the first table and fenced literals, and stays absent in Reading/Source modes")

                failures = []
                failure_report = reports / "native-table-edge-failures.json"
                failure_report.write_text("[]\n")

                def retain_failure(name, error):
                    capture(f"native-table-edge-{name}-failure.png")
                    failures.append({"case": name, "error": repr(error), "traceback": traceback.format_exc()})
                    failure_report.write_text(json.dumps(failures, indent=2) + "\n")
                    print(traceback.format_exc(), flush=True)
                    xdo("key", "--clearmodifiers", "Escape", "Escape"); pause()

                try:
                    run_checks([normal_dark, normal_light, queued, keyboard, nested], retain_failure, count_checks=lambda: len(results))
                finally:
                    set_theme(True)

            def table_input_checks():
                source = "# Table input\n\n| A | B |\n| --- | --- |\n| old | 1 |\n| other | 2 |\n\nAfter\n"
                note = fixture / "Notes/Table input.md"
                note.write_text(source)
                time.sleep(.6)
                open_note("Table input")
                command("Live preview")
                click_control("edit-table-cell:0:1:0")
                key("ctrl+a")
                queued_steps(("text", "First | queued"), ("key", "Tab"), ("text", "8"), ("key", "Return"), ("text", "9"), ("key", "ctrl+s"))
                time.sleep(.5)
                expected = source.replace("old | 1", "First \\| queued | 8").replace("other | 2", "other | 9")
                assert note.read_text() == expected, repr(note.read_text())
                key("Escape")
                key("ctrl+End")
                type_text("PROSE")
                key("ctrl+s")
                assert note.read_text() == expected + "PROSE", repr(note.read_text())
                results.append("One-frame text/Tab/text/Enter/text/save reached three intended cells and Escape restored immediate prose input")

            def table_link_checks():
                source = "# Table links\n\n| Destination | Literal |\n| --- | --- |\n| [[Home]] | `[[Never open]]` |\n"
                note = fixture / "Notes/Table links.md"
                note.write_text(source)
                time.sleep(.6)
                open_note("Table links")
                command("Live preview")
                click_control("edit-table-cell:0:1:1")
                key("ctrl+Return")
                title_contains("Table links")
                assert not (fixture / "Never open.md").exists()
                click_control("edit-table-cell:0:1:0")
                key("ctrl+a")
                queued_text("[[Home]]", "ctrl+Return")
                title_contains("Home")
                assert note.read_text() == source
                open_note("Table links")
                command("Live preview")
                left, top, right, bottom = control_bounds("edit-table-cell:0:1:0")
                xdo("mousemove", str(round((left + right) / 2)), str(round((top + bottom) / 2)), "keydown", "ctrl", "click", "1", "keyup", "ctrl")
                pause()
                title_contains("Home")
                assert note.read_text() == source
                results.append("Ctrl+Enter and Ctrl+click followed table wiki links, including queued input; inline-code links remained literal")

            def table_edit_checks():
                source = "# Visual table GUI\n\n| Item | Value |\n| :--- | ---: |\n| **Alpha** | 2 |\n| Beta | 1 |\n\nAfter table π\n"
                note = fixture / "Notes/Visual table GUI.md"
                note.write_text(source)
                time.sleep(.6)
                open_note("Visual table GUI")
                command("Source editing")
                assert control_bounds("edit-table-cell:0:1:0", required=False) is None
                command("Live preview")
                click_control("edit-table-cell:0:1:0")
                key("ctrl+a")
                type_text("Edited π | value", delay=100)
                key("Tab")
                type_text("5")
                key("ctrl+s")
                expected = source.replace("**Alpha**", "Edited π \\| value").replace("| 2 |", "| 5 |")
                assert note.read_text() == expected, repr(note.read_text())
                capture("native-edit-table-dark.png")
                results.append("Live table cells edited Unicode and escaped pipes, navigated with Tab and preserved surrounding Markdown")
                key("ctrl+z")
                key("ctrl+s")
                assert note.read_text() == expected.replace("| 5 |", "| 2 |"), repr(note.read_text())
                key("ctrl+y")
                key("ctrl+s")
                assert note.read_text() == expected
                key("Return")
                type_text("7")
                key("shift+Tab")
                queued_text("Queued | cell", "Tab", "ctrl+s")
                expected = expected.replace("| Beta | 1 |", "| Queued \\| cell | 7 |")
                assert note.read_text() == expected, repr(note.read_text())
                results.append("Enter and Shift+Tab navigated cells; shared document undo/redo and same-frame typing/Tab/save kept exact text")

                def table_action(row, column, label):
                    click_control(f"edit-table-cell:0:{row}:{column}", button="3")
                    click_control(f"table-action:{label}")
                    key("ctrl+s")

                table_action(1, 0, "Insert row below")
                inserted = expected.replace("| Queued \\| cell | 7 |", "|  |  |\n| Queued \\| cell | 7 |")
                assert note.read_text() == inserted, repr(note.read_text())
                key("ctrl+z")
                key("ctrl+s")
                assert note.read_text() == expected, repr(note.read_text())
                key("ctrl+y")
                key("ctrl+s")
                assert note.read_text() == inserted
                table_action(2, 0, "Delete row")
                assert note.read_text() == expected
                table_action(1, 0, "Move row down")
                moved = expected.replace("| Edited π \\| value | 5 |\n| Queued \\| cell | 7 |", "| Queued \\| cell | 7 |\n| Edited π \\| value | 5 |")
                assert note.read_text() == moved, repr(note.read_text())
                table_action(2, 0, "Move row up")
                assert note.read_text() == expected
                table_action(1, 1, "Sort descending")
                assert note.read_text() == moved
                table_action(1, 1, "Sort ascending")
                assert note.read_text() == expected
                results.append("Native table menu inserted/deleted/reordered rows and sorted numbers; structure undo/redo restored the whole document")
                table_action(1, 0, "Insert column right")
                # Escaped pipes belong to cells, not column delimiters.
                expanded = expected.replace("| Item | Value |", "| Item |  | Value |").replace("| :--- | ---: |", "| :--- | --- | ---: |").replace(" value | 5 |", " value |  | 5 |").replace(" cell | 7 |", " cell |  | 7 |")
                assert note.read_text() == expanded, repr(note.read_text())
                table_action(1, 1, "Move column right")
                swapped = expanded.replace("| Item |  | Value |", "| Item | Value |  |").replace("| :--- | --- | ---: |", "| :--- | ---: | --- |").replace(" value |  | 5 |", " value | 5 |  |").replace(" cell |  | 7 |", " cell | 7 |  |")
                assert note.read_text() == swapped, repr(note.read_text())
                table_action(1, 2, "Move column left")
                assert note.read_text() == expanded
                table_action(1, 1, "Delete column")
                assert note.read_text() == expected
                table_action(1, 0, "Align center")
                centered = expected.replace("| :--- |", "| :---: |")
                assert note.read_text() == centered
                key("ctrl+z")
                key("ctrl+s")
                assert note.read_text() == expected
                results.append("Native column insertion/moves/deletion retained escaped pipes and alignment; alignment undo kept all note bytes")
                command("Switch appearance")
                capture("native-edit-table-light.png")
                command("Switch appearance")
                click_control("edit-fold:0")
                assert control_bounds("edit-table:0", required=False) is None
                click_control("edit-fold:0")
                assert control_bounds("edit-table:0")
                command("Reading view")
                assert control_bounds("edit-table-cell:0:1:0", required=False) is None
                type_text("READING_TABLE_MUST_NOT_EDIT")
                key("ctrl+s")
                assert note.read_text() == expected
                open_note("Home")
                open_note("Visual table GUI")
                command("Live preview")
                assert control_bounds("edit-table:0")
                assert note.read_text() == expected
                results.append("Edited tables survived note reopening, heading folding and both themes; Reading stayed read-only")
                nested_source = "# Nested editing\r\n\r\n> | Q | N |\r\n> | --- | --- |\r\n> | quoted | 1 |\r\n\r\n- | L | N |\r\n  | --- | --- |\r\n  | listed | 2 |\r\n\r\n```md\r\n| Fence | Cell |\r\n| --- | --- |\r\n| Literal | 0 |\r\n```\r\n"
                nested = fixture / "Notes/Nested editing.md"
                nested.write_bytes(nested_source.encode())
                time.sleep(.6)
                open_note("Nested editing")
                command("Live preview")
                assert control_bounds("edit-table:2", required=False) is None
                click_control("edit-table-cell:0:1:0")
                key("ctrl+a")
                type_text("quote π | cell", delay=100)
                key("ctrl+s")
                nested_source = nested_source.replace("quoted", "quote π \\| cell")
                click_control("edit-table-cell:1:1:0")
                key("ctrl+a")
                type_text("list edit")
                key("ctrl+s")
                nested_source = nested_source.replace("listed", "list edit")
                assert nested.read_bytes() == nested_source.encode(), repr(nested.read_bytes())
                table_action(1, 0, "Insert row below")
                nested_source = nested_source.replace("> | quote π \\| cell | 1 |\r\n", "> | quote π \\| cell | 1 |\r\n> |  |  |\r\n")
                assert nested.read_bytes() == nested_source.encode()
                click_control("edit-table-cell:1:1:1")
                key("ctrl+a")
                type_text("3")
                key("Return")
                type_text("4")
                key("ctrl+s")
                nested_source = nested_source.replace("  | list edit | 2 |\r\n", "  | list edit | 3 |\r\n  |  | 4 |\r\n")
                assert nested.read_bytes() == nested_source.encode(), repr(nested.read_bytes())
                key("Escape")
                key("ctrl+End")
                type_text("\nNormal prose")
                key("ctrl+s")
                assert nested.read_bytes() == (nested_source + "\nNormal prose").encode(), repr(nested.read_bytes())
                capture("native-edit-table-nested.png")
                results.append("Quoted/list tables preserved CRLF and fenced literal tables; Enter added a row and Escape returned to normal prose editing")
                wide_source = "| " + " | ".join(f"Column {index:02}" for index in range(20)) + " |\n"
                wide_source += "| " + " | ".join("---" for _ in range(20)) + " |\n"
                wide_source += "| " + " | ".join(f"Value {index:02}" for index in range(20)) + " |\n"
                wide_source += "\nAfter wide table\n"
                wide = fixture / "Notes/Wide edit table.md"
                wide.write_text(wide_source)
                time.sleep(.6)
                open_note("Wide edit table")
                command("Live preview")
                assert control_bounds("edit-table-cell:0:1:19", required=False) is None
                table = control_bounds("edit-table:0")
                for _ in range(10):
                    if control_bounds("edit-table-cell:0:1:19", required=False): break
                    xdo("mousemove", "800", str(round((table[1] + table[3]) / 2)), "click", "--repeat", "20", "--delay", "1", "7")
                    pause()
                click_control("edit-table-cell:0:1:19")
                last_cell = control_bounds("edit-table-cell:0:1:19")
                assert last_cell[3] - last_cell[1] > 31, last_cell
                key("ctrl+a")
                type_text("WIDE_EDIT")
                key("ctrl+s")
                assert wide.read_text() == wide_source.replace("Value 19", "WIDE_EDIT"), repr(wide.read_text())
                capture("native-edit-table-wide.png")
                results.append("A 20-column Live table scrolled to and edited its final cell while retaining every other column")

            def inline_styles_checks():
                control = fixture / "Inline positive control.md"
                control.write_text("# Plain table\n\n| A | B |\n|---|---|\n| plain | ==yellow== |\n")
                time.sleep(.6);open_note("Inline positive control");command("Reading view")
                control_bounds("table-cell:0:1:1")
                capture("native-inline-positive-control.png")
                results.append("An ordinary native Markdown table rendered before testing the reported compound-style defect")

                source = (ROOT / "tests/fixtures/inline-styles.md").read_text()
                # Retain the screenshot's five-column shape and long alignment
                # row, without putting private note contents in the repository.
                start=source.index("| day |")
                end=source.index("\n\n",start)
                rows=source[start:end].splitlines()
                assert len(rows)==4
                rows[0]+=" DW subject | section |"
                rows[1]="| "+" | ".join(":"+"-"*160+":" for _ in range(5))+" |"
                rows[2:]=[row+" Rust | Notes |" for row in rows[2:]]
                source=source[:start]+"\n".join(rows)+source[end:]
                note = fixture / "Shared inline styles.md";note.write_text(source)
                time.sleep(.6);open_note("Shared inline styles");command("Reading view")
                # These controls exist only when the actual native table renderer is used.
                # The old executable falls back for this document and fails here.
                table = control_bounds("table:0")
                assert table[2]-table[0] < 650, table
                def region(name):
                    bounds = control_bounds(name)
                    return ImageGrab.grab(xdisplay=display).crop(tuple(round(v) for v in bounds)).convert("RGB")
                def green(name):
                    count = sum(g>r+15 and g>b+15 for r,g,b in pixel_data(region(name)))
                    assert count > 45, (name,count)
                def yellow(name):
                    count = sum(r>g>b+20 and r-g<70 for r,g,b in pixel_data(region(name)))
                    assert count > 45, (name,count)
                green("table-cell:0:1:2");yellow("table-cell:0:1:1")
                results.append("The reported rgba mark and ==11h== rendered as colored backgrounds inside the actual native Reading table, with compact text instead of raw attributes")
                for dark,label in [(True,"dark"),(False,"light")]:
                    set_theme(dark);green("table-cell:0:1:2");yellow("table-cell:0:1:1")
                    control_bounds("callout:0");green("table-cell:1:1:1")
                    ink=sum(abs(r-43)<4 and abs(g-122)<4 and abs(b-75)<4 for r,g,b in pixel_data(region("table-cell:0:0:1")))
                    assert ink>4,(label,ink)
                    capture("native-inline-reading-"+label+".png")
                assert note.read_text()==source
                results.append("Both themes painted the explicit green foreground, bold header and shared callout-table highlight without changing source bytes")
                command("Live preview")
                for dark,label in [(True,"dark"),(False,"light")]:
                    set_theme(dark)
                    green("edit-table-cell:0:1:2");yellow("edit-table-cell:0:1:1")
                    capture("native-inline-live-"+label+".png")
                assert note.read_text()==source
                results.append("Actual Live table cells used the same rgba and yellow styles in dark and light themes and retained the original markup")
                command("Source editing");editor_end();type_text("EDIT_UNDO_CONTROL");key("ctrl+z");key("ctrl+s")
                assert note.read_text()==source
                capture("native-inline-source.png")
                results.append("Source mode retained editable raw markup and ordinary typing, Undo and Save restored the exact original document")
                target=fixture/"Inline preview target.md"
                target_text="| total | actual |\n|---|---|\n| ==11h== | <mark style='background-color:rgba(46,204,113,.4);color:inherit'>56h</mark> |\n"
                target.write_text(target_text)
                link=fixture/"Inline preview source.md";link_text="[[Inline preview target|Styles preview]]\n";link.write_text(link_text)
                time.sleep(.6);open_note("Inline preview source");command("Reading view")
                bounds=control_bounds("reading-block:0")
                xdo("mousemove","--sync",str(round(bounds[0]+25)),str(round((bounds[1]+bounds[3])/2)));time.sleep(.8)
                control_bounds("page-preview-card")
                green("page-preview/table-cell:0:1:1");yellow("page-preview/table-cell:0:1:0")
                capture("native-inline-page-preview.png")
                assert target.read_text()==target_text and link.read_text()==link_text and note.read_text()==source
                xdo("mousemove","--sync","1100","800");time.sleep(.5)
                results.append("The actual hover preview rendered the same table backgrounds without opening or rewriting either linked note")
                set_theme(True);open_note("Home")

            def table_checks():
                source = (ROOT / "tests/fixtures/table.md").read_text()
                note = fixture / "Table GUI.md"
                note.write_text(source)
                time.sleep(.6)
                open_note("Table GUI")
                command("Reading view")
                table = control_bounds("table:0")
                # These reference positions predate the 34px client title bar.
                titlebar_height = 34 if control_bounds("window-title-drag", required=False) else 0
                assert abs(table[2] - table[0] - 303.171875) < 3, table
                assert abs(table[3] - table[1] - 130.640625) < 1, table
                assert abs(table[1] - titlebar_height - 241.84375) < 1, table
                middle = control_bounds("table-cell:0:1:1")
                centered = control_bounds("table-text:0:1:1")
                assert abs(middle[0] + middle[2] - centered[0] - centered[2]) < 1, (middle, centered)
                right = control_bounds("table-cell:0:1:2")
                number = control_bounds("table-text:0:1:2")
                assert abs(right[2] - number[2] - 8.5) < 1, (right, number)
                code_panel = control_bounds("code-block:0")
                assert abs(code_panel[1] - titlebar_height - 491.1875) < 1, code_panel
                assert control_bounds("table-cell:0:3:1")
                capture("native-reading-table-dark.png")
                click_control("fold:0")
                assert control_bounds("table:0", required=False) is None
                assert control_bounds("code-block:0")
                click_control("fold:0")
                assert control_bounds("table:0")
                assert note.read_text() == source
                results.append("Reading tables retained empty cells, native code panels and heading folding without changing Markdown")
                click_control("table-link:0:2:0:0")
                title_contains("Home")
                assert note.read_text() == source
                assert not (fixture / "Home\\.md").exists()
                open_note("Table GUI")
                command("Reading view")
                command("Switch appearance")
                capture("native-reading-table-light.png")
                assert control_bounds("table:0")
                command("Switch appearance")
                results.append("A table wiki alias opened its existing note; light/dark tables preserved the exact source")
                wide_source = "| " + " | ".join(f"Column {index:02}" for index in range(20)) + " |\n"
                wide_source += "| " + " | ".join("---" for _ in range(20)) + " |\n"
                wide_source += "| " + " | ".join(f"Value {index:02} π" for index in range(20)) + " |\n"
                wide = fixture / "Wide table GUI.md"
                wide.write_text(wide_source)
                time.sleep(.6)
                open_note("Wide table GUI")
                command("Reading view")
                assert control_bounds("table-cell:0:1:0")
                assert control_bounds("table-cell:0:1:19", required=False) is None
                table = control_bounds("table:0")
                for _ in range(8):
                    if control_bounds("table-cell:0:1:19", required=False):
                        break
                    xdo("mousemove", "800", str(round((table[1] + table[3]) / 2)), "click", "--repeat", "20", "--delay", "1", "7")
                    pause()
                assert control_bounds("table-cell:0:1:19")
                assert wide.read_text() == wide_source
                capture("native-reading-table-wide.png")
                results.append("Horizontal scrolling reached the final column of a 20-column table without clipping away its content or changing the note")

            def persian_checks():
                source = (ROOT / "tests/fixtures/persian.md").read_text()
                note = fixture / "Persian GUI.md"
                note.write_text(source)
                time.sleep(.6)
                open_note("Persian GUI")
                for mode, command_name in [("source", "Source editing"), ("live", "Live preview"), ("reading", "Reading view")]:
                    command(command_name)
                    capture(f"native-persian-{mode}.png")
                    assert note.read_text() == source
                type_text("READING_PERSIAN_MUST_STAY_EXACT")
                key("ctrl+s")
                assert note.read_text() == source
                command("Source editing")
                key("ctrl+End")
                # Unicode symbols stay mapped until this private app exits.
                type_text("\nسلام از صفحه‌کلید", delay=100)
                key("ctrl+s")
                assert note.read_text() == source + "\nسلام از صفحه‌کلید", repr(note.read_text())
                key("ctrl+z")
                key("ctrl+s")
                assert note.read_text() == source
                key("ctrl+a")
                key("ctrl+c")
                key("ctrl+End")
                key("Return")
                key("ctrl+v")
                key("ctrl+s")
                copied = source + "\n" + source
                assert note.read_text() == copied
                key("ctrl+a")
                key("BackSpace")
                key("ctrl+s")
                assert note.read_text() == ""
                key("ctrl+z")
                key("ctrl+s")
                assert note.read_text() == copied
                key("ctrl+a")
                key("ctrl+v")
                key("ctrl+s")
                assert note.read_text() == source
                results.append("Persian Unicode keyboard input, native copy/paste, selection/deletion/undo and read-only behavior preserved exact bytes; screenshots require separate glyph/direction inspection")

            def tab_overflow_checks():
                sources = {}
                for index in range(10):
                    name = f"Overflow{index:02}.md"
                    sources[name] = f"# Overflow{index:02}\n\nExact tab body {index}.\n"
                    (fixture / name).write_text(sources[name])
                time.sleep(.6)
                for index in range(10):
                    key("ctrl+o")
                    type_text(f"Overflow{index:02}")
                    key("ctrl+Return")
                    title_contains(f"Overflow{index:02}")
                last = control_bounds("tab:Overflow09.md")
                assert last[2] - last[0] >= 32, last
                assert max(abs(a-b) for a,b in zip(last, control_bounds("tab-full:Overflow09.md"))) < 1, last
                assert control_bounds("new-tab")
                capture("native-tab-overflow.png")
                for index in range(8, -1, -1):
                    key("ctrl+shift+Tab")
                    title_contains(f"Overflow{index:02}")
                    rect = control_bounds(f"tab:Overflow{index:02}.md")
                    assert rect[2] - rect[0] >= 32, (index, rect)
                    assert max(abs(a-b) for a,b in zip(rect, control_bounds(f"tab-full:Overflow{index:02}.md"))) < 1, (index, rect)
                results.append("Opening ten tabs and switching backward kept each active tab fully visible through horizontal overflow")
                xdo("windowsize", window, "860", "560")
                pause()
                key("ctrl+Tab")
                title_contains("Overflow01")
                rect = control_bounds("tab:Overflow01.md")
                assert rect[2] - rect[0] >= 32, rect
                assert max(abs(a-b) for a,b in zip(rect, control_bounds("tab-full:Overflow01.md"))) < 1, rect
                assert control_bounds("new-tab")
                capture("native-tab-overflow-compact.png", (860, 560))
                for name, source in sources.items():
                    assert (fixture / name).read_text() == source
                xdo("windowsize", window, "1440", "960")
                pause()
                results.append("The active tab stayed visible at compact width while switching, with every note unchanged")

            def code_checks():
                source = "Before\n\n```text\n**literal** [[No link]]\n> literal π\n```\n\nAfter\n"
                note = fixture / "Code GUI.md"
                note.write_text(source)
                time.sleep(.6)
                open_note("Code GUI")
                command("Live preview")
                capture("native-code-live.png")
                bounds = tuple(round(value) for value in control_bounds("editor"))
                picture = ImageGrab.grab(xdisplay=display).crop(bounds)
                panel = sum(pixel[:3] == (27, 32, 35) for pixel in pixel_data(picture))
                assert panel >= 60000, f"Missing full-width code panel: {panel} pixels"
                block = control_bounds("code-block:0")
                assert 100 <= block[3] - block[1] <= 104, block
                assert control_bounds("code-source:0", required=False) is None
                assert note.read_text() == source
                results.append("Inactive fenced code used a full-width panel, measured code line height, and literal source-preserving content")
                click_control("code-block:0")
                assert control_bounds("code-source:0")
                capture("native-code-active.png")
                assert note.read_text() == source
                command("Source editing")
                assert control_bounds("code-source:0")
                capture("native-code-source.png")
                assert note.read_text() == source
                results.append("Activating any code line exposed the fences; Source editing retained the code panel and exact source bytes")
                command("Live preview")
                block = control_bounds("code-block:0")
                xdo("mousemove", str(round(block[2] - 18)), str(round(block[1] + 12)))
                pause()
                capture("native-code-copy.png")
                click_control("copy-code:0")
                create_note_named('Clipboard result.md')
                command("Source editing")
                editor_end()
                key("ctrl+a")
                key("ctrl+v")
                key("ctrl+s")
                expected_code = "**literal** [[No link]]\n> literal π\n"
                assert (fixture / "Clipboard result.md").read_text() == expected_code
                assert note.read_text() == source
                key("ctrl+o")
                type_text("Code GUI")
                key("ctrl+Return")
                title_contains("Code GUI")
                results.append("The generated Copy control put only the exact Unicode code body on the system clipboard; native paste excluded both fences and preserved the original note")
                command("Live preview")
                editor_end()
                type_text("CODE_TRAILING_EDIT")
                key("ctrl+s")
                assert note.read_text() == source + "CODE_TRAILING_EDIT"
                key("ctrl+z")
                key("ctrl+s")
                assert note.read_text() == source
                key("ctrl+a")
                type_text("Code replaced")
                key("ctrl+s")
                assert note.read_text() == "Code replaced"
                assert control_bounds("code-block:0", required=False) is None
                key("ctrl+z")
                key("ctrl+s")
                assert note.read_text() == source
                assert control_bounds("code-block:0")
                results.append("Editing after code, deleting the complete document, and undoing both operations preserved source and refreshed code ranges")

            def typography_checks():
                source = "> Quoted **warm** body.\n> > Nested body.\n\nHow are you?!\n---\n\n- First row\n- Second row\n\n```text\n> literal\n```\n"
                note = fixture / "Typography GUI.md"
                note.write_text(source)
                time.sleep(.6)
                open_note("Typography GUI")
                command("Live preview")
                capture("native-typography-live.png")
                bounds = tuple(round(value) for value in control_bounds("editor"))
                picture = ImageGrab.grab(xdisplay=display).crop(bounds)
                bars = sum(pixel[:3] == (23, 127, 131) for pixel in pixel_data(picture))
                assert bars >= 100, f"Missing live quote bars: {bars} pixels"
                assert control_bounds("quote:1")
                assert control_bounds("quote:2", required=False) is None
                marker = control_bounds("setext:0")
                assert 28 <= marker[3] - marker[1] <= 31, marker
                marker_pixels = pixel_data(ImageGrab.grab(xdisplay=display).crop(tuple(round(v) for v in marker)))
                assert sum(pixel[:3] == (104, 115, 117) for pixel in marker_pixels) > 2, "Setext marker disappeared"
                assert note.read_text() == source
                results.append("Live quotes painted nested teal bars, code stayed literal, and Setext underline remained visible at body size without changing source")
                click_control("quote:0")
                assert control_bounds("quote-source:0")
                capture("native-typography-active.png")
                command("Source editing")
                capture("native-typography-source.png")
                bounds = tuple(round(value) for value in control_bounds("editor"))
                picture = ImageGrab.grab(xdisplay=display).crop(bounds)
                assert not any(pixel[:3] == (23, 127, 131) for pixel in pixel_data(picture))
                assert note.read_text() == source
                results.append("Activating a quote exposed its source; Source editing removed live quote bars and retained all Markdown bytes")
                command("Live preview")
                editor_end()
                key("ctrl+a")
                type_text("Replacement")
                key("ctrl+s")
                assert note.read_text() == "Replacement"
                assert control_bounds("quote:0", required=False) is None
                key("ctrl+z")
                key("ctrl+s")
                assert note.read_text() == source
                assert control_bounds("quote:0")
                results.append("Replacing and undoing a decorated document refreshed quote ranges without stale offsets or source loss")

            def folding_checks():
                open_note("Home")
                original_home = (fixture / "Home.md").read_text()
                command("Source editing")
                capture("native-source-reference.png")
                assert (fixture / "Home.md").read_text() == original_home
                create_note_named('Notes/Folding GUI.md')
                command("Source editing")
                editor_end()
                key("ctrl+a")
                source = "# Folding GUI\n\nBefore\n\n## Parent\n\nPARENT_BODY\n\n### Child\n\nCHILD_BODY\n\n## Sibling\n\nSIBLING_BODY\n\n[[#Child]]"
                type_text(source)
                key("ctrl+s")
                folded = fixture / "Notes/Folding GUI.md"
                assert folded.read_text() == source
                command("Reading view")
                assert control_bounds("reading-block:3")
                assert control_bounds("reading-block:5")
                click_control("fold:4")
                pause()
                assert control_bounds("reading-block:3")
                assert control_bounds("reading-block:5", required=False) is None
                click_control("fold:2")
                pause()
                assert control_bounds("reading-block:3", required=False) is None
                assert control_bounds("reading-block:4", required=False) is None
                assert control_bounds("reading-block:7")
                capture("native-folded-heading.png")
                click_control("fold:2")
                pause()
                assert control_bounds("reading-block:3")
                assert control_bounds("reading-block:5", required=False) is None
                assert folded.read_text() == source
                results.append("Reading heading folding hid nested content, retained child fold state, kept siblings visible, and preserved exact source bytes")
                click_control("fold:2")
                click_control("reading-block:8")
                pause()
                assert control_bounds("reading-block:3")
                assert control_bounds("reading-block:5")
                assert folded.read_text() == source
                capture("native-folded-anchor.png")
                results.append("Following an internal heading link expanded its collapsed ancestors and target without editing the note")
                command("Source editing")
                assert control_bounds("edit-body:1")
                assert control_bounds("edit-body:2")
                click_control("edit-fold:2")
                pause()
                assert control_bounds("edit-body:1")
                assert control_bounds("edit-body:2", required=False) is None
                click_control("edit-fold:1")
                pause()
                assert control_bounds("edit-heading:2", required=False) is None
                assert control_bounds("edit-body:1", required=False) is None
                assert control_bounds("edit-heading:3")
                capture("native-editing-folded-heading.png")
                click_control("edit-fold:1")
                pause()
                assert control_bounds("edit-body:1")
                assert control_bounds("edit-body:2", required=False) is None
                assert folded.read_text() == source
                command("Switch appearance")
                assert control_bounds("edit-body:2", required=False) is None
                assert control_bounds("edit-heading:3")
                capture("native-editing-folded-light.png")
                xdo("windowsize", window, "1040", "800")
                pause()
                assert control_bounds("edit-body:2", required=False) is None
                assert control_bounds("edit-heading:3")
                capture("native-editing-folded-compact.png", (1040, 800))
                xdo("windowsize", window, "1440", "960")
                pause()
                command("Switch appearance")
                command("Live preview")
                assert control_bounds("edit-body:2", required=False) is None
                click_control("edit-heading:2")
                key("Down")
                assert control_bounds("edit-body:2", required=False) is None
                key("End")
                type_text(" VISIBLE_SUFFIX")
                key("ctrl+s")
                assert folded.read_text() == source.replace("## Sibling\n", "## Sibling VISIBLE_SUFFIX\n")
                key("ctrl+z")
                key("ctrl+s")
                assert folded.read_text() == source
                key("ctrl+End")
                type_text("\nVISIBLE_EDIT")
                key("ctrl+s")
                assert folded.read_text() == source + "\nVISIBLE_EDIT"
                key("ctrl+z")
                key("ctrl+s")
                assert folded.read_text() == source
                key("ctrl+End")
                key("ctrl+Return")
                pause()
                assert control_bounds("edit-body:2")
                assert folded.read_text() == source
                results.append("Editing heading folds retained nested state across Source/Live, arrows skipped hidden rows, sibling edits/undo preserved hidden source and anchors expanded their target")
                key("ctrl+End")
                click_control("edit-fold:3")
                pause()
                assert control_bounds("edit-body:3", required=False) is None
                assert control_bounds("edit-heading:3")
                key("ctrl+End")
                assert control_bounds("edit-body:3")
                assert folded.read_text() == source
                (fixture / "Assets").mkdir(exist_ok=True)
                Image.new("RGB", (32, 24), "teal").save(fixture / "Assets/fold.png")
                source = source.replace("CHILD_BODY", "CHILD_BODY\n\n```md\n# Not a heading\n```\n\n![[Assets/fold.png|64]]")
                key("ctrl+a")
                type_text(source)
                key("ctrl+s")
                time.sleep(.6)
                assert folded.read_text() == source, repr(folded.read_text())
                key("ctrl+Home")
                assert control_bounds("code-block:0")
                assert control_bounds("embed:0")
                click_control("edit-fold:2")
                pause()
                assert control_bounds("code-block:0", required=False) is None
                assert control_bounds("embed:0", required=False) is None
                key("ctrl+End")
                type_text("\nAFTER_FOLDED_ASSETS")
                key("ctrl+s")
                assert folded.read_text() == source + "\nAFTER_FOLDED_ASSETS"
                assert control_bounds("code-block:0", required=False) is None
                assert control_bounds("embed:0", required=False) is None
                assert control_bounds("edit-heading:4", required=False) is None
                click_control("edit-fold:2")
                pause()
                key("ctrl+Home")
                assert control_bounds("code-block:0")
                assert control_bounds("embed:0")
                key("ctrl+End")
                click_control("edit-fold:3")
                key("ctrl+Home")
                assert control_bounds("edit-body:3", required=False) is None
                old_image = control_bounds("embed:0")
                assert abs(old_image[3] - old_image[1] - 48) < 1.1
                Image.new("RGB", (48, 48), "purple").save(fixture / "Assets/fold.png")
                time.sleep(.8)
                new_image = control_bounds("embed:0")
                assert abs(new_image[3] - new_image[1] - 64) < 1.1
                assert control_bounds("edit-body:3", required=False) is None
                assert folded.read_text() == source + "\nAFTER_FOLDED_ASSETS"
                capture("native-editing-folded-image-refresh.png")
                results.append("The final section folded safely from an EOF caret and Ctrl+End revealed it; folded code/images stayed hidden through edits and returned on expansion")
            def file_toolbar_checks():
                nonlocal app, window
                click_control("button:File explorer")
                pause()
                click_control("new-folder")
                key("ctrl+a")
                type_text("Toolbar folder")
                key("Return")
                assert (fixture / "Toolbar folder").is_dir()
                assert control_bounds("folder:Toolbar folder")
                results.append("The generated New Folder toolbar icon created an empty folder")
                click_control("file-sort")
                click_control("sort-descending")
                pause()
                assert control_bounds("folder:Toolbar folder")[1] < control_bounds("folder:Notes")[1]
                xdo("key", "--clearmodifiers", "ctrl+q")
                app.wait(timeout=10)
                app = launch_app(launch_command)
                window = ready_window()
                assert control_bounds("folder:Toolbar folder")[1] < control_bounds("folder:Notes")[1]
                click_control("file-sort")
                click_control("sort-ascending")
                pause()
                assert control_bounds("folder:Notes")[1] < control_bounds("folder:Toolbar folder")[1]
                results.append("The generated Sort icon changed folder order and the chosen direction survived a real restart")
                click_control("new-folder")
                key("ctrl+a")
                type_text("Notes/Nested")
                key("Return")
                create_note_named('Notes/Nested/Toolbar child.md')
                title_contains("Toolbar child")
                assert control_bounds("file:Notes/Nested/Toolbar child.md")
                child = fixture / "Notes/Nested/Toolbar child.md"
                original = child.read_text()
                click_control("file-collapse")
                pause()
                assert control_bounds("file:Notes/Nested/Toolbar child.md", required=False) is None
                click_control("folder:Notes")
                pause()
                assert control_bounds("folder:Notes/Nested")
                assert control_bounds("file:Notes/Nested/Toolbar child.md", required=False) is None
                click_control("folder:Notes/Nested")
                pause()
                assert control_bounds("file:Notes/Nested/Toolbar child.md")
                assert child.read_text() == original
                capture("native-file-toolbar.png")
                results.append("The generated Collapse All icon collapsed nested folders, which could be reopened without changing the note")
            def keyboard_focus_checks():
                key("ctrl+t")
                command("Source editing",available=False)
                key("Escape")
                anchor=fixture/"Keyboard mode anchor.md"
                anchor_source="# Keyboard mode anchor\n"
                anchor.write_text(anchor_source);time.sleep(.6)
                open_note("Keyboard mode anchor")
                command("Source editing")
                control_bounds("editor-focused")
                results.append("The mode command stayed unavailable on a blank tab and became available for an actual Markdown document, matching the reference")
                first = fixture / "Notes/First steps.md"
                original = first.read_text()
                open_note("First steps")
                key("ctrl+End")
                type_text("QUICK_OPEN_FOCUS")
                key("ctrl+s")
                assert first.read_text() == original + "QUICK_OPEN_FOCUS", repr(first.read_text())
                assert anchor.read_text()==anchor_source
                results.append("Quick opening a note accepted immediate keyboard editing without clicking the editor")
                key("ctrl+shift+f")
                key("ctrl+a")
                type_text("path:Notes/")
                assert control_bounds("search:Notes/First steps.md")
                assert first.read_text() == original + "QUICK_OPEN_FOCUS"
                results.append("Global search retained keyboard focus and did not insert its query into the open note")
                create_note_named('Notes/Keyboard focus.md')
                title_contains("Keyboard focus")
                created = fixture / "Notes/Keyboard focus.md"
                initial = created.read_text()
                key("ctrl+End")
                type_text("CREATED_FOCUS")
                key("ctrl+s")
                assert created.read_text() == initial + "CREATED_FOCUS", repr(created.read_text())
                key("ctrl+shift+Tab")
                title_contains("First steps")
                key("ctrl+End")
                type_text("TAB_SWITCH_FOCUS")
                key("ctrl+s")
                assert first.read_text() == original + "QUICK_OPEN_FOCUSTAB_SWITCH_FOCUS", repr(first.read_text())
                assert created.read_text() == initial + "CREATED_FOCUS"
                capture("native-keyboard-focus.png")
                results.append("Creating a note and switching tabs accepted immediate typing while preserving the other note")
                command("Reading view")
                type_text("READING_MUST_STAY_READ_ONLY")
                key("ctrl+s")
                assert first.read_text() == original + "QUICK_OPEN_FOCUSTAB_SWITCH_FOCUS"
                key("ctrl+e")
                key("ctrl+End")
                type_text("EDITING_MODE_FOCUS")
                key("ctrl+s")
                assert first.read_text() == original + "QUICK_OPEN_FOCUSTAB_SWITCH_FOCUSEDITING_MODE_FOCUS"
                results.append("Reading mode rejected text input; returning to editing accepted immediate typing")
                command("Source editing")
                key("ctrl+End")
                before = first.read_text()

                queued_text("SAME_FRAME_SAVE", "ctrl+s")
                assert first.read_text() == before + "SAME_FRAME_SAVE", repr(first.read_text())
                command("Reading view")
                queued_text("READ_ONLY_SAME_FRAME", "ctrl+s")
                assert first.read_text() == before + "SAME_FRAME_SAVE"
                results.append("Ctrl+S saved all text queued in its UI frame before autosave; the same queued input left Reading mode unchanged")
                # Restore this shared fixture for subsequent behavior checks.
                first.write_text(original)
                time.sleep(.5)

                queued = fixture / "Notes/Queued links GUI.md"
                source = "# Queued links GUI\n\n"
                queued.write_text(source)
                time.sleep(.6)
                open_note("Queued links GUI")
                command("Source editing")
                editor_end()
                queued_text("[[fststp", "Tab", "ctrl+s")
                expected = source + "[[Notes/First steps]]"
                assert queued.read_text() == expected, repr(queued.read_text())
                queued.write_text(source)
                time.sleep(.7)
                editor_end()
                type_text("[[Notes/First steps")
                queued_text("]]", "Return", "ctrl+s")
                assert queued.read_text() == expected + "\n", repr(queued.read_text())
                queued.write_text(source)
                time.sleep(.7)
                editor_end()
                queued_text("[[NO_MATCH_X92", "Return", "Return", "ctrl+s")
                assert queued.read_text() == source + "[[NO_MATCH_X92\n\n", repr(queued.read_text())
                results.append("Queued wiki typing completed the current query; a closing delimiter followed by Enter preserved the newline and exact source")

            def tab_state_checks():
                open_note("Home")
                home_source = (fixture / "Home.md").read_text()
                first = fixture / "Notes/First steps.md"
                original = first.read_text()
                key("ctrl+t")
                open_note("First steps")
                command("Source editing")
                editor_end()
                type_text("TAB_STATE_SENTINEL")
                key("ctrl+s")
                time.sleep(.3)
                assert first.read_text() == original + "TAB_STATE_SENTINEL"
                key("ctrl+shift+Tab")
                title_contains("Home")
                key("ctrl+w")
                title_contains("First steps")
                control_bounds("editor-focused")
                key("ctrl+End")
                key("ctrl+z")
                key("ctrl+s")
                time.sleep(.3)
                assert first.read_text() == original, repr(first.read_text())
                assert (fixture / "Home.md").read_text() == home_source
                key("ctrl+y")
                key("ctrl+s")
                time.sleep(.3)
                assert first.read_text() == original + "TAB_STATE_SENTINEL"
                key("ctrl+z")
                key("ctrl+s")
                time.sleep(.3)
                assert first.read_text() == original
                results.append("Closing an earlier tab retained the active note’s undo/redo history without editing the closed note")
            def vault_checks():
                nonlocal app, window
                open_note("Home")
                original = (fixture / "Home.md").read_bytes()
                if control_bounds("vault-profile", required=False) is None:
                    command("Toggle left sidebar")
                # Masih, 23 Oct: the vault name and its gear were cut off under the status bar.
                status_top = control_bounds("status-bar")[1]
                for name in ("vault-profile", "open-settings"):
                    assert control_bounds(name)[3] <= status_top, (name, control_bounds(name), status_top)

                def manager():
                    click_control("vault-profile")
                    bounds = control_bounds("vault-manager")
                    left, top, right, bottom = bounds
                    assert 0 <= left < right <= 1440 and 0 <= top < bottom <= 960, f"Vault chooser escaped its viewport: {bounds}"
                    assert control_bounds("vault-manager-close")

                def field(name, value):
                    click_control(name)
                    key("ctrl+a")
                    paste_text(value)

                def recent(root):
                    click_control(f"vault-recent:{root}")
                    title_contains(root.name)
                    assert control_bounds("vault-manager", required=False) is None

                def roots():
                    return json.loads((base / "data/rustidian/vaults.json").read_text())["roots"]

                manager()
                assert control_bounds(f"vault-recent:{fixture}")
                capture("native-vaults-dark.png")
                click_control("vault-manager-close")
                command("Switch appearance")
                manager()
                capture("native-vaults-light.png")
                click_control("vault-create")
                field("vault-name", "Cancelled vault")
                field("vault-folder", str(base))
                click_control("vault-manager-close")
                assert not (base / "Cancelled vault").exists()
                assert (fixture / "Home.md").read_bytes() == original
                command("Switch appearance")
                results.append("The vault profile opened the recent/create/open chooser in both themes; cancelling creation preserved the active vault and created nothing")

                def autosave(enabled):
                    command("Open settings")
                    type_text("autosave")
                    assert control_bounds("settings-dark", required=False) is None
                    if json.loads(fixture_state.read_text())["autosave_enabled"] is not enabled:
                        click_control("settings-autosave", hold=.15)
                    wait_autosave(base / "state", enabled, session_path=fixture_state)
                    capture(f"native-vault-autosave-{enabled}.png")
                    key("Escape")

                created = base / "Created vault یادداشت"
                manager()
                click_control("vault-create")
                field("vault-name", created.name)
                field("vault-folder", str(base))
                capture("native-vault-create.png")
                click_control("vault-confirm")
                title_contains(created.name)
                assert list(created.iterdir()) == []
                assert roots()[0] == str(created)
                # Leaving the first vault has now persisted its session.
                fixture_state = max((base / "state/rustidian").glob("*.json"), key=lambda path: path.stat().st_mtime_ns)
                create_note_named("Home.md")
                title_contains("Home")
                assert control_bounds("editor-focused")
                created_home=created/"Home.md"
                initial_home=created_home.read_bytes()
                typed_home=b"# Created vault\n\nVAULT_CREATED_ORIGINAL\n"
                type_text(typed_home.decode())
                key("ctrl+s")
                created_source=initial_home+typed_home
                samples=[];deadline=time.monotonic()+5
                while time.monotonic()<deadline:
                    actual=created_home.read_bytes();samples.append(actual.decode())
                    if actual==created_source:break
                    time.sleep(.05)
                (reports/"native-vault-created-write.json").write_text(json.dumps({"initial":initial_home.decode(),"expected":created_source.decode(),"observed":samples},indent=2)+"\n")
                assert actual==created_source,(actual,created_source)
                manager()
                recent(fixture)
                title_contains("Home")
                assert (fixture / "Home.md").read_bytes() == original
                manager()
                click_control("vault-create")
                field("vault-name", created.name)
                field("vault-folder", str(base))
                click_control("vault-confirm")
                assert control_bounds("vault-error")
                capture("native-vault-collision.png")
                assert (created / "Home.md").read_bytes() == created_source,((created/"Home.md").read_bytes(),created_source)
                click_control("vault-manager-close")
                results.append("Creating a named Unicode vault made only its empty folder, one-click switching restored the previous note, and a name collision retained every original byte")

                opened = base / "Browsed vault"
                opened.mkdir()
                (opened / "Home.md").write_text("# Browsed vault\n\nBROWSE_ORIGINAL\n")
                manager()
                click_control("vault-open")
                deadline = time.monotonic() + 8
                picker = None
                while time.monotonic() < deadline:
                    found = subprocess.run(["xdotool", "search", "--onlyvisible", "--name", "^Open folder as vault$"], env=environment, text=True, capture_output=True)
                    if found.returncode == 0:
                        picker = found.stdout.splitlines()[-1]
                        break
                    time.sleep(.05)
                assert picker, "Browse must launch the real native folder chooser"
                focus_samples=[]
                def picker_focus(stage):
                    details=subprocess.check_output(["xwininfo","-id",picker],env=environment,text=True,timeout=3)
                    mapping=next(line.strip() for line in details.splitlines() if "Map State:" in line)
                    focus_samples.append({"stage":stage,"parent":window,"picker":picker,"focus":xdo("getwindowfocus"),"picker_mapping":mapping})
                    (reports/"native-vault-picker-focus.json").write_text(json.dumps(focus_samples,indent=2)+"\n")
                # Let GTK finish activation before unmapping it. Synchronize the
                # unmap before assigning the parent its one physical Enter key.
                xdo("windowfocus", "--sync", picker)
                pause();picker_focus("picker-ready")
                xdo("windowunmap", "--sync", picker)
                picker_focus("picker-unmapped")
                xdo("windowfocus", "--sync", window)
                pause();picker_focus("parent-ready")
                assert xdo("getwindowfocus")==window
                assert "IsUnMapped" in focus_samples[-1]["picker_mapping"]
                key("Return")
                assert control_bounds("vault-parent-enter-observed")[0]==1
                assert control_bounds("vault-manager")
                assert xdo("getwindowname", picker) == "Open folder as vault", "A busy parent must not submit or close its picker"
                title_contains(fixture.name)
                xdo("windowmap","--sync",picker)
                xdo("windowfocus", "--sync", picker)
                pause()
                modified_shortcut(xdo, "l")
                pause()
                modified_shortcut(xdo, "a")
                pause()
                # GTK path completion can race a rapid per-key typing stream.
                paste_text(str(opened), held=True)
                modified_shortcut(xdo, "a")
                modified_shortcut(xdo, "c")
                pause()
                entered="";samples=[];deadline=time.monotonic()+5
                while time.monotonic()<deadline:
                    copied=subprocess.run(["xclip","-selection","clipboard","-out","-target","UTF8_STRING"],env=environment,capture_output=True,timeout=2)
                    entered=copied.stdout.decode()
                    samples.append({"returncode":copied.returncode,"actual":entered,"error":copied.stderr.decode()})
                    if copied.returncode==0 and entered.rstrip("/")==str(opened):break
                    time.sleep(.05)
                location = {"expected": str(opened), "actual": entered, "focus": xdo("getwindowfocus"), "picker": picker,"clipboard_samples":samples}
                (reports / "native-vault-picker-entry.json").write_text(json.dumps(location, indent=2) + "\n")
                if entered.rstrip("/") != str(opened):
                    (reports / "gui-vaults-failure-location.json").write_text(json.dumps(location, indent=2) + "\n")
                    raise AssertionError(f"The native location entry must acknowledge its exact path: {location}")
                capture("native-vault-picker-location.png")
                # Activate OK: Enter can navigate into a populated directory.
                modified_shortcut(xdo, "o", "alt")
                pause()
                title_contains(opened.name)
                xdo("windowfocus", "--sync", window)
                assert roots()[0] == str(opened)
                assert (opened / "Home.md").read_text() == "# Browsed vault\n\nBROWSE_ORIGINAL\n"
                shared = base / "Other window vault"
                shared.mkdir()
                (shared / "Home.md").write_text("# Other window\n")
                catalog = base / "data/rustidian/vaults.json"
                updated = json.loads(catalog.read_text())
                updated["roots"].insert(0, str(shared))
                pending_catalog = catalog.with_suffix(".pending")
                pending_catalog.write_text(json.dumps(updated))
                pending_catalog.replace(catalog)
                assert str(shared) in roots()
                manager()
                try:
                    control_bounds(f"vault-recent:{shared}")
                except AssertionError:
                    (reports / "vault-catalog-refresh-failure.json").write_text(json.dumps({"seeded": updated, "after_open": json.loads(catalog.read_text())}, indent=2) + "\n")
                    raise
                click_control("vault-manager-close")
                nested = opened / "Notes/Nested.md"
                nested.parent.mkdir()
                nested_source = "# Nested\n\n[[Home]]\n"
                nested.write_text(nested_source)
                xdo("key", "--clearmodifiers", "ctrl+q")
                app.wait(timeout=5)
                app = launch_app([launch_command[0], str(nested)])
                window = ready_window()
                title_contains("Nested")
                title_contains(opened.name)
                click_control("editor-link:Home")
                title_contains("Home")
                assert (opened / "Home.md").read_text() == "# Browsed vault\n\nBROWSE_ORIGINAL\n"
                assert nested.read_text() == nested_source
                assert str(nested.parent) not in roots()
                results.append("Opening a nested note directly kept its registered vault, followed a root-level wiki link and avoided registering the note's subfolder")
                manager()
                recent(fixture)
                xdo("key", "--clearmodifiers", "ctrl+q")
                app.wait(timeout=5)
                app = launch_app(launch_command)
                window = ready_window()
                manager()
                assert control_bounds(f"vault-recent:{opened}")
                assert control_bounds(f"vault-recent:{created}")
                assert control_bounds(f"vault-recent:{shared}")
                results.append("Open folder as vault used the real native chooser; shared recent entries refreshed on opening and survived an actual app restart")
                click_control("vault-manager-close")

                autosave(False)
                command("Source editing")
                editor_end()
                type_text("DIRTY_VAULT_CANCEL")
                time.sleep(.8)
                assert (fixture / "Home.md").read_bytes() == original
                manager()
                click_control(f"vault-recent:{created}")
                assert control_bounds("pending-cancel")
                click_control("pending-cancel")
                title_contains(fixture.name)
                assert (fixture / "Home.md").read_bytes() == original
                key("ctrl+s")
                assert (fixture / "Home.md").read_bytes() == original + b"DIRTY_VAULT_CANCEL"
                key("ctrl+z")
                key("ctrl+s")
                assert (fixture / "Home.md").read_bytes() == original
                autosave(True)
                results.append("Cancelling a vault switch retained unsaved editing and the current vault; explicit save and undo preserved the original bytes")

                manager()
                click_control(f"vault-more:{created}")
                click_control(f"vault-forget:{created}")
                assert str(created) not in roots()
                assert (created / "Home.md").read_bytes() == created_source
                moved = base / "Moved browsed vault"
                opened.rename(moved)
                click_control(f"vault-recent:{opened}")
                assert control_bounds("vault-error")
                title_contains(fixture.name)
                assert (fixture / "Home.md").read_bytes() == original
                assert (moved / "Home.md").read_text() == "# Browsed vault\n\nBROWSE_ORIGINAL\n"
                click_control("vault-manager-close")
                results.append("Remove from list preserved the vault's notes; an externally moved recent vault reported an error and retained the active vault")


            def vault_switch_checks():
                open_note("Home")
                command("Source editing")
                editor_end()
                type_text("\nVAULT_ONE_UNDO_SENTINEL")
                key("ctrl+s")
                time.sleep(.3)
                first_home = fixture / "Home.md"
                first_source = first_home.read_text()
                command("Open vault")
                key("ctrl+a")
                missing = base / "Missing vault"
                type_text(str(missing))
                key("Return")
                time.sleep(.5)
                title_contains("Home")
                assert first_home.read_text() == first_source
                assert not missing.exists()
                second = base / "Second Vault"
                second.mkdir()
                second_home = second / "Home.md"
                second_source = "# Second vault\n\nVAULT_TWO_ORIGINAL\n"
                second_home.write_text(second_source)
                command("Open vault")
                key("ctrl+a")
                type_text(str(second))
                key("Return")
                time.sleep(.8)
                title_contains("Second Vault")
                assert second_home.read_text() == second_source
                command("Source editing")
                editor_end()
                key("ctrl+z")
                key("ctrl+s")
                time.sleep(.3)
                assert second_home.read_text() == second_source, repr(second_home.read_text())
                assert first_home.read_text() == first_source
                results.append("Failed vault opening retained the current note; equal filenames in different vaults did not share undo history")
                editor_end()
                type_text("\nVAULT_TWO_EDIT")
                key("ctrl+s")
                time.sleep(.3)
                assert second_home.read_text() == second_source + "\nVAULT_TWO_EDIT", repr(second_home.read_text())
                command("Open vault")
                key("ctrl+a")
                type_text(str(fixture))
                key("Return")
                time.sleep(.8)
                title_contains(fixture.name)
                assert first_home.read_text() == first_source
                assert second_home.read_text() == second_source + "\nVAULT_TWO_EDIT", repr(second_home.read_text())
                capture("native-vault-switch.png")
                results.append("Editing and reopening two vaults saved only the active vault and restored the original vault session")
            def explorer_checks():
                bulk = fixture / "Explorer bulk"
                bulk.mkdir(exist_ok=True)
                for number in range(5000):
                    (bulk / f"Note{number:05d}.md").write_text(f"# Note{number:05d}\n\nShort content.\n")
                time.sleep(1)
                click_control("button:File explorer")
                pause()
                # Collapse an existing expansion, then explicitly expand it.
                header = control_bounds("folder:Explorer bulk")
                click_control("folder:Explorer bulk")
                time.sleep(.3)
                if not control_bounds("file:Explorer bulk/Note00000.md", required=False):
                    click_control("folder:Explorer bulk")
                    time.sleep(.3)
                assert control_bounds("file:Explorer bulk/Note00000.md")
                assert control_bounds("file:Explorer bulk/Note04999.md", required=False) is None
                xdo("mousemove", "175", "400")
                for _ in range(6):
                    xdo("click", "--repeat", "1000", "--delay", "1", "5")
                    time.sleep(.3)
                    if control_bounds("file:Explorer bulk/Note04999.md", required=False):
                        break
                # Other workflows leave root notes below this folder. The virtual
                # list also registers overscan rows under the fixed toolbar; only
                # click after the entire target row is inside the tree viewport.
                top = control_bounds("file-collapse")[3] + 42
                bottom = control_bounds("vault-profile")[1] - 12
                for _ in range(64):
                    row = control_bounds("file:Explorer bulk/Note04999.md", required=False)
                    if row and top < row[1] and row[3] < bottom:
                        break
                    direction = "5" if row and row[3] >= bottom else "4"
                    xdo("click", "--repeat", "2", "--delay", "1", direction)
                    pause()
                row = control_bounds("file:Explorer bulk/Note04999.md")
                assert top < row[1] and row[3] < bottom, (row, top, bottom)
                click_control("file:Explorer bulk/Note04999.md")
                title_contains("Note04999")
                capture("native-large-explorer.png")
                results.append("The native explorer scrolled through 5,000 files and opened the final row; folder toggling retained the complete scroll range")
            def folder_checks():
                click_control("button:File explorer")
                pause()
                click_control("file-collapse")
                original_notes = sorted(str(path.relative_to(fixture)) for path in fixture.rglob("*.md"))
                command("New folder")
                key("ctrl+a")
                type_text("GUI empty")
                key("Return")
                assert (fixture / "GUI empty").is_dir()
                assert control_bounds("folder:GUI empty")
                command("New folder")
                key("ctrl+a")
                type_text("GUI empty")
                key("Return")
                key("Escape")
                assert sorted(str(path.relative_to(fixture)) for path in fixture.rglob("*.md")) == original_notes
                results.append("New folder appeared without a note; creating the same folder again preserved all entries")
                create_note_named('GUI empty/Child.md')
                title_contains("Child")
                assert control_bounds("file:GUI empty/Child.md")
                click_control("file:Home.md")
                title_contains("Home")
                click_control("file:GUI empty/Child.md")
                title_contains("Child")
                capture("native-empty-folder.png")
                results.append("Creating a note revealed its folder and explorer rows opened the selected files")
                time.sleep(1)
                (fixture / "External.empty").mkdir()
                assert control_bounds("folder:External.empty")
                results.append("The file watcher discovered an externally created empty folder")
                click_control("folder:External.empty", "3")
                click_control("folder-rename:External.empty")
                key("ctrl+a")
                type_text("Renamed empty")
                key("Return")
                assert not (fixture / "External.empty").exists()
                assert (fixture / "Renamed empty").is_dir()
                assert list((fixture / "Renamed empty").iterdir()) == []
                assert control_bounds("folder:Renamed empty")
                results.append("The folder context menu renamed an empty folder without adding a Markdown extension or a note")

                moving = fixture / "Move source"
                (moving / "Nested").mkdir(parents=True)
                (moving / "Empty").mkdir()
                (fixture / "Move target").mkdir()
                parent = "# Moving parent\n\n[Home](../Home.md)\n![[Move source/Nested/photo.png|64]]\n"
                child = "# Moving child\n\n[[Move source/Moving parent|parent]]\n"
                links = "# Folder links\n\n[[Move source/Nested/Moving child|child]]\n[Parent](Move%20source/Moving%20parent.md)\n"
                (moving / "Moving parent.md").write_text(parent)
                (moving / "Nested/Moving child.md").write_text(child)
                (fixture / "Folder links.md").write_text(links)
                Image.new("RGB", (32, 24), (80, 180, 120)).save(moving / "Nested/photo.png")
                image = (moving / "Nested/photo.png").read_bytes()
                time.sleep(.7)
                open_note("Moving child")
                command("Source editing")
                key("ctrl+t")
                open_note("Folder links")
                command("Source editing")
                click_control("folder:Move source", "3")
                click_control("folder-rename:Move source")
                key("ctrl+a")
                type_text("Move target/Renamed")
                key("Return")
                moved = fixture / "Move target/Renamed"
                assert not moving.exists()
                assert (moved / "Empty").is_dir()
                assert (moved / "Nested/photo.png").read_bytes() == image
                expected_child = child.replace("Move source/", "Move target/Renamed/")
                expected_parent = parent.replace("../Home.md", "../../Home.md").replace("Move source/", "Move target/Renamed/")
                expected_links = links.replace("Move source/", "Move target/Renamed/").replace("Move%20source/", "Move%20target/Renamed/")
                assert (moved / "Nested/Moving child.md").read_text() == expected_child
                assert (moved / "Moving parent.md").read_text() == expected_parent
                assert (fixture / "Folder links.md").read_text() == expected_links
                key("ctrl+End")
                type_text(" AFTER_FOLDER_RENAME")
                key("ctrl+s")
                expected_links += " AFTER_FOLDER_RENAME"
                assert (fixture / "Folder links.md").read_text() == expected_links, repr((fixture / "Folder links.md").read_text())
                key("ctrl+shift+Tab")
                title_contains("Moving child")
                editor_end()
                type_text(" AFTER_MOVE")
                key("ctrl+s")
                assert (moved / "Nested/Moving child.md").read_text() == expected_child + " AFTER_MOVE"
                key("alt+Left")
                title_contains("Child")
                key("alt+Right")
                title_contains("Moving child")
                for target in ["Notes", "Move target/Renamed/Inside", "Move target/Bad#name"]:
                    click_control("folder:Move target/Renamed", "3")
                    click_control("folder-rename:Move target/Renamed")
                    key("ctrl+a")
                    type_text(target)
                    key("Return")
                    assert moved.is_dir()
                    assert (moved / "Nested/Moving child.md").read_text() == expected_child + " AFTER_MOVE"
                    assert (fixture / "Folder links.md").read_text() == expected_links
                    if "Bad#" in target:
                        capture("native-folder-move-refused.png")
                    key("Escape")
                key("ctrl+End")
                type_text(" AFTER_CANCELLED_MOVE")
                key("ctrl+s")
                assert (moved / "Nested/Moving child.md").read_text() == expected_child + " AFTER_MOVE AFTER_CANCELLED_MOVE"
                command("Reading view")
                click_control("folder:Move target/Renamed", "3")
                click_control("folder-rename:Move target/Renamed")
                key("Escape")
                type_text("READ_ONLY_AFTER_CANCEL")
                key("ctrl+s")
                assert (moved / "Nested/Moving child.md").read_text() == expected_child + " AFTER_MOVE AFTER_CANCELLED_MOVE"
                open_note("Moving parent")
                command("Live preview")
                editor_end()
                assert control_bounds("embed:0")
                assert (moved / "Moving parent.md").read_text() == expected_parent
                capture("native-folder-move.png")
                results.append("Moving a nested folder updated note/image links and open tabs/history, preserved immediate editing and read-only mode, retained empty folders/images, and refused unsafe destinations")
            def large_search_checks():
                bulk = fixture / "Bulk"
                bulk.mkdir(exist_ok=True)
                for number in range(550):
                    (bulk / f"Note{number:04d}.md").write_text(f"# Bulk note {number}\nCommon content.\n")
                time.sleep(1)
                key("ctrl+shift+f")
                key("ctrl+a")
                type_text("path:Bulk/")
                assert control_bounds("search:Bulk/Note0000.md")
                assert control_bounds("search:Home.md", required=False) is None
                search_input = control_bounds("search-input")
                viewport = control_bounds("search-viewport")
                assert abs(viewport[2] - search_input[2]) <= 12, ("Search scrollbar belongs at the sidebar edge", search_input, viewport)
                capture("native-search-sidebar-alignment.png")
                last = None
                for _ in range(12):
                    last = control_bounds("search:Bulk/Note0549.md", required=False)
                    if last:
                        break
                    xdo("mousemove", "150", "730", "click", "--repeat", "100", "--delay", "1", "5")
                    pause()
                assert last, "The last matching note beyond result 200 never appeared"
                capture("native-large-search.png")
                click_control("search:Bulk/Note0549.md")
                title_contains("Note0549")
                results.append("Search results use the full sidebar width with its scrollbar at the edge; all 550 matches remain reachable and the last note opens")
            def bookmark_checks():
                nonlocal app, window
                folder = fixture / "Bookmark folder"
                folder.mkdir()
                path = folder / "Bookmark check.md"
                source = "# Bookmark check\n\nOriginal فارسی 🦀.\n"
                path.write_text(source)
                time.sleep(.7)
                open_note("Bookmark check")
                command("Source editing")
                editor_end()
                type_text(" CONTROL")
                key("ctrl+z")
                key("ctrl+s")
                assert path.read_text() == source
                command("Reading view")
                type_text("READ_ONLY_BOOKMARK_CONTROL")
                key("ctrl+s")
                assert path.read_text() == source
                results.append("Bookmark controls: ordinary source editing/undo/save works and Reading input leaves source intact")
                click_control("bookmarks-tab")
                assert control_bounds("bookmark-add-current")
                click_control("bookmark-add-current")
                command("Bookmark current file")

                def state_with(target):
                    for state_path in (base / "state/rustidian").glob("*.json"):
                        state = json.loads(state_path.read_text())
                        if any(item["path"] == target for item in state.get("bookmarks", {}).get("items", [])):
                            return state_path, state
                    raise AssertionError(f"No saved bookmark: {target}")

                relative = "Bookmark folder/Bookmark check.md"
                state_path, state = state_with(relative)
                assert [item["path"] for item in state["bookmarks"]["items"]] == [relative]
                assert path.read_text() == source
                results.append("Adding the current file persisted immediately outside the vault and repeated bookmarking created no duplicate")
                click_control(f"bookmark:{relative}", "3")
                click_control("bookmark-title")
                capture("native-bookmark-title-open.png")
                assert control_bounds("dialog-input-focused")
                type_text("Writing ")
                paste_text("فارسی")
                key("Return")
                assert state_with(relative)[1]["bookmarks"]["items"][0]["title"] == "Writing فارسی", repr(state_with(relative)[1]["bookmarks"]["items"][0])
                open_note("Home")
                command("Bookmark current file")
                # Explorer context menu follows the same bookmark operation for images.
                click_control("button:File explorer")
                pause()
                click_control("file-collapse")
                click_control("folder:Assets")
                click_control("file:Assets/Attachment.png", "3")
                click_control("file-bookmark:Assets/Attachment.png")
                command("Show bookmarks")
                click_control("bookmark:Assets/Attachment.png", "3")
                click_control("bookmark-up")
                state = state_with(relative)[1]
                assert [item["path"] for item in state["bookmarks"]["items"]] == [relative, "Assets/Attachment.png", "Home.md"]
                deadline = time.monotonic() + 3
                while time.monotonic() < deadline:
                    image_row = control_bounds("bookmark:Assets/Attachment.png")
                    home_row = control_bounds("bookmark:Home.md")
                    if image_row[1] < home_row[1]:
                        break
                    time.sleep(.05)
                assert image_row[1] < home_row[1], "Saved bookmark order was not redrawn"
                image_bytes = attachment.read_bytes()
                click_control("bookmark:Assets/Attachment.png")
                title_contains("Attachment")
                type_text("BOOKMARKED_IMAGE_READ_ONLY")
                key("ctrl+b")
                key("ctrl+s")
                assert attachment.read_bytes() == image_bytes
                target = folder / "Ctrl target.md"
                target.write_text("# Ctrl target\n")
                time.sleep(.7)
                open_note("Ctrl target")
                command("Bookmark current file")
                # Give the new-tab check a fresh target and a last-tab origin.
                # Earlier workflows may leave other tabs on either side.
                key("ctrl+w")
                key("ctrl+t")
                open_note("Bookmark check")
                click_control(f"bookmark:{relative}")
                title_contains("Bookmark check")
                xdo("keydown", "ctrl")
                try:
                    click_control("bookmark:Bookmark folder/Ctrl target.md")
                finally:
                    xdo("keyup", "ctrl")
                title_contains("Ctrl target")
                key("ctrl+w")
                title_contains("Bookmark check")
                click_control("bookmark:Bookmark folder/Ctrl target.md", "3")
                click_control("bookmark-remove")
                assert target.read_text() == "# Ctrl target\n"
                assert path.read_text() == source
                capture("native-bookmarks-dark.png")
                results.append("Bookmark titles retained Persian text, ordering persisted, explorer images stayed read-only and Ctrl-click opened a separate tab")

                queued_steps(("key", "F2"), ("key", "ctrl+a"),
                             ("text", "Bookmark folder/Bookmark renamed.md"), ("key", "Return"))
                relative = "Bookmark folder/Bookmark renamed.md"
                path = fixture / relative
                deadline = time.monotonic() + 5
                while not path.exists() and time.monotonic() < deadline:
                    time.sleep(.05)
                assert path.exists(), "Queued rename lost input or did not finish"
                assert path.read_text() == source
                assert state_with(relative)[1]["bookmarks"]["items"][0]["title"] == "Writing فارسی", repr(state_with(relative)[1]["bookmarks"]["items"][0])
                click_control("button:File explorer")
                pause()
                click_control("file-collapse")
                click_control("folder:Bookmark folder", "3")
                click_control("folder-rename:Bookmark folder")
                assert control_bounds("dialog-input-focused")
                key("ctrl+a")
                type_text("Pinned notes")
                key("Return")
                relative = "Pinned notes/Bookmark renamed.md"
                path = fixture / relative
                deadline = time.monotonic() + 5
                while not path.exists() and time.monotonic() < deadline:
                    time.sleep(.05)
                assert path.exists(), "Folder rename did not finish"
                assert path.read_text() == source
                state_path, state = state_with(relative)
                assert state["bookmarks"]["items"][0]["title"] == "Writing فارسی"
                command("Show bookmarks")
                click_control(f"bookmark:{relative}")
                title_contains("Bookmark renamed")
                xdo("key", "--clearmodifiers", "ctrl+q")
                app.wait(timeout=5)
                app = launch_app(launch_command)
                window = ready_window()
                title_contains("Bookmark renamed")
                assert control_bounds(f"bookmark:{relative}")
                assert state_with(relative)[1]["bookmarks"]["items"][0]["title"] == "Writing فارسی", repr(state_with(relative)[1]["bookmarks"]["items"][0])
                command("Switch appearance")
                capture("native-bookmarks-light.png")
                command("Switch appearance")
                results.append("File and folder renames updated bookmark paths without losing custom titles; a clean native restart restored the list and current file")

                # Removal never deletes the file; failed state writes roll back the shortcut.
                click_control(f"bookmark:{relative}", "3")
                click_control("bookmark-remove")
                assert control_bounds(f"bookmark:{relative}", required=False) is None
                assert path.read_text() == source
                displaced_state = base / "held-workspace-state.json"
                state_path.rename(displaced_state)
                state_path.mkdir()
                try:
                    click_control("bookmark-add-current")
                    assert control_bounds(f"bookmark:{relative}", required=False) is None
                    assert control_bounds("error-save-copy", required=False) is None
                    capture("native-bookmark-state-error.png")
                finally:
                    state_path.rmdir()
                    displaced_state.rename(state_path)
                click_control("bookmark-add-current")
                assert control_bounds(f"bookmark:{relative}")
                assert path.read_text() == source
                results.append("Removing a bookmark preserved its note; a real workspace-write failure rolled back the added shortcut and a later retry saved it")

                # A missing target must neither open a different file nor edit the active one.
                open_note("Home")
                home_before = (fixture / "Home.md").read_bytes()
                missing = path.with_name("Temporarily elsewhere.md")
                path.rename(missing)
                time.sleep(.8)
                click_control(f"bookmark:{relative}")
                title_contains("Home")
                assert (fixture / "Home.md").read_bytes() == home_before
                assert missing.read_text() == source
                click_control(f"bookmark:{relative}", "3")
                click_control("bookmark-remove")
                assert control_bounds(f"bookmark:{relative}", required=False) is None
                missing.rename(path)
                key("ctrl+t")
                click_control("bookmark-add-current")
                command("Bookmark current file")
                assert all(item["path"] for item in state_with("Assets/Attachment.png")[1]["bookmarks"]["items"])
                key("ctrl+w")
                results.append("Missing bookmark targets preserved the active note and stayed removable; a blank tab could not create an empty bookmark")

                second = base / "Bookmark other vault"
                second.mkdir()
                second_home = second / "Home.md"
                second_home.write_text("# Separate vault\n")
                command("Open vault")
                key("ctrl+a")
                type_text(str(second))
                key("Return")
                title_contains(second.name)
                command("Show bookmarks")
                assert control_bounds("bookmark:Assets/Attachment.png", required=False) is None
                click_control("bookmark-add-current")
                command("Open vault")
                key("ctrl+a")
                type_text(str(fixture))
                key("Return")
                title_contains(fixture.name)
                command("Show bookmarks")
                assert control_bounds("bookmark:Assets/Attachment.png")
                assert [item["path"] for item in state_with("Assets/Attachment.png")[1]["bookmarks"]["items"]] == ["Assets/Attachment.png", "Home.md"]
                assert second_home.read_text() == "# Separate vault\n"
                assert path.read_text() == source
                results.append("Two vaults with equal note names retained independent bookmark lists and original note bytes")

            def setting(name, value):
                click_control(name, hold=.15)
                modified_shortcut(xdo, "a")
                if value:
                    paste_text(value, held=True)
                else:
                    key("BackSpace")

            def file_move_checks():
                def inspect_links_dialog(dark):
                    box=control_bounds("move-links-dialog");heading=control_bounds("move-links-title")
                    assert abs(box[2]-box[0]-560)<=3 and 140<=box[3]-box[1]<=260,box
                    assert 28<=heading[3]-heading[1]<=34,heading
                    buttons=[control_bounds("move-links-"+name) for name in ("always","once","no")]
                    assert all(36<=bounds[3]-bounds[1]<=38 for bounds in buttons),buttons
                    assert max(bounds[1] for bounds in buttons)-min(bounds[1] for bounds in buttons)<=1,("Choices fit on one row",buttons)
                    assert buttons[0][0]<buttons[1][0]<buttons[2][0] and 23<=box[2]-buttons[2][2]<=27,(box,buttons)
                    control_bounds("move-links-always-focused");control_bounds("move-links-close")
                    sample=(round(buttons[1][0]+5),round((buttons[1][1]+buttons[1][3])/2))
                    xdo("mousemove","--sync","20","900");time.sleep(.2)
                    normal=ImageGrab.grab(xdisplay=display).getpixel(sample)[:3]
                    capture("native-file-move-links-"+("dark" if dark else "light")+"-normal.png")
                    xdo("mousemove","--sync",str(round((buttons[1][0]+buttons[1][2])/2)),str(sample[1]));time.sleep(.2)
                    hovered=ImageGrab.grab(xdisplay=display).getpixel(sample)[:3]
                    assert max(abs(a-b) for a,b in zip(normal,hovered))>=8,(normal,hovered)
                    capture("native-file-move-links-"+("dark" if dark else "light")+"-hover.png")
                    xdo("mousemove","--sync","20","900");pause()
                    results.append("Measured "+("dark" if dark else "light")+" link dialog kept compact title/buttons, right alignment, default focus and actual hover colors")
                close_tabs()
                originals={p.relative_to(fixture).as_posix():p.read_bytes() for p in fixture.rglob("*.md")}
                source=fixture/"Move source.md"
                source_bytes=b"# Move source\r\n\r\nKeep this document.\r\n"
                source.write_bytes(source_bytes)
                destination=fixture/"Move destination";destination.mkdir()
                command("Refresh vault");pause();open_note("Home")
                before_title=xdo("getwindowname",window)
                file_menu(source.name)
                assert control_bounds("menu:Open in new tab")
                assert control_bounds("menu:Make a copy")
                click_control("menu:Move file to...",hold=.15)
                control_bounds("move-input")
                type_text("Move destination")
                control_bounds("move-folder:Move destination")
                capture("native-file-move-filtered.png")
                key("Escape")
                assert source.read_bytes()==source_bytes and not list(destination.iterdir())
                assert xdo("getwindowname",window)==before_title
                assert all((fixture/name).read_bytes()==data for name,data in originals.items())
                results.append("The separate Move picker filtered real folders and cancelled without changing navigation or any file byte")

                file_menu(source.name);click_control("menu:Move file to...",hold=.15)
                type_text("Move destination");key("Return")
                moved=destination/source.name
                assert not source.exists() and moved.read_bytes()==source_bytes
                assert control_bounds("move-input",required=False) is None
                assert control_bounds("move-links-dialog",required=False) is None
                assert xdo("getwindowname",window)==before_title
                open_note("Move destination/Move source",title="Move source")
                results.append("Moving an inactive file into an existing folder preserved its exact CRLF bytes and active navigation; the updated index opened its new path immediately")

                reference=fixture/"Move reference.md"
                reference_bytes=b"[[Move destination/Move source|Alias]]\r\n[Label](Move%20destination/Move%20source.md)\r\n"
                reference.write_bytes(reference_bytes)
                command("Refresh vault");pause();command("Source editing")
                editor_end();type_text(" BEFOREMOVE");key("ctrl+s")
                edited=source_bytes+b" BEFOREMOVE"
                assert moved.read_bytes()==edited
                command("Move current file to another folder")
                type_text("Created/Nested")
                control_bounds("move-create-folder");capture("native-file-move-create-row.png")
                key("Return")
                nested=fixture/"Created/Nested/Move source.md"
                assert nested.read_bytes()==edited and not moved.exists()
                assert reference.read_bytes()==reference_bytes
                control_bounds("move-links-dialog")
                inspect_links_dialog(True)
                for name in ("always","once","no"):control_bounds("move-links-"+name)
                capture("native-file-move-links.png")
                click_control("move-links-once",hold=.15)
                expected=b"[[Created/Nested/Move source|Alias]]\r\n[Label](Created/Nested/Move%20source.md)\r\n"
                assert reference.read_bytes()==expected
                assert control_bounds("tab-full:Created/Nested/Move source.md")
                key("ctrl+z");key("ctrl+s");assert nested.read_bytes()==source_bytes
                key("ctrl+y");key("ctrl+s");assert nested.read_bytes()==edited
                capture("native-file-move-nested.png")
                results.append("Plain Enter created the offered nested folder; Just once updated both actual link forms after the physical move, kept labels/CRLF and preserved editor undo/redo")

                command("Move current file to another folder");type_text("/");key("Return")
                control_bounds("move-links-dialog")
                assert source.read_bytes()==edited and not nested.exists()
                assert reference.read_bytes()==expected
                click_control("move-links-no",hold=.15)
                assert reference.read_bytes()==expected
                assert control_bounds("tab-full:Move source.md")
                results.append("Moving back to the vault root kept the current tab and Do not update left the original reference bytes intact")

                close_target=fixture/"Notes/Close prompt target.md";close_original=b"# Close prompt target\n"
                close_target.write_bytes(close_original)
                close_reference=fixture/"Close prompt reference.md";close_links=b"[[Notes/Close prompt target]]\n";close_reference.write_bytes(close_links)
                command("Refresh vault");pause();unchanged_title=xdo("getwindowname",window)
                file_menu(close_target.relative_to(fixture).as_posix());click_control("menu:Move file to...",hold=.15);type_text("Move destination");key("Return")
                control_bounds("move-links-dialog");click_control("move-links-close",hold=.15)
                assert control_bounds("move-links-dialog",required=False) is None
                assert not close_target.exists() and (destination/close_target.name).read_bytes()==close_original
                assert close_reference.read_bytes()==close_links and xdo("getwindowname",window)==unchanged_title
                results.append("The real dialog close control dismissed an inactive file's link prompt, preserving its completed move, current tab and unchanged references")


                command("Move current file to another folder");type_text("Shift parent/Nested");key("shift+Return")
                shifted=fixture/"Shift parent/Nested/Move source.md"
                assert shifted.read_bytes()==edited and not source.exists()
                assert control_bounds("move-links-dialog",required=False) is None
                command("Move current file to another folder");type_text("/");key("Return")
                assert source.read_bytes()==edited and not shifted.exists()
                assert reference.read_bytes()==expected
                results.append("Shift+Enter also created a nested destination and a subsequent root move retained the same bytes and intentionally unchanged reference")

                # Make the reference point to the current source again, then force a numbered move.
                reference.write_bytes(b"[[Move source|Alias]]\r\n[Label](Move%20source.md)\r\n")
                occupied=destination/"Move source.md";occupied.write_bytes(b"KEEP_OCCUPIED\n")
                command("Refresh vault");pause()
                command("Move current file to another folder");type_text("Move destination");key("Return")
                numbered=destination/"Move source 1.md"
                control_bounds("move-links-dialog")
                assert numbered.read_bytes()==edited and occupied.read_bytes()==b"KEEP_OCCUPIED\n"
                reference.write_bytes(b"EXTERNAL EDIT DURING PROMPT\n")
                click_control("move-links-once",hold=.15)
                assert reference.read_bytes()==b"EXTERNAL EDIT DURING PROMPT\n"
                assert numbered.read_bytes()==edited
                assert control_bounds("error-notice")
                capture("native-file-move-external-conflict.png")
                results.append("An occupied name produced a distinct numbered file and an external reference edit during the prompt survived without overwrite, with a visible review error")

                reference.write_bytes(b"[[Move destination/Move source 1|Alias]]\r\n")
                command("Refresh vault");pause();set_theme(False)
                command("Move current file to another folder");type_text("/");key("Return")
                control_bounds("move-links-dialog");inspect_links_dialog(False);capture("native-file-move-links-light.png")
                key("Return")
                root_numbered=fixture/"Move source 1.md"
                assert root_numbered.read_bytes()==edited
                assert reference.read_bytes()==b"[[Move source 1|Alias]]\r\n"
                sessions=[p for p in (base/"state/rustidian").glob("*.json") if any(path=="Move source 1.md" for path,_ in json.loads(p.read_text()).get("tabs",[]))]
                assert len(sessions)==1 and json.loads(sessions[0].read_text())["always_update_links"] is True
                # A different filename is needed so the short link actually requires updating.
                reference.write_bytes(b"[[Move source 1|Alias]]\r\n")
                (destination/"Move source 1.md").write_bytes(b"KEEP_SECOND_COLLISION\n")
                command("Refresh vault");pause()
                command("Move current file to another folder");type_text("Move destination");key("Return")
                final=destination/"Move source 1 1.md"
                assert final.read_bytes()==edited and not root_numbered.exists()
                assert control_bounds("move-links-dialog",required=False) is None
                assert reference.read_bytes()==b"[[Move destination/Move source 1 1|Alias]]\r\n"
                capture("native-file-move-automatic.png")
                results.append("Always update persisted per-vault and the next real filename-changing move updated links without another prompt, preserving both occupied files")

                # Unsafe typed paths remain an error inside the picker with no file mutation.
                command("Move current file to another folder");type_text("../Move outside");key("shift+Return")
                assert control_bounds("move-input") and final.read_bytes()==edited
                assert not (fixture.parent/"Move outside").exists()
                key("Escape");set_theme(True)
                assert occupied.read_bytes()==b"KEEP_OCCUPIED\n"
                assert (destination/"Move source 1.md").read_bytes()==b"KEEP_SECOND_COLLISION\n"
                assert all((fixture/name).read_bytes()==data for name,data in originals.items())
                results.append("An unsafe folder was rejected without moving or escaping the vault; all unrelated original notes and occupied destinations retained their exact bytes")

                command("Open settings");key("ctrl+a");type_text("automatically")
                click_control("settings-update-move-links",hold=.15)
                assert json.loads(sessions[0].read_text())["always_update_links"] is False
                click_control("settings-update-move-links",hold=.15)
                assert json.loads(sessions[0].read_text())["always_update_links"] is True
                key("Escape")
                second=base/"Move Second Vault";second.mkdir()
                second_home=second/"Home.md";second_home.write_bytes(b"# Separate vault\n")
                command("Open vault");key("ctrl+a");type_text(str(second));key("Return");title_contains("Move Second Vault")
                command("Open settings");key("ctrl+a");type_text("automatically")
                click_control("settings-update-move-links",hold=.15)
                second_sessions=[p for p in (base/"state/rustidian").glob("*.json") if p!=sessions[0] and json.loads(p.read_text()).get("always_update_links") is True]
                assert len(second_sessions)==1
                click_control("settings-update-move-links",hold=.15)
                assert json.loads(second_sessions[0].read_text())["always_update_links"] is False
                assert json.loads(sessions[0].read_text())["always_update_links"] is True
                key("Escape")
                command("Open vault");key("ctrl+a");type_text(str(fixture));key("Return");title_contains("Move source 1 1")
                assert second_home.read_bytes()==b"# Separate vault\n"
                assert json.loads(sessions[0].read_text())["always_update_links"] is True
                results.append("The automatic link switch could be reversed and saved; a separate vault started with its own disabled preference and switching back restored the original choice")

                xdo("windowsize",window,"860","560");pause()
                command("Move current file to another folder");control_bounds("move-input")
                left,top,right,bottom=control_bounds("move-picker")
                assert 0<=left<right<=860 and 0<=top<bottom<=560,(left,top,right,bottom)
                capture("native-file-move-compact.png",(860,560));key("Escape")
                xdo("windowsize",window,"1440","960");pause()
                results.append("The real Move picker fit the minimum window and dismissed without changing its file or active tab")

            def file_copy_checks():
                close_tabs()
                originals={p.relative_to(fixture).as_posix():p.read_bytes() for p in fixture.rglob("*.md")}
                sample=fixture/"Copy source.md";source=b"# Copy source\r\n\r\nExact bytes stay here.\r\n";sample.write_bytes(source)
                collision=fixture/"Copy source 1.md";collision.write_bytes(b"KEEP_COPY_COLLISION\n")
                command("Refresh vault");pause();open_note("Home")
                file_menu("Copy source.md");click_control("menu:Make a copy",hold=.15)
                copied=fixture/"Copy source 2.md"
                title_contains(copied.stem)
                assert control_bounds("dialog-input",required=False) is None
                assert copied.read_bytes()==source and sample.read_bytes()==source
                assert collision.read_bytes()==b"KEEP_COPY_COLLISION\n"
                assert control_bounds("tab-full:Home.md")
                capture("native-file-copy.png")
                results.append("The real Make a copy menu immediately opened the next numbered copy in a new tab, preserved CRLF/source bytes and skipped the occupied name without a filename dialog")

                command("Toggle pin")
                command("Make a copy of the current file")
                twice=fixture/"Copy source 2 1.md";title_contains(twice.stem)
                assert control_bounds("unpin-tab:Copy source 2.md")
                assert twice.read_bytes()==source and copied.read_bytes()==source
                assert control_bounds("tab-full:Copy source 2.md")
                results.append("The palette command copied the numbered copy using its complete basename, matching the reference and retaining both source tabs")

                image=fixture/"Copy image.png"
                Image.new("RGB",(24,16),(47,157,212)).save(image)
                image_bytes=image.read_bytes()
                command("Refresh vault");pause();key("ctrl+t")
                file_menu(image.name);click_control("menu:Make a copy",hold=.15)
                image_copy=fixture/"Copy image 1.png";title_contains(image_copy.stem)
                assert image_copy.read_bytes()==image_bytes and image.read_bytes()==image_bytes
                assert control_bounds("attachment-image")
                command("Make a copy of the current file")
                image_twice=fixture/"Copy image 1 1.png";title_contains(image_twice.stem)
                assert image_twice.read_bytes()==image_bytes
                capture("native-file-copy-image.png")
                results.append("Both the file menu and palette copied binary images exactly and opened the resulting real image views")

                close_tabs();command("New tab")
                command("Make a copy of the current file",available=False);key("Escape")
                assert all((fixture/name).read_bytes()==data for name,data in originals.items())
                assert sample.read_bytes()==source and copied.read_bytes()==source and twice.read_bytes()==source
                assert collision.read_bytes()==b"KEEP_COPY_COLLISION\n" and image.read_bytes()==image_bytes
                results.append("The copy command was unavailable without a file, while every original document and occupied destination kept its exact bytes")

            def tab_action_checks():
                nonlocal app, window
                close_tabs()
                originals={p.relative_to(fixture).as_posix():p.read_bytes() for p in fixture.rglob("*.md")}
                notes={"Pinned first.md":b"# Pinned first\n\n[[Pinned third]]\n", "Pinned second.md":b"# Pinned second\n\nKeep second.\n", "Pinned third.md":b"# Pinned third\n\nKeep third.\n", "Pinned fourth.md":b"# Pinned fourth\n\nKeep fourth.\n"}
                for name,data in notes.items():(fixture/name).write_bytes(data)
                command("Refresh vault");pause();open_note("Pinned first")
                key("ctrl+p");type_text("Save note");assert control_bounds("command:Save note");key("Escape")
                command("Toggle pin")
                control_bounds("unpin-tab:Pinned first.md")
                open_note("Pinned second")
                assert control_bounds("tab-full:Pinned first.md")
                assert control_bounds("tab-full:Pinned second.md")
                def pinned():
                    deadline=time.monotonic()+3
                    while time.monotonic()<deadline:
                        try:
                            controls=json.loads((base/"ui-probe.json").read_text())
                            return {name.removeprefix("tab-full:"):("unpin-tab:"+name.removeprefix("tab-full:")) in controls for name in controls if name.startswith("tab-full:")}
                        except (FileNotFoundError,json.JSONDecodeError):time.sleep(.02)
                    raise AssertionError("The actual tab controls did not become readable")
                expected_pins={"Pinned first.md":True,"Pinned second.md":False}
                assert pinned()==expected_pins
                xdo("key","--clearmodifiers","ctrl+q");app.wait(timeout=5)
                sessions=[p for p in (base/"state/rustidian").glob("*.json") if any(path=="Pinned first.md" for path,_ in json.loads(p.read_text()).get("tabs",[]))]
                assert len(sessions)==1
                session_path=sessions[0]
                def persisted_pins():
                    state=json.loads(session_path.read_text())
                    return {path:state["pinned"][index] for index,(path,_) in enumerate(state["tabs"])}
                assert persisted_pins()==expected_pins
                app=launch_app(launch_command);window=ready_window()
                assert pinned()==expected_pins
                capture("native-pinned-tabs.png")
                results.append("Pinning through the palette protected the first tab while quick switching opened the second and persisted its real pin state")

                click_control("tab-full:Pinned first.md");command("Reading view")
                left,top,right,bottom=control_bounds("reading-block:1")
                xdo("mousemove",str(round(left+min(20,(right-left)/2))),str(round((top+bottom)/2)),"mousedown","1")
                time.sleep(.15);xdo("mouseup","1");pause();title_contains("Pinned third")
                assert pinned()=={"Pinned first.md":True,"Pinned second.md":False,"Pinned third.md":False}
                assert (fixture/"Pinned first.md").read_bytes()==notes["Pinned first.md"]
                results.append("An ordinary held internal-link click from the pinned document opened the target while retaining the source tab and exact Markdown")

                command("Go to previous tab");title_contains("Pinned second")
                command("Go to next tab");title_contains("Pinned third")
                command("New tab");title_contains("New tab")
                command("Close current tab");title_contains("Pinned third")
                expected_pins=pinned()
                xdo("key","--clearmodifiers","ctrl+q");app.wait(timeout=5)
                assert persisted_pins()==expected_pins
                app=launch_app(launch_command);window=ready_window()
                assert pinned()==expected_pins
                assert pinned()["Pinned first.md"]
                control_bounds("unpin-tab:Pinned first.md")
                results.append("Palette tab cycling/new/close reused the keyboard behavior, and a real restart restored the pinned document")

                click_control("tab-full:Pinned first.md",button="3")
                click_control("tab-menu-pin",hold=.15)
                assert not pinned()["Pinned first.md"]
                click_control("tab-full:Pinned first.md",button="3")
                click_control("tab-menu-pin",hold=.15)
                assert pinned()["Pinned first.md"]
                click_control("unpin-tab:Pinned first.md",hold=.15)
                assert not pinned()["Pinned first.md"]
                click_control("tab-full:Pinned first.md");open_note("Pinned fourth")
                assert "Pinned first.md" not in pinned() and len(pinned())==3
                for dark in (False,True):
                    set_theme(dark)
                    command("Toggle pin")
                    click_control("tab-full:Pinned fourth.md",button="3")
                    capture("native-tab-menu-"+("dark" if dark else "light")+".png");key("Escape")
                    command("Toggle pin")
                results.append("The real tab menu and pin glyph both toggled pinning; unpinning restored ordinary navigation without creating files, with both-theme menu captures")

                command("Toggle pin");key("ctrl+w")
                title_contains("Pinned fourth")
                assert not pinned()["Pinned fourth.md"]
                key("ctrl+w")
                assert "Pinned fourth.md" not in pinned()
                other=base/"Pin other vault";other.mkdir();(other/"Pinned second.md").write_bytes(b"# Independent pin vault\n")
                command("Toggle pin")
                original_pin_state=pinned()
                command("Open vault");modified_shortcut(xdo,"a");paste_text(str(other),held=True);key("Return");title_contains(other.name)
                assert control_bounds("unpin-tab:Pinned second.md",required=False) is None
                command("Open vault");modified_shortcut(xdo,"a");paste_text(str(fixture),held=True);key("Return");title_contains(fixture.name)
                assert pinned()==original_pin_state
                assert all((fixture/name).read_bytes()==data for name,data in originals.items())
                assert all((fixture/name).read_bytes()==data for name,data in notes.items())
                assert (other/"Pinned second.md").read_bytes()==b"# Independent pin vault\n"
                results.append("Ctrl+W first unpinned then closed, matching the reference; separate vaults retained independent pin states and every original note byte")
                close_tabs()
                command("Toggle pin",available=False);key("Escape")
                command("Close current tab",available=False);key("Escape")
                command("New tab");command("Toggle pin")
                control_bounds("unpin-tab:")
                key("ctrl+w");title_contains("New tab")
                assert control_bounds("unpin-tab:",required=False) is None
                key("ctrl+w")
                assert all((fixture/name).read_bytes()==data for name,data in notes.items())
                results.append("With no tabs the palette hid inactive pin/close commands; a deliberately created blank tab could be pinned, unpinned and closed without creating a note")

            def settings_checks():
                nonlocal app, window
                note=fixture/"Notes/Settings controls.md"
                original="# Heading\n\nA wide paragraph. " + "Unchanged Markdown text. "*30 + "\n"
                note.write_text(original);open_note("Settings controls");command("Reading view")
                def state_path():
                    paths=[p for p in (base/"state/rustidian").glob("*.json") if any(path=="Notes/Settings controls.md" for path,_ in json.loads(p.read_text()).get("tabs",[]))]
                    assert len(paths)==1;return paths[0]
                def editor_settings():
                    command("Open settings");click_control("settings-page:Editor")
                def choose(control, label):
                    click_control(control)
                    option=control_bounds(control+":"+label)
                    assert option[3]-option[1]<=30,("Settings menu option should use compact type",option)
                    if control == "settings-editing-mode":capture("native-settings-editing-mode-menu.png")
                    click_control(control+":"+label)
                command("Open settings");click_control("settings-page:Editor")
                choose("settings-default-view","Reading view")
                first=control_bounds("settings-default-view");second=control_bounds("settings-editing-mode")
                pitch=(second[1]+second[3]-first[1]-first[3])/2
                assert 70<=pitch<=84, ("Reference Settings row pitch is about76px",pitch)
                selected=control_bounds("settings-page:Editor");other=control_bounds("settings-page:Files and links")
                xdo("mousemove","1300","900");pause()
                screen=ImageGrab.grab(xdisplay=display)
                colors=[screen.getpixel((round(r[0]+2),round((r[1]+r[3])/2)))[:3] for r in (selected,other)]
                assert sum(abs(a-b) for a,b in zip(*colors))>12,colors
                capture("native-settings-editor-dark.png")
                click_control("settings-inline-title");click_control("settings-readable-line-length")
                choose("settings-default-view","Reading view");choose("settings-editing-mode","Source mode")
                path=state_path();prefs=json.loads(path.read_text())["editor_preferences"]
                assert prefs==dict(reading_view=True,source_mode=True,show_mode=True,inline_title=False,readable_line_length=False),prefs
                key("Escape")
                assert not control_bounds("note-title",required=False)
                wide=control_bounds("document-content-width")[0]
                key("ctrl+t");open_note("Settings controls")
                assert control_bounds("view-mode:Reading")
                key("ctrl+e")
                assert control_bounds("view-mode:Source mode")
                editor_settings();click_control("settings-show-mode");key("Escape")
                assert not any(k.startswith("view-mode:") for k in json.loads((base/"ui-probe.json").read_text()))
                assert note.read_text()==original
                results.append("Real Editor switches hid the inline title, widened the document, selected Reading/Source for new tabs and hid the mode indicator without altering Markdown")
                xdo("key","--clearmodifiers","ctrl+q");app.wait(timeout=5)
                app=launch_app(launch_command);window=ready_window()
                assert not control_bounds("note-title",required=False)
                assert json.loads(path.read_text())["editor_preferences"]==dict(reading_view=True,source_mode=True,show_mode=False,inline_title=False,readable_line_length=False)
                assert control_bounds("document-content-width")[0]==wide
                editor_settings();held=base/"held-editor-settings.json";path.rename(held);path.mkdir()
                try:
                    click_control("settings-inline-title")
                    assert control_bounds("dialog-error")
                    capture("native-settings-editor-write-error.png")
                finally:path.rmdir();held.rename(path)
                assert json.loads(path.read_text())["editor_preferences"]["inline_title"] is False
                key("Escape");assert not control_bounds("note-title",required=False)
                editor_settings();click_control("settings-inline-title");click_control("settings-readable-line-length");click_control("settings-show-mode")
                choose("settings-default-view","Editing view");choose("settings-editing-mode","Live Preview")
                key("Escape");assert control_bounds("note-title")
                assert control_bounds("document-content-width")[0]<wide
                assert note.read_text()==original
                results.append("Preferences survived a real restart; a blocked state-file write visibly failed and rolled back, then retry restored the title and line width with exact note bytes intact")
                def appearance(label):
                    command("Open settings");click_control("settings-page:Appearance")
                    click_control("settings-dark");click_control("settings-theme:"+label);key("Escape")
                    assert json.loads(path.read_text())["dark"] is (label=="Dark")
                appearance("Light");editor_settings();capture("native-settings-editor-light.png");key("Escape");appearance("Dark")
                assert note.read_text()==original
                results.append("Actual Appearance dropdown clicks applied and saved both themes; the Editor page stayed readable and original Markdown stayed unchanged")
                size=round(control_bounds("document-font-size")[0])
                for _ in range(abs(size-18)):command("Decrease font size" if size>18 else "Increase font size")

            def font_controls_checks():
                nonlocal app,window
                close_tabs()
                note=fixture/"Font controls.md"
                original="# Font controls\n\nA paragraph with **bold**, a [link](Home.md) and `code`.\n\n- [ ] Font task\n\n| Name | Value |\n| --- | --- |\n| Keep | Text |\n\n```rust\nlet keep = 1;\n```\n"
                note.write_text(original);open_note("Font controls");command("Reading view")
                def font(expected):
                    deadline=time.monotonic()+3
                    while time.monotonic()<deadline:
                        actual=control_bounds("document-font-size")[0]
                        if actual==expected:return
                        time.sleep(.03)
                    raise AssertionError(("Rendered document font size",expected,actual))
                def title_width():
                    bounds=control_bounds("note-title");return bounds[2]-bounds[0]
                def step_to(size):
                    actual=round(control_bounds("document-font-size")[0])
                    for _ in range(abs(size-actual)):command("Increase font size" if size>actual else "Decrease font size")
                    font(size)
                font(18);baseline=title_width();chrome=control_bounds("tab-full:Font controls.md")
                paragraph=control_bounds("reading-block:1");height=paragraph[3]-paragraph[1]
                command("Increase font size");font(19)
                enlarged=control_bounds("reading-block:1")
                assert title_width()>baseline and enlarged[3]-enlarged[1]>height
                assert control_bounds("tab-full:Font controls.md")==chrome and note.read_text()==original
                command("Decrease font size");font(18);assert abs(title_width()-baseline)<.1
                results.append("Actual palette font commands changed Reading glyph and line bounds by one pixel, preserved the tab's bounds and left all Markdown bytes unchanged")
                for dark in (False,True):
                    set_theme(dark);command("Increase font size");font(19)
                    for mode in ("Reading view","Live preview","Source editing"):
                        command(mode);font(19);assert title_width()>baseline
                        capture("native-font-controls-"+("dark" if dark else "light")+"-"+mode.split()[0].lower()+".png")
                    command("Decrease font size");font(18)
                assert note.read_text()==original
                results.append("Both actual themes used the same enlarged document size in Reading, Live and Source while preserving note bytes and ordinary controls")
                command("Reading view");command("Open settings")
                click_control("settings-font-size",hold=.15)
                for _ in range(25):key("Right")
                key("Escape");font(30)
                high=title_width();command("Increase font size");font(30);assert title_width()==high
                capture("native-font-controls-maximum.png")
                command("Open settings")
                click_control("settings-font-size",hold=.15)
                for _ in range(25):key("Left")
                key("Escape");font(10)
                low=title_width();command("Decrease font size");font(10);assert title_width()==low and low<baseline<high
                capture("native-font-controls-minimum.png");step_to(18)
                results.append("The actual settings slider reached both observed 10/30 pixel boundaries and the corresponding palette commands could not step beyond either limit")
                command("Source editing");editor_end();type_text("UNDO_FONT_PROOF");key("ctrl+s")
                assert note.read_text()==original+"UNDO_FONT_PROOF"
                command("Increase font size");font(19);key("ctrl+z");key("ctrl+s");assert note.read_text()==original
                key("ctrl+y");key("ctrl+s");assert note.read_text()==original+"UNDO_FONT_PROOF"
                key("ctrl+z");key("ctrl+s");assert note.read_text()==original
                results.append("Source font changes returned editor focus and left existing text Undo/Redo intact, with exact saved bytes after each action")
                xdo("key","--clearmodifiers","ctrl+q");app.wait(timeout=5)
                app=launch_app(launch_command);window=ready_window();title_contains("Font controls");font(19)
                editor_end();type_text("RESTART_FONT_PROOF");key("ctrl+z");key("ctrl+s");assert note.read_text()==original
                results.append("A clean native restart restored document font size and mode; real focused editing still undid and saved unchanged bytes")
                other=base/"Font preferences vault";other.mkdir();(other/".obsidian").mkdir()
                appearance=b'{"baseFontSize":22,"unrelated":"keep"}';(other/".obsidian/appearance.json").write_bytes(appearance)
                (other/"Home.md").write_text("# Other font vault\n\nKeep.\n")
                def switch(root):
                    command("Open vault");modified_shortcut(xdo,"a");paste_text(str(root),held=True);key("Return");title_contains(root.name)
                switch(other);open_note("Home");font(22);command("Decrease font size");font(21)
                switch(fixture);title_contains("Font controls");font(19)
                switch(other);title_contains("Home");font(21)
                assert (other/".obsidian/appearance.json").read_bytes()==appearance
                switch(fixture);title_contains("Font controls");font(19)
                results.append("A new vault imported its guarded Obsidian font preference, retained its independent native setting across switches and preserved the original settings bytes")
                state_files=[p for p in (base/"state/rustidian").glob("*.json") if any(path=="Font controls.md" for path,_ in json.loads(p.read_text()).get("tabs",[]))]
                assert len(state_files)==1;state=state_files[0];held=base/"held-font-state.json";state.rename(held);state.mkdir()
                try:
                    command("Increase font size");font(19);assert title_width()>baseline
                    capture("native-font-controls-write-error.png")
                finally:state.rmdir();held.rename(state)
                command("Increase font size");font(20);command("Decrease font size");font(19)
                assert json.loads(state.read_text())["document_font_size"]==19
                results.append("An actual preference-write failure rolled font size back; retry after restoring the writable state succeeded without changing note contents")
                step_to(18);command("Reading view");click_control("task:0");key("ctrl+s")
                checked=original.replace("- [ ] Font task","- [x] Font task");assert note.read_text()==checked
                command("Source editing");key("ctrl+z");key("ctrl+s");assert note.read_text()==original
                command("Reading view");font(18);assert abs(title_width()-baseline)<.1
                assert (other/"Home.md").read_text()=="# Other font vault\n\nKeep.\n" and note.read_text()==original
                results.append("After preference changes and recovery, real task clicks and text Undo still worked and the default typography and every fixture were restored")

            def tab_drag_checks(*, keys_only=False):
                nonlocal app, window
                close_tabs()
                contents = {name: f"# Drag {name}\n\nKeep {name}.\n" for name in ("Alpha", "Beta", "Gamma", "Delta")}
                for name, source in contents.items():
                    (fixture / f"Drag {name}.md").write_text(source)
                command("Refresh vault")
                for index, name in enumerate(contents):
                    if index:
                        key("ctrl+t")
                    open_note("Drag " + name)
                def order():
                    controls = json.loads((base / "ui-probe.json").read_text())
                    return [name.removeprefix("tab-full:Drag ").removesuffix(".md") for name, rect in sorted(controls.items(), key=lambda item: item[1][0]) if name.startswith("tab-full:Drag ")]
                def keyboard():
                    names = order()
                    click_control("tab-full:Drag " + names[0] + ".md")
                    key("Right"); title_contains("Drag " + names[1])
                    key("Left"); title_contains("Drag " + names[0])
                    key("End"); title_contains("Drag " + names[-1])
                    key("Right"); title_contains("Drag " + names[0])
                    key("Home"); title_contains("Drag " + names[0])
                    command("Source editing"); editor_end(); key("Left"); key("Right")
                    title_contains("Drag " + names[0])
                    results.append("Left/Right and Home/End navigate the focused tab bar; arrows in the editor keep the same document selected")
                if keys_only:
                    keyboard()
                    return
                def drag(name, target, after=False, outside=False, cancel=False):
                    source = control_bounds(f"tab-full:Drag {name}.md")
                    destination = control_bounds(f"tab-full:Drag {target}.md")
                    start = (source[0] + 20, (source[1] + source[3]) / 2)
                    end = (destination[2] - 35 if after else destination[0] + 12, destination[3] + 180 if outside else start[1])
                    xdo("mousemove", "--sync", str(round(start[0])), str(round(start[1])), "mousedown", "1")
                    try:
                        for step in range(1, 21):
                            point = [round(start[axis] + (end[axis] - start[axis]) * step / 20) for axis in (0, 1)]
                            xdo("mousemove", "--sync", str(point[0]), str(point[1]))
                            time.sleep(.025)
                        if cancel:
                            key("Escape")
                    finally:
                        xdo("mouseup", "1")
                    pause()
                capture("native-tab-drag-before.png")
                assert order() == list(contents), order()
                drag("Alpha", "Gamma", after=True)
                assert order() == ["Beta", "Gamma", "Alpha", "Delta"], ("Dragging right did not reorder tabs", order())
                title_contains("Drag Alpha")
                drag("Delta", "Beta")
                assert order() == ["Delta", "Beta", "Gamma", "Alpha"], ("Dragging left did not reorder tabs", order())
                title_contains("Drag Delta")
                results.append("Real pointer drags reordered background tabs in both directions and kept the dragged document selected")
                click_control("tab-full:Drag Beta.md")
                command("Toggle pin")
                drag("Beta", "Alpha", after=True)
                assert order() == ["Delta", "Gamma", "Alpha", "Beta"], order()
                control_bounds("unpin-tab:Drag Beta.md")
                before = order()
                drag("Gamma", "Delta", outside=True)
                assert order() == before, "Dropping outside the tab strip must preserve order"
                drag("Gamma", "Delta", cancel=True)
                assert order() == before, "Escape must cancel the drag"
                results.append("Pinned tabs retained their pin after moving; outside drops and Escape cancelled without changing tab order")
                capture("native-tab-drag-after.png")
                click_control("tab-full:Drag Alpha.md")
                command("Source editing")
                editor_end(); type_text("UNDO_AFTER_DRAG"); key("ctrl+z"); key("ctrl+s")
                assert all((fixture / f"Drag {name}.md").read_text() == source for name, source in contents.items())
                xdo("key", "--clearmodifiers", "ctrl+q"); app.wait(timeout=10)
                app = launch_app(launch_command); window = ready_window()
                assert order() == before, (before, order())
                title_contains("Drag Alpha")
                control_bounds("unpin-tab:Drag Beta.md")
                click_control("tab-full:Drag Delta.md"); title_contains("Drag Delta"); key("ctrl+w")
                assert order() == ["Gamma", "Alpha", "Beta"], order()
                keyboard()
                click_control("tab-full:Drag Beta.md"); command("Toggle pin")
                results.append("Restart retained the reordered tabs, selected document and pin; switching, closing, Undo and exact saved note contents remained intact")

            def tab_restore_checks():
                close_tabs()
                originals={name:f"# Restore {name}\n\n{name.upper()}_ORIGINAL\n" for name in ("Alpha","Beta","Gamma")}
                paths={name:fixture/f"Restore {name}.md" for name in originals}
                for name,path in paths.items():path.write_text(originals[name])
                clean=base/"Tab history setup vault";clean.mkdir();(clean/"Home.md").write_text("# History setup\n")
                def switch(root):
                    command("Open vault");modified_shortcut(xdo,"a");paste_text(str(root),held=True);key("Return");title_contains(root.name)
                # Start with a fresh per-vault history even in the combined suite.
                switch(clean);switch(fixture)
                open_note("Restore Alpha");command("Source editing")
                def tabs():
                    controls=json.loads((base/"ui-probe.json").read_text())
                    return [name.removeprefix("tab-full:") for name,bounds in sorted(controls.items(),key=lambda item:item[1][0]) if name.startswith("tab-full:")]
                def expect(*names):
                    wanted=[f"Restore {name}.md" for name in names]
                    assert tabs()==wanted,(wanted,tabs())
                    assert control_bounds("tab-count")[0]==len(wanted)
                def mode(label):control_bounds("view-mode:"+label)
                def select(name):click_control(f"tab-full:Restore {name}.md");title_contains("Restore "+name)
                expect("Alpha");before=tabs()
                command("Undo close tab",available=False);key("Escape");key("ctrl+shift+t")
                assert tabs()==before;title_contains("Restore Alpha");mode("Source mode")
                results.append("Without closed history the palette hid Undo close tab and Ctrl+Shift+T kept the current view instead of creating a blank tab")
                editor_end();type_text("ALPHA_SAVED");key("ctrl+s")
                alpha_saved=originals["Alpha"]+"ALPHA_SAVED";assert paths["Alpha"].read_text()==alpha_saved
                key("ctrl+t");open_note("Restore Beta");command("Reading view")
                key("ctrl+t");open_note("Restore Gamma");command("Live preview");expect("Alpha","Beta","Gamma")
                select("Beta");key("ctrl+w");expect("Alpha","Gamma");title_contains("Restore Gamma")
                key("ctrl+shift+t");expect("Alpha","Gamma","Beta");title_contains("Restore Beta");mode("Reading")
                capture("native-tab-restore-middle.png")
                assert paths["Beta"].read_text()==originals["Beta"]
                results.append("Closing the middle view selected its right neighbor; one Ctrl+Shift+T appended and selected the same Reading view with unchanged file bytes")
                key("ctrl+w");select("Gamma");key("ctrl+w");expect("Alpha")
                key("ctrl+shift+t");expect("Alpha","Gamma");mode("Live preview")
                command("Undo close tab");expect("Alpha","Gamma","Beta");mode("Reading")
                results.append("Two closed views returned in last-closed order through both the shortcut and actual palette command, preserving separate modes")
                select("Alpha");key("ctrl+w");expect("Gamma","Beta")
                key("ctrl+shift+t");expect("Gamma","Beta","Alpha");mode("Source mode")
                type_text("_CURSOR");key("ctrl+s");assert paths["Alpha"].read_text()==alpha_saved+"_CURSOR"
                key("ctrl+z");key("ctrl+s");assert paths["Alpha"].read_text()==alpha_saved
                key("ctrl+z");key("ctrl+s");assert paths["Alpha"].read_text()==originals["Alpha"]
                key("ctrl+y");key("ctrl+s");assert paths["Alpha"].read_text()==alpha_saved
                results.append("Reopening an unchanged Source view retained its insertion point and prior editor Undo/Redo history")
                key("ctrl+w");external="# Restore Alpha\n\nEXTERNAL_SAVED_BYTES\n";paths["Alpha"].write_text(external)
                key("ctrl+shift+t");expect("Gamma","Beta","Alpha");mode("Source mode")
                editor_end();type_text("UNDO_FOCUS_PROOF");key("ctrl+z");key("ctrl+s");assert paths["Alpha"].read_text()==external
                key("ctrl+z");key("ctrl+s");assert paths["Alpha"].read_text()==external
                paths["Alpha"].write_text(originals["Alpha"]);time.sleep(.8)
                results.append("An externally changed closed file reopened from disk; focused typing/Undo worked and another Undo did not resurrect older file contents")
                key("ctrl+t");key("ctrl+w");expect("Gamma","Beta","Alpha")
                command("Undo close tab",available=False);key("Escape");key("ctrl+shift+t");expect("Gamma","Beta","Alpha")
                results.append("Closing a blank tab added no file history and an empty-history restore left all existing views unchanged")
                key("ctrl+w");key("ctrl+w");key("ctrl+w");assert not tabs()
                empty_new_tab=control_bounds("new-tab")
                key("ctrl+shift+t");expect("Gamma");mode("Live preview")
                # With no tabs, New tab sits where the first tab starts (after the strip's 4 px item gap), as in Obsidian, not one tab-width in.
                assert empty_new_tab[0] <= control_bounds("tab-full:Restore Gamma.md")[0] + 6,(empty_new_tab,control_bounds("tab-full:Restore Gamma.md"))
                key("ctrl+shift+t");expect("Gamma","Beta");mode("Reading")
                key("ctrl+shift+t");expect("Gamma","Beta","Alpha");mode("Source mode")
                capture("native-tab-restore-last.png")
                results.append("After the final document view closed, the three saved views still reopened in order without an extra blank tab")
                key("ctrl+w");removed=paths["Alpha"].with_suffix(".removed");paths["Alpha"].rename(removed)
                key("ctrl+shift+t");title_contains("New tab")
                assert not paths["Alpha"].exists() and removed.read_text()==originals["Alpha"]
                command("Undo close tab",available=False);key("Escape")
                capture("native-tab-restore-missing.png");removed.rename(paths["Alpha"]);time.sleep(.8)
                results.append("A missing closed file consumed only that history entry and opened an empty view without recreating or overwriting the file")
                key("ctrl+w");select("Gamma");command("Toggle pin")
                key("ctrl+w");expect("Gamma","Beta");title_contains("Restore Gamma")
                assert control_bounds("unpin-tab:Restore Gamma.md",required=False) is None
                command("Undo close tab",available=False);key("Escape")
                key("ctrl+w");expect("Beta");key("ctrl+shift+t");expect("Beta","Gamma")
                assert control_bounds("unpin-tab:Restore Gamma.md",required=False) is None
                results.append("The first close only unpinned a pinned view and added no history; the next close restored it as an ordinary unpinned view")
                for dark in (False,True):
                    set_theme(dark);key("ctrl+w");key("ctrl+shift+t");expect("Beta","Gamma")
                    capture("native-tab-restore-"+("dark" if dark else "light")+".png")
                assert all(path.read_text()==originals[name] for name,path in paths.items())
                assert (clean/"Home.md").read_text()=="# History setup\n"
                results.append("Both themes preserved actual reopen behavior, all fixture files and the separate setup vault")
                close_tabs()
                bulk={name:fixture/f"Bulk {name}.md" for name in "ABCD"}
                bulk_originals={name:f"# Bulk {name}\n\nBULK_{name}_ORIGINAL\n" for name in bulk}
                for name,path in bulk.items():path.write_text(bulk_originals[name])
                switch(clean);switch(fixture)
                def bulk_tabs():return [path.removeprefix("Bulk ").removesuffix(".md") for path in tabs()]
                def bulk_expect(*names):assert bulk_tabs()==list(names),(names,bulk_tabs())
                def bulk_select(name):click_control(f"tab-full:Bulk {name}.md");title_contains("Bulk "+name)
                open_note("Bulk A");command("Toggle pin")
                for name in "BCD":key("ctrl+t");open_note("Bulk "+name)
                bulk_expect("A","B","C","D");bulk_select("C");command("Close all other tabs")
                bulk_expect("A","C");title_contains("Bulk C");control_bounds("unpin-tab:Bulk A.md")
                command("Close all other tabs",available=False);key("Escape")
                key("ctrl+shift+t");bulk_expect("A","C","D")
                key("ctrl+shift+t");bulk_expect("A","C","D","B")
                capture("native-tab-restore-close-others.png")
                results.append("Close all other tabs kept the selected view and pinned A, hid its inapplicable command and restored the actual reference order D then B")
                bulk_select("A");command("Close all other tabs");bulk_expect("A");control_bounds("unpin-tab:Bulk A.md")
                command("Close all other tabs",available=False);key("Escape")
                for restored,expected in [("B",["A","B"]),("D",["A","B","D"]),("C",["A","B","D","C"])]:
                    command("Undo close tab");bulk_expect(*expected);title_contains("Bulk "+restored)
                results.append("A pinned selected view closed all other unpinned views, stayed pinned and restored them B, D, C in observed visual-close order")
                for name in "BCD":bulk_select(name);command("Toggle pin")
                command("Close all other tabs",available=False);key("Escape")
                key("ctrl+t");command("Close all other tabs",available=False);key("Escape");key("ctrl+w")
                bulk_expect("A","B","D","C")
                assert all(path.read_text()==bulk_originals[name] for name,path in bulk.items())
                assert all(path.read_text()==originals[name] for name,path in paths.items())
                for name in "ABDC":bulk_select(name);command("Toggle pin")
                results.append("All-pinned views and an active blank view with only pinned neighbors hid bulk close and preserved every original note")
                command("Open settings");click_control("settings-autosave");key("Escape")
                bulk_select("A");command("Source editing");editor_end();type_text("LOCAL_PENDING")
                assert bulk["A"].read_text()==bulk_originals["A"]
                external=bulk_originals["A"]+"EXTERNAL_PENDING";bulk["A"].write_text(external)
                bulk_select("C");command("Close all other tabs");control_bounds("pending-cancel")
                bulk_expect("A","B","D","C");assert bulk["A"].read_text()==external
                click_control("pending-cancel");bulk_expect("A","B","D","C")
                command("Close all other tabs");click_control("pending-save");control_bounds("pending-cancel")
                bulk_expect("A","B","D","C");assert bulk["A"].read_text()==external
                capture("native-tab-restore-bulk-conflict.png")
                results.append("An external conflict in a closing note kept all views open; Cancel and an actual failed Save click preserved the newer disk bytes")
                bulk["A"].write_text(bulk_originals["A"]);click_control("pending-save")
                bulk_expect("C");assert bulk["A"].read_text()==bulk_originals["A"]+"LOCAL_PENDING"
                for expected in [("C","D"),("C","D","B"),("C","D","B","A")]:key("ctrl+shift+t");bulk_expect(*expected)
                title_contains("Bulk A");key("ctrl+z");key("ctrl+s");assert bulk["A"].read_text()==bulk_originals["A"]
                results.append("Once the conflict cleared, Save and continue saved only the closing notes, kept C selected and retained the saved edit's Undo after ordered restoration")
                editor_end();type_text("DISCARDED_UNSAVED")
                discarded_external=bulk_originals["A"]+"KEEP_NEWER_DISK";bulk["A"].write_text(discarded_external)
                bulk_select("C");command("Close all other tabs");click_control("pending-discard")
                bulk_expect("C");assert bulk["A"].read_text()==discarded_external
                key("ctrl+shift+t");bulk_expect("C","A");title_contains("Bulk A")
                editor_end();type_text("UNDO_FOCUS_PROOF");key("ctrl+z");key("ctrl+s");assert bulk["A"].read_text()==discarded_external
                key("ctrl+z");key("ctrl+s");assert bulk["A"].read_text()==discarded_external
                bulk["A"].write_text(bulk_originals["A"]);time.sleep(.8)
                command("Open settings");click_control("settings-autosave");key("Escape")
                assert all(path.read_text()==bulk_originals[name] for name,path in bulk.items())
                results.append("Explicit Discard retained the newer disk file; restoring that view read fresh bytes and could not Undo the discarded contents back into it")

            def note_creator_checks():
                nonlocal app, window
                close_tabs()
                originals={p.relative_to(fixture).as_posix():p.read_bytes() for p in fixture.rglob("*.md")}
                origin=fixture/"Creator origin.md";origin.write_bytes(b"# Creator origin\n\nPreserve this note.\n")
                templates=fixture/"Creator templates";templates.mkdir()
                template=templates/"Unique template.md"
                source=b"# {{title}}\r\n{{date:YYYY-MM-DD}}\r\n{{content}}\r\n- [ ] Keep\r\n"
                template.write_bytes(source)
                output=fixture/"Creator output";output.mkdir()
                command("Refresh vault");pause();open_note("Creator origin");command("Source editing")
                key("ctrl+p");type_text("Save note");assert control_bounds("command:Save note");key("Escape")

                def configure(**values):
                    command("Open settings");type_text("unique note")
                    for name,value in values.items():setting("settings-unique-"+name,value)
                    key("Escape")

                configure(folder="Creator output",format="[Pair-]YYYYMMDDHHmm",template="Creator templates/Unique template.md")
                sessions=[p for p in (base/"state/rustidian").glob("*.json") if any(path=="Creator origin.md" for path,_ in json.loads(p.read_text()).get("tabs",[]))]
                assert len(sessions)==1
                session_path=sessions[0]
                expected={"folder":"Creator output","format":"[Pair-]YYYYMMDDHHmm","template":"Creator templates/Unique template.md"}
                assert json.loads(session_path.read_text())["note_aids"]["unique"]==expected
                for dark in (True,False):
                    set_theme(dark)
                    command("Open settings");type_text("unique note")
                    capture("native-unique-settings-"+("dark" if dark else "light")+".png");key("Escape")
                command("Switch appearance")
                xdo("windowsize",window,"700","610");pause()
                command("Open settings");type_text("unique note")
                for name in ("folder","format","template"):
                    rect=control_bounds("settings-unique-"+name)
                    assert 0<=rect[0]<rect[2]<=700 and 0<=rect[1]<rect[3]<=610,(name,rect)
                capture("native-unique-settings-compact.png");key("Escape")
                xdo("windowsize",window,"1440","960");pause()
                results.append("Unique-note settings persist exact folder/format/template values and remain visible in both themes and a compact window")

                original_origin=origin.read_bytes()
                before={p.relative_to(fixture).as_posix():p.read_bytes() for p in fixture.rglob("*.md")}
                editor_end();command("Unique note creator: Add unique internal link");key("ctrl+s")
                inserted=origin.read_bytes()
                assert inserted.startswith(original_origin+b"[[Pair-") and inserted.endswith(b"]]")
                assert {p.relative_to(fixture).as_posix() for p in fixture.rglob("*.md")}==set(before)
                key("Escape");assert control_bounds("editor-focused")
                type_text("|LABEL");key("ctrl+s")
                assert origin.read_bytes()==inserted[:-2]+b"|LABEL]]",origin.read_bytes()
                capture("native-unique-internal-link.png")
                key("ctrl+z");key("ctrl+s");assert origin.read_bytes()==inserted
                key("ctrl+z");key("ctrl+s");assert origin.read_bytes()==original_origin
                key("ctrl+y");key("ctrl+s");assert origin.read_bytes()==inserted
                key("ctrl+y");key("ctrl+s");assert origin.read_bytes()==inserted[:-2]+b"|LABEL]]"
                key("ctrl+z");key("ctrl+s");assert origin.read_bytes()==inserted
                key("ctrl+z");key("ctrl+s");assert origin.read_bytes()==original_origin
                command("Reading view")
                command("Unique note creator: Add unique internal link",available=False)
                key("Escape");assert origin.read_bytes()==original_origin
                command("Source editing")
                assert {p.relative_to(fixture).as_posix():p.read_bytes() for p in fixture.rglob("*.md")}==before
                results.append("Unique internal link inserted without creating a file, preserved single undo/redo, positioned the caret inside its brackets and stayed unavailable in Reading")

                while datetime.now().astimezone().second>45:time.sleep(.2)
                prefix=datetime.now().astimezone().strftime("Pair-%Y%m%d%H%M")
                command("Unique note creator: Create new unique note")
                first=output/(prefix+".md")
                today=datetime.now().astimezone().strftime("%Y-%m-%d")
                expected_bytes=("# "+first.stem+"\r\n"+today+"\r\n{{content}}\r\n- [ ] Keep\r\n").encode()
                assert first.read_bytes()==expected_bytes
                title_contains(first.stem)
                first.write_bytes(b"KEEP_UNIQUE_FIRST\n");time.sleep(.5)
                key("ctrl+t")
                assert datetime.now().astimezone().strftime("Pair-%Y%m%d%H%M")==prefix
                command("Unique note creator: Create new unique note")
                created=list(output.glob("*.md"));assert len(created)==2
                second=next(p for p in created if p!=first)
                assert first.read_bytes()==b"KEEP_UNIQUE_FIRST\n" and second.read_bytes().startswith(("# "+second.stem+"\r\n").encode())
                assert template.read_bytes()==source
                capture("native-unique-created.png")
                results.append("Actual same-prefix unique creation used the configured template, retained CRLF/unknown tokens and preserved the first file on collision")

                configure(format="[Literal]",template="")
                command("Unique note creator: Create new unique note")
                literal=output/"Literal.md";assert literal.read_bytes()==b""
                literal.write_bytes(b"KEEP_LITERAL\n");time.sleep(.4);key("ctrl+t")
                blank_title=xdo("getwindowname",window)
                before={p.relative_to(fixture).as_posix():p.read_bytes() for p in fixture.rglob("*.md")}
                command("Unique note creator: Create new unique note")
                assert control_bounds("error-notice")
                assert xdo("getwindowname",window)==blank_title
                assert {p.relative_to(fixture).as_posix():p.read_bytes() for p in fixture.rglob("*.md")}==before
                capture("native-unique-constant-error.png")
                click_control("error-dismiss",hold=.15)
                assert control_bounds("error-notice",required=False) is None
                for values in ({"format":"[Missing-]YYYYMMDDHHmm","template":"Creator templates/Missing"},{"folder":"Missing creator folder","template":""}):
                    configure(**values)
                    command("Unique note creator: Create new unique note")
                    assert control_bounds("error-notice")
                    assert xdo("getwindowname",window)==blank_title
                    assert {p.relative_to(fixture).as_posix():p.read_bytes() for p in fixture.rglob("*.md")}==before
                    click_control("error-dismiss",hold=.15)
                    assert control_bounds("error-notice",required=False) is None
                assert not (fixture/"Missing creator folder").exists()
                results.append("Constant-name collisions and missing templates/folders gave visible errors without opening or overwriting a file")

                configure(**expected)
                other=base/"Creators other vault";other.mkdir();(other/"Home.md").write_bytes(b"# Other creator vault\n")
                def switch(root):
                    command("Open vault");modified_shortcut(xdo,"a");paste_text(str(root),held=True);key("Return");title_contains(root.name)
                switch(other)
                command("Unique note creator: Create new unique note")
                other_notes=[p for p in other.glob("*.md") if p.name!="Home.md"]
                assert len(other_notes)==1 and other_notes[0].read_bytes()==b""
                assert not (other/"Creator output").exists()
                switch(fixture)
                assert json.loads(session_path.read_text())["note_aids"]["unique"]==expected
                app.terminate();app.wait(timeout=5);app=launch_app(launch_command);window=ready_window()
                assert json.loads(session_path.read_text())["note_aids"]["unique"]==expected
                previous=set(output.glob("*.md"))
                command("Unique note creator: Create new unique note")
                added=set(output.glob("*.md"))-previous;assert len(added)==1
                assert next(iter(added)).read_bytes().startswith(b"# Pair-")
                assert (other/"Home.md").read_bytes()==b"# Other creator vault\n"
                results.append("A native restart and vault switch retained the original unique-note preferences while a second vault used independent defaults")

                random_vault=base/"Random creator vault";random_vault.mkdir()
                for name in ("One.md","Two.md","Nested/Three.md","__pycache__/Skip.md",".hidden/Skip.md"):
                    path=random_vault/name;path.parent.mkdir(exist_ok=True);path.write_bytes(("# "+name+"\nKeep this file.\n").encode())
                switch(random_vault)
                before={p.relative_to(random_vault).as_posix():p.read_bytes() for p in random_vault.rglob("*.md")}
                valid={name+" — "+random_vault.name+" — Rustidian" for name in ("One","Two","Three")}
                for _ in range(3):
                    key("ctrl+t");blank_title=xdo("getwindowname",window)
                    command("Random note: Open random note")
                    assert xdo("getwindowname",window)!=blank_title and xdo("getwindowname",window) in valid
                assert {p.relative_to(random_vault).as_posix():p.read_bytes() for p in random_vault.rglob("*.md")}==before
                capture("native-random-note.png")
                results.append("Random note opened actual Markdown files from three blank-tab origins and left every document byte unchanged")

                empty=base/"Empty creator vault";empty.mkdir();switch(empty)
                blank_title=xdo("getwindowname",window)
                command("Random note: Open random note")
                assert xdo("getwindowname",window)==blank_title and not list(empty.iterdir())
                switch(fixture)
                assert all((fixture/name).read_bytes()==data for name,data in originals.items())
                assert origin.read_bytes()==b"# Creator origin\n\nPreserve this note.\n" and template.read_bytes()==source
                assert first.read_bytes()==b"KEEP_UNIQUE_FIRST\n" and literal.read_bytes()==b"KEEP_LITERAL\n"
                results.append("An empty-vault random command created no files and opened no note; all original notes/templates and collision contents survived")

            def note_aids_checks():
                open_note("Home")
                command("Source editing")
                key("ctrl+p")
                type_text("Save note")
                assert control_bounds("command:Save note"), "The existing-command positive control must work"
                key("Escape")
                key("ctrl+p")
                type_text("Daily notes: Open today's daily note")
                click_control("command:Daily notes: Open today's daily note", hold=.15)
                today = datetime.now().astimezone().strftime("%Y-%m-%d")
                title_contains(today)
                daily = fixture / (today + ".md")
                assert daily.read_bytes() == b"", "Unconfigured daily notes must start empty, as in Obsidian"
                click_control("button:Open today's daily note", hold=.15)
                assert daily.read_bytes() == b"" and len(list(fixture.glob(today + "*.md"))) == 1
                results.append("Default daily note opened from a real command and reopened without duplication or content changes")

                templates = fixture / "Templates"
                templates.mkdir(exist_ok=True)
                meeting_source = "Hello {{title}}\n{{date:YYYY-MM-DD}} {{time:HH:mm}}\n{{content}}\n"
                (templates / "Meeting.md").write_text(meeting_source)
                (templates / "Daily.md").write_text("# {{title}}\n\n- [ ] Review {{date}}\n")
                target = fixture / "Notes/Template target.md"
                original = "# Before\n\n"
                target.write_text(original)
                command("Refresh vault")
                pause()

                def settings_page(query):
                    command("Open settings")
                    type_text(query)

                def wait_text(path, expected):
                    deadline = time.monotonic() + 4
                    while time.monotonic() < deadline:
                        if path.read_text() == expected:
                            return
                        pause()
                    raise AssertionError((path.name, path.read_text(), expected))

                settings_page("templates")
                setting("settings-template-folder", "Templates")
                setting("settings-template-date", "DD/MM/YYYY")
                setting("settings-template-time", "HH:mm")
                capture("native-note-aids-settings.png")
                key("Escape")
                states = [(p, json.loads(p.read_text())) for p in (base / "state/rustidian").glob("*.json")]
                matching = [(p, state) for p, state in states if state.get("note_aids", {}).get("templates", {}).get("folder") == "Templates" and any(path == today + ".md" for path, _ in state["tabs"])]
                assert len(matching) == 1, "Identify the actual vault's saved settings before checking persistence"
                session_path = matching[0][0]
                assert matching[0][1]["note_aids"]["templates"]["dateFormat"] == "DD/MM/YYYY"

                open_note("Template target")
                command("Source editing")
                editor_end()
                command("Templates: Insert template")
                assert control_bounds("template:Templates/Meeting.md")
                capture("native-template-chooser.png")
                key("Escape")
                assert target.read_text() == original
                paste_text("Cancelled chooser typing", held=True)
                key("ctrl+s")
                wait_text(target, original + "Cancelled chooser typing")
                key("ctrl+z")
                key("ctrl+s")
                wait_text(target, original)
                editor_end()
                click_control("button:Insert template", hold=.15)
                type_text("Mtg")
                assert control_bounds("template:Templates/Daily.md", required=False) is None
                click_control("template:Templates/Meeting.md", hold=.15)
                key("ctrl+s")
                inserted = target.read_text()
                assert inserted.startswith(original + "Hello Template target\n"), inserted
                assert "{{content}}" in inserted and "{{date" not in inserted and "{{time" not in inserted
                key("ctrl+z")
                key("ctrl+s")
                wait_text(target, original)
                key("ctrl+shift+z")
                key("ctrl+s")
                wait_text(target, inserted)
                assert (templates / "Meeting.md").read_text() == meeting_source
                capture("native-template-insert.png")
                results.append("Template chooser cancellation preserved bytes; selected title/date/time expanded at the caret with single undo/redo and unchanged template source")

                editor_end()
                paste_text("\nDATE=", held=True)
                command("Templates: Insert current date")
                key("ctrl+s")
                assert target.read_text().endswith("DATE=" + datetime.now().astimezone().strftime("%d/%m/%Y"))
                editor_end()
                paste_text(" TIME=", held=True)
                command("Templates: Insert current time")
                key("ctrl+s")
                tail = target.read_text().rsplit(" TIME=", 1)[1]
                assert len(tail) == 5 and tail[2] == ":" and tail.replace(":", "").isdigit(), tail
                results.append("Date and time commands used the persisted template formats and the real editor insertion point")

                settings_page("daily")
                setting("settings-daily-folder", "Journal")
                setting("settings-daily-format", "YYYY/MMMM/YYYY-MMM-DD")
                setting("settings-daily-template", "Templates/Daily")
                capture("native-daily-settings.png")
                key("Escape")
                now = datetime.now().astimezone()
                relative = "Journal/" + now.strftime("%Y/%B/%Y-%b-%d") + ".md"
                command("Daily notes: Open today's daily note")
                title_contains(now.strftime("%Y-%b-%d"))
                journal = fixture / relative
                assert journal.read_text() == "# " + journal.stem + "\n\n- [ ] Review " + now.strftime("%d/%m/%Y") + "\n"
                command("Source editing")
                editor_end()
                paste_text("\nKept daily text", held=True)
                key("ctrl+s")
                kept = journal.read_text()
                command("Daily notes: Open today's daily note")
                assert journal.read_text() == kept
                capture("native-daily-note.png")
                results.append("Configured daily notes created nested folders and expanded the selected template once; reopening preserved edited content")

                settings_page("daily")
                setting("settings-daily-folder", "Missing daily")
                setting("settings-daily-template", "Templates/Missing")
                key("Escape")
                previous_title = xdo("getwindowname", window)
                command("Daily notes: Open today's daily note")
                assert xdo("getwindowname", window) == previous_title and not (fixture / "Missing daily").exists()
                assert journal.read_text() == kept
                settings_page("templates")
                setting("settings-template-folder", "Missing templates")
                key("Escape")
                command("Templates: Insert template")
                assert control_bounds("template-error")
                key("Return")
                assert control_bounds("template-error") and journal.read_text() == kept
                key("Escape")
                settings_page("templates")
                setting("settings-template-folder", "Templates")
                key("Escape")
                results.append("Missing daily/template files reported errors, retained the active note and did not create empty directories or insert unrelated text")

                open_note("Template target")
                command("Source editing")
                editor_end()
                command("Templates: Insert template")
                target.write_text("# External replacement\nKeep this source.\n")
                time.sleep(1)
                click_control("template:Templates/Meeting.md", hold=.15)
                assert control_bounds("dialog-error")
                assert target.read_text() == "# External replacement\nKeep this source.\n"
                capture("native-template-changed-note.png")
                key("Escape")
                results.append("A note changed externally while the chooser was open rejected the stale insertion point and preserved the replacement")

                other = base / "Aids other vault"
                other.mkdir(exist_ok=True)
                (other / "Home.md").write_text("# Other vault\n")
                command("Open vault")
                key("ctrl+a")
                type_text(str(other))
                key("Return")
                title_contains(other.name)
                settings_page("templates")
                setting("settings-template-folder", "Other templates")
                key("Escape")
                command("Open vault")
                key("ctrl+a")
                type_text(str(fixture))
                key("Return")
                title_contains(fixture.name)
                settings_page("templates")
                assert json.loads(session_path.read_text())["note_aids"]["templates"]["folder"] == "Templates"
                key("Escape")
                command("Templates: Insert template")
                assert control_bounds("template:Templates/Meeting.md")
                key("Escape")
                assert (other / "Home.md").read_text() == "# Other vault\n"
                assert (templates / "Meeting.md").read_text() == meeting_source
                results.append("Switching vaults restored independent template settings and left both vaults' source notes unchanged")

            def workspace_checks():
                nonlocal app, window
                close_tabs()
                first = fixture / "Workspace A.md"
                second = fixture / "Workspace B.md"
                first_source = "# Workspace A\n\nFirst saved layout.\n"
                second_source = "# Workspace B\n\nSecond saved layout.\n"
                first.write_text(first_source)
                second.write_text(second_source)
                command("Refresh vault")
                pause()
                open_note("Workspace A")
                command("Source editing")
                key("ctrl+p")
                type_text("Save note")
                assert control_bounds("command:Save note"), "The existing-command positive control must work"
                key("Escape")
                storage = base / "state/rustidian/workspaces"
                before_paths = set(storage.glob("*.json"))

                def manager():
                    command("Workspaces: Manage workspace layouts")
                    bounds = control_bounds("workspace-name")
                    assert 0 <= bounds[0] < bounds[2] <= 1440 and 0 <= bounds[1] < bounds[3] <= 960, bounds
                    assert control_bounds("workspace-close")
                    return bounds

                def save_layout(name, keyboard=False):
                    click_control("workspace-name", hold=.15)
                    modified_shortcut(xdo, "a")
                    paste_text(name, held=True)
                    if keyboard:
                        key("Return")
                    else:
                        click_control("workspace-save", hold=.15)
                    assert control_bounds("workspace-load:" + name)

                def stored():
                    return json.loads(layout_path.read_text())

                def saved(name):
                    return next(entry["layout"] for entry in stored()["entries"] if entry["name"] == name)

                def tab_order():
                    controls = json.loads((base / "ui-probe.json").read_text())
                    return [name[len("tab-full:"):] for name, _ in sorted(((name, rect) for name, rect in controls.items() if name.startswith("tab-full:")), key=lambda item:item[1][0])]

                def assert_layout(expected):
                    names = [tab["path"] for tab in expected["tabs"]]
                    if tab_order() != names:
                        (reports / "workspace-load-failure.json").write_text(json.dumps({"expected":expected,"actual_tabs":tab_order(),"stored":stored(),"controls":json.loads((base/"ui-probe.json").read_text())},indent=2)+"\n")
                    assert tab_order() == names, (tab_order(), names)
                    title_contains(Path(names[expected["active"]]).stem)
                    for saved_tab in expected["tabs"]:
                        actual_pin=control_bounds("unpin-tab:"+saved_tab["path"],required=False) is not None
                        assert actual_pin == saved_tab["pinned"], (saved_tab,actual_pin)
                    for side, key_name, width in (("left", "explorer", "left_width"), ("right", "details", "right_width")):
                        bounds = control_bounds("sidebar:" + key_name, required=False)
                        assert (bounds is not None) == expected[side], (side, bounds, expected)
                        if bounds:
                            assert abs((bounds[2] - bounds[0]) - expected[width]) <= 2.5, (bounds, expected[width])
                    is_read = expected["tabs"][expected["active"]]["view"] == "Read"
                    assert (control_bounds("editor", required=False) is None) == is_read

                def load(name):
                    manager()
                    capture("native-workspaces-load-" + name + "-before.png")
                    (reports/ ("workspace-load-"+name+"-before.json")).write_text(json.dumps({"stored":stored(),"controls":json.loads((base/"ui-probe.json").read_text())},indent=2)+"\n")
                    target_bounds=control_bounds("workspace-load:"+name)
                    modal_bounds=control_bounds("workspace-manager")
                    assert modal_bounds[0]<=target_bounds[0]<target_bounds[2]<=modal_bounds[2] and modal_bounds[1]<=target_bounds[1]<target_bounds[3]<=modal_bounds[3], (name,target_bounds,modal_bounds)
                    click_control("workspace-load:" + name, hold=.15)
                    assert control_bounds("workspace-name", required=False) is None

                manager()
                paste_text("Cancelled layout", held=True)
                capture("native-workspaces-empty.png")
                key("Escape")
                assert set(storage.glob("*.json")) == before_paths
                control_bounds("editor-focused")
                results.append("Workspace manager command opened; cancelling a name created no metadata and returned keyboard focus")

                if control_bounds("sidebar:explorer", required=False) is None:
                    command("Toggle left sidebar")
                if control_bounds("sidebar:details", required=False) is None:
                    command("Toggle right sidebar")
                left = control_bounds("sidebar:explorer")
                xdo("mousemove", str(round(left[2]-1)), "440", "mousedown", "1")
                time.sleep(.15)
                xdo("mousemove", str(round(left[2]+32)), "440")
                time.sleep(.15)
                xdo("mouseup", "1")
                pause()
                resized = control_bounds("sidebar:explorer")
                assert abs((resized[2]-resized[0]) - (left[2]-left[0])) >= 20, (left,resized)
                command("Reading view")
                manager()
                save_layout("Writing", keyboard=True)
                new_paths = set(storage.glob("*.json")) - before_paths
                assert len(new_paths) == 1
                layout_path = new_paths.pop()
                writing = saved("Writing")
                assert writing["tabs"] == [{"path":"Workspace A.md","view":"Read","editing_view":"Source","pinned":False}], writing["tabs"]
                capture("native-workspaces-dark.png")
                original_metadata = layout_path.read_bytes()
                paste_text("Unsaved name", held=True)
                key("Escape")
                assert layout_path.read_bytes() == original_metadata
                command("Switch appearance")
                manager()
                capture("native-workspaces-light.png")
                xdo("windowsize", window, "860", "560")
                pause()
                small = control_bounds("workspace-name")
                assert 0 <= small[0] < small[2] <= 860 and 0 <= small[1] < small[3] <= 560, small
                assert control_bounds("workspace-save") and control_bounds("workspace-load:Writing")
                capture("native-workspaces-compact.png", size=(860,560))
                click_control("workspace-close",hold=.15)
                xdo("windowsize", window, "1440", "960")
                pause()
                command("Switch appearance")
                results.append("Named layout saved a resized sidebar and Reading/Source modes; cancel preserved bytes and manager controls fit both themes and compact viewport")

                key("ctrl+t")
                open_note("Workspace B")
                command("Source editing")
                command("Toggle left sidebar")
                right = control_bounds("sidebar:details")
                xdo("mousemove", str(round(right[0]+1)), "440", "mousedown", "1")
                time.sleep(.15)
                xdo("mousemove", str(round(right[0]-32)), "440")
                time.sleep(.15)
                xdo("mouseup", "1")
                pause()
                resized_right = control_bounds("sidebar:details")
                (reports/"workspace-right-resize.json").write_text(json.dumps({"before":right,"after":resized_right},indent=2)+"\n")
                assert abs((resized_right[2]-resized_right[0]) - (right[2]-right[0])) >= 20, (right,resized_right)
                manager();save_layout("Reading");key("Escape")
                reading = saved("Reading")
                assert [tab["path"] for tab in reading["tabs"]] == ["Workspace A.md","Workspace B.md"]
                assert reading["active"] == 1 and not reading["left"] and reading["right"]
                load("Writing");assert_layout(writing)
                load("Reading");assert_layout(reading)
                click_control("tab-full:Workspace A.md", hold=.15)
                title_contains("Workspace A")
                assert control_bounds("editor", required=False) is None
                capture("native-workspaces-restored-reading.png")
                click_control("tab-full:Workspace B.md", hold=.15)
                title_contains("Workspace B")
                # Clicking a tab retains tab-bar focus for Left/Right navigation.
                # Focus the restored document before checking its selection.
                click_control("editor")
                control_bounds("editor-focused")
                modified_shortcut(xdo,"a");modified_shortcut(xdo,"c")
                assert xdo("getwindowname",window).startswith("Workspace B")
                copied = subprocess.check_output(["xclip","-selection","clipboard","-out"],env=environment,text=True)
                assert copied == second_source, repr(copied)
                key("End")
                assert first.read_text() == first_source and second.read_text() == second_source
                results.append("Both layouts restored tab order, active file, per-tab modes and sidebar sizes; actual restored tabs displayed and copied the correct document")

                command("Toggle pin")
                manager();save_layout("Reading");key("Escape")
                pinned_reading=saved("Reading")
                assert [tab["pinned"] for tab in pinned_reading["tabs"]]==[False,True]
                load("Writing");assert_layout(writing)
                load("Reading");assert_layout(pinned_reading)
                control_bounds("unpin-tab:Workspace B.md")
                capture("native-workspaces-pinned-layout.png")
                command("Toggle pin")
                manager();save_layout("Reading");key("Escape")
                reading=saved("Reading")
                assert [tab["pinned"] for tab in reading["tabs"]]==[False,False]
                assert first.read_text()==first_source and second.read_text()==second_source
                (reports/"workspace-pinned-layout.json").write_text(json.dumps({"pinned_layout":pinned_reading,"unpinned_layout":reading},indent=2)+"\n")
                results.append("Named layouts restored the actual pinned and unpinned tab controls; saving the reversed pin choice preserved both original note bytes")

                key("ctrl+w")
                command("Workspaces: Save layout")
                assert control_bounds("workspace-name", required=False) is None
                assert len(stored()["entries"]) == 2
                updated = saved("Reading")
                assert [tab["path"] for tab in updated["tabs"]] == ["Workspace A.md"]
                command("Workspaces: Load workspace layout")
                paste_text("Wrtng",held=True)
                assert control_bounds("workspace-load:Reading",required=False) is None
                key("Return");assert_layout(writing)
                command("Workspaces: Save and load another layout")
                assert control_bounds("workspace-load:Reading")
                key("Escape");assert_layout(writing)
                load("Reading");assert_layout(updated)
                results.append("All four workspace commands work: active overwrite keeps one entry, fuzzy load selects it and cancelled save-and-load retains current tabs")

                xdo("key","--clearmodifiers","ctrl+q");app.wait(timeout=5)
                app=launch_app(launch_command);window=ready_window()
                title_contains("Workspace A")
                manager()
                assert control_bounds("workspace-load:Writing") and control_bounds("workspace-load:Reading")
                key("Escape")
                other = base / "Workspace other vault"
                other.mkdir()
                (other / "Home.md").write_text("# Other workspace vault\n")
                command("Open vault");modified_shortcut(xdo,"a");paste_text(str(other),held=True);key("Return")
                title_contains(other.name)
                manager()
                assert control_bounds("workspace-load:Writing",required=False) is None
                save_layout("Other vault")
                assert len(list(storage.glob("*.json"))) == len(before_paths)+2
                key("Escape")
                command("Open vault");modified_shortcut(xdo,"a");paste_text(str(fixture),held=True);key("Return")
                title_contains(fixture.name)
                manager()
                assert control_bounds("workspace-load:Writing") and control_bounds("workspace-load:Other vault",required=False) is None
                key("Escape")
                results.append("Named layouts and active files survived actual restart; another vault retained an independent layout list")

                # Missing and corrupt metadata must retain the current real document.
                original_metadata = layout_path.read_bytes()
                missing = stored()
                missing["entries"][0]["layout"]["tabs"][0]["path"] = "Missing workspace note.md"
                layout_path.write_text(json.dumps(missing))
                manager();click_control("workspace-load:Writing",hold=.15)
                assert control_bounds("workspace-error")
                title_contains("Workspace A")
                assert first.read_text() == first_source
                key("Escape")
                layout_path.write_text("{unfinished")
                manager()
                assert control_bounds("workspace-error")
                assert layout_path.read_text() == "{unfinished"
                key("Escape")
                layout_path.write_bytes(original_metadata)
                load("Writing");assert_layout(writing)
                results.append("Missing workspace files and corrupt metadata reported errors while retaining current tabs, saved entries and original note bytes")

                command("Source editing")
                command("Open settings");type_text("autosave")
                session_files=[p for p in (base/"state/rustidian").glob("*.json") if any(path=="Workspace A.md" for path,_ in json.loads(p.read_text()).get("tabs",[]))]
                assert len(session_files)==1
                session_path=session_files[0]
                if json.loads(session_path.read_text())["autosave_enabled"]:
                    click_control("settings-autosave",hold=.15)
                wait_autosave(base/"state",False,session_path=session_path)
                key("Escape");editor_end();paste_text("UNSAVED_WORKSPACE",held=True)
                assert first.read_text()==first_source
                manager();click_control("workspace-load:Reading",hold=.15)
                click_control("pending-cancel",hold=.15)
                control_bounds("editor-focused")
                assert first.read_text()==first_source
                manager();click_control("workspace-load:Reading",hold=.15)
                click_control("pending-save",hold=.15)
                assert first.read_text()==first_source+"UNSAVED_WORKSPACE"
                assert_layout(updated)
                first.write_text(first_source);time.sleep(.6)
                command("Source editing");editor_end();paste_text("CONFLICTED_WORKSPACE",held=True)
                first.write_text("# External workspace version\n");time.sleep(.7)
                manager();click_control("workspace-load:Writing",hold=.15)
                click_control("pending-save",hold=.15)
                assert control_bounds("pending-cancel")
                assert first.read_text()=="# External workspace version\n"
                click_control("pending-cancel",hold=.15)
                command("Save a copy");modified_shortcut(xdo,"a");paste_text("Workspace recovered.md",held=True);key("Return")
                assert "CONFLICTED_WORKSPACE" in (fixture/"Workspace recovered.md").read_text()
                assert first.read_text()=="# External workspace version\n"
                first.write_text(first_source);time.sleep(.6)
                load("Writing")
                assert control_bounds("pending-discard")
                assert "CONFLICTED_WORKSPACE" in (fixture/"Workspace recovered.md").read_text()
                click_control("pending-discard",hold=.15)
                assert_layout(writing)
                command("Open settings");type_text("autosave")
                click_control("settings-autosave",hold=.15)
                wait_autosave(base/"state",True,session_path=session_path)
                key("Escape")
                results.append("Cancelled dirty load kept unsaved editing; Save & continue retained edits, while a conflict refused overwrite and allowed separate recovery")

                manager()
                click_control("workspace-delete:Writing",hold=.15)
                click_control("workspace-delete:Reading",hold=.15)
                assert stored()["entries"]==[] and stored()["active"] is None
                key("Escape")
                assert first.read_text()==first_source and second.read_text()==second_source
                assert (other/"Home.md").read_text()=="# Other workspace vault\n"
                results.append("Deleting saved layouts removed only metadata and preserved every original workspace document")

            def recovery_checks():
                nonlocal app, window
                close_tabs()
                originals = {p.relative_to(fixture).as_posix():hashlib.sha256(p.read_bytes()).hexdigest() for p in fixture.rglob("*.md")}
                target = fixture / "Recovery target.md"
                second = fixture / "Recovery second.md"
                original = "# Original\r\n\r\nسلام\r\n- [x] Keep this\r\n".encode()
                changed = "# Changed\n\nسلام\n- [ ] New version\n".encode()
                target.write_bytes(original)
                second.write_text("# Second recovery note\n")
                command("Refresh vault");pause();open_note("Recovery target")
                command("Source editing")
                key("ctrl+p");type_text("Save note")
                assert control_bounds("command:Save note"), "Existing commands must work before checking the missing one"
                key("Escape")
                command("Open settings");type_text("file recovery")
                left,top,right,bottom=control_bounds("settings-recovery-interval")
                xdo("mousemove",str(round((left+right)/2)),str(round((top+bottom)/2)),"click","--repeat","2","--delay","70","1")
                key("ctrl+a");type_text("0");key("Return");key("Tab")
                sessions=[p for p in (base/"state/rustidian").glob("*.json") if any(path=="Recovery target.md" for path,_ in json.loads(p.read_text()).get("tabs",[]))]
                assert len(sessions)==1
                session_path=sessions[0]
                assert json.loads(session_path.read_text())["recovery_config"]["intervalMinutes"]==0
                capture("native-recovery-settings.png")
                key("Escape")

                def versions(minimum=1):
                    deadline=time.monotonic()+8
                    while time.monotonic()<deadline:
                        controls=json.loads((base/"ui-probe.json").read_text())
                        ids=sorted(int(name.split(":",1)[1]) for name in controls if name.startswith("recovery-version:"))
                        if len(ids)>=minimum and "recovery-ready" in controls:
                            geometry=dict(line.split("=",1) for line in xdo("getwindowgeometry","--shell",window).splitlines() if "=" in line)
                            dialog=control_bounds("recovery-dialog")
                            assert 0<=dialog[0]<dialog[2]<=int(geometry["WIDTH"]) and 0<=dialog[1]<dialog[3]<=int(geometry["HEIGHT"]),dialog
                            for name in ("recovery-files","recovery-close","recovery-copy","recovery-restore","recovery-changes"):
                                rect=control_bounds(name)
                                assert dialog[0]<=rect[0]<rect[2]<=dialog[2] and dialog[1]<=rect[1]<rect[3]<=dialog[3],(name,rect,dialog)
                            return ids
                        time.sleep(.1)
                    raise AssertionError(("Missing stored recovery versions",minimum,controls))

                def history(minimum=1):
                    command("File recovery: Open local history")
                    assert control_bounds("recovery-files")
                    return versions(minimum)

                def copy_snapshot():
                    control_bounds("recovery-ready")
                    before = control_bounds("recovery-files")
                    click_control("recovery-copy",hold=.15)
                    copied = subprocess.check_output(["xclip","-selection","clipboard","-out"],env=environment)
                    time.sleep(.15)
                    after = control_bounds("recovery-files")
                    assert after == before, ("Copy acknowledgement must not move the recovery controls", before, after)
                    return copied

                def choose_version(version):
                    click_control("recovery-version:"+str(version),hold=.15)
                    control_bounds("recovery-selected:"+str(version))

                def choose_file(query):
                    click_control("recovery-files",hold=.15);key("ctrl+a");paste_text(query,held=True);key("Return")
                    return versions()

                initial=history()[0]
                assert copy_snapshot()==original
                assert target.read_bytes()==original
                capture("native-recovery-initial.png")
                key("Escape");control_bounds("editor-focused")
                results.append("File recovery command opened stored history; Copy preserved exact Unicode/CRLF and closing returned editor focus")

                open_note("Recovery second");history();assert copy_snapshot()==second.read_bytes();key("Escape")
                open_note("Recovery target")
                command("Open settings");type_text("file recovery");click_control("settings-recovery-view",hold=.15)
                control_bounds("recovery-loaded")
                assert control_bounds("recovery-ready",required=False) is None
                choose_file("rcvryscnd");assert copy_snapshot()==second.read_bytes()
                choose_file("Recovery target");assert copy_snapshot()==original
                key("Escape")
                results.append("Settings View opened the file chooser; fuzzy keyboard selection switched between two stored files without changing their bytes")

                unopened=fixture/("Unopened recovery "+"long filename "*8+".md")
                first_unopened=b"# Closed note\n\nFIRST_CLOSED_SNAPSHOT\n"
                second_unopened=b"# Closed note\n\nSECOND_CLOSED_VERSION_IS_DIFFERENT\n"
                assert "tab-full:"+unopened.name not in json.loads((base/"ui-probe.json").read_text())
                unopened.write_bytes(first_unopened);time.sleep(.6)
                command("Open settings");type_text("file recovery");click_control("settings-recovery-view",hold=.15)
                choose_file("Unopened recovery");closed_initial=versions()[0]
                assert copy_snapshot()==first_unopened and unopened.read_bytes()==first_unopened
                key("Escape")
                unopened.write_bytes(second_unopened);time.sleep(.6)
                command("Open settings");type_text("file recovery");click_control("settings-recovery-view",hold=.15)
                choose_file("Unopened recovery");versions(2)
                assert copy_snapshot()==second_unopened
                choose_version(closed_initial)
                assert copy_snapshot()==first_unopened and unopened.read_bytes()==second_unopened
                assert "tab-full:"+unopened.name not in json.loads((base/"ui-probe.json").read_text())
                capture("native-recovery-unopened.png");key("Escape")
                assert target.read_bytes()==original
                results.append("Two external versions of a never-opened note were captured; selecting and copying either history version preserved its current bytes and the active document")

                click_control("editor");key("ctrl+a");paste_text(changed.decode(),held=True);key("ctrl+s")
                assert target.read_bytes()==changed
                ids=history(2)
                assert ids[0]==initial
                choose_version(initial)
                assert copy_snapshot()==original
                click_control("recovery-changes",hold=.15)
                capture("native-recovery-changes.png")
                picture=Image.open(reports/"native-recovery-changes.png").convert("RGB")
                assert sum(color==(70,33,35) for color in pixel_data(picture))>5
                assert sum(color==(29,65,46) for color in pixel_data(picture))>5
                assert copy_snapshot()==original and target.read_bytes()==changed
                version_rect=control_bounds("recovery-version:"+str(initial))
                click_control("recovery-restore",hold=.15)
                assert target.read_bytes()==original
                versions(2);time.sleep(.6)
                assert control_bounds("recovery-version:"+str(initial))==version_rect, "Background captures must not move a displayed history version"
                choose_version(ids[-1])
                assert copy_snapshot()==changed
                key("Escape")
                results.append("Changed notes created a second snapshot; older selection, visible additions/removals and Restore preserved exact bytes plus the displaced version")

                history(2)
                store=base/"state/rustidian/recovery"/session_path.with_suffix(".redb").name
                permissions=store.stat().st_mode & 0o777
                owner=subprocess.Popen(["xclip","-selection","clipboard","-in","-quiet"],stdin=subprocess.PIPE,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,env=environment)
                try:
                    owner.stdin.write(b"RECOVERY_READ_ERROR_CLIPBOARD");owner.stdin.close();pause()
                    store.chmod(0)
                    assert not os.access(store,os.R_OK),"The read-error fixture must actually refuse access"
                    click_control("recovery-version:"+str(initial),hold=.15)
                    control_bounds("recovery-error")
                    assert control_bounds("recovery-ready",required=False) is None
                    click_control("recovery-copy",hold=.15)
                    assert subprocess.check_output(["xclip","-selection","clipboard","-out"],env=environment)==b"RECOVERY_READ_ERROR_CLIPBOARD"
                    click_control("recovery-restore",hold=.15)
                    assert target.read_bytes()==original
                    capture("native-recovery-read-error.png")
                finally:
                    store.chmod(permissions)
                    owner.terminate();owner.wait(timeout=5)
                choose_version(initial)
                assert copy_snapshot()==original and target.read_bytes()==original
                key("Escape")
                results.append("Unreadable history showed an error and disabled stale Copy/Restore; restoring access made the exact selected version available again without changing the document")

                command("Open settings");type_text("autosave")
                if json.loads(session_path.read_text())["autosave_enabled"]:
                    click_control("settings-autosave",hold=.15)
                wait_autosave(base/"state",False,session_path=session_path)
                key("Escape");click_control("editor");key("ctrl+a");paste_text("UNSAVED_RECOVERY\n",held=True)
                history(2);choose_version(initial);control_bounds("recovery-ready")
                click_control("recovery-restore",hold=.15)
                click_control("pending-cancel",hold=.15)
                assert target.read_bytes()==original
                key("Escape");click_control("editor");key("ctrl+a");key("ctrl+c")
                assert subprocess.check_output(["xclip","-selection","clipboard","-out"],env=environment)==b"UNSAVED_RECOVERY\n"
                key("End")
                history(2);choose_version(initial);control_bounds("recovery-ready")
                click_control("recovery-restore",hold=.15);click_control("pending-save",hold=.15)
                assert target.read_bytes()==original
                key("Escape");history(3)
                preserved=[]
                for version in versions(3):
                    choose_version(version)
                    preserved.append(copy_snapshot())
                assert b"UNSAVED_RECOVERY\n" in preserved
                key("Escape")
                results.append("Dirty Restore cancellation retained the unsaved buffer; Save and continue preserved those exact edits in history before restoring")

                history(3);choose_version(initial);control_bounds("recovery-ready")
                target.write_bytes(b"# EXTERNAL_RECOVERY\n")
                click_control("recovery-restore",hold=.15)
                assert control_bounds("recovery-error") and target.read_bytes()==b"# EXTERNAL_RECOVERY\n"
                assert copy_snapshot()==original
                capture("native-recovery-conflict.png")
                key("Escape")
                target.write_bytes(original)
                command("Refresh vault");pause()
                results.append("A note changed outside the history dialog refused Restore, retained the replacement and kept the selected snapshot copyable")

                deleted=base/"deleted-recovery-target.md"
                target.rename(deleted);pause()
                history(3);choose_version(initial);control_bounds("recovery-ready")
                click_control("recovery-restore",hold=.15)
                assert target.read_bytes()==original
                capture("native-recovery-deleted-note.png");key("Escape")
                target.rename(base/"deleted-recovery-target-again.md");pause()
                history(3);choose_version(initial);control_bounds("recovery-ready")
                target.write_bytes(b"# New collision\n")
                click_control("recovery-restore",hold=.15)
                assert control_bounds("recovery-error") and target.read_bytes()==b"# New collision\n"
                key("Escape");target.write_bytes(original);command("Refresh vault");pause()
                results.append("A previously deleted note was recreated from its exact snapshot; a file appearing meanwhile was preserved without overwrite")

                xdo("key","--clearmodifiers","ctrl+q");app.wait(timeout=5)
                app=launch_app(launch_command);window=ready_window()
                open_note("Recovery target")
                history(3);choose_version(initial)
                assert copy_snapshot()==original
                key("Escape")
                assert json.loads(session_path.read_text())["recovery_config"]["intervalMinutes"]==0
                results.append("History and recovery settings survived a real application restart")

                other=base/"Recovery other vault";other.mkdir()
                (other/"Recovery target.md").write_bytes(b"# Other vault snapshot\n")
                command("Open vault");key("ctrl+a");paste_text(str(other),held=True);key("Return")
                title_contains(other.name);open_note("Recovery target")
                history();assert copy_snapshot()==b"# Other vault snapshot\n";key("Escape")
                command("Open vault");key("ctrl+a");paste_text(str(fixture),held=True);key("Return")
                title_contains(fixture.name);open_note("Recovery target")
                history(3);choose_version(initial)
                assert copy_snapshot()==original;key("Escape")
                assert (other/"Recovery target.md").read_bytes()==b"# Other vault snapshot\n"
                results.append("Equal file names in separate vaults had isolated snapshots and switching back restored the original history")

                command("Switch appearance")
                history(3)
                capture("native-recovery-light.png")
                xdo("windowsize",window,"860","560");pause()
                bounds=control_bounds("recovery-dialog")
                assert 0<=bounds[0]<bounds[2]<=860 and 0<=bounds[1]<bounds[3]<=560,bounds
                for name in ["recovery-files","recovery-copy","recovery-restore","recovery-close"]:
                    b=control_bounds(name)
                    assert bounds[0]<=b[0]<b[2]<=bounds[2] and bounds[1]<=b[1]<b[3]<=bounds[3],(name,b,bounds)
                capture("native-recovery-compact.png",size=(860,560))
                key("Escape");xdo("windowsize",window,"1440","960");pause();command("Switch appearance")
                results.append("The real history controls fitted a compact window and rendered in both themes")

                command("Open settings");type_text("file recovery");click_control("settings-recovery-clear",hold=.15)
                click_control("recovery-clear-cancel",hold=.15)
                control_bounds("settings-recovery-view-ready");click_control("settings-recovery-view",hold=.15)
                choose_file("Recovery target");versions(3);assert copy_snapshot() in (original,changed,b"UNSAVED_RECOVERY\n")
                key("Escape")
                command("Open settings");type_text("file recovery");click_control("settings-recovery-clear",hold=.15)
                click_control("recovery-clear-confirm",hold=.15)
                control_bounds("settings-recovery-view-ready");click_control("settings-recovery-view",hold=.15)
                control_bounds("recovery-loaded")
                assert not any(name.startswith("recovery-version:") for name in json.loads((base/"ui-probe.json").read_text()))
                assert control_bounds("recovery-ready",required=False) is None
                assert target.read_bytes()==original
                key("Escape")
                for name,digest in originals.items():
                    assert hashlib.sha256((fixture/name).read_bytes()).hexdigest()==digest,name
                assert second.read_text()=="# Second recovery note\n"
                assert unopened.read_bytes()==second_unopened
                results.append("Clear history cancellation preserved snapshots; confirmed Clear removed history only and every original Markdown file stayed unchanged")

            def system_open_checks():
                original = "# Existing control\n\nOPEN_ORIGINAL\n"
                note = fixture / "Notes/Open existing control.md"
                note.write_text(original)
                time.sleep(.6)
                open_note("Open existing control")
                command("Source editing")
                editor_end();type_text(" EDIT");key("ctrl+s")
                assert note.read_text() == original + " EDIT"
                key("ctrl+z");key("ctrl+s")
                assert note.read_text() == original
                key("Escape")
                results.append("The existing quick switcher and real editor opened, saved and undid a file before testing system Open")

                def roots():
                    return json.loads((base / "data/rustidian/vaults.json").read_text())["roots"]

                def picker_window(title, required=True):
                    deadline = time.monotonic() + (8 if required else .1)
                    while time.monotonic() < deadline:
                        found = subprocess.run(["xdotool", "search", "--onlyvisible", "--name", "^" + title + "$"], env=environment, text=True, capture_output=True)
                        if found.returncode == 0:
                            return found.stdout.splitlines()[-1]
                        time.sleep(.05)
                    assert not required, f"{title} must show its real native chooser"
                    return None

                def start(kind, via="menu"):
                    label = "Open file" if kind == "file" else "Open folder"
                    if via == "menu":
                        click_control("file-menu", hold=.15)
                        control_bounds("file-menu-open-file")
                        control_bounds("file-menu-open-folder")
                        capture(f"native-system-open-{kind}-menu.png")
                        file_row = control_bounds("file-menu-open-file")
                        folder_row = control_bounds("file-menu-open-folder")
                        assert folder_row[1] - file_row[1] <= 26, ("Open menu must use the compact row rhythm", file_row, folder_row)
                        menu = control_bounds("file-menu")
                        assert menu[3] - menu[1] <= 28, ("File control must fit inside the title bar", menu)
                        click_control("file-menu-open-" + kind, hold=.15)
                    elif via == "palette":
                        command(label + "…")
                    else:
                        key("ctrl+shift+o" if kind == "file" else "ctrl+alt+o")
                    picker = picker_window(label)
                    control_bounds("system-open-picker")
                    xdo("windowfocus", "--sync", picker)
                    pause()
                    return picker

                def settled(title):
                    deadline = time.monotonic() + 5
                    while picker_window(title, required=False) and time.monotonic() < deadline:
                        time.sleep(.05)
                    assert picker_window(title, required=False) is None
                    xdo("windowfocus", "--sync", window)
                    deadline = time.monotonic() + 5
                    while control_bounds("system-open-picker", required=False) and time.monotonic() < deadline:
                        time.sleep(.05)
                    assert control_bounds("system-open-picker", required=False) is None

                selections = []
                def choose(kind, path, via="menu"):
                    start(kind, via)
                    modified_shortcut(xdo, "l");pause()
                    modified_shortcut(xdo, "a");pause()
                    paste_text(str(path), held=True)
                    modified_shortcut(xdo, "a")
                    modified_shortcut(xdo, "c");pause()
                    actual = "";deadline = time.monotonic() + 4
                    while time.monotonic() < deadline:
                        copied = subprocess.run(["xclip", "-selection", "clipboard", "-out", "-target", "UTF8_STRING"], env=environment, capture_output=True, timeout=2)
                        actual = copied.stdout.decode()
                        if copied.returncode == 0 and actual.rstrip("/") == str(path):
                            break
                        time.sleep(.05)
                    selections.append({"kind":kind,"via":via,"expected":str(path),"actual":actual})
                    (reports / "native-system-open-selections.json").write_text(json.dumps(selections, ensure_ascii=False, indent=2) + "\n")
                    assert actual.rstrip("/") == str(path), selections[-1]
                    capture(f"native-system-open-{kind}-picker.png")
                    modified_shortcut(xdo, "o", "alt");pause()
                    settled("Open file" if kind == "file" else "Open folder")

                # Both independent cancellation routes must release the owned chooser.
                start("file");key("Escape");settled("Open file")
                title_contains("Open existing control")
                picker = start("folder", "shortcut")
                xdo("windowunmap", "--sync", picker)
                xdo("windowfocus", "--sync", window)
                click_control("system-open-cancel", hold=.15)
                assert control_bounds("system-open-picker", required=False) is None
                assert subprocess.run(["xdotool", "getwindowname", picker], env=environment, capture_output=True).returncode != 0
                title_contains("Open existing control")
                assert note.read_text() == original
                results.append("Visible File actions and the folder shortcut launched native pickers; native Escape and parent Cancel preserved the document and closed the chooser")

                nested = fixture / "Notes/Existing text سلام.txt"
                nested_source = "Plain text original\n"
                nested.write_text(nested_source)
                choose("file", nested, "shortcut")
                title_contains("Existing text")
                assert roots()[0] == str(fixture)
                command("Source editing");editor_end();type_text(" LOCAL");key("ctrl+s")
                assert nested.read_text() == nested_source + " LOCAL"
                key("ctrl+z");key("ctrl+s")
                assert nested.read_text() == nested_source
                results.append("Open file accepted an existing nested Unicode text path, kept the active vault root and edited the actual file with Undo")

                broken_root = base / "Unreadable document vault"
                broken_root.mkdir()
                broken = broken_root / "Invalid UTF8.md"
                broken.write_bytes(b"# Unreadable\n\xff\xfe")
                choose("file", broken)
                control_bounds("error-notice")
                capture("native-system-open-invalid-document.png")
                title_contains("Existing text")
                assert roots()[0] == str(fixture)
                assert nested.read_text() == nested_source and broken.read_bytes() == b"# Unreadable\n\xff\xfe"
                assert note.read_text() == original
                results.append("An existing but invalid UTF-8 Markdown file reported a real opening error and retained the previous vault, tab and all bytes")

                other = base / "Opened folder سلام"
                other.mkdir()
                outside = other / "Outside.md"
                outside.write_text("# Outside\n\nOUTSIDE_ORIGINAL\n")
                selected = other / "Selected after guard.md"
                selected_source = "# Selected\n\nSELECTED_ORIGINAL\n"
                selected.write_text(selected_source)
                child = other / "Nested registered vault"
                child.mkdir()
                child_note = child / "Registered child.md"
                child_source = "# Child\n\n[[Home]]\n"
                child_note.write_text(child_source)
                (child / "Home.md").write_text("# Child home\n")
                choose("folder", other, "palette")
                title_contains(other.name)
                assert roots()[0] == str(other)
                open_note("Outside")
                command("Source editing");editor_end();type_text(" OTHER");key("ctrl+s")
                assert outside.read_text().endswith(" OTHER")
                choose("folder", other)
                title_contains("Outside")
                key("ctrl+z");key("ctrl+s")
                assert outside.read_text() == "# Outside\n\nOUTSIDE_ORIGINAL\n"
                results.append("Open folder registered the chosen existing vault; reopening that active folder kept the actual document and its Undo history")

                choose("folder", child)
                title_contains(child.name)
                choose("folder", fixture)
                choose("file", child_note)
                title_contains("Registered child")
                assert roots()[0] == str(child)
                command("Source editing")
                editor_end();key("ctrl+Home");key("Down");key("Down");key("Right");key("Right");key("ctrl+Return")
                title_contains("Home")
                assert roots()[0] == str(child)
                assert child_note.read_text() == child_source
                results.append("Opening outside the active vault selected the deepest registered root and followed its own root-level wiki link")

                choose("file", note, "palette")
                title_contains("Open existing control")
                assert roots()[0] == str(fixture)
                session_path = next(path for path in (base / "state/rustidian").glob("*.json") if "Notes/Open existing control.md" in path.read_text())
                autosave_before = json.loads(session_path.read_text())["autosave_enabled"]
                def autosave(enabled):
                    command("Open settings");type_text("autosave")
                    if json.loads(session_path.read_text())["autosave_enabled"] is not enabled:
                        click_control("settings-autosave", hold=.15)
                    wait_autosave(base / "state", enabled, session_path=session_path)
                    key("Escape")
                autosave(False)
                command("Source editing");editor_end();type_text(" UNSAVED")
                assert note.read_text() == original
                choose("file", selected)
                control_bounds("pending-cancel")
                capture("native-system-open-unsaved.png")
                click_control("pending-cancel", hold=.15)
                title_contains("Open existing control")
                assert roots()[0] == str(fixture)
                assert note.read_text() == original and selected.read_text() == selected_source
                key("ctrl+s");assert note.read_text() == original + " UNSAVED"
                key("ctrl+z");key("ctrl+s");assert note.read_text() == original
                choose("folder", other)
                title_contains("Outside")
                assert control_bounds("tab-full:Selected after guard.md", required=False) is None
                results.append("Cancelling cross-vault Open file retained exact unsaved text; later Open folder restored its workspace without a stale cancelled file")

                choose("file", note)
                title_contains("Open existing control")
                command("Source editing");editor_end();type_text(" SAVED")
                choose("file", selected, "shortcut")
                click_control("pending-save", hold=.15)
                title_contains("Selected after guard")
                assert roots()[0] == str(other)
                assert note.read_text() == original + " SAVED"
                assert selected.read_text() == selected_source
                command("Source editing");editor_end();type_text(" EXTERNAL");key("ctrl+s")
                assert selected.read_text() == selected_source + " EXTERNAL"
                assert note.read_text() == original + " SAVED"
                key("ctrl+z");key("ctrl+s");assert selected.read_text() == selected_source
                results.append("Save and continue saved the old vault before opening the requested registered-vault file; subsequent editing and Undo stayed in the selected file")

                choose("file", note)
                command("Source editing");editor_end();key("ctrl+a");paste_text(original);key("ctrl+s")
                assert note.read_text() == original
                editor_end();type_text(" DISCARD")
                choose("file", outside)
                click_control("pending-discard", hold=.15)
                title_contains("Outside")
                assert note.read_text() == original
                choose("file", note)
                title_contains("Open existing control")
                assert note.read_text() == original
                autosave(autosave_before)
                set_theme(False)
                click_control("file-menu", hold=.15)
                capture("native-system-open-light.png")
                key("Escape");set_theme(True)
                open_note("Home")
                results.append("Discard explicitly abandoned only unsaved edits; switching back restored the correct file, and File actions also rendered in the light theme")

            def emoji_checks():
                source = (ROOT / "tests/fixtures/emoji.md").read_text()
                note = fixture / "Emoji checks 📂.md"
                note.write_text(source)
                time.sleep(.6);open_note("Emoji checks")
                def colored(name):
                    bounds = control_bounds(name)
                    image = ImageGrab.grab(xdisplay=display).crop(tuple(round(value) for value in bounds)).convert("RGB")
                    count = sum(max(pixel)>110 and max(pixel)-min(pixel)>45 for pixel in pixel_data(image))
                    assert count > 30, (name, "expected actual colored emoji pixels", count, bounds)
                for dark in (True, False):
                    set_theme(dark)
                    command("Reading view")
                    colored("reading-block:1")
                    colored("note-title")
                    control_bounds("table:0")
                    capture("native-emoji-reading-"+("dark" if dark else "light")+".png")
                    command("Live preview")
                    capture("native-emoji-live-"+("dark" if dark else "light")+".png")
                    command("Source editing")
                    capture("native-emoji-source-"+("dark" if dark else "light")+".png")
                    assert note.read_text() == source
                results.append("Actual colored emoji pixels rendered in the Reading body and editable filename in both themes; Live/Source/table captures retained every original byte")
                editor_end();paste_text(" 🧭");key("ctrl+s")
                assert note.read_text() == source + " 🧭"
                key("ctrl+z");key("ctrl+s")
                assert note.read_text() == source
                results.append("Pasting a new arbitrary emoji into Source saved the exact Unicode bytes and Undo restored the original note")
                link = fixture / "Emoji preview source.md"
                link_source = "# Emoji preview\n\n[[Emoji checks 📂|Emoji preview]]\n"
                link.write_text(link_source)
                time.sleep(.6);open_note("Emoji preview source");command("Reading view")
                bounds = control_bounds("reading-block:1")
                xdo("mousemove", "--sync", str(round(bounds[0]+25)), str(round((bounds[1]+bounds[3])/2)))
                time.sleep(.8)
                control_bounds("page-preview-card")
                colored("page-preview/reading-block:1")
                capture("native-emoji-hover.png")
                xdo("mousemove", "--sync", "1100", "800", "click", "1");pause()
                assert note.read_text() == source and link.read_text() == link_source
                set_theme(True)
                results.append("The actual linked-note hover used the same colored glyphs without altering source or target")

            def visual_controls_checks():
                """One compact native regression for the reported control/proportion failures."""
                failures = []
                observations = {}
                def verify(label, check):
                    try:
                        check()
                        observations[label] = {"status": "passed"}
                    except AssertionError as error:
                        failures.append(label)
                        observations[label] = {"status": "failed", "error": str(error)}
                        capture("native-visual-control-failure-" + label + ".png")
                    (reports / "visual-controls-results.json").write_text(json.dumps(observations, indent=2) + "\n")

                graph_root = base / "Graph proportions"
                graph_root.mkdir()
                sources = {}
                for index in range(64):
                    source = f"# Label-{index:02}\n\n"
                    if index == 0:
                        source += "\n".join(f"[[Label-{other:02}]]" for other in range(1, 64)) + "\n"
                    sources[f"Label-{index:02}.md"] = source
                    (graph_root / f"Label-{index:02}.md").write_text(source)
                command("Open vault");key("ctrl+a");paste_text(str(graph_root), held=True);key("Return")
                title_contains(graph_root.name);open_note("Label-00")
                command("Graph view: Open graph view")
                def graph_ready():
                    deadline = time.monotonic() + 8
                    while time.monotonic() < deadline:
                        if control_bounds("global-graph:busy")[0] == 0 and control_bounds("global-graph:node-count")[0] == 64:
                            return
                        time.sleep(.05)
                    raise AssertionError("The proportions fixture must finish its real graph layout")
                graph_ready()

                def graph_pixels(dark):
                    xdo("mousemove", "--sync", "100", "20");pause()
                    capture(f"native-graph-proportions-{'dark' if dark else 'light'}.png")
                    picture = ImageGrab.grab(xdisplay=display).convert("RGB")
                    canvas = control_bounds("global-graph:canvas")
                    background = picture.getpixel((round(canvas[0] + 15), round(canvas[1] + 15)))
                    samples = []
                    glyphs = []
                    for index in range(12):
                        bounds = control_bounds(f"global-graph:node:Label-{index:02}.md")
                        cx, cy = round((bounds[0] + bounds[2]) / 2), round((bounds[1] + bounds[3]) / 2)
                        color = picture.getpixel((cx, cy))
                        spans = []
                        for axis in (0, 1):
                            edges = []
                            for sign in (-1, 1):
                                distance = 0
                                while distance < 40:
                                    point = (cx + sign * distance, cy) if axis == 0 else (cx, cy + sign * distance)
                                    if max(abs(a-b) for a,b in zip(picture.getpixel(point), color)) > 24:
                                        break
                                    distance += 1
                                edges.append(distance)
                            spans.append(sum(edges) - 1)
                        samples.append({"node": index, "diameter": spans})
                        assert 5 <= min(spans) and max(spans) <= 20, ("Default graph dots must remain proportionate after Fit", samples[-1])
                        # Identical Label-NN words distinguish hub font growth from
                        # genuine glyph differences without reading a font-size probe.
                        first_y = cy + (spans[1] + 1) // 2 + 3
                        rows = []
                        for y in range(first_y, min(picture.height, first_y + 34)):
                            count = sum(pixel_contrast(picture.getpixel((x, y)), background) >= 3
                                        for x in range(max(0, cx - 34), min(picture.width, cx + 35)))
                            if count >= 3:
                                rows.append(y)
                        runs = []
                        for y in rows:
                            if not runs or y > runs[-1][-1] + 1:runs.append([])
                            runs[-1].append(y)
                        text = next((run for run in runs if len(run) >= 7), None)
                        if text is not None:
                            glyphs.append({"node": index, "height": len(text)})
                    (reports / f"graph-proportions-{'dark' if dark else 'light'}.json").write_text(json.dumps({"dots": samples, "glyphs": glyphs}, indent=2) + "\n")
                    hub = next((sample["height"] for sample in glyphs if sample["node"] == 0), None)
                    leaves = sorted(sample["height"] for sample in glyphs if sample["node"] != 0)
                    assert hub is not None and len(leaves) >= 4, ("Hub and several leaf labels must actually be visible", glyphs)
                    assert hub <= 15 and abs(hub - leaves[len(leaves)//2]) <= 1, ("Graph label glyphs must stay compact and consistent across node degrees", glyphs)

                for dark in (True, False):
                    set_theme(dark)
                    verify("graph-" + ("dark" if dark else "light"), lambda: graph_pixels(dark))
                    if control_bounds("global-graph:filter", required=False):click_control("global-graph:section:filters", hold=.15)
                    if not control_bounds("global-graph:node-size", required=False):click_control("global-graph:section:display", hold=.15)
                    for name in ("text-fade", "node-size", "link-thickness"):
                        verify(name + "-" + ("dark" if dark else "light"), lambda name=name: slider_pixels("global-graph:" + name, "graph-" + name, dark))
                    click_control("global-graph:section:display", hold=.15)
                    if not control_bounds("global-graph:force-center", required=False):click_control("global-graph:section:forces", hold=.15)
                    for name in ("force-center", "force-repel", "force-link", "force-distance"):
                        verify(name + "-" + ("dark" if dark else "light"), lambda name=name: slider_pixels("global-graph:" + name, "graph-" + name, dark))
                    click_control("global-graph:reset", hold=.15);graph_ready()
                    click_control("global-graph:section:forces", hold=.15)
                    click_control("global-graph:fit", hold=.15)
                assert all((graph_root / name).read_text() == source for name, source in sources.items())
                command("Open vault");key("ctrl+a");paste_text(str(fixture), held=True);key("Return")
                title_contains(fixture.name)
                note = fixture / "Visual controls.md"
                source = "# Visual controls\n\n" + "\n\n".join(f"Line {index}: keep the scrollbar visible and the original note intact." for index in range(160)) + "\n"
                note.write_text(source);open_note("Visual controls");command("Reading view")
                for dark in (True, False):
                    set_theme(dark);command("Open settings");click_control("settings-page:Appearance")
                    verify("settings-font-" + ("dark" if dark else "light"), lambda: slider_pixels("settings-font-size", "settings-font", dark))
                    key("Escape")
                    size = round(control_bounds("document-font-size")[0])
                    for _ in range(abs(size - 18)):command("Decrease font size" if size > 18 else "Increase font size")
                xdo("mousemove", "--sync", "900", "500", "click", "--repeat", "3", "--delay", "15", "5");pause()
                verify("scrollbar", lambda: scrollbar_pixels(note, source))
                set_theme(True)
                assert note.read_text() == source
                assert not failures, ("Native visual/control regressions", observations)
                results.append("Actual graph dots and labels stay compact; graph and Settings slider thumbs remain filled and compact through idle/hover/drag in both themes, with all fixture bytes preserved")

            def graph_checks():
                nonlocal app, window
                graph_root = base / "Graph native vault"
                graph_root.mkdir()
                sources = {
                    "Home.md":"# Home\n\n[[Alpha]]\n[[Beta]]\n[[Projects/Plan]]\n[[Missing target]]\n\n#start\n",
                    "Alpha.md":"# Alpha\n\n[[Gamma]]\n[[Beta]]\n\n#team/green\n",
                    "Beta.md":"# Beta\n\n[[Delta]]\n\n#team/blue\n",
                    "Gamma.md":"# Gamma\n\n[[Epsilon]]\n\n#team/green\n",
                    "Delta.md":"# Delta\n",
                    "Epsilon.md":"# Epsilon\n",
                    "Isolated.md":"# Isolated\n",
                    "Projects/Plan.md":"# Plan\n\n[[Alpha]]\n\n#project\n",
                }
                for name, text in sources.items():
                    path = graph_root / name
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_text(text)
                command("Open vault");key("ctrl+a");paste_text(str(graph_root), held=True);key("Return")
                title_contains(graph_root.name);open_note("Home")
                command("Source editing");editor_end();type_text(" GRAPH_CONTROL");key("ctrl+s")
                assert (graph_root / "Home.md").read_text() == sources["Home.md"] + " GRAPH_CONTROL"
                key("ctrl+z");key("ctrl+s")
                assert (graph_root / "Home.md").read_text() == sources["Home.md"]
                results.append("The graph fixture opened through the real vault flow and ordinary editing/save/Undo passed before the graph command")

                def metric(prefix, name):
                    return control_bounds(prefix + ":" + name)[0]

                def ready(prefix, nodes, edges=None):
                    deadline = time.monotonic() + 8
                    while time.monotonic() < deadline:
                        controls = json.loads((base / "ui-probe.json").read_text())
                        if controls.get(prefix+":busy", [1])[0] == 0 and controls.get(prefix+":node-count", [-1])[0] == nodes and (edges is None or controls.get(prefix+":edge-count", [-1])[0] == edges):
                            return
                        time.sleep(.05)
                    capture("native-graph-not-ready.png")
                    raise AssertionError((prefix, nodes, edges, {key:value for key,value in controls.items() if key.startswith(prefix)}))

                def filters(prefix):
                    if control_bounds(prefix+":section:filters", required=False) is None:
                        click_control(prefix+":settings", hold=.15)
                    if control_bounds(prefix+":filter", required=False) is None:
                        click_control(prefix+":section:filters", hold=.15)
                    control_bounds(prefix+":filter")

                def filter_text(prefix, value):
                    click_control(prefix+":filter", hold=.15);key("ctrl+a")
                    if value:type_text(value)
                    else:key("BackSpace")

                click_control("button:Open graph view", hold=.15)
                title_contains("Graph view")
                ready("global-graph", 9, 9)
                filters("global-graph")
                capture("native-graph-global-dark.png")
                filter_text("global-graph", "path:Projects");ready("global-graph", 1, 0)
                filter_text("global-graph", "path:DOES_NOT_EXIST");ready("global-graph", 0, 0)
                capture("native-graph-empty.png")
                filter_text("global-graph", "");ready("global-graph", 9, 9)
                click_control("global-graph:existing-only", hold=.15);ready("global-graph", 8, 8)
                click_control("global-graph:existing-only", hold=.15);ready("global-graph", 9, 9)
                click_control("global-graph:orphans", hold=.15);ready("global-graph", 8, 9)
                click_control("global-graph:orphans", hold=.15);ready("global-graph", 9, 9)
                click_control("global-graph:tags", hold=.15);ready("global-graph", 13, 14)
                click_control("global-graph:tags", hold=.15);ready("global-graph", 9, 9)
                results.append("The native global graph showed actual resolved links; positive/empty queries, existing-only, orphan and tag controls changed the expected topology")

                click_control("global-graph:settings", hold=.15)
                click_control("global-graph:fit", hold=.15)
                left, top, right, bottom = control_bounds("global-graph:canvas")
                zoom = metric("global-graph", "zoom")
                xdo("mousemove", "--sync", str(round((left+right)/2)), str(round((top+bottom)/2)), "click", "4");pause()
                assert metric("global-graph", "zoom") > zoom
                pan = [metric("global-graph", name) for name in ("pan-x", "pan-y")]
                x, y = round(right-25), round(bottom-25)
                xdo("mousemove", "--sync", str(x), str(y), "mousedown", "1")
                try:
                    for step in range(1, 7):
                        xdo("mousemove", str(x-step*10), str(y-step*5));time.sleep(.04)
                finally:xdo("mouseup", "1")
                pause()
                assert abs(metric("global-graph", "pan-x")-pan[0]) > 20
                click_control("global-graph:fit", hold=.15)
                click_control("global-graph:node:Alpha.md", hold=.15)
                title_contains("Alpha")
                assert control_bounds("global-graph:canvas", required=False) is None
                results.append("Physical wheel and blank-canvas drag zoomed and panned; clicking the actual painted Alpha node opened its note")

                open_note("Home")
                command("Graph view: Open local graph")
                ready("local-graph", 5, 4);filters("local-graph")
                def depth(value, count):
                    left, top, right, bottom = control_bounds("local-graph:depth-slider")
                    xdo("mousemove", "--sync", str(round(left+7)), str(round((top+bottom)/2)), "click", "1");pause()
                    for _ in range(value-1):key("Right")
                    assert metric("local-graph", "depth") == value
                    ready("local-graph", count)
                depth(2, 7);depth(3, 8);depth(1, 5)
                click_control("local-graph:neighbor", hold=.15);ready("local-graph", 5, 6)
                click_control("local-graph:outgoing", hold=.15);ready("local-graph", 1, 0)
                click_control("local-graph:outgoing", hold=.15);ready("local-graph", 5, 6)
                click_control("local-graph:neighbor", hold=.15);ready("local-graph", 5, 4)
                capture("native-graph-local-dark.png")
                results.append("The separate local graph matched reference depths one/two/three and independent outgoing/neighbor controls")
                click_control("local-graph:settings", hold=.15)
                click_control("local-graph:fit", hold=.15)
                click_control("local-graph:node:Beta.md", hold=.15)
                title_contains("Beta");ready("local-graph", 4, 3)
                set_theme(False);capture("native-graph-local-light.png")
                click_control("local-graph:close", hold=.15)
                assert control_bounds("local-graph:canvas", required=False) is None
                set_theme(True)
                results.append("A local-node click opened Beta while its graph followed the active note; the linked pane rendered in both themes and closed normally")

                close_tabs();open_note("Home");key("ctrl+w")
                command("Graph view: Open graph view");ready("global-graph", 9, 9)
                assert control_bounds("tab-count")[0] == 1
                command("Undo close tab")
                title_contains("Home")
                assert control_bounds("tab-count")[0] == 2
                key("ctrl+w");title_contains("Graph view");ready("global-graph", 9, 9)
                results.append("Undo close tab retained a sole fileless graph and restored the closed note alongside it")

                # Two tabs may retain the same document path but different view types.
                close_tabs();open_note("Home");command("Graph view: Open graph view")
                ready("global-graph", 9, 9)
                key("ctrl+t");open_note("Home")
                assert control_bounds("tab-count")[0] == 2
                assert control_bounds("global-graph:canvas", required=False) is None
                xdo("key", "--clearmodifiers", "ctrl+q");app.wait(timeout=5)
                app=launch_app([str(ROOT / args.binary), str(graph_root)]);window=ready_window()
                title_contains("Home")
                assert control_bounds("tab-count")[0] == 2
                assert control_bounds("global-graph:canvas", required=False) is None
                key("ctrl+w");title_contains("Graph view");ready("global-graph", 9, 9)
                results.append("Restart restored the active document beside its same-path Graph tab; closing the document revealed the retained graph")

                labels=graph_root/"StableLabels";labels.mkdir()
                for index in range(64):
                    text=f"# Label-{index:02}\n\n"
                    if index==0:text+="\n".join(f"[[StableLabels/Label-{other:02}]]" for other in range(1,64))+"\n"
                    (labels/f"Label-{index:02}.md").write_text(text)
                command("Refresh vault");filters("global-graph")
                filter_text("global-graph","path:StableLabels");ready("global-graph",64,63)
                click_control("global-graph:settings",hold=.15);click_control("global-graph:fit",hold=.15)
                xdo("mousemove","--sync","100","80");pause()
                capture("native-graph-labels-before.png")
                canvas=control_bounds("global-graph:canvas")
                area=(round(canvas[0]+260),round(canvas[1]+100),round(canvas[2]-260),round(canvas[3]-100))
                before=Image.open(reports/"native-graph-labels-before.png").convert("RGB")
                pause();capture("native-graph-labels-control.png")
                control=Image.open(reports/"native-graph-labels-control.png").convert("RGB")
                def changed(first,second):
                    return sum(max(pixel)>16 for pixel in pixel_data(ImageChops.difference(first,second)))
                noise=changed(before.crop(area),control.crop(area))
                assert noise<=20,("The unchanged graph must be settled before comparing a pan",noise)
                comparisons=[]
                for index,(dx,dy) in enumerate([(13,5),(17,9),(-7,-4)]):
                    pan=[metric("global-graph",key) for key in ("pan-x","pan-y")]
                    x,y=round(canvas[2]-30),round(canvas[3]-30)
                    xdo("mousemove","--sync",str(x),str(y),"mousedown","1");time.sleep(.12)
                    try:
                        for step in range(1,9):
                            xdo("mousemove",str(round(x+dx*step/8)),str(round(y+dy*step/8)));time.sleep(.035)
                    finally:xdo("mouseup","1")
                    xdo("mousemove","--sync","100","80");pause()
                    shift=[round(metric("global-graph",key)-pan[axis]) for axis,key in enumerate(("pan-x","pan-y"))]
                    assert any(shift),("The pan must actually move the graph",shift)
                    capture(f"native-graph-labels-pan-{index}.png")
                    after=Image.open(reports/f"native-graph-labels-pan-{index}.png").convert("RGB")
                    moved=tuple(value+shift[axis%2] for axis,value in enumerate(area))
                    difference=changed(before.crop(area),after.crop(moved))
                    wrong_alignment=changed(before.crop(area),after.crop(area))
                    comparisons.append({"shift":shift,"translated_changed_pixels":difference,"untranslated_changed_pixels":wrong_alignment,"control_noise_pixels":noise})
                    (reports/"graph-label-stability.json").write_text(json.dumps(comparisons,indent=2)+"\n")
                    assert wrong_alignment>500,("The comparison must reject a missing translation",wrong_alignment)
                    assert difference<=20,("Graph labels changed while only panning",comparisons[-1])
                    before=after
                results.append("Three physical pans preserved the translated dense-graph pixels and labels; independent stationary and wrong-alignment controls passed")

                direction_sources={"Focus.md":"[[Child]]\n[[Peer]]\n","Parent.md":"[[Focus]]\n","Child.md":"# Child\n","Peer.md":"[[Focus]]\n"}
                direction=graph_root/"Direction";direction.mkdir()
                for name,text in direction_sources.items():(direction/name).write_text(text)
                command("Refresh vault");filters("global-graph")
                filter_text("global-graph","path:Direction");ready("global-graph",4,4)
                def direction_mode(prefix):
                    if control_bounds(prefix+":section:display",required=False) is None:click_control(prefix+":settings",hold=.15)
                    if control_bounds(prefix+":color-mode",required=False) is None:click_control(prefix+":section:display",hold=.15)
                    click_control(prefix+":color-mode",hold=.15)
                    click_control(prefix+":color-mode:Link direction",hold=.15)
                    click_control(prefix+":fit",hold=.15)
                def check_direction_colors(prefix,dark,hover=True):
                    focus=control_bounds(prefix+":node:Direction/Focus.md")
                    if hover:
                        xdo("mousemove","--sync",str(round((focus[0]+focus[2])/2)),str(round((focus[1]+focus[3])/2)))
                    else:
                        xdo("mousemove","--sync","0","0")
                    pause()
                    name="native-graph-"+prefix+"-directions-"+("dark" if dark else "light")+".png"
                    capture(name);image=Image.open(reports/name).convert("RGB")
                    colors={}
                    for label in ("Parent","Child","Peer"):
                        bounds=control_bounds(prefix+":node:Direction/"+label+".md")
                        colors[label]=image.getpixel((round((bounds[0]+bounds[2])/2),round((bounds[1]+bounds[3])/2)))
                    r,g,b=colors["Parent"];assert b>r+60 and b>g+30,("Incoming note must use the blue legend color",colors)
                    r,g,b=colors["Child"];assert g>r+60 and g>b+20,("Outgoing note must use the green legend color",colors)
                    r,g,b=colors["Peer"];assert b>g+40 and r>g+20,("A reciprocal link must use the purple legend color",colors)
                click_control("global-graph:node:Direction/Focus.md",hold=.15);title_contains("Focus")
                command("Graph view: Open graph view");ready("global-graph",4,4)
                assert control_bounds("global-graph:color-mode",required=False) is not None, "The default color mode and legend must be discoverable on opening"
                for dark in (True,False):set_theme(dark);check_direction_colors("global-graph",dark,hover=False)
                results.append("Global graph opens with visible incoming, outgoing and reciprocal colors in both themes without selecting a mode or hovering")
                direction_mode("global-graph")
                for dark in (True,False):set_theme(dark);check_direction_colors("global-graph",dark)
                focus=control_bounds("global-graph:node:Direction/Focus.md")
                click_control("global-graph:node:Direction/Focus.md",hold=.15);title_contains("Focus")
                command("Graph view: Open local graph");ready("local-graph",4,4)
                for dark in (True,False):set_theme(dark);check_direction_colors("local-graph",dark,hover=False)
                direction_mode("local-graph")
                for dark in (False,True):set_theme(dark);check_direction_colors("local-graph",dark)
                click_control("local-graph:close",hold=.15)
                for name,text in direction_sources.items():assert (direction/name).read_text()==text
                results.append("The actual color menu rendered incoming blue, outgoing green and reciprocal purple links in global/local graphs and both themes without changing files")
                for name, original in sources.items():
                    assert (graph_root/name).read_text() == original, name
                command("Open vault");key("ctrl+a");paste_text(str(fixture), held=True);key("Return")
                title_contains(fixture.name);open_note("Home")
                results.append("Every graph operation and real restart preserved all eight original note files and returned to the previous vault")

            def graph_state_checks():
                nonlocal app, window
                observations = {}
                originals = {}
                sources = {"Focus.md": "# Focus\n\n[[Child]]\n[[Peer]]\n", "Parent.md": "# Parent\n\n[[Focus]]\n",
                           "Child.md": "# Child\n\n[[Leaf]]\n", "Peer.md": "# Peer\n\n[[Focus]]\n",
                           "Leaf.md": "# Leaf\n", "Isolated.md": "# Isolated\n"}
                value_names = ("text-fade", "node-size", "link-thickness", "force-center", "force-repel", "force-link", "force-distance", "color-mode")
                checked_names = ("tags", "attachments", "existing-only", "orphans", "incoming", "outgoing", "neighbor", "arrows")

                def metric(prefix, name):
                    return control_bounds(prefix + ":" + name)[0]

                def ready(prefix, nodes=None):
                    deadline = time.monotonic() + 8
                    while time.monotonic() < deadline:
                        controls = json.loads((base / "ui-probe.json").read_text())
                        if controls.get(prefix + ":busy", [1])[0] == 0 and prefix + ":node-count" in controls:
                            if nodes is None or controls[prefix + ":node-count"][0] == nodes:
                                return
                        time.sleep(.05)
                    raise AssertionError(("Graph did not finish its real layout", prefix, nodes, controls))

                def open_vault(root):
                    command("Open vault"); key("ctrl+a"); paste_text(str(root), held=True); key("Return")
                    title_contains(root.name)

                def fresh(label, local=False):
                    key("Escape")
                    root = base / ("Graph state " + label)
                    root.mkdir()
                    for name, source in sources.items():
                        path = root / name
                        path.write_text(source)
                        originals[path] = source.encode()
                    before = set((base / "state/rustidian").glob("*.json"))
                    open_vault(root); open_note("Focus")
                    command("Graph view: Open local graph" if local else "Graph view: Open graph view")
                    prefix = "local-graph" if local else "global-graph"
                    ready(prefix, 4 if local else 6)
                    deadline = time.monotonic() + 3
                    while time.monotonic() < deadline:
                        created = set((base / "state/rustidian").glob("*.json")) - before
                        matching = [path for path in created if any(tab[0] == "Focus.md" for tab in json.loads(path.read_text()).get("tabs", []))]
                        if len(matching) == 1:
                            return root, prefix, matching[0]
                        time.sleep(.05)
                    raise AssertionError(("Fresh graph fixture did not get one private session file", created))

                def settings(prefix, opened):
                    if bool(metric(prefix, "settings-open")) != opened:
                        click_control(prefix + ":settings", hold=.15)
                    assert bool(metric(prefix, "settings-open")) == opened

                def section(prefix, name, opened):
                    settings(prefix, True)
                    if bool(metric(prefix, "section-open:" + name)) != opened:
                        click_control(prefix + ":section:" + name, hold=.15)
                    assert bool(metric(prefix, "section-open:" + name)) == opened

                def only_section(prefix, name):
                    for other in ("filters", "groups", "display", "forces"):
                        if other != name:
                            section(prefix, other, False)
                    section(prefix, name, True)

                def write_field(name, text):
                    click_control(name, hold=.15); key("ctrl+a"); paste_text(text, held=True)

                def read_field(name):
                    click_control(name, hold=.15); key("ctrl+a"); key("ctrl+c")
                    return subprocess.check_output(["xclip", "-selection", "clipboard", "-out", "-target", "UTF8_STRING"], env=environment, text=True, timeout=3)

                def camera(prefix):
                    return [metric(prefix, name) for name in ("pan-x", "pan-y", "zoom")]

                def positions(prefix):
                    controls = json.loads((base / "ui-probe.json").read_text())
                    return {name.split(":node:", 1)[1]: [(bounds[0] + bounds[2]) / 2, (bounds[1] + bounds[3]) / 2]
                            for name, bounds in controls.items() if name.startswith(prefix + ":node:")}

                def drag(start, delta, cancel=False):
                    xdo("mousemove", "--sync", str(round(start[0])), str(round(start[1])), "mousedown", "1")
                    try:
                        for step in range(1, 9):
                            xdo("mousemove", "--sync", str(round(start[0] + delta[0] * step / 8)), str(round(start[1] + delta[1] * step / 8)))
                            time.sleep(.035)
                        if cancel:
                            key("Escape")
                            xdo("mousemove", "--sync", str(round(start[0] + delta[0] + 15)), str(round(start[1] + delta[1] + 10)))
                            pause()
                    finally:
                        xdo("mouseup", "1")
                    xdo("mousemove", "--sync", "0", "0"); pause()

                def pan(prefix):
                    left, top, right, bottom = control_bounds(prefix + ":canvas")
                    before = camera(prefix)
                    drag((left + 25, bottom - 25), (42, -27))
                    after = camera(prefix)
                    assert abs(after[0] - before[0] - 42) < 2 and abs(after[1] - before[1] + 27) < 2, ("Blank canvas must pan", before, after)

                def snapshot(prefix):
                    names = ["settings-open", "depth"] + ["section-open:" + name for name in ("filters", "groups", "display", "forces")]
                    names += ["value:" + name for name in value_names] + ["checked:" + name for name in checked_names]
                    return {"controls": {name: metric(prefix, name) for name in names}, "camera": camera(prefix),
                            "nodes": metric(prefix, "node-count"), "edges": metric(prefix, "edge-count")}

                def same_snapshot(prefix, expected):
                    actual = snapshot(prefix)
                    assert actual["nodes"] == expected["nodes"] and actual["edges"] == expected["edges"], ("Restored graph topology differs", expected, actual)
                    assert all(abs(actual["controls"][key] - value) < .003 for key, value in expected["controls"].items()), ("Graph settings/sections were not restored", expected, actual)
                    assert all(abs(a - b) < .5 for a, b in zip(actual["camera"], expected["camera"])), ("Graph camera changed during restoration", expected, actual)
                    return actual

                def restart(root):
                    nonlocal app
                    xdo("key", "--clearmodifiers", "ctrl+q"); app.wait(timeout=5)
                    app = launch_app([str(ROOT / args.binary), str(root)])
                    return app

                def slider_change(prefix, name):
                    before = metric(prefix, "value:" + name)
                    left, top, right, bottom = control_bounds(prefix + ":" + name)
                    xdo("mousemove", "--sync", str(round(left + (right - left - 35) * .35)), str(round((top + bottom) / 2)), "click", "1"); pause()
                    key("Right")
                    assert abs(metric(prefix, "value:" + name) - before) > .001, ("The native slider must change its actual value", prefix, name, before)

                def group_pixels(prefix):
                    # A narrow local canvas can place a dot behind Settings.
                    # Hide that overlay without fitting or changing the camera.
                    opened = bool(metric(prefix, "settings-open"))
                    settings(prefix, False)
                    try:
                        xdo("mousemove", "--sync", "0", "0"); pause()
                        bounds = control_bounds(prefix + ":node:Child.md")
                        picture = ImageGrab.grab(xdisplay=display).convert("RGB")
                        rgb = picture.getpixel((round((bounds[0] + bounds[2]) / 2), round((bounds[1] + bounds[3]) / 2)))
                        assert rgb[0] > rgb[1] + 60 and rgb[0] > rgb[2] + 60, ("The saved group must color the actual Child dot red", rgb)
                    finally:
                        settings(prefix, opened)

                def configure(prefix, local):
                    only_section(prefix, "filters")
                    write_field(prefix + ":filter", "-path:Isolated")
                    if local:
                        click_control(prefix + ":depth-slider"); key("Right")
                        assert metric(prefix, "depth") > 1
                    click_control(prefix + ":existing-only", hold=.15)
                    ready(prefix, 5)
                    only_section(prefix, "groups"); click_control(prefix + ":group-add", hold=.15)
                    write_field(prefix + ":group:0:query", "path:Child"); ready(prefix, 5)
                    only_section(prefix, "display"); click_control(prefix + ":arrows", hold=.15)
                    for name in ("text-fade", "node-size", "link-thickness"):
                        slider_change(prefix, name)
                    only_section(prefix, "forces")
                    for name in ("force-center", "force-repel", "force-link", "force-distance"):
                        slider_change(prefix, name)
                    ready(prefix, 5)
                    only_section(prefix, "filters"); section(prefix, "groups", True)
                    click_control(prefix + ":fit", hold=.15); pan(prefix)
                    left, top, right, bottom = control_bounds(prefix + ":canvas")
                    zoom = metric(prefix, "zoom")
                    xdo("mousemove", "--sync", str(round(left + 40)), str(round(top + 40)), "click", "5"); pause()
                    assert 0 < metric(prefix, "zoom") < zoom
                    group_pixels(prefix)

                def check_fields(prefix):
                    settings(prefix, True)
                    assert read_field(prefix + ":filter") == "-path:Isolated"
                    assert read_field(prefix + ":group:0:query") == "path:Child"
                    group_pixels(prefix)

                def node_drag_case():
                    _, prefix, _ = fresh("node drag")
                    # This path deliberately needs no newly added value probes:
                    # the installed baseline must fail on the actual node/camera bug.
                    if control_bounds(prefix + ":section:display", required=False):
                        click_control(prefix + ":settings", hold=.15)
                    click_control(prefix + ":fit", hold=.15)
                    xdo("mousemove", "0", "0"); pause()
                    before = positions(prefix); before_camera = camera(prefix)
                    assert set(before) == set(sources)
                    picture_before = ImageGrab.grab(xdisplay=display).convert("RGB")
                    capture("native-graph-state-node-before.png")
                    drag(before["Focus.md"], (53, 37))
                    after = positions(prefix); after_camera = camera(prefix)
                    capture("native-graph-state-node-after.png")
                    assert set(after) == set(before), ("A node drag must not navigate or remove nodes", before, after)
                    assert all(abs(a - b) < .5 for a, b in zip(before_camera, after_camera)), ("Dragging a node must not pan/zoom the camera", before_camera, after_camera)
                    assert all(abs(after["Focus.md"][axis] - before["Focus.md"][axis] - delta) < 2 for axis, delta in enumerate((53, 37))), (before, after)
                    assert all(abs(after[name][axis] - point[axis]) < 1 for name, point in before.items() if name != "Focus.md" for axis in (0, 1)), ("Only the grabbed node may move", before, after)
                    picture_after = ImageGrab.grab(xdisplay=display).convert("RGB")
                    canvas = control_bounds(prefix + ":canvas")
                    background = picture_after.getpixel((round(canvas[0] + 8), round(canvas[1] + 8)))
                    edge_samples = []
                    for neighbor in ("Child.md", "Parent.md", "Peer.md"):
                        start, end = after["Focus.md"], after[neighbor]
                        for fraction in (.2, .35, .5, .65, .8):
                            x, y = [round(a + (b - a) * fraction) for a, b in zip(start, end)]
                            points = [(px, py) for px in range(x - 1, x + 2) for py in range(y - 1, y + 2)]
                            old = max(pixel_contrast(picture_before.getpixel(point), background) for point in points)
                            new = max(pixel_contrast(picture_after.getpixel(point), background) for point in points)
                            if old < 1.2 and new > 1.3:
                                edge_samples.append({"neighbor": neighbor, "point": [x, y], "old_contrast": old, "new_contrast": new})
                                break
                    assert len(edge_samples) >= 2, ("Connected edge pixels must follow the dragged node into previously blank locations", edge_samples)
                    observations["node-pixels"] = {"before": before, "after": after, "camera_before": before_camera, "camera_after": after_camera, "edge_samples": edge_samples}
                    drag(after["Focus.md"], (31, -28), cancel=True)
                    cancelled = positions(prefix)
                    assert all(abs(cancelled[name][axis] - point[axis]) < 1 for name, point in after.items() for axis in (0, 1)), ("Escape must restore the node's whole drag and keep it cancelled until mouse release", after, cancelled)
                    assert camera(prefix) == after_camera
                    pan(prefix)
                    click_control(prefix + ":settings", hold=.15)
                    if not control_bounds(prefix + ":filter", required=False):
                        click_control(prefix + ":section:filters", hold=.15)
                    bounds = control_bounds(prefix + ":filter")
                    before_camera = camera(prefix); before = positions(prefix)
                    drag(((bounds[0] + bounds[2]) / 2, (bounds[1] + bounds[3]) / 2), (22, 8))
                    assert camera(prefix) == before_camera and positions(prefix) == before, "Dragging the settings overlay must not move the canvas or its nodes"
                    click_control(prefix + ":settings", hold=.15)
                    click_control(prefix + ":node:Child.md", hold=.15); title_contains("Child")
                    results.append("A real node drag moved only its dot and connected edge pixels without panning; Escape cancelled the whole drag, blank canvas still panned, Settings blocked dragging, and a click opened the note")

                def global_restart_case():
                    nonlocal app, window
                    root, prefix, _ = fresh("global restart")
                    configure(prefix, False); settings(prefix, False)
                    expected = snapshot(prefix)
                    capture("native-graph-state-global-before-restart.png")
                    app = restart(root); window = ready_window(); ready(prefix, 5)
                    observations["global-restored"] = same_snapshot(prefix, expected)
                    check_fields(prefix)
                    capture("native-graph-state-global-restored.png")
                    results.append("A real restart restored global graph filters, group color/query, display/forces, section expansion, hidden Settings and exact camera; visible topology and red group pixels matched")

                def local_restart_case():
                    nonlocal app, window
                    root, prefix, _ = fresh("local restart", local=True)
                    configure(prefix, True)
                    expected = snapshot(prefix)
                    capture("native-graph-state-local-before-restart.png")
                    app = restart(root); window = ready_window(); title_contains("Focus"); ready(prefix, 5)
                    observations["local-restored"] = same_snapshot(prefix, expected)
                    check_fields(prefix)
                    capture("native-graph-state-local-restored.png")
                    results.append("A real restart reopened the local graph beside its note and restored depth, filter/groups, display/forces, settings sections and camera with actual topology/group colors intact")

                def vault_isolation_case():
                    first, prefix, _ = fresh("vault A")
                    configure(prefix, False); settings(prefix, False)
                    global_expected = snapshot(prefix)
                    open_note("Focus"); command("Graph view: Open local graph"); ready("local-graph", 4)
                    local_default = snapshot("local-graph")
                    configure("local-graph", True)
                    local_expected = snapshot("local-graph")
                    second, other, _ = fresh("vault B")
                    assert metric(other, "node-count") == 6 and control_bounds(other + ":group:0:query", required=False) is None
                    assert metric(other, "value:node-size") == 1 and metric(other, "checked:existing-only") == 0
                    open_vault(first); title_contains("Focus"); ready("local-graph", 5)
                    same_snapshot("local-graph", local_expected); check_fields("local-graph")
                    only_section("local-graph", "forces"); click_control("local-graph:reset", hold=.15); ready("local-graph", 4)
                    actual = snapshot("local-graph")
                    assert all(actual["controls"][key] == value for key, value in local_default["controls"].items() if key.startswith(("value:", "checked:")) or key == "depth"), ("Reset must restore this pane's defaults", local_default, actual)
                    click_control("local-graph:close", hold=.15)
                    command("Graph view: Open graph view"); ready(prefix, 5)
                    same_snapshot(prefix, global_expected); check_fields(prefix)
                    open_vault(second); ready(other, 6)
                    assert metric(other, "value:node-size") == 1 and metric(other, "checked:existing-only") == 0
                    results.append("Two vaults retain independent global/local preferences and cameras; resetting only the local pane restored its controls without altering the global pane or the other vault")

                def write_failure_case():
                    nonlocal app, window
                    root, prefix, state_path = fresh("write failure")
                    only_section(prefix, "filters")
                    old = metric(prefix, "checked:existing-only")
                    before = state_path.read_bytes()
                    held = base / "graph-state-held-durable.json"
                    state_path.rename(held); state_path.mkdir()
                    try:
                        click_control(prefix + ":existing-only", hold=.15)
                        control_bounds("error-notice")
                        capture("native-graph-state-write-error.png")
                        assert held.read_bytes() == before, "A failed atomic state write changed the previous durable bytes"
                    finally:
                        state_path.rmdir(); held.rename(state_path)
                    click_control("error-dismiss", hold=.15)
                    # Return to the observed old value before applying a successful retry.
                    if metric(prefix, "checked:existing-only") != old:
                        click_control(prefix + ":existing-only", hold=.15)
                    click_control(prefix + ":existing-only", hold=.15)
                    assert metric(prefix, "checked:existing-only") != old
                    app = restart(root); window = ready_window(); ready(prefix, 6)
                    assert metric(prefix, "checked:existing-only") != old
                    results.append("An obstructed private atomic state write visibly failed without replacing durable bytes; restoring the destination allowed a real retry/restart with no note edits")

                def failure(name, error):
                    observations[name] = {"status": "failed", "error": repr(error), "traceback": traceback.format_exc()}
                    capture("native-graph-state-failure-" + name + ".png")
                    (reports / "native-graph-state-results.json").write_text(json.dumps(observations, indent=2) + "\n")

                def success(name, count):
                    assert all(path.read_bytes() == original for path, original in originals.items()), "Graph controls changed original note bytes"
                    observations[name] = {"status": "passed", "checks": count}
                    (reports / "native-graph-state-results.json").write_text(json.dumps(observations, indent=2) + "\n")

                try:
                    run_checks([node_drag_case, global_restart_case, local_restart_case, vault_isolation_case, write_failure_case],
                               failure, count_checks=lambda: len(results), on_success=success)
                finally:
                    assert all(path.read_bytes() == original for path, original in originals.items()), "A graph workflow changed original note bytes"
                open_vault(fixture); open_note("Home")

            def palette_queue_checks():
                sources={p.relative_to(fixture).as_posix():hashlib.sha256(p.read_bytes()).hexdigest() for p in fixture.rglob("*.md")}
                for shortcut in ("ctrl+p","ctrl+o"):
                    command("Open settings")
                    queued_steps(("key","Escape"),("key",shortcut))
                    pause()
                    assert control_bounds("dialog-input-focused",required=False), "Earlier Escape closed the newly opened command palette or quick-open"
                    type_text("Toggle right sidebar" if shortcut=="ctrl+p" else "Home")
                    if shortcut=="ctrl+p":
                        assert control_bounds("command:Toggle right sidebar")
                        key("Escape")
                    else:
                        key("Return");title_contains("Home")
                for shortcut in ("ctrl+p","ctrl+o"):
                    for query in (None,"Queued query must not enter the note"):
                        steps=[("key",shortcut)]
                        if query is not None:steps.append(("text",query))
                        queued_steps(*steps,("key","Escape"));pause()
                        assert control_bounds("dialog-input",required=False) is None, "Escape after the opening shortcut must still dismiss it"
                        key("ctrl+s")
                        assert hashlib.sha256((fixture/"Home.md").read_bytes()).hexdigest()==sources["Home.md"], ("Dismissed dialog text leaked into the note",shortcut,query)
                for name,digest in sources.items():assert hashlib.sha256((fixture/name).read_bytes()).hexdigest()==digest,name
                results.append("An earlier queued Escape dismissed Settings without closing the replacement palette/quick-open or inserting its query into notes; a later Escape still dismissed it")
                before=control_bounds("sidebar:details",required=False) is not None
                def queued(query):
                    os.kill(app.pid,signal.SIGSTOP)
                    try:
                        xdo("key","--clearmodifiers","ctrl+p")
                        xdo("type","--clearmodifiers","--delay","1","--",query)
                        xdo("key","--clearmodifiers","Return")
                    finally:
                        os.kill(app.pid,signal.SIGCONT)
                    pause();pause()
                queued("Toggle right sidebar")
                capture("native-palette-queued.png")
                after=control_bounds("sidebar:details",required=False) is not None
                assert after != before,("Queued palette did not execute the typed command",before,after)
                assert control_bounds("workspace-manager",required=False) is None
                results.append("The palette received shortcut, full query and Enter together and executed the requested sidebar action")
                queued("NoSuchNativeCommandInvalid123456")
                assert control_bounds("dialog-input")
                assert control_bounds("workspace-manager",required=False) is None
                assert (control_bounds("sidebar:details",required=False) is not None)==after
                key("Escape")
                queued("Toggle right sidebar")
                assert (control_bounds("sidebar:details",required=False) is not None)==before
                for name,digest in sources.items():assert hashlib.sha256((fixture/name).read_bytes()).hexdigest()==digest,name
                results.append("A queued unknown command performed no action; a second valid query restored the sidebar and every Markdown byte stayed unchanged")
            if args.section == "graph-state":
                graph_state_checks()
                (reports / "gui-results.json").write_text(json.dumps({"checks": results, "status": "passed"}, indent=2) + "\n")
                print(json.dumps({"status": "passed", "checks": len(results), "section": "graph-state"}))
                return
            if args.section == "palette-queued":
                palette_queue_checks()
                (reports/"gui-results.json").write_text(json.dumps({"checks":results,"status":"passed"},indent=2)+"\n")
                print(json.dumps({"status":"passed","checks":len(results),"section":"palette-queued"}))
                return
            def unicode_queue_checks():
                note=fixture/"Keyboard calibration.md";original="# Keyboard calibration\n\n"
                note.write_text(original);command("Refresh vault");pause();open_note("Keyboard calibration");command("Source editing");editor_end()
                type_text("ASCII_BASE");key("ctrl+s")
                assert note.read_text()==original+"ASCII_BASE"
                modified_shortcut(xdo,"a");paste_text(original,held=True);key("ctrl+s")
                assert note.read_text()==original
                suffix="سلام π صفحه‌کلید"
                os.kill(app.pid,signal.SIGSTOP)
                try:
                    xdo("type","--clearmodifiers","--delay","100","--",suffix)
                finally:
                    os.kill(app.pid,signal.SIGCONT)
                pause();pause();key("ctrl+s")
                actual=note.read_text()
                (reports/"unicode-keyboard-calibration.json").write_text(json.dumps({"stable_keymap":os.environ.get("RUSTIDIAN_STABLE_UNICODE","1")=="1","expected":original+suffix,"actual":actual,"exact":actual==original+suffix},ensure_ascii=False,indent=2)+"\n")
                capture("native-unicode-keyboard.png")
                assert actual==original+suffix,repr(actual)
                results.append("ASCII typing and queued Persian, Greek and ZWNJ key events saved exact bytes through the same native editor")
            if args.section == "unicode-queued":
                unicode_queue_checks()
                (reports/"gui-results.json").write_text(json.dumps({"checks":results,"status":"passed"},indent=2)+"\n")
                print(json.dumps({"status":"passed","checks":len(results),"section":"unicode-queued"}))
                return
            if args.section == "emoji":
                emoji_checks()
                (reports / "gui-results.json").write_text(json.dumps({"checks": results, "status": "passed"}, indent=2)+"\n")
                print(json.dumps({"status": "passed", "checks": len(results), "section": args.section}))
                return
            if args.section == "graph":
                graph_checks()
                (reports / "gui-results.json").write_text(json.dumps({"checks": results, "status": "passed"}, indent=2)+"\n")
                print(json.dumps({"status": "passed", "checks": len(results), "section": args.section}))
                return
            if args.section == "system-open":
                system_open_checks()
                (reports / "gui-results.json").write_text(json.dumps({"checks": results, "status": "passed"}, indent=2)+"\n")
                print(json.dumps({"status": "passed", "checks": len(results), "section": args.section}))
                return
            if args.section == "recovery":
                recovery_checks()
                (reports/"gui-results.json").write_text(json.dumps({"checks":results,"status":"passed"},indent=2)+"\n")
                print(json.dumps({"status":"passed","checks":len(results),"section":"recovery"}))
                return
            if args.section == "workspaces":
                workspace_checks()
                (reports / "gui-results.json").write_text(json.dumps({"checks": results, "status": "passed"}, indent=2)+"\n")
                print(json.dumps({"status": "passed", "checks": len(results), "section": "workspaces"}))
                return
            if args.section == "note-aids":
                note_aids_checks()
                (reports / "gui-results.json").write_text(json.dumps({"checks": results, "status": "passed"}, indent=2)+"\n")
                print(json.dumps({"status": "passed", "checks": len(results), "section": "note-aids"}))
                return
            if args.section == "file-move":
                file_move_checks()
                (reports / "gui-results.json").write_text(json.dumps({"checks": results, "status": "passed"}, indent=2)+"\n")
                print(json.dumps({"status": "passed", "checks": len(results), "section": "file-move"}))
                return
            if args.section == "file-copy":
                file_copy_checks()
                (reports / "gui-results.json").write_text(json.dumps({"checks": results, "status": "passed"}, indent=2)+"\n")
                print(json.dumps({"status": "passed", "checks": len(results), "section": "file-copy"}))
                return
            if args.section == "tab-actions":
                tab_action_checks()
                (reports / "gui-results.json").write_text(json.dumps({"checks": results, "status": "passed"}, indent=2)+"\n")
                print(json.dumps({"status": "passed", "checks": len(results), "section": "tab-actions"}))
                return
            if args.section == "note-creators":
                note_creator_checks()
                (reports / "gui-results.json").write_text(json.dumps({"checks": results, "status": "passed"}, indent=2)+"\n")
                print(json.dumps({"status": "passed", "checks": len(results), "section": "note-creators"}))
                return
            if args.section == "chrome":
                chrome_checks()
                (reports / "gui-results.json").write_text(json.dumps({"checks": results, "status": "passed"}, indent=2)+"\n")
                print(json.dumps({"status": "passed", "checks": len(results), "section": "chrome"}))
                return
            if args.section == "bookmarks":
                bookmark_checks()
                (reports / "gui-results.json").write_text(json.dumps({"checks": results, "status": "passed"}, indent=2)+"\n")
                print(json.dumps({"status": "passed", "checks": len(results), "section": "bookmarks"}))
                return
            if args.section == "callouts":
                callout_checks()
                (reports / "gui-results.json").write_text(json.dumps({"checks": results, "status": "passed"}, indent=2)+"\n")
                print(json.dumps({"status": "passed", "checks": len(results), "section": "callouts"}))
                return
            if args.section == "persian":
                persian_checks()
                (reports / "gui-results.json").write_text(json.dumps({"checks": results, "status": "passed"}, indent=2)+"\n")
                print(json.dumps({"status": "passed", "checks": len(results), "section": "persian"}))
                return
            if args.section == "note-dialogs":
                note_dialog_checks()
                (reports / "gui-results.json").write_text(json.dumps({"checks": results, "status": "passed"}, indent=2)+"\n")
                print(json.dumps({"status": "passed", "checks": len(results), "section": "note-dialogs"}))
                return
            if args.section == "palettes":
                palette_checks()
                (reports / "gui-results.json").write_text(json.dumps({"checks": results, "status": "passed"}, indent=2)+"\n")
                print(json.dumps({"status": "passed", "checks": len(results), "section": "palettes"}))
                return
            if args.section == "tab-keys":
                tab_drag_checks(keys_only=True)
                (reports / "gui-results.json").write_text(json.dumps({"checks": results, "status": "passed"}, indent=2)+"\n")
                print(json.dumps({"status": "passed", "checks": len(results), "section": "tab-keys"}))
                return
            if args.section == "tab-drag":
                tab_drag_checks()
                (reports / "gui-results.json").write_text(json.dumps({"checks": results, "status": "passed"}, indent=2)+"\n")
                print(json.dumps({"status": "passed", "checks": len(results), "section": "tab-drag"}))
                return
            if args.section == "trash-options":
                trash_options_checks()
                (reports / "gui-results.json").write_text(json.dumps({"checks": results, "status": "passed"}, indent=2)+"\n")
                print(json.dumps({"status": "passed", "checks": len(results), "section": "trash-options"}))
                return
            if args.section == "table-resize":
                table_resize_checks()
                (reports / "gui-results.json").write_text(json.dumps({"checks": results, "status": "passed"}, indent=2)+"\n")
                print(json.dumps({"status": "passed", "checks": len(results), "section": "table-resize"}))
                return
            if args.section == "table-input":
                table_input_checks()
                (reports / "gui-results.json").write_text(json.dumps({"checks": results, "status": "passed"}, indent=2)+"\n")
                print(json.dumps({"status": "passed", "checks": len(results), "section": "table-input"}))
                return
            if args.section == "table-edges":
                table_edge_checks()
                (reports / "gui-results.json").write_text(json.dumps({"checks": results, "status": "passed"}, indent=2)+"\n")
                print(json.dumps({"status": "passed", "checks": len(results), "section": "table-edges"}))
                return
            if args.section == "table-links":
                table_link_checks()
                (reports / "gui-results.json").write_text(json.dumps({"checks": results, "status": "passed"}, indent=2)+"\n")
                print(json.dumps({"status": "passed", "checks": len(results), "section": "table-links"}))
                return
            if args.section == "table-edit":
                table_edit_checks()
                table_link_checks()
                table_input_checks()
                (reports / "gui-results.json").write_text(json.dumps({"checks": results, "status": "passed"}, indent=2)+"\n")
                print(json.dumps({"status": "passed", "checks": len(results), "section": "table-edit"}))
                return
            if args.section == "inline-styles":
                inline_styles_checks()
                (reports / "gui-results.json").write_text(json.dumps({"checks": results, "status": "passed"}, indent=2)+"\n")
                print(json.dumps({"status": "passed", "checks": len(results), "section": "inline-styles"}))
                return
            if args.section == "tables":
                table_checks()
                (reports / "gui-results.json").write_text(json.dumps({"checks": results, "status": "passed"}, indent=2)+"\n")
                print(json.dumps({"status": "passed", "checks": len(results), "section": "tables"}))
                return
            if args.section == "vaults":
                vault_checks()
                (reports / "gui-results.json").write_text(json.dumps({"checks": results, "status": "passed"}, indent=2)+"\n")
                print(json.dumps({"status": "passed", "checks": len(results), "section": "vaults"}))
                return
            if args.section == "reading-code":
                reading_code_checks()
                (reports / "gui-results.json").write_text(json.dumps({"checks": results, "status": "passed"}, indent=2)+"\n")
                print(json.dumps({"status": "passed", "checks": len(results), "section": "reading-code"}))
                return
            if args.section == "keyboard-focus":
                keyboard_focus_checks()
                (reports / "gui-results.json").write_text(json.dumps({"checks": results, "status": "passed"}, indent=2)+"\n")
                print(json.dumps({"status": "passed", "checks": len(results), "section": "keyboard-focus"}))
                return
            if args.section == "syntax":
                syntax_checks()
                (reports / "gui-results.json").write_text(json.dumps({"checks": results, "status": "passed"}, indent=2)+"\n")
                print(json.dumps({"status": "passed", "checks": len(results), "section": "syntax"}))
                return
            if args.section == "tab-overflow":
                tab_overflow_checks()
                (reports / "gui-results.json").write_text(json.dumps({"checks": results, "status": "passed"}, indent=2)+"\n")
                print(json.dumps({"status": "passed", "checks": len(results), "section": "tab-overflow"}))
                return
            if args.section == "code":
                code_checks()
                (reports / "gui-results.json").write_text(json.dumps({"checks": results, "status": "passed"}, indent=2)+"\n")
                print(json.dumps({"status": "passed", "checks": len(results), "section": "code"}))
                return
            if args.section == "typography":
                typography_checks()
                (reports / "gui-results.json").write_text(json.dumps({"checks": results, "status": "passed"}, indent=2)+"\n")
                print(json.dumps({"status": "passed", "checks": len(results), "section": "typography"}))
                return
            if args.section == "folding":
                folding_checks()
                (reports / "gui-results.json").write_text(json.dumps({"checks": results, "status": "passed"}, indent=2)+"\n")
                print(json.dumps({"status": "passed", "checks": len(results), "section": "folding"}))
                return
            if args.section == "file-toolbar":
                file_toolbar_checks()
                (reports / "gui-results.json").write_text(json.dumps({"checks": results, "status": "passed"}, indent=2)+"\n")
                print(json.dumps({"status": "passed", "checks": len(results), "section": "file-toolbar"}))
                return
            if args.section == "tab-state":
                tab_state_checks()
                (reports / "gui-results.json").write_text(json.dumps({"checks": results, "status": "passed"}, indent=2)+"\n")
                print(json.dumps({"status": "passed", "checks": len(results), "section": "tab-state"}))
                return
            if args.section == "vault-switch":
                vault_switch_checks()
                (reports / "gui-results.json").write_text(json.dumps({"checks": results, "status": "passed"}, indent=2)+"\n")
                print(json.dumps({"status": "passed", "checks": len(results), "section": "vault-switch"}))
                return
            if args.section == "explorer":
                explorer_checks()
                (reports / "gui-results.json").write_text(json.dumps({"checks": results, "status": "passed"}, indent=2)+"\n")
                print(json.dumps({"status": "passed", "checks": len(results), "section": "explorer"}))
                return
            if args.section == "folders":
                folder_checks()
                (reports / "gui-results.json").write_text(json.dumps({"checks": results, "status": "passed"}, indent=2)+"\n")
                print(json.dumps({"status": "passed", "checks": len(results), "section": "folders"}))
                return
            if args.section == "large-search":
                large_search_checks()
                (reports / "gui-results.json").write_text(json.dumps({"checks": results, "status": "passed"}, indent=2)+"\n")
                print(json.dumps({"status": "passed", "checks": len(results), "section": "large-search"}))
                return
            if args.section in ("hover", "title", "scrolling", "diagrams", "task-clipping"):
                {"hover":hover_checks, "title":title_checks, "scrolling":scrolling_checks, "diagrams":diagram_checks, "task-clipping":task_clip_checks}[args.section]()
                (reports / "gui-results.json").write_text(json.dumps({"checks":results,"status":"passed"},indent=2)+"\n")
                print(json.dumps({"status":"passed","checks":len(results),"section":args.section}))
                return
            if args.section == "quick-ranking":
                quick_ranking_checks()
                (reports / "gui-results.json").write_text(json.dumps({"checks":results,"status":"passed"},indent=2)+"\n")
                print(json.dumps({"status":"passed","checks":len(results),"section":"quick-ranking"}))
                return
            if args.section == "undo":
                undo_checks()
                (reports/"gui-results.json").write_text(json.dumps({"status":"passed","checks":results},indent=2)+"\n")
                print(json.dumps({"status":"passed","checks":len(results),"section":"undo"}))
                return
            if args.section == "settings":
                settings_checks()
                (reports/"gui-results.json").write_text(json.dumps({"status":"passed","checks":results},indent=2)+"\n")
                print(json.dumps({"status":"passed","checks":len(results),"section":"settings"}))
                return
            if args.section == "font-controls":
                font_controls_checks()
                (reports/"gui-results.json").write_text(json.dumps({"status":"passed","checks":results},indent=2)+"\n")
                print(json.dumps({"status":"passed","checks":len(results),"section":"font-controls"}))
                return
            if args.section == "visual-controls":
                visual_controls_checks()
                (reports/"gui-results.json").write_text(json.dumps({"status":"passed","checks":results},indent=2)+"\n")
                print(json.dumps({"status":"passed","checks":len(results),"section":"visual-controls"}))
                return
            if args.section == "tab-restore":
                tab_restore_checks()
                (reports/"gui-results.json").write_text(json.dumps({"status":"passed","checks":results},indent=2)+"\n")
                print(json.dumps({"status":"passed","checks":len(results),"section":"tab-restore"}))
                return
            if args.section == "page-preview":
                page_preview_checks()
                (reports/"gui-results.json").write_text(json.dumps({"status":"passed","checks":results},indent=2)+"\n")
                print(json.dumps({"status":"passed","checks":len(results),"section":"page-preview"}))
                return

            if args.section == "navigation":
                navigation_checks()
                (reports / "gui-results.json").write_text(json.dumps({"checks": results, "status": "passed"}, indent=2)+"\n")
                print(json.dumps({"status": "passed", "checks": len(results), "section": args.section}))
                return
            if args.section == "native-highlights":
                native_highlight_checks()
                (reports/"gui-results.json").write_text(json.dumps({"checks":results,"status":"passed"},indent=2)+"\n")
                print(json.dumps({"status":"passed","checks":len(results),"section":"native-highlights"}))
                return
            if args.section == "colors":
                color_checks()
                (reports / "gui-results.txt").write_text("\n".join(results) + "\n")
                (reports / "gui-results.json").write_text(json.dumps({"checks": results, "status": "passed"}, indent=2)+"\n")
                print(json.dumps({"status": "passed", "checks": len(results), "section": args.section}))
                return
            if args.section == "essential":
                essential_checks()
                (ROOT / "reports/gui-results.json").write_text(json.dumps({"checks": results, "status": "passed"}, indent=2) + "\n")
                print(json.dumps({"status": "passed", "checks": len(results), "section": args.section}))
                return
            if args.section == "tasks":
                task_checks()
                task_clip_checks()
                (reports / "gui-results.json").write_text(json.dumps({"checks": results, "status": "passed"}, indent=2)+"\n")
                print(json.dumps({"status": "passed", "checks": len(results), "section": "tasks"}))
                return
            if args.section == "attachments":
                attachment_tab_checks()
                (reports / "gui-results.json").write_text(json.dumps({"checks": results, "status": "passed"}, indent=2)+"\n")
                print(json.dumps({"status": "passed", "checks": len(results), "section": args.section}))
                return
            if args.section == "images":
                image_checks()
                (reports / "gui-results.json").write_text(json.dumps({"checks": results, "status": "passed"}, indent=2)+"\n")
                print(json.dumps({"status": "passed", "checks": len(results), "section": "images"}))
                return
            if args.section == "properties":
                properties_checks()
                (reports / "gui-results.json").write_text(json.dumps({"checks": results, "status": "passed"}, indent=2)+"\n")
                print(json.dumps({"status": "passed", "checks": len(results), "section": "properties"}))
                return

            tab_state_checks()
            open_note("Home")
            command("Live preview")
            home = fixture / "Home.md"
            original_files = sorted(str(path.relative_to(fixture)) for path in fixture.rglob("*.md"))
            # Default live preview is editable, and debounce saves without Ctrl+S.
            editor_end()
            type_text("\nAUTOSAVE_PROBE\n")
            time.sleep(1)
            assert "AUTOSAVE_PROBE" in home.read_text()
            capture("native-live-preview.png")
            results.append("Live preview edited original Markdown and autosaved without Ctrl+S")

            command("Reading view")
            xdo("mousemove", "650", "120", "click", "1")
            title_contains("Home")
            type_text("READING_MODE_MUST_NOT_EDIT")
            key("ctrl+s")
            assert "READING_MODE_MUST_NOT_EDIT" not in home.read_text()
            capture("native-reading.png")
            results.append("Reading view rejected text input")

            # Exercise the generated new-tab icon rather than only its shortcut.
            click_control("new-tab")
            pause()
            title_contains("New tab")
            capture("native-empty-tab.png")
            assert sorted(str(path.relative_to(fixture)) for path in fixture.rglob("*.md")) == original_files
            key("ctrl+w")
            title_contains("Home")
            results.append("Generated new-tab icon opened an empty workspace without creating a note")

            open_note("First steps")
            key("alt+Left")
            title_contains("Home")
            key("alt+Right")
            title_contains("First steps")
            key("ctrl+w")
            title_contains("New tab")
            results.append("Ordinary quick-open reused one tab; history navigated notes; closing left no extra tabs")

            open_note("Home")
            command("Source editing")
            editor_end()
            type_text("\n[[fststp")
            capture("native-link-completion.png")
            key("Tab")
            key("ctrl+s")
            assert home.read_text().endswith("[[Notes/First steps]]")
            assert "[[fststp" not in home.read_text()
            key("ctrl+Return")
            title_contains("First steps")
            command("Source editing")
            editor_end()
            type_text("\n[[Notes/Created by link]]")
            key("ctrl+Return")
            title_contains("Created by link")
            assert (fixture / "Notes/Created by link.md").is_file()
            key("F2")
            key("ctrl+a")
            type_text("Moved/Renamed note.md")
            key("Return")
            title_contains("Renamed note")
            assert not (fixture / "Notes/Created by link.md").exists()
            assert (fixture / "Moved/Renamed note.md").exists()
            assert "[[Moved/Renamed note]]" in (fixture / "Notes/First steps.md").read_text()
            key("F2")
            key("ctrl+a")
            type_text("Home.md")
            before_collision = home.read_text()
            key("Return")
            assert home.read_text() == before_collision
            title_contains("Renamed note")
            key("Escape")
            deleted_bytes = (fixture / "Moved/Renamed note.md").read_bytes()
            command("Move file to system Trash")
            key("Return")
            title_contains("New tab")
            assert control_bounds("tab-full:Notes/First steps.md")
            assert control_bounds("editor", required=False) is None
            assert control_bounds("close-tab:Moved/Renamed note.md", required=False) is None
            assert home.read_text() == before_collision
            assert not (fixture / "Moved/Renamed note.md").exists()
            trashed = list((base / "data/Trash/files").glob("Renamed note*"))
            assert any(path.read_bytes() == deleted_bytes for path in trashed)
            results.append("F2 moved a note and updated links; collision preserved both files; deletion remained recoverable in system Trash")
            key("ctrl+o")
            type_text("cnnctdthnk")
            key("Return")
            title_contains("Connected thinking")
            results.append("Wiki-link completion, Ctrl+Enter navigation, missing-note creation and fuzzy quick-open used native paths")
            # Ctrl+Enter opens another tab; finish that scenario before the
            # independent two-tab history and view-mode checks.
            key("ctrl+w")
            assert control_bounds("tab-full:Notes/Connected thinking.md", required=False) is None
            close_tabs()
            open_note("Home")
            command("Reading view")
            key("ctrl+t")
            open_note("First steps")
            key("alt+Left")
            title_contains("First steps")
            command("Source editing")
            open_note("Connected thinking")
            key("alt+Left")
            title_contains("First steps")
            key("ctrl+Tab")
            title_contains("Home")
            editor_end()
            type_text("OTHER_TAB_MUST_STAY_READING")
            assert "OTHER_TAB_MUST_STAY_READING" not in home.read_text()
            key("ctrl+Tab")
            title_contains("First steps")
            results.append("Each tab retained its own history and reading/editing mode")

            note = fixture / "Notes/First steps.md"
            editor_end()
            type_text("\nFORMAT_PROBE")
            key("ctrl+shift+Left")
            key("ctrl+b")
            key("ctrl+s")
            assert "**FORMAT_PROBE**" in note.read_text()
            before_undo = note.read_text()
            key("ctrl+z")
            key("ctrl+s")
            assert note.read_text() != before_undo
            key("ctrl+y")
            key("ctrl+s")
            assert note.read_text() == before_undo
            results.append("Bold formatting and native undo/redo preserved the expected source")

            key("ctrl+f")
            type_text("FORMAT_PROBE")
            capture("native-find.png")
            key("Escape")
            key("ctrl+b")
            key("ctrl+s")
            assert "**FORMAT_PROBE**" not in note.read_text()
            assert "FORMAT_PROBE" in note.read_text()
            results.append("Find selected the requested text; formatting toggled that exact selection")

            external = "# External reload\nThe watcher must reload this clean buffer.\n"
            note.write_text(external)
            time.sleep(1.2)
            editor_end()
            type_text("AFTER_RELOAD")
            time.sleep(1)
            assert note.read_text().startswith(external)
            assert "AFTER_RELOAD" in note.read_text()
            results.append("Filesystem notification reloaded a clean buffer before subsequent edits")

            type_text("\nUNSAVED_CONFLICT_PROBE\n")
            note.write_text("# External version\nKeep the disk version.\n")
            time.sleep(1.1)
            assert note.read_text() == "# External version\nKeep the disk version.\n"
            capture("external-conflict.png")
            assert control_bounds("error-save-copy")
            open_before = xdo("getwindowname", window)
            key("ctrl+o")
            type_text("Connected thinking")
            key("Return")
            assert xdo("getwindowname", window) == open_before
            key("ctrl+w")
            capture("unsaved-changes.png")
            key("Escape")
            title_contains("First steps")
            results.append("Autosave conflict preserved disk/buffer; navigation and dirty close could not discard edits")

            command("Save a copy")
            key("ctrl+a")
            type_text("Notes/Recovered edits.md")
            key("Return")
            title_contains("Recovered edits")
            assert "UNSAVED_CONFLICT_PROBE" in (fixture / "Notes/Recovered edits.md").read_text()
            assert note.read_text() == "# External version\nKeep the disk version.\n"
            results.append("Conflicted edits recovered into a separate note without overwriting disk")

            create_note_named('Notes/Created in GUI.md')
            title_contains("Created in GUI")
            assert (fixture / "Notes/Created in GUI.md").read_text() == ""
            command("Split source")
            capture("native-split.png")
            xdo("windowsize", window, "860", "560")
            pause()
            capture("native-compact.png", (860, 560))
            xdo("windowsize", window, "1440", "960")
            pause()
            command("Switch appearance")
            capture("native-light.png")
            results.append("New note, split, minimum-size and light views rendered successfully")

            # A deliberate quit is guarded because the conflicted buffer remains open.
            key("ctrl+q")
            capture("native-exit-guard.png")
            key("Escape")
            assert app.poll() is None
            results.append("Quitting with a conflicted tab was canceled without losing the running app")
            # Restart once without the deliberately conflicted buffer, then test a clean exit.
            app.terminate()
            app.wait(timeout=5)
            app = launch_app(launch_command)
            window = ready_window()
            # Rename/bookmark actions may already have persisted an older layout.
            # Arrange the two-tab restart case through real close actions.
            close_tabs()
            open_note("Home")
            command("Reading view")
            key("ctrl+t")
            open_note("Created in GUI")
            command("Source editing")
            xdo("key", "--clearmodifiers", "ctrl+q")
            app.wait(timeout=5)
            session_files = list((base / "state/rustidian").glob("*.json"))
            assert len(session_files) == 1
            session = json.loads(session_files[0].read_text())
            assert len(session["tabs"]) == 2 and session["active"] == 1
            assert [path for path, view in session["tabs"]] == ["Home.md", "Notes/Created in GUI.md"]
            assert [view for path, view in session["tabs"]] == ["Read", "Source"]
            app = launch_app(launch_command)
            window = ready_window()
            title_contains("Created in GUI")
            key("ctrl+Tab")
            title_contains("Home")
            capture("native-restored.png")
            editor_end()
            type_text("RESTORED_READ_ONLY_MUST_NOT_EDIT")
            assert "RESTORED_READ_ONLY_MUST_NOT_EDIT" not in home.read_text()
            key("ctrl+Tab")
            editor_end()
            type_text("RESTORED_SOURCE_EDIT")
            key("ctrl+s")
            assert "RESTORED_SOURCE_EDIT" in (fixture / "Notes/Created in GUI.md").read_text()
            results.append("Clean exit/restart restored selected tab and independent reading/source modes")
            (reports/"gui-all-baseline.json").write_text(json.dumps({"status":"passed","checks":results.copy()},indent=2)+"\n")
            failure_details = []
            (reports / "gui-all-failures.json").write_text("[]\n")

            def retain_failure(name, error):
                capture(f"gui-all-{name}-failure.png")
                candidates = [path for path in fixture.rglob("*") if path.is_file() and path.suffix.lower() in (".md", ".txt")]
                recent = sorted(candidates, key=lambda path: path.stat().st_mtime_ns, reverse=True)[:5]
                (reports / f"gui-all-{name}-failure-notes.json").write_text(json.dumps({str(path.relative_to(fixture)): path.read_text() for path in recent if path.stat().st_size <= 500_000}, ensure_ascii=False, indent=2)+"\n")
                failure_details.append({"workflow": name, "error": repr(error), "traceback": traceback.format_exc()})
                (reports / "gui-all-failures.json").write_text(json.dumps(failure_details, indent=2)+"\n")
                print(traceback.format_exc(), flush=True)
                # Close failed fixture menus only; retain unsaved text for diagnosis.
                xdo("key", "--clearmodifiers", "Escape", "Escape")
                pause()

            workflow_results=[]
            (reports/"gui-all-workflows.json").write_text("[]\n")
            def retain_success(name,completed):
                workflow_results.append({"workflow":name,"checks":completed,"status":"passed"})
                (reports/"gui-all-workflows.json").write_text(json.dumps(workflow_results,indent=2)+"\n")
            run_checks([
                properties_checks,
                image_checks,
                attachment_tab_checks,
                file_copy_checks,
                file_move_checks,
                task_checks,
                task_clip_checks,
                essential_checks,
                undo_checks,
                color_checks,
                native_highlight_checks,
                navigation_checks,
                page_preview_checks,
                quick_ranking_checks,
                hover_checks,
                title_checks,
                scrolling_checks,
                diagram_checks,
                large_search_checks,
                folder_checks,
                explorer_checks,
                vault_checks,
                vault_switch_checks,
                system_open_checks,
                graph_checks,
                graph_state_checks,
                visual_controls_checks,
                emoji_checks,
                file_toolbar_checks,
                folding_checks,
                typography_checks,
                code_checks,
                syntax_checks,
                tab_overflow_checks,
                keyboard_focus_checks,
                reading_code_checks,
                table_checks,
                inline_styles_checks,
                table_edit_checks,
                table_link_checks,
                table_input_checks,
                table_resize_checks,
                table_edge_checks,
                trash_options_checks,
                palette_checks,
                callout_checks,
                bookmark_checks,
                persian_checks,
                note_aids_checks,
                note_creator_checks,
                tab_action_checks,
                tab_restore_checks,
                tab_drag_checks,
                settings_checks,
                font_controls_checks,
                workspace_checks,
                palette_queue_checks,
                unicode_queue_checks,
                recovery_checks,
            ], retain_failure,count_checks=lambda:len(results),on_success=retain_success)
            (reports / "gui-results.json").write_text(json.dumps({"checks": results, "status": "passed"}, indent=2)+"\n")
            print(json.dumps({"status": "passed", "checks": len(results), "report": str(reports / "gui-results.json")}))
        except BaseException:
            log.flush()
            shutil.copyfile(reports / "gui-smoke.log", reports / f"gui-{args.section}-failure-app.log")
            capture("gui-failure.png")
            capture(f"gui-{args.section}-failure.png")
            candidates = [path for path in fixture.rglob("*") if path.is_file() and path.suffix.lower() in (".md", ".txt")]
            recent = sorted(candidates, key=lambda path: path.stat().st_mtime_ns, reverse=True)[:5]
            (reports / f"gui-{args.section}-failure-notes.json").write_text(json.dumps({str(path.relative_to(fixture)): path.read_text() for path in recent if path.stat().st_size <= 500_000}, ensure_ascii=False, indent=2)+"\n")
            (reports / "gui-results.json").write_text(json.dumps({"status": "failed", "checks": results}, indent=2)+"\n")
            raise
        finally:
            shutil.copyfile(reports / "gui-results.json", reports / f"gui-{args.section}-results.json")
            app.terminate()
            try:
                app.wait(timeout=5)
            except subprocess.TimeoutExpired:
                app.kill()
                app.wait()
            if window_manager is not None:
                window_manager.terminate()
                window_manager.wait(timeout=5)
            if unicode_keyboard is not None:
                unicode_keyboard.close()
            server.terminate()
            server.wait(timeout=5)
            xvfb_log.close()
            log.close()


if __name__ == "__main__":
    main()
