"""Three warm interleaved launch comparisons, using disposable profiles/vaults."""
import json
import hashlib
import os
from pathlib import Path
import platform
import shutil
import signal
import statistics
import subprocess
import tempfile
import time
from PIL import ImageGrab

ROOT = Path(__file__).resolve().parents[1]
REPORTS = ROOT / "reports"

def memory_tree(pid):
    observations = []
    def visit(process):
        try:
            status = Path(f"/proc/{process}/smaps_rollup").read_text()
            values = {line.split(":")[0]: int(line.split()[1]) for line in status.splitlines() if line.startswith(("Rss:", "Pss:", "Private_Clean:", "Private_Dirty:"))}
            observations.append({"pid": process, "rss_kib": values["Rss"], "pss_kib": values["Pss"], "private_kib": values["Private_Clean"] + values["Private_Dirty"]})
            for child in Path(f"/proc/{process}/task/{process}/children").read_text().split(): visit(int(child))
        except (FileNotFoundError, ProcessLookupError, PermissionError): pass
    visit(pid)
    return {"processes": observations, **{key: sum(row[key] for row in observations) for key in ["rss_kib", "pss_kib", "private_kib"]}}

with tempfile.TemporaryDirectory(prefix="rust-notes-benchmark-") as temporary:
    base = Path(temporary)
    vault = base / "Benchmark Vault"
    shutil.copytree(ROOT / "demo-vault", vault)
    settings = vault / ".obsidian"
    settings.mkdir(exist_ok=True)
    original = Path("/home/masih/Desktop/o/Main Vault/.obsidian")
    appearance = json.loads((original / "appearance.json").read_text())
    appearance["theme"] = "obsidian"
    (settings / "appearance.json").write_text(json.dumps(appearance))
    shutil.copytree(original / "snippets", settings / "snippets", dirs_exist_ok=True)
    (settings / "app.json").write_text(json.dumps({"defaultViewMode": "preview"}))
    (settings / "community-plugins.json").write_text("[]")
    profile = base / "obsidian-profile"
    profile.mkdir()
    (profile / "obsidian.json").write_text(json.dumps({"vaults": {"0123456789abcdef": {"path": str(vault), "ts": 1, "open": True}}}))
    read_fd, write_fd = os.pipe()
    server = subprocess.Popen(["Xvfb", "-displayfd", str(write_fd), "-screen", "0", "1440x960x24", "-nolisten", "tcp", "-ac"], pass_fds=(write_fd,), stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    os.close(write_fd)
    display = ":" + os.read(read_fd, 64).decode().strip()
    os.close(read_fd)
    env = {**os.environ, "DISPLAY": display, "WAYLAND_DISPLAY": "", "WINIT_UNIX_BACKEND": "x11", "ELECTRON_OZONE_PLATFORM_HINT": "x11", "XDG_STATE_HOME": str(base / "native-state")}
    def xdo(*arguments): return subprocess.check_output(["xdotool", *arguments], env=env, text=True, stderr=subprocess.DEVNULL).strip()
    samples = []
    try:
        for run, kind in enumerate(["native", "obsidian", "native", "obsidian", "obsidian", "native", "native", "obsidian"]):
            warmup = run < 2
            log = (REPORTS / f"benchmark-{kind}-{run}.log").open("w")
            command = [str(ROOT / "target/release/rust-notes"), str(vault)] if kind == "native" else ["/home/masih/Desktop/o/Obsidian.AppImage", "--no-sandbox", f"--user-data-dir={profile}", "--ozone-platform=x11"]
            started = time.monotonic()
            app = subprocess.Popen(command, env=env, stdout=log, stderr=log, start_new_session=True)
            try:
                window = None
                deadline = started + 20
                while time.monotonic() < deadline:
                    assert app.poll() is None, f"{kind} exited before its window appeared"
                    try:
                        window = xdo("search", "--onlyvisible", "--name", "Rust Notes$" if kind == "native" else "Obsidian").splitlines()[0]
                        break
                    except (subprocess.CalledProcessError, IndexError): time.sleep(0.02)
                assert window, f"{kind} did not show a window"
                xdo("windowsize", window, "1440", "960")
                xdo("windowmove", window, "0", "0")
                xdo("windowfocus", window)
                if warmup and kind == "obsidian":
                    time.sleep(1)
                    xdo("key", "--clearmodifiers", "ctrl+o")
                    time.sleep(0.2)
                    xdo("type", "Home")
                    time.sleep(0.2)
                    xdo("key", "Return")
                while time.monotonic() < deadline:
                    title = xdo("getwindowname", window)
                    pixels = ImageGrab.grab(xdisplay=display).crop((420, 150, 1150, 310))
                    bright = sum(1 for r, g, b in pixels.get_flattened_data() if min(r, g, b) > 130)
                    if "Home" in title and bright > 1200: break
                    time.sleep(0.02)
                else: raise AssertionError(f"{kind}: Home content did not become visible")
                launch_ms = (time.monotonic() - started) * 1000
                if warmup and kind == "native": xdo("key", "--clearmodifiers", "ctrl+e")
                time.sleep(1)
                memory = memory_tree(app.pid)
                if not warmup:
                    ImageGrab.grab(xdisplay=display).save(REPORTS / f"benchmark-{kind}.png")
                    row = {"kind": kind, "run": run, "launch_visible_ms": launch_ms, "window_title": title, **memory}
                    samples.append(row)
                    print(json.dumps({key: row[key] for key in ["kind", "run", "launch_visible_ms", "pss_kib"]}), flush=True)
                xdo("key", "--clearmodifiers", "ctrl+q")
                app.wait(timeout=8)
            finally:
                if app.poll() is None:
                    os.killpg(app.pid, signal.SIGTERM)
                    app.wait(timeout=8)
                log.close()
        summary = {kind: {"median_launch_visible_ms": statistics.median(row["launch_visible_ms"] for row in samples if row["kind"] == kind), "median_pss_mib": statistics.median(row["pss_kib"] for row in samples if row["kind"] == kind) / 1024} for kind in ["native", "obsidian"]}
        result = {"method": "warm visible Home content; 3 interleaved samples/app; same sample vault, 1440x960 Xvfb, software rendering; reading mode; plugins disabled; process-tree PSS", "platform": platform.platform(), "obsidian": "installed AppImage, captured window version", "samples": samples, "summary": summary}
        result["corpus"] = [{"path": str(path.relative_to(vault)), "bytes": path.stat().st_size, "sha256": hashlib.sha256(path.read_bytes()).hexdigest()} for path in sorted(vault.rglob("*.md"))]
        result["build"] = {"binary_sha256": hashlib.sha256((ROOT / "target/release/rust-notes").read_bytes()).hexdigest(), "binary_bytes": (ROOT / "target/release/rust-notes").stat().st_size, "rustc": subprocess.check_output(["rustc", "--version"], text=True).strip(), "measured_at_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())}
        (REPORTS / "desktop-benchmark.json").write_text(json.dumps(result, indent=2) + "\n")
        print(json.dumps(summary), flush=True)
    finally:
        server.terminate()
        server.wait(timeout=5)
