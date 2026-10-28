"""The Agent terminal on the GUI harness's disposable vault and private display.

A stand-in agent installed as `elpis` and `claude` (no `codex`) is first on PATH.
It prints a banner, echoes each line as `you said: …`, reports its rows×cols for
`size` (numbered, so each report is new), prints 1,200 numbered lines for `lines`, styled words for `colors`, and
exits on `exit`. It never calls a model. The terminal's text is read from the UI
probe's `terminal-row:<row>:<text>` entries.
"""
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import time
from types import SimpleNamespace


AGENT = r'''#!/bin/bash
# A stand-in for the Agent terminal checks: no model, no network.
name=${0##*/}
printf 'FAKE %s ready pid %s\n' "$name" "$$"
trap 'printf "interrupted\n"' INT
reports=0
while :; do
  IFS= read -r line
  status=$?
  if [ "$status" -gt 128 ]; then continue; fi
  if [ "$status" -ne 0 ]; then exit 0; fi
  case $line in
    exit) printf 'bye\n'; exit 0 ;;
    size) reports=$((reports + 1)); read -r rows cols < <(stty size); printf 'size %s: %sx%s\n' "$reports" "$rows" "$cols" ;;
    lines) for ((i = 1; i <= 1200; i++)); do printf 'line %s\n' "$i"; done ;;
    colors) printf '\033[1mbold\033[0m \033[3mitalic\033[0m \033[4munderline\033[0m \033[7minverse\033[0m \033[31mred\033[0m \033[32mgreen\033[0m \033[34mblue\033[0m \033[38;5;208morange\033[0m \033[38;2;0;160;255mtruecolor\033[0m \033[30;47mblack-on-white\033[0m\n' ;;
    *) printf 'you said: %s\n' "$line" ;;
  esac
done
'''


def prepare(base, environment):
    """Before the app starts: the stand-in first on a PATH without real agents."""
    folder = Path(base) / "agent-terminal" / "bin"
    folder.mkdir(parents=True)
    system = "/usr/bin:/bin"
    for name in ("elpis", "codex", "claude"):
        assert shutil.which(name, path=system) is None, f"The test PATH must not hold a real {name}"
    for name in ("elpis", "claude"):
        program = folder / name
        program.write_text(AGENT)
        program.chmod(0o700)
    environment["PATH"] = str(folder) + os.pathsep + system


def run(scope):
    """Receives the harness's helper closures and a relaunch callback."""
    h = SimpleNamespace(**scope)
    base = Path(h.base)
    folder = base / "agent-terminal" / "bin"
    app = h.app

    def controls():
        try:
            return json.loads((base / "ui-probe.json").read_text())
        except (FileNotFoundError, json.JSONDecodeError):
            return {}

    def wait(predicate, reason, timeout=8):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            value = predicate()
            if value:
                return value
            time.sleep(.04)
        h.capture("native-agent-terminal-timeout.png")
        raise AssertionError(f"{reason}; terminal rows: {rows()}")

    def rows():
        return [name.split(":", 2)[2] for name in sorted(controls()) if name.startswith("terminal-row:")]

    def metric(name):
        return controls().get(name, [None])[0]

    def shows(text, timeout=8):
        wait(lambda: any(text in row for row in rows()), f"The terminal never showed {text!r}", timeout)

    def latest(pattern):
        found = [match for row in rows() if (match := re.search(pattern, row))]
        return found[-1] if found else None

    def pid():
        """The running agent's process, as the panel reports it."""
        return int(metric("terminal-pid") or 0)

    def banner():
        match = latest(r"FAKE (\w+) ready pid (\d+)")
        return (match.group(1), int(match.group(2))) if match else None

    def enter(line):
        if line:
            h.xdo("type", "--clearmodifiers", "--delay", "1", "--", line)
        h.xdo("key", "--clearmodifiers", "Return")
        h.pause()

    def size():
        previous = latest(r"size (\d+): ")
        count = int(previous.group(1)) if previous else 0
        enter("size")
        match = wait(lambda: (found := latest(r"size (\d+): (\d+)x(\d+)")) and int(found.group(1)) > count and found,
                     "The agent did not report its size")
        reported = (int(match.group(2)), int(match.group(3)))
        assert reported == (metric("terminal-rows"), metric("terminal-cols")), (reported, controls())
        return reported

    def stand_ins():
        found = []
        for entry in Path("/proc").iterdir():
            if entry.name.isdigit():
                try:
                    command = (entry / "cmdline").read_bytes()
                except OSError:
                    continue
                if str(folder).encode() in command:
                    found.append(int(entry.name))
        return found

    def focus_terminal():
        h.click_control("agent-terminal")
        wait(lambda: metric("terminal-focused") == 1, "Clicking the terminal must give it keyboard focus")

    def clipboard(text):
        owner = subprocess.Popen(["xclip", "-selection", "clipboard", "-in", "-quiet"], stdin=subprocess.PIPE,
                                 stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, env=h.environment)
        owner.stdin.write(text.encode())
        owner.stdin.close()
        h.pause()
        return owner

    # 1. Nothing starts until the tab is shown; Ctrl+Shift+A shows it and the agent appears within 2 s.
    h.open_note("Home")
    assert not stand_ins(), "An agent started before the Agent tab was shown"
    assert not h.control_bounds("agent-terminal", required=False)
    started = time.monotonic()
    h.xdo("key", "--clearmodifiers", "ctrl+shift+a")
    wait(lambda: banner(), "Ctrl+Shift+A must start the agent and show its banner", timeout=4)
    elapsed = time.monotonic() - started
    assert elapsed <= 2.0, f"The banner took {elapsed:.2f} s"
    assert banner()[0] == "elpis", "Elpis is the default agent"
    h.control_bounds("panel:agent")
    wait(lambda: metric("terminal-focused") == 1, "Opening the tab gives the terminal keyboard focus")
    assert len(stand_ins()) == 1
    h.capture("native-agent-terminal-open.png")
    h.results.append(f"Ctrl+Shift+A showed the Agent tab; the default Elpis stand-in's banner appeared after {elapsed:.2f} s")

    # 2. The menu lists only the agents on PATH, in menu order.
    h.click_control("agent-backend")
    wait(lambda: "agent-backend:Elpis" in controls(), "The agent menu did not open")
    listed = sorted((bounds[1], name) for name, bounds in controls().items() if name.startswith("agent-backend:"))
    assert [name for _, name in listed] == ["agent-backend:Elpis", "agent-backend:Claude"], listed
    h.capture("native-agent-terminal-menu.png")
    h.click_control("agent-backend")
    wait(lambda: "agent-backend:Elpis" not in controls(), "The agent menu did not close")
    h.results.append("The agent menu listed only Elpis and Claude, the agents on the test PATH, in that order")

    # 3. Typed text and Enter reach the agent; its output appears live.
    focus_terminal()
    enter("hello agent")
    shows("you said: hello agent")
    h.results.append("Typed text and Enter reached the agent and its echo appeared")

    # 4. Every key goes to the agent; the app's shortcuts stay quiet.
    tabs = sorted(name for name in controls() if name.startswith("tab-full:"))
    for value in ("Escape", "Tab", "alt+x", "Up", "ctrl+p", "ctrl+t"):
        h.key(value)
        assert metric("terminal-focused") == 1, f"{value} moved keyboard focus away from the terminal"
    # The stand-in's terminal echoes control bytes as ^X: Escape, Alt+x, Up, Ctrl+P, Ctrl+T.
    shows("^[x^[[A^P^T")
    assert "dialog-input-focused" not in controls(), "An app shortcut opened a dialog while the terminal had focus"
    assert sorted(name for name in controls() if name.startswith("tab-full:")) == tabs, "Ctrl+T reached the app"
    h.key("ctrl+c")
    shows("interrupted")
    enter("after keys")
    shows("you said: after keys")
    h.results.append("Escape, Tab, Alt+x, Up, Ctrl+P, Ctrl+T and Ctrl+C reached the agent as terminal bytes; focus and the app stayed unchanged")

    # 5. A wider sidebar gives the agent more columns.
    before = size()
    pane = h.control_bounds("sidebar:details")
    y = str(round((pane[1] + pane[3]) / 2))
    # Grab the edge from inside the panel: the editor covers its other half.
    h.xdo("mousemove", "--sync", str(round(pane[0] + 2)), y, "mousedown", "1")
    h.pause()
    h.xdo("mousemove", "--sync", str(round(pane[0] - 200)), y)
    h.pause()
    h.xdo("mouseup", "1")
    h.pause()
    wider = h.control_bounds("sidebar:details")
    assert wider[2] - wider[0] >= pane[2] - pane[0] + 100, (pane, wider)
    focus_terminal()
    after = size()
    assert after[1] > before[1] and after[0] == before[0], (before, after)
    h.results.append(f"Dragging the sidebar wider resized the agent's terminal from {before[0]}x{before[1]} to {after[0]}x{after[1]}")

    # 6. Styled output, then an idle panel draws no frames.
    enter("colors")
    shows("black-on-white")
    h.xdo("mousemove", "--sync", str(round((pane[0] + pane[2]) / 2)), str(round(pane[3] - 30)))
    time.sleep(.8)
    probe = base / "ui-probe.json"
    stamp = probe.stat().st_mtime_ns
    time.sleep(1.5)
    assert probe.stat().st_mtime_ns == stamp, "An idle Agent terminal kept drawing frames"
    h.capture("native-agent-terminal-colors.png")
    h.results.append("Bold, italic, underline, inverse, 16, 256 and true colours drew; an idle terminal drew no frames for 1.5 s")

    # 7. The wheel scrolls back at least 1,000 lines; new output returns to the bottom.
    enter("lines")
    shows("line 1200")
    terminal = h.control_bounds("agent-terminal")
    h.xdo("mousemove", "--sync", str(round((terminal[0] + terminal[2]) / 2)), str(round((terminal[1] + terminal[3]) / 2)))
    h.xdo("click", "--repeat", "400", "--delay", "1", "4")
    wait(lambda: (metric("terminal-scrollback") or 0) >= 1000, "The wheel did not scroll back 1,000 lines")
    offset = metric("terminal-scrollback")
    numbers = [int(match.group(1)) for row in rows() if (match := re.fullmatch(r"line (\d+)", row))]
    assert numbers and min(numbers) <= 200, numbers
    h.capture("native-agent-terminal-scrollback.png")
    enter("size")
    wait(lambda: metric("terminal-scrollback") == 0, "New output must bring the view back to the bottom")
    h.results.append(f"The wheel scrolled back {offset:.0f} lines, to line {min(numbers)} of 1,200; new output returned to the bottom")

    # 8. Choosing another agent restarts the terminal with it and persists per vault.
    first = pid()
    h.click_control("agent-backend")
    h.click_control("agent-backend:Claude")
    wait(lambda: banner() and banner()[0] == "claude", "Choosing Claude did not start it")
    wait(lambda: first not in stand_ins(), "The previous agent outlived the switch")
    assert len(stand_ins()) == 1, stand_ins()
    saved = list((base / "state/rustidian/agent").glob("*.json"))
    assert len(saved) == 1 and json.loads(saved[0].read_text()) == {"backend": "claude"}, saved
    h.results.append("Choosing Claude ended the Elpis stand-in, started Claude and saved the choice for the vault")

    # 9. Restart starts a fresh process on a fresh screen.
    before_restart = pid()
    focus_terminal()
    enter("before restart")
    shows("you said: before restart")
    h.click_control("agent-restart")
    wait(lambda: banner() and banner()[1] != before_restart, "Restart did not start a fresh agent")
    assert not any("before restart" in row for row in rows()), rows()
    wait(lambda: before_restart not in stand_ins(), "The restarted agent's old process remained")
    h.results.append("Restart ended the old process and showed a fresh banner on a fresh screen")

    # 10. An agent that exits leaves its last screen and "Exited (code 0)"; Restart works again.
    focus_terminal()
    enter("exit")
    wait(lambda: "agent-exit:0" in controls(), "Exit did not show Exited (code 0)")
    assert "agent-status:Exited" in controls()
    shows("bye")
    h.capture("native-agent-terminal-exited.png")
    exited = pid()
    h.click_control("agent-restart")
    wait(lambda: banner() and banner()[1] != exited, "Restart after exit did not start a fresh agent")
    assert "agent-exit:0" not in controls()
    h.results.append("After exit the last screen stayed with Exited (code 0); Restart started a fresh agent")

    # 11. Ctrl+Shift+A hides the tab from the terminal; the top-right button shows and hides it.
    running = pid()
    focus_terminal()
    h.key("ctrl+shift+a")
    wait(lambda: "agent-terminal" not in controls(), "Ctrl+Shift+A in the terminal did not hide the tab")
    assert running in stand_ins(), "Hiding the tab must not end the agent"
    h.click_control("toggle-agent")
    wait(lambda: "agent-terminal" in controls(), "The Agent button did not show the tab")
    assert pid() == running, "Showing the tab again must keep the same agent"
    h.click_control("toggle-agent")
    wait(lambda: "agent-terminal" not in controls(), "The Agent button did not hide the tab")
    h.key("ctrl+shift+a")
    wait(lambda: "agent-terminal" in controls(), "Ctrl+Shift+A did not show the tab again")
    h.results.append("Ctrl+Shift+A hid the tab from the focused terminal; the Agent button showed and hid it; the agent kept running")

    # 12. Both themes at two sizes: header and terminal inside the sidebar, nothing overlapping.
    enter("colors")
    shows("black-on-white")
    h.click_control("editor")
    window = h.xdo("search", "--onlyvisible", "--name", "Rustidian$").splitlines()[0]
    for dark in (False, True):
        h.set_theme(dark)
        for width, height in ((900, 640), (1440, 960)):
            h.xdo("windowsize", window, str(width), str(height))
            h.pause()
            wait(lambda: metric("terminal-cols"), "The terminal did not draw after resizing")
            layout(h, controls, width, height)
            h.capture(f"native-agent-terminal-{'dark' if dark else 'light'}-{width}x{height}.png", (width, height))
    h.results.append("Light and dark at 900×640 and 1440×960: tab, header, menu, Restart, status and terminal rows fit the sidebar without overlap")

    # 13. Ctrl+Shift+V pastes the clipboard.
    focus_terminal()
    owner = clipboard("pasted from the clipboard")
    try:
        h.key("ctrl+shift+v")
        enter("")
        shows("you said: pasted from the clipboard")
    finally:
        owner.terminate()
        owner.wait(timeout=5)
    h.results.append("Ctrl+Shift+V pasted the clipboard into the agent")

    # 14. Quitting from the terminal ends the agent; the vault's choice survives a restart.
    focus_terminal()
    h.xdo("key", "--clearmodifiers", "ctrl+q")
    app.wait(timeout=10)
    wait(lambda: not stand_ins(), "An agent process outlived its window", timeout=5)
    app = h.relaunch()
    h.xdo("key", "--clearmodifiers", "ctrl+shift+a")
    wait(lambda: banner() and banner()[0] == "claude", "The vault's Claude choice did not survive a restart")
    h.xdo("key", "--clearmodifiers", "ctrl+q")
    app.wait(timeout=10)
    wait(lambda: not stand_ins(), "An agent process outlived its window", timeout=5)
    h.results.append("Ctrl+Q from the terminal closed the window and its agent; after relaunch the vault's Claude choice started")


def layout(h, controls, width, height):
    current = controls()

    def bounds(name):
        assert name in current, (name, sorted(current))
        return current[name]

    def inside(inner, outer, label):
        assert (outer[0] - 1 <= inner[0] < inner[2] <= outer[2] + 1
                and outer[1] - 1 <= inner[1] < inner[3] <= outer[3] + 1), (width, height, label, inner, outer)

    pane = bounds("sidebar:details")
    assert 0 <= pane[0] < pane[2] <= width and 0 <= pane[1] < pane[3] <= height, (width, height, pane)
    header = bounds("agent-header")
    terminal = bounds("agent-terminal")
    for name, rect in (("panel:agent", bounds("panel:agent")), ("agent-header", header), ("agent-terminal", terminal)):
        inside(rect, pane, name)
    status = [rect for name, rect in current.items() if name.startswith("agent-status:")]
    assert len(status) == 1, status
    backend, restart = bounds("agent-backend"), bounds("agent-restart")
    for name, rect in (("agent-backend", backend), ("agent-restart", restart), ("agent-status", status[0])):
        inside(rect, header, name)
    assert backend[2] <= restart[0] + 1 and restart[2] <= status[0][0] + 1, (backend, restart, status)
    assert header[3] <= terminal[1] + 1, (header, terminal)
    for name, rect in current.items():
        if name.startswith("terminal-row:"):
            inside(rect, terminal, name)
