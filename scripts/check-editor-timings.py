"""Check complete native editor measurements against the declared CPU limits."""
import json
from pathlib import Path
from statistics import median
import sys


def check(path):
    report = json.loads(Path(path).read_text())
    conditions = report["conditions"]
    if [condition["blocks"] for condition in conditions] != [20, 200, 500]:
        raise ValueError(f"{path}: incomplete corpus")
    for condition in conditions:
        if condition["insertions_verified"] != 25:
            raise ValueError(f"{path}: input preservation failed")
        if any(len(condition[phase]) != 20 for phase in ("stable", "edited")):
            raise ValueError(f"{path}: incomplete samples")
        idle = median(sample["ui_us"] for sample in condition["stable"])
        edited = median(sample["ui_us"] for sample in condition["edited"])
        print(json.dumps({"report": str(path), "bytes": condition["source_bytes"],
                          "idle_us": idle, "edited_us": edited}), flush=True)
        if idle > 5000 or edited > 16700:
            raise ValueError(f"{path}: editor CPU limit exceeded")


if __name__ == "__main__":
    if len(sys.argv) != 3:
        raise SystemExit("Pass unfolded and folded raw JSON reports")
    for path in sys.argv[1:]:
        check(path)
