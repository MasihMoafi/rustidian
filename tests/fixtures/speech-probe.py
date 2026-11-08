#!/usr/bin/env python3
"""No model or audio device: a controllable worker using Kokoro's line protocol."""
import base64
import json
import os
from pathlib import Path
import sys
import time

log = Path(os.environ["READ_ALOUD_LOG"])
mode = log.with_suffix(".mode").read_text()
with log.with_suffix(".pids").open("a") as output:
    output.write(str(os.getpid()) + "\n")
if mode == "startup":
    log.with_suffix(".blocked").write_text(str(os.getpid()))
    time.sleep(60)
print(json.dumps({"ready": True, "device": "cpu"}), flush=True)
for line in sys.stdin:
    request = json.loads(line)
    if mode == "reply":
        log.with_suffix(".blocked").write_text(str(os.getpid()))
        time.sleep(60)
    with log.with_suffix(".voiced").open("a") as output:
        output.write(request["text"] + "\n")
    with log.with_suffix(".requests").open("a") as output:
        output.write(json.dumps(request) + "\n")
    time.sleep(.3)
    pcm = base64.b64encode((request["text"] + "\n").encode()).decode()
    print(json.dumps({"id": request["id"], "ok": True, "rate": 24000, "pcm": pcm}), flush=True)
