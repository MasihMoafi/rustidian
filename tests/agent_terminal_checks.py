"""The Agent terminal on the GUI harness's disposable vault and private display.

A stand-in agent installed as `elpis` and `claude` (no `codex`) is first on PATH.
It prints a banner, echoes each line as `you said: …`, reports its rows×cols for
`size` (numbered, so each report is new) and exits on `exit`; it also prints
1,200 numbered lines (`lines`), ticks in the background (`ticks`), a 50 MB
flood (`flood`), styled words (`colors`), glyphs (`glyphs`) and switches to and
from the alternate screen (`alt`, `main`). Like some agents it ignores SIGHUP.
It never calls a model. The terminal's text is read from the UI probe's
`terminal-row:<row>:<text>` entries.
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
trap '' HUP
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
    ticks) (for i in 1 2 3 4 5 6 7 8 9 10; do sleep 0.3; printf 'tick %s\n' "$i"; done) & ;;
    flood) head -c 50000000 /dev/zero | tr '\0' 'x' | fold -w 100; printf 'flood done\n' ;;
    alt) printf '\033[?1049h\033[?1007h\033[HALT SCREEN\n' ;;
    main) printf '\033[?1007l\033[?1049lMAIN SCREEN\n' ;;
    colors) printf '\033[1mbold\033[0m \033[3mitalic\033[0m \033[4munderline\033[0m \033[7minverse\033[0m \033[31mred\033[0m \033[32mgreen\033[0m \033[34mblue\033[0m \033[38;5;208morange\033[0m \033[38;2;0;160;255mtruecolor\033[0m \033[30;47mblack-on-white\033[0m\n' ;;
    glyphs) printf '┌──┐ ⠋⠙⠹ 中文 سلام دنیا\n└──┘ │done│\n' ;;
    *) printf 'you said: %s\n' "$line" ;;
  esac
done
'''

# Variables an agent must not inherit, and one it must keep.
STRIPPED = {"CLAUDE_CONFIG_DIR": "claude-config", "AI_AGENT_RUN": "1", "CODEX_SANDBOX": "seatbelt",
            "TMUX": "/tmp/tmux-test,1,0", "TERM_PROGRAM": "vscode", "WT_SESSION": "outer"}


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
    environment.update(STRIPPED, CODEX_HOME=str(Path(base) / "agent-terminal" / "codex-home"),
                       RUSTIDIAN_FRAME_LOG=str(Path(base) / "agent-terminal" / "frames.log"))


def run(scope):
    """Receives the harness's helper closures and a relaunch callback."""
    h = SimpleNamespace(**scope)
    base, fixture = Path(h.base), Path(h.fixture)
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

    def status():
        names = [name.split(":", 1)[1] for name in controls() if name.startswith("agent-status:")]
        return names[0] if names else None

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

    def click(name):
        # A notice the machine raises (a full inotify limit on a busy desktop)
        # can cover the top-right controls; it is not this feature's.
        for _ in range(3):
            if "error-dismiss" not in controls():
                break
            h.click_control("error-dismiss")
        h.click_control(name)

    def focus_terminal():
        click("agent-terminal")
        wait(lambda: metric("terminal-focused") == 1, "Clicking the terminal must give it keyboard focus")

    def wheel(button, clicks):
        terminal = h.control_bounds("agent-terminal")
        h.xdo("mousemove", "--sync", str(round((terminal[0] + terminal[2]) / 2)), str(round((terminal[1] + terminal[3]) / 2)))
        h.xdo("click", "--repeat", str(clicks), "--delay", "1", str(button))

    def idle(reason):
        probe = base / "ui-probe.json"
        time.sleep(.8)
        stamp = probe.stat().st_mtime_ns
        time.sleep(1.5)
        assert probe.stat().st_mtime_ns == stamp, reason

    def clipboard(text):
        owner = subprocess.Popen(["xclip", "-selection", "clipboard", "-in", "-quiet"], stdin=subprocess.PIPE,
                                 stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, env=h.environment)
        owner.stdin.write(text.encode())
        owner.stdin.close()
        h.pause()
        return owner

    def paste(keys, text):
        owner = clipboard(text)
        try:
            h.key(keys)
            enter("")
            shows(f"you said: {text}")
        finally:
            owner.terminate()
            owner.wait(timeout=5)

    # 1. Nothing starts until the tab is shown; Ctrl+Shift+A shows it and the agent
    #    appears within 2 s, in the vault, without the calling session's variables.
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
    agent = pid()
    environment = dict(entry.split("=", 1) for entry in Path(f"/proc/{agent}/environ").read_text().split("\0") if "=" in entry)
    assert environment.get("TERM") == "xterm-256color" and environment.get("COLORTERM") == "truecolor", environment
    assert not set(STRIPPED) & set(environment), sorted(set(STRIPPED) & set(environment))
    assert environment.get("CODEX_HOME") == h.environment["CODEX_HOME"], "CODEX_HOME must reach the agent"
    assert os.readlink(f"/proc/{agent}/cwd") == str(fixture), "The agent must run in the vault root"
    h.capture("native-agent-terminal-open.png")
    h.results.append(f"Ctrl+Shift+A showed the Agent tab; the Elpis stand-in's banner appeared after {elapsed:.2f} s, "
                     "in the vault root, with TERM=xterm-256color, without CLAUDE*/AI_AGENT*/CODEX_SANDBOX/TMUX/TERM_PROGRAM/WT_SESSION, with CODEX_HOME")

    # 2. The menu lists only the agents on PATH, in menu order.
    click("agent-backend")
    wait(lambda: "agent-backend:Elpis" in controls(), "The agent menu did not open")
    listed = sorted((bounds[1], name) for name, bounds in controls().items() if name.startswith("agent-backend:"))
    assert [name for _, name in listed] == ["agent-backend:Elpis", "agent-backend:Claude"], listed
    h.capture("native-agent-terminal-menu.png")
    click("agent-backend")
    wait(lambda: "agent-backend:Elpis" not in controls(), "The agent menu did not close")
    h.results.append("The agent menu listed only Elpis and Claude, the agents on the test PATH, in that order")

    # 3. Typed text and Enter reach the agent; its output appears live.
    focus_terminal()
    enter("hello agent")
    shows("you said: hello agent")
    h.results.append("Typed text and Enter reached the agent and its echo appeared")

    # 4. Every key goes to the agent; the app's shortcuts stay quiet.
    tabs = sorted(name for name in controls() if name.startswith("tab-full:"))
    notes = sorted(str(path) for path in fixture.rglob("*.md"))
    mode = sorted(name for name in controls() if name.startswith("view-mode:"))
    for value in ("Escape", "Tab", "alt+x", "Up", "ctrl+p", "ctrl+t", "ctrl+e", "ctrl+n", "ctrl+q"):
        h.key(value)
        assert metric("terminal-focused") == 1, f"{value} moved keyboard focus away from the terminal"
    # The stand-in's terminal echoes control bytes as ^X; Ctrl+Q is flow control there.
    shows("^[x^[[A^P^T^E^N")
    h.key("ctrl+w")
    assert "dialog-input-focused" not in controls(), "An app shortcut opened a dialog while the terminal had focus"
    assert sorted(name for name in controls() if name.startswith("tab-full:")) == tabs, "Ctrl+T or Ctrl+W reached the app"
    assert sorted(str(path) for path in fixture.rglob("*.md")) == notes, "Ctrl+N reached the app"
    assert sorted(name for name in controls() if name.startswith("view-mode:")) == mode, "Ctrl+E reached the app"
    h.key("ctrl+c")
    shows("interrupted")
    enter("after keys")
    shows("you said: after keys")
    h.results.append("Escape, Tab, Alt+x, Up and Ctrl+P/T/E/N/Q/W/C reached the agent as terminal bytes; focus, tabs, notes, view and the window stayed")

    # 5. A drag selects text; Ctrl+Shift+C copies it.
    first = next(bounds for name, bounds in sorted(controls().items()) if name.startswith("terminal-row:00:"))
    terminal = h.control_bounds("agent-terminal")
    cell = (terminal[2] - terminal[0]) / metric("terminal-cols")
    y = str(round((first[1] + first[3]) / 2))
    h.xdo("mousemove", "--sync", str(round(terminal[0] + cell / 2)), y, "mousedown", "1")
    h.xdo("mousemove", "--sync", str(round(terminal[0] + cell * 2)), y)
    h.xdo("mousemove", "--sync", str(round(terminal[0] + cell * 3.5)), y, "mouseup", "1")
    h.pause()
    wait(lambda: metric("terminal-selected") == 1, "A drag over the terminal did not select")
    h.key("ctrl+shift+c")
    copied = wait(lambda: subprocess.run(["xclip", "-selection", "clipboard", "-out"], env=h.environment,
                                         capture_output=True, text=True, timeout=5).stdout, "Nothing was copied")
    assert copied == "FAKE", copied
    h.results.append("A drag selected the banner's first word and Ctrl+Shift+C copied it")

    # 6. Ctrl+V pastes text from the clipboard, as does Ctrl+Shift+V.
    focus_terminal()
    paste("ctrl+v", "pasted with ctrl v")
    paste("ctrl+shift+v", "pasted with ctrl shift v")
    h.results.append("Ctrl+V and Ctrl+Shift+V pasted the clipboard into the agent")

    # 7. A wider sidebar gives the agent more columns.
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

    # 8. Styles and glyphs draw; an idle terminal draws no frames.
    enter("colors")
    shows("black-on-white")
    enter("glyphs")
    shows("│done│")
    h.xdo("mousemove", "--sync", str(round((pane[0] + pane[2]) / 2)), str(round(pane[3] - 30)))
    idle("An idle Agent terminal kept drawing frames")
    h.capture("native-agent-terminal-colors.png")
    h.results.append("Bold, italic, underline, inverse, 16/256/true colours, box drawing, braille, CJK and Persian drew; an idle terminal drew no frames for 1.5 s")

    # 9. Output into a hidden tab draws nothing; showing it again shows the output.
    enter("ticks")
    click("toggle-agent")
    wait(lambda: "agent-terminal" not in controls(), "The Agent button did not hide the tab")
    h.xdo("mousemove", "--sync", "700", "600")
    idle("A hidden Agent tab drew frames while its agent printed")
    click("toggle-agent")
    shows("tick 10")
    h.results.append("While the tab was hidden, the agent's output drew no frames; shown again, it showed the output")

    # 10. The wheel scrolls back at least 1,000 lines; output keeps the view, typing returns to the bottom.
    focus_terminal()
    enter("lines")
    shows("line 1200")
    wheel(4, 400)
    wait(lambda: (metric("terminal-scrollback") or 0) >= 1000, "The wheel did not scroll back 1,000 lines")
    numbers = [int(match.group(1)) for row in rows() if (match := re.fullmatch(r"line (\d+)", row))]
    assert numbers and min(numbers) <= 200, numbers
    h.capture("native-agent-terminal-scrollback.png")
    enter("size")
    wait(lambda: metric("terminal-scrollback") == 0, "Typing must bring the view back to the bottom")
    enter("ticks")
    wheel(4, 5)
    wait(lambda: (metric("terminal-scrollback") or 0) > 0, "The wheel did not scroll back")
    held = metric("terminal-scrollback")
    time.sleep(1.2)
    assert metric("terminal-scrollback") >= held, "New output pulled the view away from where the user scrolled"
    h.key("ctrl+u")
    wait(lambda: metric("terminal-scrollback") == 0, "A key must bring the view back to the bottom")
    shows("tick 10")
    h.results.append(f"The wheel reached line {min(numbers)} of 1,200; output kept a scrolled view; a key returned to the bottom")

    # 11. In an alternate screen that asks for it, the wheel sends arrow keys.
    enter("alt")
    shows("ALT SCREEN")
    wheel(4, 1)
    shows("^[[A^[[A^[[A")
    h.key("ctrl+u")
    enter("main")
    shows("MAIN SCREEN")
    h.results.append("In the alternate screen with mode 1007 the wheel sent Up arrows")

    # 12. A 50 MB flood keeps frames short and memory bounded.
    frames = base / "agent-terminal" / "frames.log"
    status_file = Path(f"/proc/{app.pid}/status")
    resident = lambda: int(re.search(r"VmRSS:\s+(\d+)", status_file.read_text()).group(1))
    before_kb, began = resident(), time.time() * 1000
    enter("flood")
    shows("flood done", timeout=180)
    ended, after_kb = time.time() * 1000, resident()
    durations = [int(fields[1]) for line in frames.read_text().splitlines()
                 if len(fields := line.split()) == 3 and began <= int(fields[2]) <= ended]
    assert durations and max(durations) < 50_000, (len(durations), max(durations, default=0))
    assert after_kb - before_kb < 100 * 1024, (before_kb, after_kb)
    h.results.append(f"A 50 MB flood took {(ended - began) / 1000:.1f} s; its {len(durations)} frames stayed under "
                     f"{max(durations) / 1000:.1f} ms of UI time; memory grew {(after_kb - before_kb) / 1024:.1f} MB")

    # 13. Choosing another agent restarts the terminal with it and persists per vault.
    first = pid()
    click("agent-backend")
    click("agent-backend:Claude")
    wait(lambda: banner() and banner()[0] == "claude", "Choosing Claude did not start it")
    wait(lambda: first not in stand_ins(), "The previous agent outlived the switch")
    wait(lambda: len(stand_ins()) == 1, "More than one agent runs in the window")
    saved = list((base / "state/rustidian/agent").glob("*.json"))
    assert len(saved) == 1 and json.loads(saved[0].read_text()) == {"backend": "claude"}, saved
    h.results.append("Choosing Claude ended the Elpis stand-in, started Claude and saved the choice for the vault")

    # 14. Restart starts a fresh process on a fresh screen.
    before_restart = pid()
    focus_terminal()
    enter("before restart")
    shows("you said: before restart")
    click("agent-restart")
    wait(lambda: banner() and banner()[1] != before_restart, "Restart did not start a fresh agent")
    assert not any("before restart" in row for row in rows()), rows()
    wait(lambda: before_restart not in stand_ins(), "The restarted agent's old process remained")
    h.results.append("Restart ended the old process and showed a fresh banner on a fresh screen")

    # 15. An agent that exits leaves its last screen and "Exited (code 0)"; Restart works again.
    focus_terminal()
    enter("exit")
    wait(lambda: status() == "Exited (code 0)", "Exit did not show Exited (code 0)")
    shows("bye")
    h.capture("native-agent-terminal-exited.png")
    exited = pid()
    click("agent-restart")
    wait(lambda: banner() and banner()[1] != exited, "Restart after exit did not start a fresh agent")
    wait(lambda: status() == "Running", "The status did not return to Running")
    h.results.append("After exit the last screen stayed with Exited (code 0); Restart started a fresh agent")

    # 16. Ctrl+Shift+A hides the tab from the terminal; the top-right button shows and hides it.
    running = pid()
    focus_terminal()
    h.key("ctrl+shift+a")
    wait(lambda: "agent-terminal" not in controls(), "Ctrl+Shift+A in the terminal did not hide the tab")
    assert running in stand_ins(), "Hiding the tab must not end the agent"
    click("toggle-agent")
    wait(lambda: "agent-terminal" in controls(), "The Agent button did not show the tab")
    assert pid() == running, "Showing the tab again must keep the same agent"
    click("toggle-agent")
    wait(lambda: "agent-terminal" not in controls(), "The Agent button did not hide the tab")
    h.key("ctrl+shift+a")
    wait(lambda: "agent-terminal" in controls(), "Ctrl+Shift+A did not show the tab again")
    h.results.append("Ctrl+Shift+A hid the tab from the focused terminal; the Agent button showed and hid it; the agent kept running")

    # 17. Both themes at two sizes: header and terminal inside the sidebar, nothing overlapping.
    enter("colors")
    enter("glyphs")
    shows("│done│")
    click("editor")
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

    # 18. Ctrl+Q in the terminal goes to the agent; from the editor it quits, ending the agent.
    focus_terminal()
    h.key("ctrl+q")
    assert metric("terminal-focused") == 1, "Ctrl+Q in the terminal must not leave it"
    click("editor")
    h.xdo("key", "--clearmodifiers", "ctrl+q")
    app.wait(timeout=10)
    wait(lambda: not stand_ins(), "An agent process outlived its window", timeout=5)
    h.results.append("Ctrl+Q in the terminal reached the agent and kept the window; from the editor it quit and the agent ended")

    # 19. The vault's choice survives a restart; killing Rustidian outright still ends the agent.
    app = h.relaunch()
    h.xdo("key", "--clearmodifiers", "ctrl+shift+a")
    # The restored sidebar keeps its last, narrow width, so the banner may wrap.
    shows("FAKE claude ready")
    survivor = pid()
    app.kill()
    app.wait(timeout=10)
    wait(lambda: survivor not in stand_ins() and not stand_ins(), "The agent outlived a killed Rustidian", timeout=3)
    h.results.append("After relaunch the vault's Claude choice started; kill -9 of Rustidian ended its agent too")


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
