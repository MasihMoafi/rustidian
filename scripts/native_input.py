"""Shared native input and settings acknowledgements for isolated checks."""
import json
from pathlib import Path
import subprocess
import time


class UnicodeKeyboard:
    """Keep test symbols mapped for the lifetime of our private Xvfb app.

    xdotool's temporary per-character mapping can disappear before the native
    event loop decodes a queued key. Never change the real desktop's keymap.
    """
    def __init__(self, environment, text):
        import ctypes
        import os
        from Xlib import display, X
        assert environment["DISPLAY"] != os.environ.get("DISPLAY"), "Use the private Xvfb display only"
        self.connection = display.Display(environment["DISPLAY"])
        modifiers = {code for row in self.connection.get_modifier_mapping() for code in row if code}
        maximum = self.connection.display.info.max_keycode
        rows = self.connection.get_keyboard_mapping(160, maximum-159)
        candidates = [(160+index, tuple(row)) for index, row in enumerate(rows)
                      if 160+index not in modifiers and not any(32 <= symbol <= 126 for symbol in row)]
        characters = sorted({character for character in text if ord(character) > 127})
        assert len(candidates) >= len(characters), "Insufficient private test keycodes"
        library = ctypes.CDLL("libxkbcommon.so.0")
        library.xkb_utf32_to_keysym.argtypes = [ctypes.c_uint32]
        library.xkb_utf32_to_keysym.restype = ctypes.c_uint32
        self.original = candidates[:len(characters)]
        for (code, original), character in zip(self.original, characters):
            symbol = library.xkb_utf32_to_keysym(ord(character))
            assert symbol, repr(character)
            self.connection.change_keyboard_mapping(code, [(symbol,)+(X.NoSymbol,)*(len(original)-1)])
        self.connection.sync()

    def close(self):
        for code, original in self.original:
            self.connection.change_keyboard_mapping(code, [original])
        self.connection.sync()
        self.connection.close()

SAVE_KEY_INTERVAL_MS = 40
SAVE_CHORD = ("keydown", "ctrl", "key", "--delay", str(SAVE_KEY_INTERVAL_MS), "s", "keyup", "ctrl")


def save_shortcut(xdo):
    # Hold Control through S's press/release. xdotool's combined ctrl+s
    # releases Control first, which can hide a shortcut from frame-state users.
    modified_shortcut(xdo, "s")


def modified_shortcut(xdo, key, modifier="ctrl"):
    xdo("keydown", modifier, "key", "--delay", str(SAVE_KEY_INTERVAL_MS), key, "keyup", modifier)


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
