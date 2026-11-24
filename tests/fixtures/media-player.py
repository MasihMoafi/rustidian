#!/usr/bin/python3
"""A recording desktop handler; video uses a real silent player on the test display."""
import json
import os
from pathlib import Path
import subprocess
import sys
from urllib.parse import unquote, urlsplit

with Path(os.environ["MEDIA_BROWSER_LOG"]).open("a") as log:
    log.write(repr(sys.argv[1:]) + "\n")
uri = urlsplit(sys.argv[1])
if uri.scheme == "file" and Path(uri.path).suffix.lower() in (".mp4", ".webm", ".mov"):
    player = subprocess.Popen(
        ["ffplay", "-hide_banner", "-loglevel", "error", "-autoexit", "-an", "-threads", "1", unquote(uri.path)],
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
    )
    with Path(os.environ["MEDIA_PLAYER_LOG"]).open("a") as log:
        log.write(json.dumps({"pid": player.pid, "uri": sys.argv[1], "event": "started", "handler": Path(sys.argv[0]).name}) + "\n")
    try:
        code = player.wait(timeout=30)
    except subprocess.TimeoutExpired:
        player.terminate()
        player.wait(timeout=5)
        code = -1
    with Path(os.environ["MEDIA_PLAYER_LOG"]).open("a") as log:
        log.write(json.dumps({"pid": player.pid, "event": "finished", "code": code}) + "\n")
