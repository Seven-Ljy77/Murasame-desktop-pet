"""Batch-apply remove_outline.py to a folder of emote PNGs.

Outputs NN.png (source order) plus a manifest.json describing each expression,
so the frontend can map files to Chinese labels without non-ASCII filenames.

Usage:
    python batch_remove_outline.py <source-dir> <output-dir>
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

from PIL import Image

sys.path.insert(0, str(Path(__file__).parent))
from remove_outline import remove_outline  # noqa: E402


def sort_key(path: Path) -> int:
    match = re.match(r"(\d+)", path.stem)
    return int(match.group(1)) if match else 10 ** 9


def label_for(path: Path) -> str:
    parts = path.stem.split("·", 1)
    raw = parts[1] if len(parts) == 2 else path.stem
    return raw.replace(";", "·").replace("；", "·")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source")
    parser.add_argument("output")
    parser.add_argument(
        "--repaint-inner",
        action="store_true",
        help="repaint enclosed separators instead of leaving them white",
    )
    args = parser.parse_args()

    source_dir = Path(args.source)
    output_dir = Path(args.output)
    output_dir.mkdir(parents=True, exist_ok=True)

    files = sorted(source_dir.glob("*.png"), key=sort_key)
    if not files:
        raise SystemExit(f"no PNGs found in {source_dir}")

    manifest = []
    print(f"{'file':<12} {'source':<24} {'size':<10} {'outer':>7} {'inner':>6} {'repaint':>8}")
    for index, path in enumerate(files, start=1):
        with Image.open(path) as image:
            result, stats = remove_outline(image, fill_inner=args.repaint_inner)
        name = f"{index:02d}.png"
        result.save(output_dir / name)
        manifest.append(
            {
                "file": name,
                "index": index,
                "label": label_for(path),
                "source": path.name,
                "size": list(result.size),
            }
        )
        print(
            f"{name:<12} {path.name:<24} {result.size[0]}x{result.size[1]:<6} "
            f"{stats['outer_ring_px']:>7} {stats['inner_islands_found']:>6} "
            f"{stats['inner_islands_repainted']:>8}"
        )

    (output_dir / "manifest.json").write_text(
        json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    print(f"\nwrote {len(manifest)} images + manifest.json to {output_dir}")


if __name__ == "__main__":
    main()
