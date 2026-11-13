"""Offline Qwen CustomVoice worker, using the reader's existing PCM protocol."""
import base64
import json
import os
import sys

os.environ["HF_HUB_OFFLINE"] = "1"
os.environ["TRANSFORMERS_OFFLINE"] = "1"
os.environ.setdefault("OMP_NUM_THREADS", "2")

import numpy as np
import torch
from qwen_tts import Qwen3TTSModel
from speech_text import prepare

torch.set_num_threads(2)
device = "cuda" if torch.cuda.is_available() else "cpu"
accelerated = None
if device == "cuda":
    try:
        from faster_qwen3_tts import FasterQwen3TTS
        accelerated = FasterQwen3TTS
    except ImportError:
        pass
if accelerated is not None:
    model = accelerated.from_pretrained(
        sys.argv[1], device="cuda:0", dtype=torch.float16,
        attn_implementation="sdpa", max_seq_len=1024, local_files_only=True,
    )
    model.warmup(prefill_len=100)
else:
    model = Qwen3TTSModel.from_pretrained(
        sys.argv[1],
        device_map="cuda:0" if device == "cuda" else "cpu",
        dtype=torch.float16 if device == "cuda" else torch.float32,
        attn_implementation="sdpa",
        local_files_only=True,
    )
voice = os.environ.get("RUSTIDIAN_QWEN_VOICE", "aiden")
print(json.dumps({"ready": True, "device": device}), flush=True)
for line in sys.stdin:
    request = json.loads(line)
    try:
        with torch.inference_mode():
            audio, rate = model.generate_custom_voice(
                text=prepare(request["text"]),
                language="English",
                speaker=voice,
                instruct="Read this article clearly and naturally, with a calm explanatory tone and pauses at punctuation.",
                max_new_tokens=512,
            )
        samples = audio[0]
        speed = float(request.get("speed", 1.0))
        if speed != 1.0:
            import librosa
            samples = librosa.effects.time_stretch(samples, rate=speed)
        pcm = (np.clip(samples, -1, 1) * 32767).astype("<i2").tobytes()
        print(json.dumps({"id": request["id"], "ok": True, "rate": rate,
                          "pcm": base64.b64encode(pcm).decode("ascii")}), flush=True)
    except Exception as error:
        print(json.dumps({"id": request["id"], "ok": False,
                          "error": str(error)[:300]}), flush=True)
