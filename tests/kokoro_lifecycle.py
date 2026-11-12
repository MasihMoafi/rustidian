#!/usr/bin/env python3
"""Exercise the shipped speech worker's lifetime without loading a voice or GPU."""
import base64
import json
import os
from pathlib import Path
import select
import subprocess
import sys
import tempfile
import time


def prepare(base, stage):
    modules = base / "voice-modules"
    modules.mkdir()
    (modules / "onnxruntime.py").write_text('''
import os, time
from pathlib import Path
def set_default_logger_severity(level): pass
def get_available_providers(): return ["CPUExecutionProvider"]
class InferenceSession:
    def __init__(self, *args, **kwargs):
        marker = Path(os.environ["KOKORO_TEST_MARKER"])
        marker.with_suffix(".pid").write_text(str(os.getpid()))
        marker.with_suffix(".loading").touch()
        if os.environ["KOKORO_TEST_STAGE"] == "loading": time.sleep(120)
    def get_providers(self): return ["CPUExecutionProvider"]
''')
    (modules / "kokoro_onnx.py").write_text('''
import os, time
from pathlib import Path
class Kokoro:
    @classmethod
    def from_session(cls, *args): return cls()
    def create(self, text, **kwargs):
        if text != "Ready.":
            Path(os.environ["KOKORO_TEST_MARKER"]).with_suffix(".synthesis").touch()
            if os.environ["KOKORO_TEST_STAGE"] == "synthesis": time.sleep(120)
        return [0.0] * 240, 24000
''')
    (modules / "numpy.py").write_text('''
class Samples:
    def __mul__(self, value): return self
    def astype(self, dtype): return self
    def tobytes(self): return b"\\0" * 480
def clip(samples, minimum, maximum): return Samples()
''')
    for name in ("model.onnx", "voices.bin"):
        (base / name).touch()
    player = base / "silent-player"
    player.write_text("#!/bin/sh\ncat >/dev/null\n")
    player.chmod(0o700)
    return dict(
        PYTHONPATH=str(modules), PYTHONDONTWRITEBYTECODE="1",
        RUSTIDIAN_KOKORO_PYTHON=sys.executable,
        RUSTIDIAN_KOKORO_MODEL=str(base / "model.onnx"),
        RUSTIDIAN_KOKORO_VOICES=str(base / "voices.bin"),
        RUSTIDIAN_READ_ALOUD_PLAYER=str(player),
        KOKORO_TEST_MARKER=str(base / "worker"), KOKORO_TEST_STAGE=stage,
    )


def wait_for(predicate, message, seconds=5):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        if predicate():
            return
        time.sleep(.02)
    raise AssertionError(message)


def alive(pid):
    try:
        status = Path(f"/proc/{pid}/stat").read_text()
        return status[status.rfind(")") + 2:].split()[0] != "Z"
    except FileNotFoundError:
        return False


def check(worker, stage):
    with tempfile.TemporaryDirectory(prefix="kokoro-lifetime-") as temporary:
        base = Path(temporary)
        env = dict(os.environ, **prepare(base, stage))
        child = subprocess.Popen(
            [sys.executable, "-u", str(worker), env["RUSTIDIAN_KOKORO_MODEL"],
             env["RUSTIDIAN_KOKORO_VOICES"]],
            env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            text=True,
        )
        try:
            def reply():
                assert select.select([child.stdout], [], [], 5)[0], "Voice did not reply"
                return json.loads(child.stdout.readline())

            if stage == "loading":
                wait_for(lambda: (base / "worker.loading").exists(), "Voice never began loading")
            else:
                assert reply()["ready"]
                for request_id in range(1, 3 if stage == "idle" else 2):
                    child.stdin.write(json.dumps(dict(id=request_id, text="Read this.", out=str(base / "out.wav"))) + "\n")
                    child.stdin.flush()
                    if stage == "synthesis":
                        wait_for(lambda: (base / "worker.synthesis").exists(), "Synthesis never began")
                    else:
                        response = reply()
                        assert response["id"] == request_id and response["ok"], response
                        assert response["rate"] == 24000
                        assert base64.b64decode(response["pcm"]) == b"\0" * 480
                        assert child.poll() is None, "Voice must remain reusable while the app is open"
            child.stdin.close()
            try:
                child.wait(timeout=2)
            except subprocess.TimeoutExpired:
                raise AssertionError(f"Voice survived closing its app pipe during {stage}") from None
            assert child.returncode == 0, child.stderr.read()
            print(f"PASS: app pipe closes during {stage}; voice exits immediately")
        finally:
            if child.poll() is None:
                child.kill()
            child.wait()
            for stream in (child.stdin, child.stdout, child.stderr):
                stream.close()


if __name__ == "__main__":
    worker = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(__file__).resolve().parents[1] / "assets/read-aloud/kokoro_worker.py"
    failures = []
    for stage in ("idle", "loading", "synthesis"):
        try:
            check(worker, stage)
        except AssertionError as error:
            failures.append(str(error))
            print(f"FAIL: {error}")
    if failures:
        raise SystemExit(1)
