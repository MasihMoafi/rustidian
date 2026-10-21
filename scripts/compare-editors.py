"""Serial, external edit-and-save comparison. Run release apps on an idle CI host."""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import re
import statistics
import subprocess
import sys
import tempfile
import time

from PIL import ImageGrab
from native_input import SAVE_KEY_INTERVAL_MS, begin_save_shortcut, release_save_shortcut, wait_autosave


def wait_bytes(path, expected, timeout=3):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            if path.read_bytes() == expected:
                return
        except FileNotFoundError:
            pass  # Atomic replacement may briefly change the directory entry.
        time.sleep(.001)
    raise AssertionError(f"Exact saved input missing: {path.name}")


def self_check():
    with tempfile.TemporaryDirectory(prefix="rn-measure-") as directory:
        path = Path(directory) / "note.md"
        path.write_bytes(b"before")
        try:
            wait_bytes(path, b"after", .01)
        except AssertionError:
            pass
        else:
            raise AssertionError("Measurement accepted unchanged source")
        path.write_bytes(b"after")
        wait_bytes(path, b"after")


def corpus():
    source = "# Editor benchmark\n\n"
    for index in range(500):
        source += (
            f"### Section {index}\n\n> A quote with **emphasis** and unicode π.\n\n"
            f'~~~rust\nfn main() {{\n    let value = "body π {index}";\n'
            '    // Preserve literal **markdown** [[No link]]\n}\n~~~\n\n'
            "Plain paragraph for a real document.\n\n"
        )
    assert len(source.encode()) == 103300
    return source.encode()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--ours", type=Path)
    parser.add_argument("--ferrite", type=Path)
    parser.add_argument("--ours-source-commit")
    parser.add_argument("--trace-keys", action="store_true")
    parser.add_argument("--self-check", action="store_true")
    args = parser.parse_args()
    self_check()
    if args.self_check:
        print("Measurement rejects unchanged input and accepts exact changed bytes")
        return
    assert args.ours and args.ferrite
    root = Path(__file__).resolve().parents[1]
    harness_commit = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip()
    source_commit = args.ours_source_commit or harness_commit
    assert re.fullmatch("[0-9a-f]{40}", source_commit), "Expected the full application source SHA"
    reports = root / "reports" / "editor-comparison"
    reports.mkdir(parents=True, exist_ok=True)
    report = {
        "status": "running", "metric": "synthetic insertion + explicit held-Control Save chord until exact bytes are observed",
        "save_key_interval_ms": SAVE_KEY_INTERVAL_MS,
        "input_gap_ms": 50,
        "save_release_after_measurement": True,
        "keyboard_diagnostic": args.trace_keys,
        "ui_probe_during_measurement": False,
        "units": "milliseconds", "warmups": 5, "samples_per_round": 25,
        "source_bytes": len(corpus()), "corpus_sha256": hashlib.sha256(corpus()).hexdigest(),
        "versions": {"ferrite": "v0.3.0", "rust-notes": source_commit},
        "harness_commit": harness_commit,
        "binary_sha256": {name: hashlib.sha256(path.read_bytes()).hexdigest() for name, path in [("rust-notes", args.ours), ("ferrite", args.ferrite)]},
        "cpu": subprocess.check_output(["lscpu"], text=True),
        "limitations": ["Combined edit/save response, not display latency or internal CPU time.",
                        "Includes common xdotool subprocess and 1ms file polling overhead.",
                        "Both apps receive one S press/release with a 40ms key interval; Control stays held until exact saved bytes are observed, then releases before another sample.",
                        "The common 50ms typing-to-Save gap is included; it prevents frame-current shortcut matching from treating an earlier ordinary letter as Ctrl+letter.",
                        "Same source size, wrapping, 18px text and software Xvfb host; font families, UI features and save durability differ.",
                        "Two alternating rounds are a bounded observation, not proof of the world's fastest editor."],
        "rounds": [],
    }

    def preserve():
        (reports / "results.json").write_text(json.dumps(report, indent=2) + "\n")

    preserve()
    try:
        with tempfile.TemporaryDirectory(prefix="rn-compare-") as directory:
            base = Path(directory)
            with (reports / "xvfb.log").open("w") as output:
                xvfb = subprocess.Popen(["Xvfb", "-displayfd", "1", "-screen", "0", "1440x960x24", "-noreset"], stdout=subprocess.PIPE, stderr=output, text=True)
                try:
                    display = ":" + xvfb.stdout.readline().strip()
                    assert display != ":"
                    for round_number, order in enumerate([("rust-notes", "ferrite"), ("ferrite", "rust-notes")]):
                        for name in order:
                            profile = base / f"{round_number}-{name}"
                            vault = profile / "vault"
                            vault.mkdir(parents=True)
                            note = vault / "Home.md"
                            expected = corpus()
                            note.write_bytes(expected)
                            env = {**os.environ, "DISPLAY": display, "LIBGL_ALWAYS_SOFTWARE": "1",
                                   "XDG_STATE_HOME": str(profile / "state"), "XDG_DATA_HOME": str(profile / "data"),
                                   "XDG_CONFIG_HOME": str(profile / "config"), "RUST_NOTES_UI_PROBE": str(profile / "probe.json"),
                                   "FERRITE_DATA_DIR": str(profile / "ferrite")}
                            env.pop("WAYLAND_DISPLAY", None)
                            portable = profile / "ferrite"
                            portable.mkdir()
                            (portable / "config.json").write_text(json.dumps({"font_size": 18.0, "word_wrap": True, "default_view_mode": "raw", "auto_save_enabled_default": False}))

                            def xdo(*parts):
                                return subprocess.check_output(["xdotool", *map(str, parts)], env=env, text=True, stderr=subprocess.DEVNULL, timeout=5).strip()

                            def ready_window(process):
                                deadline = time.monotonic() + 20
                                while time.monotonic() < deadline:
                                    assert process.poll() is None, f"{name} exited before a window"
                                    try:
                                        window = xdo("search", "--onlyvisible", "--pid", process.pid).splitlines()[0]
                                        break
                                    except (subprocess.CalledProcessError, IndexError):
                                        time.sleep(.05)
                                else:
                                    raise AssertionError(f"{name} window timeout")
                                xdo("windowsize", window, 1440, 960)
                                xdo("windowmove", window, 0, 0)
                                time.sleep(2)
                                xdo("windowfocus", "--sync", window)
                                assert xdo("getwindowfocus") == window, f"{name} must own X keyboard input"
                                time.sleep(.1)
                                return window

                            binary = args.ours if name == "rust-notes" else args.ferrite
                            command = [str(binary.resolve()), str(vault if name == "rust-notes" else note)]
                            with (reports / f"{round_number}-{name}.log").open("w") as output:
                                app = subprocess.Popen(command, cwd=profile, env=env, stdout=output, stderr=output)
                                recorder = None
                                try:
                                    window = ready_window(app)
                                    if name == "rust-notes":
                                        def palette(text):
                                            xdo("key", "--clearmodifiers", "ctrl+p")
                                            time.sleep(.2)
                                            xdo("type", "--clearmodifiers", "--delay", 30, text)
                                            time.sleep(.1)
                                            xdo("key", "Return")
                                            time.sleep(.3)
                                        palette("Source editing")
                                        palette("Open settings")
                                        xdo("type", "--clearmodifiers", "--delay", 15, "autosave")
                                        deadline = time.monotonic() + 3
                                        while time.monotonic() < deadline:
                                            probe = json.loads((profile / "probe.json").read_text())
                                            if "settings-dark" not in probe and "settings-autosave" in probe:
                                                break
                                            time.sleep(.01)
                                        assert "settings-dark" not in probe and "settings-autosave" in probe
                                        probe = json.loads((profile / "probe.json").read_text())
                                        bounds = probe["settings-autosave"]
                                        sessions = list((profile / "state/rust-notes").glob("*.json"))
                                        assert len(sessions) <= 1
                                        assert all(json.loads(path.read_text())["autosave_enabled"] is True for path in sessions)
                                        x1, y1, x2, y2 = bounds
                                        xdo("mousemove", "--sync", "--window", window, int((x1 + x2) / 2), int((y1 + y2) / 2))
                                        xdo("click", 1)
                                        # Escape is handled before the settings UI. Queuing
                                        # it with the click can close the dialog first.
                                        session_path = wait_autosave(profile / "state", False)
                                        xdo("key", "Escape")
                                        time.sleep(.3)
                                        xdo("key", "--clearmodifiers", "ctrl+q")
                                        app.wait(timeout=5)
                                        sessions = list((profile / "state/rust-notes").glob("*.json"))
                                        assert len(sessions) == 1
                                        session = json.loads(sessions[0].read_text())
                                        assert session["autosave_enabled"] is False
                                        assert session["tabs"][session["active"]] == ["Home.md", "Source"]
                                        env.pop("RUST_NOTES_UI_PROBE")
                                        (profile / "probe.json").unlink()
                                        app = subprocess.Popen(command, cwd=profile, env=env, stdout=output, stderr=output)
                                        window = ready_window(app)
                                        assert not (profile / "probe.json").exists(), "Measured editing must not write control-probe files"
                                    trace = reports / f"{round_number}-{name}-keys.jsonl"
                                    if args.trace_keys:
                                        ready = profile / "recorder-ready"
                                        recorder = subprocess.Popen([sys.executable, str(root / "scripts/record-keyboard.py"),
                                            "--output", str(trace), "--ready", str(ready)], env=env, stdout=output, stderr=output)
                                        deadline = time.monotonic()+3
                                        while not ready.exists() and time.monotonic() < deadline:
                                            assert recorder.poll() is None, "Keyboard observer exited"
                                            time.sleep(.02)
                                        assert ready.exists(), "Keyboard observer was not ready"
                                    xdo("mousemove", "--window", window, 650, 350)
                                    xdo("click", 1)
                                    xdo("key", "--clearmodifiers", "ctrl+End")
                                    time.sleep(.3)
                                    assert note.read_bytes() == expected, "Setup must preserve the exact benchmark input"
                                    samples = []
                                    for sample in range(30):
                                        inserted = chr(97 + sample % 26)
                                        expected += inserted.encode()
                                        start = time.perf_counter_ns()
                                        xdo("type", "--clearmodifiers", "--delay", 0, inserted)
                                        time.sleep(report["input_gap_ms"] / 1000)
                                        save = begin_save_shortcut(env)
                                        try:
                                            wait_bytes(note, expected)
                                            elapsed = (time.perf_counter_ns() - start) / 1_000_000
                                        except Exception:
                                            actual = note.read_bytes()
                                            diagnostic = {"app": name, "round": round_number, "sample": sample,
                                                "expected_bytes": len(expected), "actual_bytes": len(actual),
                                                "actual_tail": repr(actual[-80:]), "expected_tail": repr(expected[-80:]),
                                                "window_title": xdo("getwindowname", window),
                                                "pointer": xdo("getmouselocation", "--shell"), "input_focus": xdo("getwindowfocus")}
                                            probe = profile / "probe.json"
                                            if probe.exists():
                                                diagnostic["controls"] = json.loads(probe.read_text())
                                            if trace.exists():
                                                diagnostic["keyboard_events"] = [json.loads(line) for line in trace.read_text().splitlines()[-30:]]
                                            report["failed_input"] = diagnostic
                                            ImageGrab.grab(xdisplay=display).save(reports / f"{round_number}-{name}-failure.png")
                                            print(json.dumps(diagnostic), flush=True)
                                            raise
                                        finally:
                                            try:
                                                _, error = save.communicate(timeout=5)
                                            finally:
                                                release_save_shortcut(env)
                                            assert save.returncode == 0, error.decode()
                                        if sample >= 5:
                                            samples.append(elapsed)
                                        time.sleep(.04)
                                    ImageGrab.grab(xdisplay=display).save(reports / f"{round_number}-{name}.png")
                                    report["rounds"].append({"round": round_number, "app": name, "mode": "source/raw", "samples_ms": samples, "inputs_verified": 30})
                                    preserve()
                                except BaseException as failure:
                                    ImageGrab.grab(xdisplay=display).save(reports / f"{round_number}-{name}-native-failure.png")
                                    probe = profile / "probe.json"
                                    if probe.exists():
                                        (reports / f"{round_number}-{name}-failure-controls.json").write_bytes(probe.read_bytes())
                                    report["failed_native_stage"] = {"app":name,"round":round_number,"error":repr(failure),
                                        "sessions":[json.loads(path.read_text()) for path in (profile / "state/rust-notes").glob("*.json")]}
                                    preserve()
                                    raise
                                finally:
                                    if recorder is not None:
                                        recorder.terminate()
                                        recorder.wait(timeout=5)
                                    app.terminate()
                                    try:
                                        app.wait(timeout=5)
                                    except subprocess.TimeoutExpired:
                                        app.kill()
                                        app.wait(timeout=5)
                finally:
                    xvfb.terminate()
                    xvfb.wait(timeout=5)
        summaries = {}
        for name in ["rust-notes", "ferrite"]:
            samples = sorted(sample for run in report["rounds"] if run["app"] == name for sample in run["samples_ms"])
            assert len(samples) == 50
            summaries[name] = {"median_ms": statistics.median(samples), "p95_ms": samples[math.ceil(.95 * len(samples)) - 1]}
        report["summary"] = summaries
        report["meets_observed_target"] = all(summaries["rust-notes"][metric] <= summaries["ferrite"][metric] for metric in ["median_ms", "p95_ms"])
        report["status"] = "completed"
        print(json.dumps({"summary": summaries, "meets_observed_target": report["meets_observed_target"]}))
        if not report["meets_observed_target"]:
            raise SystemExit("Rust Notes missed the observed median/p95 edit-and-save target; retain measurements and investigate.")
    except Exception as error:
        report["status"] = "failed"
        report["error"] = str(error)
        raise
    finally:
        preserve()


if __name__ == "__main__":
    main()
