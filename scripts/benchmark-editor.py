"""Run the declared native editor experiment after building its release example."""
import argparse
from datetime import datetime, timezone
import fcntl
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess

ROOT = Path(__file__).resolve().parents[1]


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--corpus", choices=["repeated", "diverse"], default="repeated")
    parser.add_argument("--folded", action="store_true")
    args = parser.parse_args()
    binary = ROOT / "target/release/examples/editor_benchmark"
    with (ROOT / "reports/.gui-smoke.lock").open("w") as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            raise SystemExit("A native GUI run is active; finish it before measuring.")
        assert "RUST_NOTES_UI_PROBE" not in os.environ
        paths = ["examples/editor_benchmark.rs", "Cargo.toml", "Cargo.lock"]
        paths += sorted(str(path.relative_to(ROOT)) for path in (ROOT / "src").rglob("*.rs"))
        paths += sorted(str(path.relative_to(ROOT)) for path in (ROOT / "vendor").rglob("*.rs"))
        paths += sorted(str(path.relative_to(ROOT)) for path in (ROOT / "vendor").rglob("Cargo.toml"))
        paths += sorted(str(path.relative_to(ROOT)) for path in (ROOT / "assets/fonts").iterdir() if path.suffix in [".ttf", ".otf"])
        provenance = {
            "started_utc": datetime.now(timezone.utc).isoformat(),
            "platform": platform.platform(),
            "cpu": next(
                line.split(":", 1)[1].strip()
                for line in Path("/proc/cpuinfo").read_text().splitlines()
                if line.startswith("model name")
            ),
            "rustc": subprocess.check_output(["rustc", "--version"], text=True).strip(),
            "binary_sha256": digest(binary),
            "source_sha256": {path: digest(ROOT / path) for path in paths},
            "load_average_before": list(os.getloadavg()),
        }
        result = subprocess.run(
            [str(binary)] + (["--diverse"] if args.corpus == "diverse" else []) + (["--folded"] if args.folded else []),
            cwd=ROOT, text=True, capture_output=True, timeout=120,
        )
        args.output.with_suffix(".log").write_text(result.stderr)
        result.check_returncode()
        report = json.loads(result.stdout)
        assert digest(binary) == provenance["binary_sha256"]
        assert all(digest(ROOT / path) == value for path, value in provenance["source_sha256"].items())
        provenance["finished_utc"] = datetime.now(timezone.utc).isoformat()
        provenance["load_average_after"] = list(os.getloadavg())
        report["provenance"] = provenance
        args.output.write_text(json.dumps(report, indent=2) + "\n")
        print(f"Recorded {len(report['conditions'])} native editor conditions in {args.output}")


if __name__ == "__main__":
    main()
