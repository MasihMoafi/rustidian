"""Rustidian read aloud: local Kokoro, on the GPU when one is there.

argv: MODEL VOICES. Requests contain an id and text. Replies contain the id, sample rate,
and base64-encoded mono 16-bit PCM, or an error.
The first line out is {"ready": true, "device": "cuda" | "cpu"}.
"""

import base64
import glob
import json
import os
import sys

# onnxruntime finds CUDA and cuDNN through the loader path; the NVIDIA wheels (installed with
# PyTorch, for example) keep them in site-packages/nvidia/*/lib. Set the path once, then restart.
if not os.environ.get("RUSTIDIAN_KOKORO_LIBS"):
    libraries = sorted(
        {
            path
            for entry in sys.path
            for path in glob.glob(os.path.join(entry, "nvidia", "*", "lib"))
        }
    )
    os.environ["RUSTIDIAN_KOKORO_LIBS"] = "1"
    if libraries:
        os.environ["LD_LIBRARY_PATH"] = os.pathsep.join(
            libraries + [os.environ.get("LD_LIBRARY_PATH", "")]
        )
        os.execv(sys.executable, [sys.executable, "-u", *sys.argv])

# Loading or synthesis can keep the request loop from noticing EOF. Watch the
# app's pipe separately so even an abrupt app exit immediately releases the GPU.
if sys.platform == "linux":
    import select
    import threading

    def exit_when_app_closes():
        pipe = select.poll()
        pipe.register(sys.stdin.fileno(), select.POLLHUP | select.POLLERR)
        pipe.poll()
        os._exit(0)

    threading.Thread(target=exit_when_app_closes, daemon=True).start()

import kokoro_onnx
import onnxruntime
import numpy as np

onnxruntime.set_default_logger_severity(3)
providers = [
    name
    for name in ("CUDAExecutionProvider", "CPUExecutionProvider")
    if name in onnxruntime.get_available_providers()
]
session = onnxruntime.InferenceSession(sys.argv[1], providers=providers)
model = kokoro_onnx.Kokoro.from_session(session, sys.argv[2])
voice = os.environ.get("RUSTIDIAN_KOKORO_VOICE", "af_heart")
speed = float(os.environ.get("RUSTIDIAN_KOKORO_SPEED", "1.0"))
model.create(
    "Ready.", voice=voice, speed=speed, lang="en-us"
)  # load the voice before the first request
device = "cuda" if session.get_providers()[0] == "CUDAExecutionProvider" else "cpu"
print(json.dumps({"ready": True, "device": device}), flush=True)
for line in sys.stdin:
    request = json.loads(line)
    try:
        samples, rate = model.create(
            request["text"], voice=voice, speed=request.get("speed", speed), lang="en-us"
        )
        pcm = (np.clip(samples, -1, 1) * 32767).astype("<i2").tobytes()
        print(json.dumps({"id": request["id"], "ok": True, "rate": rate,
                          "pcm": base64.b64encode(pcm).decode("ascii")}), flush=True)
    except Exception as error:  # noqa: BLE001 -- one bad piece of text must not end the worker
        print(
            json.dumps({"id": request["id"], "ok": False, "error": str(error)[:300]}),
            flush=True,
        )
