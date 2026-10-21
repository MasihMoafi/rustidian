#!/usr/bin/env python3
"""Exercise the real native window on a disposable vault; needs Xvfb, xdotool, Pillow."""
import argparse
import fcntl
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import tempfile
import time

from PIL import Image, ImageGrab

ROOT = Path(__file__).resolve().parents[1]


def pixel_data(picture):
    """Support the Pillow version supplied by the declared Ubuntu runner."""
    modern = getattr(picture, "get_flattened_data", None)
    return modern() if modern else picture.getdata()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", default="target/debug/rust-notes")
    parser.add_argument("--default-vault", action="store_true", help="Test an extracted executable beside its sample vault, without a vault argument")
    parser.add_argument("--section", choices=["all", "properties", "images", "tasks", "large-search", "folders", "explorer", "vault-switch", "tab-state", "file-toolbar", "folding", "typography", "code", "tab-overflow", "syntax", "keyboard-focus", "reading-code", "tables", "table-edit", "table-links", "table-input", "callouts", "persian", "attachments", "bookmarks", "essential"], default="all")
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
            ["Xvfb", "-displayfd", str(write_fd), "-screen", "0", "1440x960x24", "-nolisten", "tcp", "-ac"],
            pass_fds=(write_fd,), stdout=subprocess.DEVNULL, stderr=xvfb_log,
        )
        os.close(write_fd)
        with os.fdopen(read_fd) as display_file:
            display = ":" + display_file.readline().strip()
        assert display[1:].isdigit() and server.poll() is None, (reports / "gui-xvfb.log").read_text()
        environment = dict(os.environ, DISPLAY=display, WAYLAND_DISPLAY="", WINIT_UNIX_BACKEND="x11", XDG_STATE_HOME=str(base / "state"), XDG_DATA_HOME=str(base / "data"), RUST_NOTES_UI_PROBE=str(base / "ui-probe.json"))
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
        launch_command = [str(ROOT / args.binary), str(fixture)]
        if args.default_vault:
            bundled_binary = base / "rust-notes"
            shutil.copy2(ROOT / args.binary, bundled_binary)
            launch_command = [str(bundled_binary)]
        app = subprocess.Popen(launch_command, cwd=base, env=environment, stdout=log, stderr=log)
        results = []
        (reports / "gui-results.json").write_text(json.dumps({"status": "running", "checks": []}, indent=2)+"\n")

        def xdo(*arguments):
            return subprocess.check_output(["xdotool", *arguments], env=environment, text=True, stderr=subprocess.DEVNULL).strip()

        def pause():
            time.sleep(0.25)
            assert app.poll() is None, "The native app exited unexpectedly"

        def key(value):
            xdo("key", "--clearmodifiers", value)
            pause()

        def type_text(value, delay=1):
            for index, line in enumerate(value.split("\n")):
                if index: xdo("key", "--clearmodifiers", "Return")
                if line: xdo("type", "--clearmodifiers", "--delay", str(delay), "--", line)
            pause()

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
            assert not required, f"No visible control: {name}"
            return None

        def click_control(name, button="1"):
            left, top, right, bottom = control_bounds(name)
            xdo("mousemove", str(round((left + right) / 2)), str(round((top + bottom) / 2)), "click", button)
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
            xdo("windowfocus", window)
            xdo("windowsize", window, "1440", "960")
            xdo("windowmove", window, "0", "0")
            pause()
            return window

        try:
            window = ready_window()
            title_contains("Home")
            capture("native-desktop.png")
            results.append("Native window opened; three-pane home captured")

            def command(value):
                key("ctrl+p")
                type_text(value)
                key("Return")

            def editor_end():
                bounds = control_bounds("editor", required=False)
                if bounds:
                    left, top, right, bottom = bounds
                    xdo("mousemove", str(round((left + right) / 2)), str(round(max(90, min(900, top + 20)))), "click", "1")
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
                xdo("mousemove", "70", "20", "click", "1")
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
                    command(unavailable)
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
                key("alt+Left")
                title_contains("Attachment links GUI")
                assert control_bounds("editor")
                editor_end()
                type_text("\n[Open image](../Assets/Viewer%5B1%5D.png)")
                key("ctrl+s")
                key("ctrl+Return")
                title_contains("Viewer[1].png")
                assert control_bounds("attachment-image")
                key("alt+Left")
                title_contains("Attachment links GUI")
                key("alt+Right")
                title_contains("Viewer[1].png")
                assert control_bounds("attachment-image")
                results.append("Image link completion kept its extension; Ctrl+Enter and history navigated between source note and read-only image")

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
                app = subprocess.Popen(launch_command, cwd=base, env=environment, stdout=log, stderr=log)
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
                type_text("control")
                key("ctrl+s")
                assert note.read_text() == source + "control"
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
                app = subprocess.Popen(launch_command, cwd=base, env=environment, stdout=log, stderr=log)
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
                results.append("Reading and Live tasks preserve exact Markdown, nested/Unicode tasks and undo/redo; Source and fenced task syntax stay literal")
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
                assert abs(table[2] - table[0] - 303.171875) < 3, table
                assert abs(table[3] - table[1] - 130.640625) < 1, table
                assert abs(table[1] - 241.84375) < 1, table
                middle = control_bounds("table-cell:0:1:1")
                centered = control_bounds("table-text:0:1:1")
                assert abs(middle[0] + middle[2] - centered[0] - centered[2]) < 1, (middle, centered)
                right = control_bounds("table-cell:0:1:2")
                number = control_bounds("table-text:0:1:2")
                assert abs(right[2] - number[2] - 8.5) < 1, (right, number)
                code_panel = control_bounds("code-block:0")
                assert abs(code_panel[1] - 491.1875) < 1, code_panel
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
                # xdotool remaps a spare X11 keycode for Unicode. Allow the
                # native event loop to consume each mapping before it changes.
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
                xdo("mousemove", "70", "20", "click", "1")
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
                app = subprocess.Popen(launch_command, cwd=base, env=environment, stdout=log, stderr=log)
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
                editor_end()
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
                xdo("mousemove", "70", "20", "click", "1")
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
                xdo("mousemove", "70", "20", "click", "1")
                pause()
                if control_bounds("folder:Bulk", required=False):
                    click_control("folder:Bulk")
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
                type_text("Writing فارسی", delay=100)
                key("Return")
                assert state_with(relative)[1]["bookmarks"]["items"][0]["title"] == "Writing فارسی", repr(state_with(relative)[1]["bookmarks"]["items"][0])
                open_note("Home")
                command("Bookmark current file")
                # Explorer context menu follows the same bookmark operation for images.
                xdo("mousemove", "70", "20", "click", "1")
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
                xdo("mousemove", "70", "20", "click", "1")
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
                app = subprocess.Popen(launch_command, cwd=base, env=environment, stdout=log, stderr=log)
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
            if args.section == "essential":
                essential_checks()
                (ROOT / "reports/gui-results.json").write_text(json.dumps({"checks": results, "status": "passed"}, indent=2) + "\n")
                print(json.dumps({"status": "passed", "checks": len(results), "section": args.section}))
                return
            if args.section == "tasks":
                task_checks()
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
            xdo("mousemove", "573", "20", "click", "1")
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
            command("Move file to system Trash")
            key("Return")
            title_contains("New tab")
            assert not (fixture / "Moved/Renamed note.md").exists()
            trashed = list((base / "data/Trash/files").glob("Renamed note*"))
            assert trashed and trashed[0].read_text().startswith("# Created by link")
            results.append("F2 moved a note and updated links; collision preserved both files; deletion remained recoverable in system Trash")
            key("ctrl+o")
            type_text("cnnctdthnk")
            key("Return")
            title_contains("Connected thinking")
            results.append("Wiki-link completion, Ctrl+Enter navigation, missing-note creation and fuzzy quick-open used native paths")
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
            app = subprocess.Popen(launch_command, cwd=base, env=environment, stdout=log, stderr=log)
            window = ready_window()
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
            assert [view for path, view in session["tabs"]] == ["Read", "Source"]
            app = subprocess.Popen(launch_command, cwd=base, env=environment, stdout=log, stderr=log)
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
            properties_checks()
            image_checks()
            attachment_tab_checks()
            task_checks()
            essential_checks()
            large_search_checks()
            folder_checks()
            explorer_checks()
            vault_switch_checks()
            file_toolbar_checks()
            folding_checks()
            typography_checks()
            code_checks()
            syntax_checks()
            tab_overflow_checks()
            keyboard_focus_checks()
            reading_code_checks()
            table_checks()
            table_edit_checks()
            table_link_checks()
            table_input_checks()
            callout_checks()
            bookmark_checks()
            persian_checks()
            (reports / "gui-results.json").write_text(json.dumps({"checks": results, "status": "passed"}, indent=2)+"\n")
            print(json.dumps({"status": "passed", "checks": len(results), "report": str(reports / "gui-results.json")}))
        except BaseException:
            capture("gui-failure.png")
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
            server.terminate()
            server.wait(timeout=5)
            xvfb_log.close()
            log.close()


if __name__ == "__main__":
    main()
