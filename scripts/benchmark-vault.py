"""Generate the declared deterministic corpus and run the release Rust benchmark."""
import json
import hashlib
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
with tempfile.TemporaryDirectory(prefix="rustidian-corpus-") as temporary:
    root = Path(temporary)
    total_bytes = 0
    for number in range(5000):
        path = root / ("Home.md" if number == 0 else f"Note {number:05d}.md")
        text = f"# Note {number:05d}\n\n" + "Connected notes, 你好, **Markdown** and [[Home]]. Search and navigate this vault.\n\n" * 16
        path.write_text(text)
        total_bytes += len(text.encode())
    completed = subprocess.run([str(ROOT / "target/release/examples/vault_benchmark"), str(root)], check=True, capture_output=True, text=True)
    result = {"corpus": {"notes": 5000, "utf8_bytes": total_bytes, "recipe": "script benchmark-vault.py; sequential names, 16 identical paragraphs per note"}, "samples": json.loads(completed.stdout)}
    result["build"] = {"binary_sha256": hashlib.sha256((ROOT / "target/release/examples/vault_benchmark").read_bytes()).hexdigest(), "rustc": subprocess.check_output(["rustc", "--version"], text=True).strip()}
    (ROOT / "reports/vault-benchmark.json").write_text(json.dumps(result, indent=2) + "\n")
    print(completed.stdout)
