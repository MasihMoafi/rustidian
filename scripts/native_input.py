"""Shared native input and settings acknowledgements for isolated checks."""
import json
from pathlib import Path
import subprocess
import time

SAVE_KEY_INTERVAL_MS = 40
SAVE_CHORD = ("keydown", "ctrl", "key", "--delay", str(SAVE_KEY_INTERVAL_MS), "s", "keyup", "ctrl")


def save_shortcut(xdo):
    # Hold Control through S's press/release. xdotool's combined ctrl+s
    # releases Control first, which can hide a shortcut from frame-state users.
    xdo(*SAVE_CHORD)


def begin_save_shortcut(environment):
    # The measured caller releases Control only after observing exact saved
    # bytes, so a slow frame cannot miss the chord's modifier state.
    return subprocess.Popen(["xdotool", *SAVE_CHORD[:-2]], env=environment,
        stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)


def release_save_shortcut(environment):
    subprocess.run(["xdotool", "keyup", "ctrl"], env=environment, check=True,
        stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, timeout=5)


def wait_autosave(state_root, enabled, timeout=3, *, session_path=None):
    deadline = time.monotonic() + timeout
    observed = []
    while time.monotonic() < deadline:
        paths = [Path(session_path)] if session_path is not None else list((Path(state_root) / "rust-notes").glob("*.json"))
        observed = [(path, json.loads(path.read_text())) for path in paths]
        if len(observed) == 1 and observed[0][1]["autosave_enabled"] is enabled:
            return observed[0][0]
        time.sleep(.01)
    raise AssertionError(f"Autosave must persist as {enabled} after one click; observed {observed}")
