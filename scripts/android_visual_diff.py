#!/usr/bin/env python3
from __future__ import annotations

import argparse
import sys
from pathlib import Path

from PIL import Image


def diff_ratio(
    baseline_path: Path,
    current_path: Path,
    crop_top: int,
    crop_bottom: int,
) -> float:
    with Image.open(baseline_path).convert("RGB") as base_img, Image.open(
        current_path
    ).convert("RGB") as curr_img:
        if base_img.size != curr_img.size:
            curr_img = curr_img.resize(base_img.size, Image.Resampling.BILINEAR)

        width, height = base_img.size
        top = max(0, crop_top)
        bottom = max(0, crop_bottom)
        y1 = min(top, height)
        y2 = max(y1, height - bottom)
        base_img = base_img.crop((0, y1, width, y2))
        curr_img = curr_img.crop((0, y1, width, y2))

        b = base_img.tobytes()
        c = curr_img.tobytes()
        if len(b) != len(c):
            return 1.0

        total = 0
        for bv, cv in zip(b, c):
            total += abs(bv - cv)

        return total / (len(b) * 255.0) if b else 0.0


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Compare Android screenshot baselines with a normalized pixel diff threshold."
    )
    parser.add_argument("--baseline-dir", required=True)
    parser.add_argument("--current-dir", required=True)
    parser.add_argument("--threshold", type=float, default=0.03)
    parser.add_argument("--crop-top", type=int, default=80)
    parser.add_argument("--crop-bottom", type=int, default=0)
    args = parser.parse_args()

    baseline_dir = Path(args.baseline_dir)
    current_dir = Path(args.current_dir)

    if not baseline_dir.exists():
        print(f"Baseline dir not found: {baseline_dir}", file=sys.stderr)
        return 2
    if not current_dir.exists():
        print(f"Current dir not found: {current_dir}", file=sys.stderr)
        return 2

    baseline_files = sorted(baseline_dir.glob("*.png"))
    if not baseline_files:
        print(f"No baseline png files in: {baseline_dir}", file=sys.stderr)
        return 2

    failed = False
    for baseline in baseline_files:
        current = current_dir / baseline.name
        if not current.exists():
            print(f"[MISS] {baseline.name} missing in current dir", file=sys.stderr)
            failed = True
            continue

        ratio = diff_ratio(
            baseline, current, crop_top=args.crop_top, crop_bottom=args.crop_bottom
        )
        status = "PASS" if ratio <= args.threshold else "FAIL"
        print(f"[{status}] {baseline.name}: diff={ratio:.6f}, threshold={args.threshold:.6f}")
        if ratio > args.threshold:
            failed = True

    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
