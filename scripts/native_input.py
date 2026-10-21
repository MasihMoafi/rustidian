"""Shared physical input used by native GUI checks and editor comparisons."""

SAVE_KEY_INTERVAL_MS = 40


def save_shortcut(xdo):
    # Hold Control through S's press/release. xdotool's combined ctrl+s
    # releases Control first, which can hide a shortcut from frame-state users.
    xdo("keydown", "ctrl", "key", "--delay", str(SAVE_KEY_INTERVAL_MS), "s", "keyup", "ctrl")
