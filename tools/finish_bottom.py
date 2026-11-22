"""Finish the bottom edge: restore clothing, drop the white bar, re-cut the rim.

The sticker outline runs along the bottom cut of every emoticon, and part of
that white also belongs to the shirt, so the outline pass nibbles at the
garment's lower edge. Both are pure white and touching, which is why no purely
local rule separates them.

So the bottom gets two extra passes:

1. take back every pixel the outline pass removed in the bottom band, straight
   from the source art — down here that white really is clothing;
2. crop the flat white bar the sticker carries along the very bottom (trailing
   rows that are entirely pure white). After the crop the artwork ends at the
   image edge, so the garment looks whole instead of floating above a gap;
3. cut again, but only inside that band: white connected to the transparent
   background is rim and goes away, white sealed in by the crop stays. Step 1
   brought back a few rim slivers at the corners; this removes them without
   touching the restored clothing.

Usage:
    python finish_bottom.py <source_dir> <asset_dir> [--rows 45] [--dry-run]

The asset directory must contain manifest.json produced by
batch_remove_outline.py, which maps each output back to its source file.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path

import numpy as np
from PIL import Image, ImageFilter

from remove_outline import (
    COLOUR_PAD_PASSES,
    EDGE_BLUR_RADIUS,
    WHITE_MIN_CHANNEL,
    pad_colours,
    white_connected_to_silhouette,
)


def trailing_white_rows(image: Image.Image, limit: int = 40) -> int:
    array = np.array(image.convert("RGBA"))
    rgb = array[:, :, :3].astype(int)
    alpha = array[:, :, 3]

    rows = 0
    for y in range(array.shape[0] - 1, max(array.shape[0] - limit, 0) - 1, -1):
        visible = alpha[y] > 0
        if visible.sum() == 0:
            rows += 1
            continue
        pure = visible & (rgb[y].min(axis=1) == 255)
        if pure.sum() == visible.sum():
            rows += 1
        else:
            break
    return rows


def finish(source_path: Path, asset_path: Path, rows: int) -> tuple[Image.Image, dict]:
    source = np.array(Image.open(source_path).convert("RGBA"))
    asset = np.array(Image.open(asset_path).convert("RGBA"))
    if source.shape != asset.shape:
        raise SystemExit(f"size mismatch for {asset_path.name}: {source.shape} vs {asset.shape}")

    height = asset.shape[0]
    band = np.zeros(asset.shape[:2], dtype=bool)
    band[max(0, height - rows) :, :] = True

    restore = (asset[:, :, 3] == 0) & (source[:, :, 3] > 40) & band
    patched = asset.copy()
    patched[restore] = source[restore]

    image = Image.fromarray(patched, "RGBA")
    trimmed = trailing_white_rows(image)
    if trimmed == 0:
        # No sticker bar at the bottom: this artwork was not affected by the
        # bottom-edge problem, so leave it exactly as the outline pass left it.
        return image, {"restored_px": 0, "trimmed_rows": 0, "recut_px": 0}
    image = image.crop((0, 0, image.size[0], image.size[1] - trimmed))

    array = np.array(image)
    rgb = array[:, :, :3].astype(np.int16)
    alpha = array[:, :, 3].astype(np.int16)
    white = (rgb.min(axis=2) >= WHITE_MIN_CHANNEL) & (alpha > 0)
    rim = white_connected_to_silhouette(white, alpha == 0)

    # Only clean up the band that was just touched; the rest of the image was
    # already finished by remove_outline.py.
    recut = np.zeros(rim.shape, dtype=bool)
    top = max(0, height - rows - trimmed)
    recut[top:, :] = rim[top:, :]

    if recut.any():
        shape = (alpha > 0) & ~recut
        mask = Image.fromarray((shape * 255).astype(np.uint8))
        alpha = np.array(mask.filter(ImageFilter.GaussianBlur(EDGE_BLUR_RADIUS))).astype(np.int16)
        rgb = pad_colours(rgb, shape, COLOUR_PAD_PASSES)
        image = Image.fromarray(
            np.dstack(
                [
                    np.clip(rgb, 0, 255).astype(np.uint8),
                    np.clip(alpha, 0, 255).astype(np.uint8),
                ]
            ),
            "RGBA",
        )

    return image, {
        "restored_px": int(restore.sum()),
        "trimmed_rows": trimmed,
        "recut_px": int(recut.sum()),
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source_dir")
    parser.add_argument("asset_dir")
    parser.add_argument("--rows", type=int, default=45, help="height of the restored band")
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()

    source_dir = Path(args.source_dir)
    asset_dir = Path(args.asset_dir)
    manifest = json.loads((asset_dir / "manifest.json").read_text(encoding="utf-8"))

    for item in manifest:
        asset_path = asset_dir / item["file"]
        source_path = source_dir / item["source"]
        image, stats = finish(source_path, asset_path, args.rows)
        item["size"] = list(image.size)
        print(
            f"{item['file']}: restored {stats['restored_px']:>5}px, "
            f"trimmed {stats['trimmed_rows']}px, recut {stats['recut_px']:>4}px "
            f"-> {image.size[0]}x{image.size[1]}"
        )
        if not args.dry_run:
            image.save(asset_path)

    if not args.dry_run:
        (asset_dir / "manifest.json").write_text(
            json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
        )


if __name__ == "__main__":
    main()
