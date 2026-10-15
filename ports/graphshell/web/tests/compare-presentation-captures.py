#!/usr/bin/env python3
"""Same-state WebGL2/WebGPU capture comparison: full-frame RGB RMS, max RGB and alpha deltas.

Compares before-loss, after-restore and odd-resize PNGs from two `run-presentation-webgl.py --mode compare`
output directories. The default contract is RGB RMS <= 1.0, max RGB delta <= 16, alpha delta 0 (Mark's R157
presentation limit); `--contract FILE` takes a JSON object with rgb_rms_max, max_rgb_delta and max_alpha_delta.
Prints the rows as JSON and exits 1 unless every capture is the same size and inside the contract.
"""
import argparse, json, math, sys
from pathlib import Path
from PIL import Image

NAMES = ["before-loss", "after-restore", "odd-resize"]
CONTRACT = {"rgb_rms_max": 1.0, "max_rgb_delta": 16, "max_alpha_delta": 0}


def compare(a, b):
    A = Image.open(a).convert("RGBA")
    B = Image.open(b).convert("RGBA")
    if A.size != B.size:
        return {"size_mismatch": [list(A.size), list(B.size)]}
    sq = n = mrgb = ma = 0
    pa, pb = A.tobytes(), B.tobytes()
    for i in range(0, len(pa), 4):
        for c in range(3):
            d = pa[i + c] - pb[i + c]
            sq += d * d
            n += 1
            mrgb = max(mrgb, abs(d))
        ma = max(ma, abs(pa[i + 3] - pb[i + 3]))
    return {"rgb_rms": math.sqrt(sq / n), "max_rgb_delta": mrgb, "max_alpha_delta": ma, "size": list(A.size)}


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("webgl2", type=Path, help="run-presentation-webgl.py output directory for the WebGL2 route")
    parser.add_argument("webgpu", type=Path, help="run-presentation-webgl.py output directory for the WebGPU route")
    parser.add_argument("--contract", type=Path, help="JSON contract overriding the defaults")
    args = parser.parse_args()
    contract = json.loads(args.contract.read_text()) if args.contract else CONTRACT
    rows = []
    for name in NAMES:
        row = {"name": name, **compare(args.webgl2 / f"{name}.png", args.webgpu / f"{name}.png")}
        row["pass"] = (row.get("rgb_rms", math.inf) <= contract["rgb_rms_max"]
                       and row.get("max_rgb_delta", math.inf) <= contract["max_rgb_delta"]
                       and row.get("max_alpha_delta", math.inf) <= contract["max_alpha_delta"])
        rows.append(row)
    print(json.dumps({"contract": contract, "rows": rows}, indent=2))
    return 0 if all(row["pass"] for row in rows) else 1


if __name__ == "__main__":
    sys.exit(main())
