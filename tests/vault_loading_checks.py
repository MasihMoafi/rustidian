"""Native Desktop-root and cancellable loading checks on private fixture files."""
import hashlib
import json
from pathlib import Path
import subprocess
import time
from types import SimpleNamespace


def prepare(base, environment, fixture):
    desktop = base / "home/Desktop"
    desktop.parent.mkdir()
    fixture.rename(desktop)
    environment["HOME"] = str(desktop.parent)
    environment["RUSTIDIAN_DEFAULT_VAULT"] = str(desktop)
    environment.pop("RUST_NOTES_DEFAULT_VAULT", None)
    catalog = base / "data/rustidian/vaults.json"
    catalog.parent.mkdir(parents=True, exist_ok=True)
    catalog.write_text(json.dumps({"roots": [str(desktop)]}) + "\n")
    for relative, text in {
        "p/Project native.md": "# Project native\n\n[[Home]]\nPROJECT_ORIGINAL\n",
        "o/Personal native.md": "# Personal native\n\nPERSONAL_ORIGINAL\n",
        "p/node_modules/Excluded sentinel dependencies.md": "excluded\n",
        "p/build/Excluded sentinel build.md": "excluded\n",
        "o/.hidden/Excluded sentinel hidden.md": "excluded\n",
    }.items():
        path = desktop / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)
    outside = base / "Outside notes"
    outside.mkdir()
    (outside / "Excluded sentinel symlink.md").write_text("outside original\n")
    (desktop / "p/Linked outside").symlink_to(outside, target_is_directory=True)
    return desktop


def run(scope):
    h = SimpleNamespace(**scope)
    desktop = h.fixture
    catalog = h.base / "data/rustidian/vaults.json"
    probe = h.base / "ui-probe.json"
    snapshots = {path: path.read_bytes() for path in [desktop / "Home.md",
                 desktop / "p/Project native.md", desktop / "o/Personal native.md",
                 h.base / "Outside notes/Excluded sentinel symlink.md"]}
    evidence = []

    def controls():
        try:
            return json.loads(probe.read_text())
        except (FileNotFoundError, json.JSONDecodeError):
            return {}

    def metric(name):
        value = controls().get(name)
        return value[0] if value is not None else None

    def wait(predicate, message, timeout=8):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            assert h.current_app().poll() is None, "The private native app exited"
            if predicate():
                return
            time.sleep(.01)
        raise AssertionError(message)

    def roots():
        return json.loads(catalog.read_text())["roots"]

    def manager():
        h.click_control("vault-profile")
        h.control_bounds("vault-manager")

    def unchanged():
        for path, original in snapshots.items():
            assert path.read_bytes() == original, f"Vault navigation changed {path.name}"

    def workers():
        tasks = Path(f"/proc/{h.current_app().pid}/task")
        found = []
        for task in tasks.iterdir():
            try:
                if (task / "comm").read_text().strip() == "rustidian-index":
                    found.append(int(task.name))
            except FileNotFoundError:
                pass
        return found

    def watches():
        # The vault's native watches; replacing or re-registering them changes this set.
        found = set()
        for info in Path(f"/proc/{h.current_app().pid}/fdinfo").iterdir():
            try:
                found |= {(info.name, *line.split()[1:3]) for line in info.read_text().splitlines()
                          if line.startswith("inotify wd:")}
            except FileNotFoundError:
                pass
        return found

    def raw_click(name):
        bounds = controls().get(name)
        assert bounds is not None, f"Missing real control: {name}"
        left, top, right, bottom = bounds
        h.xdo("mousemove", "--sync", str(round((left + right) / 2)),
              str(round((top + bottom) / 2)), "click", "1")

    def folder_picker():
        found = subprocess.run(["xdotool", "search", "--onlyvisible", "--name",
                                "^Open folder as vault$"], env=h.environment,
                               capture_output=True, text=True, timeout=3)
        return found.stdout.splitlines()[-1] if found.returncode == 0 else None

    wait(lambda: metric("vault-indexing") == 0, "Desktop initial indexing did not finish")
    h.title_contains("Desktop")
    assert roots() == [str(desktop)], roots()
    for name in ("Project native", "Personal native"):
        h.open_note(name)
        h.title_contains("Desktop")
    h.key("ctrl+o")
    h.type_text("Excluded sentinel")
    assert not [name for name in controls() if name.startswith("quick-open:")], controls()
    h.key("Escape")
    unchanged()
    h.results.append("The isolated Desktop default listed only Desktop, opened p/o notes under that root and excluded dependency/build/hidden/symlink notes from Quick open without changing originals")

    for dark in (True, False):
        h.set_theme(dark)
        h.xdo("windowsize", h.current_window(), "900", "640")
        h.pause()
        manager()
        visible = controls()
        rows = [name for name in visible if name.startswith("vault-recent:")]
        assert rows == [f"vault-recent:{desktop}"], rows
        for name in ("vault-manager", "vault-manager-close", *rows):
            left, top, right, bottom = visible[name]
            assert 0 <= left < right <= 900 and 0 <= top < bottom <= 640, (name, visible[name])
        card = visible["vault-actions-card"]
        clip = visible["vault-actions-clip"]
        assert (clip[0] <= card[0] < card[2] <= clip[2]
                and clip[1] <= card[1] < card[3] <= clip[3]), (card, clip)
        assert metric("vault-body-overflow") <= 1, "A sole-Desktop chooser should fit without scrolling"
        row = visible[rows[0]]
        assert 22 <= row[3] - row[1] <= 58, ("Vault rows must retain compact readable spacing", row)
        h.capture("native-desktop-vault-" + ("dark" if dark else "light") + ".png", (900, 640))
        h.click_control("vault-manager-close")
    h.results.append("The sole Desktop chooser row and Close control stayed within a real900×640 window in both themes; exact screenshots retained for visual review")

    h.restart_desktop([desktop / "p/Project native.md"])
    h.title_contains("Project native")
    h.title_contains("Desktop")
    assert roots() == [str(desktop)], roots()
    h.restart_desktop([])
    h.title_contains("Desktop")
    wait(lambda: metric("vault-indexing") == 0, "Default Desktop restart did not finish")
    h.open_note("Home")
    unchanged()
    h.results.append("Actual restarts with a nested file and with no arguments retained Desktop as the only registered/default vault and preserved all original note bytes")

    personal = desktop / "o/Personal native.md"
    original = snapshots[personal]
    external = original.replace(b"PERSONAL_ORIGINAL", b"PERSONAL_EXTERNAL")
    assert external != original and len(external) == len(original)
    h.open_note("Personal native")
    h.command("Source editing")
    h.click_control("editor")

    def editor_bytes():
        # Copy from the already focused editor; reopening the file would hide a
        # missed watcher event by reading its new contents directly from disk.
        h.key("ctrl+a")
        h.key("ctrl+c")
        return subprocess.check_output(["xclip", "-selection", "clipboard", "-out",
                                        "-target", "UTF8_STRING"], env=h.environment,
                                       timeout=3)

    assert editor_bytes() == original, "The Source editor must show the original Personal note"
    wait(lambda: metric("vault-indexing") == 0, "Personal note opening did not settle")
    refresh_watches = watches()
    assert refresh_watches, "The active vault must be watched before external edits"
    try:
        personal.write_bytes(external)
        wait(lambda: editor_bytes() == external,
             "The already-open Source editor did not refresh a same-size external write")
        wait(lambda: metric("vault-indexing") == 0, "External text refresh did not finish")
        assert watches() == refresh_watches, "An ordinary external text refresh re-registered the vault's watches"
        evidence.append({"state": "external-text-refreshed", "watches": len(watches()),
                         "editor_sha256": hashlib.sha256(external).hexdigest()})
    finally:
        personal.write_bytes(original)
    wait(lambda: editor_bytes() == original,
         "The already-open Source editor did not refresh the restored original bytes")
    wait(lambda: metric("vault-indexing") == 0, "Restoring the external text did not finish")
    assert watches() == refresh_watches, "Restoring ordinary text re-registered the vault's watches"
    evidence.append({"state": "external-text-restored", "watches": len(watches()),
                     "editor_sha256": hashlib.sha256(original).hexdigest()})
    unchanged()
    h.open_note("Home")
    h.results.append("Same-size external edits and restoration reached the already-open Source editor through the real watcher, retaining the exact native watches through both text refreshes")

    # Enough ordinary Markdown for a real busy frame; no runtime delay hook or
    # fake index result is injected, and the fixture stays below 4096 files/64MiB.
    slow = h.base / "Loading Desktop"
    payload = ("ordinary bounded fixture text\n" * 565)[:16384]
    for group in range(96):
        directory = slow / ("p" if group % 2 == 0 else "o") / f"Group{group:03d}"
        directory.mkdir(parents=True)
        for number in range(32):
            (directory / f"Note{number:02d}.md").write_text(payload)
    wait(lambda: metric("vault-indexing") == 0, "The active Desktop indexing did not settle")
    baseline_watches = watches()
    assert baseline_watches and not workers(), "The active Desktop must be watched with no indexing worker left"
    manager()
    h.click_control("vault-open")
    # Open deliberately starts the native picker. Cancel that real picker to
    # exercise the manual Folder field, then let the parent page settle.
    wait(folder_picker, "Open a folder did not show its real native picker")
    h.xdo("windowfocus", "--sync", folder_picker())
    h.key("Escape")
    wait(lambda: folder_picker() is None, "Cancelling the native picker did not close it")
    wait(lambda: "vault-loading-spinner" not in controls(),
         "The chooser stayed disabled after cancelling the native picker")
    h.xdo("windowfocus", "--sync", h.current_window())
    h.pause()
    h.click_control("vault-folder")
    h.key("ctrl+a")
    h.paste_text(str(slow))
    h.pause()
    h.key("ctrl+a")
    h.key("ctrl+c")
    entered = subprocess.check_output(["xclip", "-selection", "clipboard", "-out",
                                       "-target", "UTF8_STRING"], env=h.environment,
                                      text=True, timeout=3)
    assert entered == str(slow), ("The Folder field must contain the exact loading fixture", entered)
    h.pause()
    raw_click("vault-confirm")
    wait(lambda: metric("vault-loading") == 1 and workers(),
         "Opening a bounded real vault must expose its busy state and named indexing worker")
    busy_workers = set(workers())
    evidence.append({"state": "loading", "worker_tids": sorted(busy_workers),
                     "baseline_watches": len(baseline_watches),
                     "fixture_files": 3072, "fixture_bytes": 3072 * len(payload.encode()),
                     "loading": metric("vault-loading"), "indexing": metric("vault-indexing")})
    cancel_started = time.monotonic()
    raw_click("vault-manager-close")
    wait(lambda: "vault-manager" not in controls() and metric("vault-loading") == 0,
         "Closing the busy chooser did not cancel the pending vault switch")
    wait(lambda: not workers() and watches() == baseline_watches,
         "Cancelled vault work left a worker or changed the current vault's watches")
    cancel_ms = (time.monotonic() - cancel_started) * 1000
    h.title_contains("Desktop")
    h.title_contains("Home")
    assert str(slow) not in roots(), "A cancelled vault was added to the recent list"
    h.open_note("Personal native")
    h.open_note("Home")
    unchanged()
    evidence.append({"state": "cancelled", "cancel_ms": cancel_ms, "worker_tids": workers(),
                     "loading": metric("vault-loading"), "indexing": metric("vault-indexing"),
                     "roots": roots(), "original_sha256": {path.name: hashlib.sha256(data).hexdigest()
                                                               for path, data in snapshots.items()}})
    (h.reports / "native-desktop-vault-loading.json").write_text(json.dumps(evidence, indent=2) + "\n")
    h.results.append("The real chooser closed during named background indexing, cancelled its worker, retained the current Desktop/Home workspace and left every original note byte unchanged")
