# Local pronunciation

Kokoro receives phonemes from [Misaki](https://github.com/hexgrad/misaki), as
[kokoro-onnx recommends](https://github.com/thewh1teagle/kokoro-onnx#voices).
`speech_text.py` expands written abbreviations in the spoken copy and uses
Misaki's English part-of-speech analysis to choose contextual pronunciations.
The original note is unchanged.

The Python environment needs `misaki[en]==0.9.4` and spaCy's `en_core_web_sm`
language model in addition to Kokoro's existing dependencies. Misaki supports
Python 3.10–3.12. The reader prefers `~/.local/share/rustidian/voice-venv/bin/python`;
`RUSTIDIAN_KOKORO_PYTHON` still overrides this location. Install the language model
during setup; reading runs offline and never downloads it implicitly.

Settings → Read aloud also offers Local (Qwen). It uses the official
`Qwen/Qwen3-TTS-12Hz-1.7B-CustomVoice` checkpoint, including its `speech_tokenizer`
folder, in `~/.local/share/rustidian/models/`, and the `qwen-tts` runtime in
`~/.local/share/rustidian/qwen-venv/`. `RUSTIDIAN_QWEN_PYTHON`,
`RUSTIDIAN_QWEN_MODEL` and `RUSTIDIAN_QWEN_VOICE` override these defaults.
The reader prefers `~/.local/share/rustidian/qwen-fast-venv/` when installed.
That environment uses `faster-qwen3-tts==0.5.4` with its `qwen-tts-hf` compatibility
runtime and Transformers 5. It uses CUDA graphs with the same official weights;
the ordinary runtime remains supported. Keep the two runtimes in separate environments
because `qwen-tts` and `qwen-tts-hf` provide the same Python module.
Qwen receives a calm article-reading instruction. Changing local engines releases
the previous worker before loading the next, so both models do not occupy the GPU.

Run pronunciation regressions with the configured voice Python:

```sh
~/.local/share/rustidian/voice-venv/bin/python tests/local_speech.py
```
