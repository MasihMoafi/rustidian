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

from PIL import Image, ImageGrab

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
from native_input import UnicodeKeyboard, modified_shortcut, save_shortcut, wait_autosave


def pixel_data(picture):
    """Support the Pillow version supplied by the declared Ubuntu runner."""
    modern = getattr(picture, "get_flattened_data", None)
    return modern() if modern else picture.getdata()


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
    parser.add_argument("--binary", default="target/debug/rust-notes")
    parser.add_argument("--default-vault", action="store_true", help="Test an extracted executable beside its sample vault, without a vault argument")
    parser.add_argument("--open-file", choices=["md", "txt"], help="Check opening and saving an individual document")
    parser.add_argument("--scroll-document", type=Path, help="Read-only source copied into the private scrolling fixture")
    parser.add_argument("--section", choices=["all", "properties", "images", "tasks", "task-clipping", "large-search", "folders", "explorer", "vault-switch", "vaults", "tab-state", "file-toolbar", "folding", "typography", "code", "tab-overflow", "syntax", "keyboard-focus", "reading-code", "tables", "table-edit", "table-links", "table-input", "callouts", "persian", "attachments", "bookmarks", "essential", "colors", "navigation", "hover", "title", "scrolling", "diagrams", "chrome", "note-aids", "note-creators", "tab-actions", "file-copy", "workspaces", "recovery", "palette-queued", "unicode-queued"], default="all")
    args = parser.parse_args()
    reports = ROOT / "reports"
    reports.mkdir(exist_ok=True)
    run_lock = (reports / ".gui-smoke.lock").open("w")
    try:
        fcntl.flock(run_lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except BlockingIOError:
        raise SystemExit("A GUI run is already active; wait for its result.")
    with tempfile.TemporaryDirectory(prefix="rust-notes-gui-") as temporary:
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
        environment = dict(os.environ, DISPLAY=display, WAYLAND_DISPLAY="", WINIT_UNIX_BACKEND="x11", XDG_CONFIG_HOME=str(base / "config"), XDG_STATE_HOME=str(base / "state"), XDG_DATA_HOME=str(base / "data"), GSETTINGS_BACKEND="memory", RUST_NOTES_UI_PROBE=str(base / "ui-probe.json"))
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
            if os.environ.get("RUST_NOTES_STABLE_UNICODE", "1") == "1":
                unicode_keyboard = UnicodeKeyboard(environment, Path(__file__).read_text())
        except Exception:
            server.terminate()
            server.wait(timeout=5)
            log.close()
            xvfb_log.close()
            raise
        environment.pop("RUST_NOTES_DEFAULT_VAULT",None)
        if args.section == "chrome" and shutil.which("openbox"):
            window_manager = subprocess.Popen(["openbox", "--sm-disable"], env=environment,
                stdout=log, stderr=log)
            time.sleep(.4)
        launch_command = [str(ROOT / args.binary), str(fixture)]
        if args.open_file:
            startup_path = fixture / f"Open me سلام.{args.open_file}"
            startup_text = "# File opening\n\nOriginal text سلام\n"
            startup_path.write_text(startup_text)
            launch_command = [str(ROOT / args.binary), str(startup_path)]
        if args.default_vault:
            bundled_binary = base / "rust-notes"
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
            left, top, right, bottom = control_bounds(name)
            pointer = ("mousemove", "--sync", str(round((left + right) / 2)), str(round((top + bottom) / 2)))
            if hold:
                xdo(*pointer)
                time.sleep(.1)
                assert control_bounds(name) == [left, top, right, bottom]
                xdo("mousedown", button)
                try:
                    time.sleep(hold)
                finally:
                    xdo("mouseup", button)
            else:
                xdo(*pointer, "click", button)
            pause()

        def title_contains(value):
            deadline = time.monotonic() + 5
            while time.monotonic() < deadline:
                assert app.poll() is None, "The native app exited while opening a note"
                title = xdo("getwindowname", window)
                if value in title:
                    return
                time.sleep(.05)
            raise AssertionError((value, title))

        def open_note(value):
            key("ctrl+o")
            type_text(value)
            key("Return")
            title_contains(value)

        def ready_window():
            deadline = time.monotonic() + 20
            window = None
            while time.monotonic() < deadline:
                assert app.poll() is None, (reports / "gui-smoke.log").read_text()
                try:
                    window = xdo("search", "--onlyvisible", "--name", "Rust Notes$").splitlines()[0]
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
            while not probe.exists() and time.monotonic() < deadline:
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
                    if os.environ.get("RUST_NOTES_TRACE_COMMANDS") == "1":
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
                    raise AssertionError(("Requested command availability/result differs",value,available,matches))
                trace("query")
                key("Return")
                if not available:
                    assert control_bounds("dialog-input")
                    assert not any(name.startswith("command:") for name in json.loads((base/"ui-probe.json").read_text()))
                trace("result")

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

            def callout_checks():
                source = (ROOT / "tests/fixtures/callouts.md").read_text()
                note = fixture / "Notes/Callout checks.md"
                note.write_text(source)
                time.sleep(.6)
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
                key("ctrl+n")
                key("ctrl+a")
                type_text("Notes/Properties GUI.md")
                key("Return")
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
                key("ctrl+n")
                key("ctrl+a")
                type_text("Notes/Image GUI.md")
                key("Return")
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
                state = json.loads(next((base / "state/rust-notes").glob("*.json")).read_text())
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
                for label in ["Open in new tab", "Make a copy", "Bookmark…", "Copy path", "Rename…", "Delete"]:
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
                click_control("menu:Delete")
                control_bounds("dialog-input")
                key("Escape")
                assert (fixture / "Menu renamed.md").exists(), "Cancel must preserve the note"
                file_menu("Menu renamed.md")
                click_control("menu:Delete")
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

            def navigation_checks():
                note = fixture / "Notes/Navigation UI.md"
                target = fixture / "Notes/Linked target.md"
                file_uri = target.as_uri()
                source = "# Navigation UI\n\n[[Notes/Linked target|Wiki target]]\n\n[Open target](Linked%20target.md#Section%20two)\n\n[Open file](" + file_uri + ")\n"
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
                before_requests = requests.read_text() if requests.exists() else ""
                click_link(3)
                deadline = time.monotonic() + 3
                while (not requests.exists() or requests.read_text() == before_requests) and time.monotonic() < deadline:
                    time.sleep(.05)
                assert requests.exists() and file_uri in requests.read_text()[len(before_requests):], "File URIs must reach the desktop file handler"
                title_contains("Navigation UI")
                assert not fixture.joinpath("file:").exists(), "A URI must not create spurious notes"
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
                control_bounds("settings-dark")
                click_control("settings-autosave")
                state_files = [p for p in (base / "state" / "rust-notes").glob("*.json") if any(path == "Notes/Essential UI.md" for path, _ in json.loads(p.read_text()).get("tabs", []))]
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
                capture("native-new-note-dialog.png")
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
                key("ctrl+n")
                key("ctrl+a")
                type_text("Reading clipboard.md")
                key("Return")
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
                key("ctrl+n")
                key("ctrl+a")
                type_text("Clipboard result.md")
                key("Return")
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
                key("ctrl+n")
                key("ctrl+a")
                type_text("Notes/Folding GUI.md")
                key("Return")
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
                key("ctrl+n")
                key("ctrl+a")
                type_text("Notes/Nested/Toolbar child.md")
                key("Return")
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
                command("Source editing")
                first = fixture / "Notes/First steps.md"
                original = first.read_text()
                open_note("First steps")
                key("ctrl+End")
                type_text("QUICK_OPEN_FOCUS")
                key("ctrl+s")
                assert first.read_text() == original + "QUICK_OPEN_FOCUS", repr(first.read_text())
                results.append("Quick opening a note accepted immediate keyboard editing without clicking the editor")
                key("ctrl+shift+f")
                key("ctrl+a")
                type_text("path:Notes/")
                assert control_bounds("search:Notes/First steps.md")
                assert first.read_text() == original + "QUICK_OPEN_FOCUS"
                results.append("Global search retained keyboard focus and did not insert its query into the open note")
                key("ctrl+n")
                key("ctrl+a")
                type_text("Notes/Keyboard focus.md")
                key("Return")
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
                    return json.loads((base / "data/rust-notes/vaults.json").read_text())["roots"]

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
                fixture_state = max((base / "state/rust-notes").glob("*.json"), key=lambda path: path.stat().st_mtime_ns)
                key("ctrl+n")
                type_text("Home")
                key("Return")
                type_text("# Created vault\n\nVAULT_CREATED_ORIGINAL\n")
                key("ctrl+s")
                created_source = (created / "Home.md").read_bytes()
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
                assert (created / "Home.md").read_bytes() == created_source
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
                # Hide the picker before giving the parent a physical key. GTK
                # can otherwise reclaim focus and accept that Return itself.
                xdo("windowunmap",picker)
                xdo("windowfocus", "--sync", window)
                pause()
                assert xdo("getwindowfocus")==window
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
                catalog = base / "data/rust-notes/vaults.json"
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
                assert control_bounds("file:Explorer bulk/Note04999.md")
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
                key("ctrl+n")
                key("ctrl+a")
                type_text("GUI empty/Child.md")
                key("Return")
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
                results.append("All 550 search matches remained reachable; scrolling opened the last note beyond the previous 200-result cap")
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
                    for state_path in (base / "state/rust-notes").glob("*.json"):
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
                sessions=[p for p in (base/"state/rust-notes").glob("*.json") if any(path=="Pinned first.md" for path,_ in json.loads(p.read_text()).get("tabs",[]))]
                assert len(sessions)==1
                session_path=sessions[0]
                def pinned():
                    state=json.loads(session_path.read_text())
                    return {path:state["pinned"][index] for index,(path,_) in enumerate(state["tabs"])}
                assert pinned()=={"Pinned first.md":True,"Pinned second.md":False}
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
                app.terminate();app.wait(timeout=5);app=launch_app(launch_command);window=ready_window()
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
                    state=json.loads(session_path.read_text())
                    if state["dark"]!=dark:command("Switch appearance")
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
                sessions=[p for p in (base/"state/rust-notes").glob("*.json") if any(path=="Creator origin.md" for path,_ in json.loads(p.read_text()).get("tabs",[]))]
                assert len(sessions)==1
                session_path=sessions[0]
                expected={"folder":"Creator output","format":"[Pair-]YYYYMMDDHHmm","template":"Creator templates/Unique template.md"}
                assert json.loads(session_path.read_text())["note_aids"]["unique"]==expected
                for dark in (True,False):
                    if json.loads(session_path.read_text())["dark"]!=dark:command("Switch appearance")
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
                valid={name+" — "+random_vault.name+" — Rust Notes" for name in ("One","Two","Three")}
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
                states = [(p, json.loads(p.read_text())) for p in (base / "state/rust-notes").glob("*.json")]
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
                storage = base / "state/rust-notes/workspaces"
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
                assert writing["tabs"] == [{"path":"Workspace A.md","view":"Read","editing_view":"Source"}]
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
                control_bounds("editor-focused")
                modified_shortcut(xdo,"a");modified_shortcut(xdo,"c")
                assert xdo("getwindowname",window).startswith("Workspace B")
                copied = subprocess.check_output(["xclip","-selection","clipboard","-out"],env=environment,text=True)
                assert copied == second_source, repr(copied)
                key("End")
                assert first.read_text() == first_source and second.read_text() == second_source
                results.append("Both layouts restored tab order, active file, per-tab modes and sidebar sizes; actual restored tabs displayed and copied the correct document")

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
                session_files=[p for p in (base/"state/rust-notes").glob("*.json") if any(path=="Workspace A.md" for path,_ in json.loads(p.read_text()).get("tabs",[]))]
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
                sessions=[p for p in (base/"state/rust-notes").glob("*.json") if any(path=="Recovery target.md" for path,_ in json.loads(p.read_text()).get("tabs",[]))]
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
                store=base/"state/rust-notes/recovery"/session_path.with_suffix(".redb").name
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

            def palette_queue_checks():
                sources={p.relative_to(fixture).as_posix():hashlib.sha256(p.read_bytes()).hexdigest() for p in fixture.rglob("*.md")}
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
                (reports/"unicode-keyboard-calibration.json").write_text(json.dumps({"stable_keymap":os.environ.get("RUST_NOTES_STABLE_UNICODE","1")=="1","expected":original+suffix,"actual":actual,"exact":actual==original+suffix},ensure_ascii=False,indent=2)+"\n")
                capture("native-unicode-keyboard.png")
                assert actual==original+suffix,repr(actual)
                results.append("ASCII typing and queued Persian, Greek and ZWNJ key events saved exact bytes through the same native editor")
            if args.section == "unicode-queued":
                unicode_queue_checks()
                (reports/"gui-results.json").write_text(json.dumps({"checks":results,"status":"passed"},indent=2)+"\n")
                print(json.dumps({"status":"passed","checks":len(results),"section":"unicode-queued"}))
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
            if args.section == "table-input":
                table_input_checks()
                (reports / "gui-results.json").write_text(json.dumps({"checks": results, "status": "passed"}, indent=2)+"\n")
                print(json.dumps({"status": "passed", "checks": len(results), "section": "table-input"}))
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
            if args.section == "navigation":
                navigation_checks()
                (reports / "gui-results.json").write_text(json.dumps({"checks": results, "status": "passed"}, indent=2)+"\n")
                print(json.dumps({"status": "passed", "checks": len(results), "section": args.section}))
                return
            if args.section == "colors":
                color_checks()
                (reports / "gui-results.txt").write_text("\n".join(results) + "\n")
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

            key("ctrl+n")
            key("ctrl+a")
            type_text("Notes/Created in GUI.md")
            key("Return")
            title_contains("Created in GUI")
            assert (fixture / "Notes/Created in GUI.md").read_text().startswith("# Created in GUI")
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
            session_files = list((base / "state/rust-notes").glob("*.json"))
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
                task_checks,
                task_clip_checks,
                essential_checks,
                color_checks,
                navigation_checks,
                hover_checks,
                title_checks,
                scrolling_checks,
                diagram_checks,
                large_search_checks,
                folder_checks,
                explorer_checks,
                vault_checks,
                vault_switch_checks,
                file_toolbar_checks,
                folding_checks,
                typography_checks,
                code_checks,
                syntax_checks,
                tab_overflow_checks,
                keyboard_focus_checks,
                reading_code_checks,
                table_checks,
                table_edit_checks,
                table_link_checks,
                table_input_checks,
                callout_checks,
                bookmark_checks,
                persian_checks,
                note_aids_checks,
                note_creator_checks,
                tab_action_checks,
                workspace_checks,
                palette_queue_checks,
                unicode_queue_checks,
                recovery_checks,
            ], retain_failure,count_checks=lambda:len(results),on_success=retain_success)
            (reports / "gui-results.json").write_text(json.dumps({"checks": results, "status": "passed"}, indent=2)+"\n")
            print(json.dumps({"status": "passed", "checks": len(results), "report": str(reports / "gui-results.json")}))
        except BaseException:
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
