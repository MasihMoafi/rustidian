"""Shared physical input used by native GUI checks and editor comparisons."""
import subprocess

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
