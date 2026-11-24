"""Offline GPU speech: publish each PCM chunk while synthesis continues."""
import base64
import contextlib
import json
import os
import sys

os.environ["HF_HUB_OFFLINE"] = "1"
os.environ["TRANSFORMERS_OFFLINE"] = "1"
os.environ.setdefault("OMP_NUM_THREADS", "2")

# Package progress output must never enter the JSON protocol.
with contextlib.redirect_stdout(sys.stderr):
    import numpy as np
    import torch
    from faster_qwen3_tts import FasterQwen3TTS
    from speech_text import prepare

    if not torch.cuda.is_available():
        raise RuntimeError("Qwen requires the NVIDIA GPU")
    torch.set_num_threads(2)
    model = FasterQwen3TTS.from_pretrained(
        sys.argv[1], device="cuda:0", dtype=torch.float16,
        attn_implementation="sdpa", max_seq_len=2048, local_files_only=True,
    )
    model.warmup(prefill_len=100)
print(json.dumps({"ready": True, "device": "cuda"}), flush=True)
for line in sys.stdin:
    request = json.loads(line)
    try:
        voice = request.get("voice", "serena")
        style = (
            "Read clearly in a warm, smooth, gently breathy feminine voice, with a relaxed, "
            "confident, subtly sultry tone. Keep the words clear and the pace natural."
            if voice == "serena" else
            "Read this article clearly and naturally, with a calm explanatory tone and pauses at punctuation."
        )
        with contextlib.redirect_stdout(sys.stderr), torch.inference_mode():
            for samples, rate, _timing in model.generate_custom_voice_streaming(
                text=prepare(request["text"]), language="English", speaker=voice,
                instruct=style, chunk_size=8, max_new_tokens=1024,
            ):
                pcm = (np.clip(samples, -1, 1) * 32767).astype("<i2").tobytes()
                if pcm:
                    print(json.dumps({"id": request["id"], "ok": True, "rate": rate,
                                      "pcm": base64.b64encode(pcm).decode("ascii"),
                                      "more": True}), file=sys.__stdout__, flush=True)
        print(json.dumps({"id": request["id"], "ok": True, "done": True}), flush=True)
    except Exception as error:  # noqa: BLE001 -- report model failures through the speech protocol
        print(json.dumps({"id": request["id"], "ok": False,
                          "error": str(error)[:300]}), flush=True)
