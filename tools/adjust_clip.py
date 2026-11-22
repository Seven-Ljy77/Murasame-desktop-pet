"""Nudge one clip's start/end without re-encoding, then update segments.json.

Handy while fine-tuning a hand-cut transcript: the clip is re-cut from the
original MP3 on MPEG frame boundaries, so only the requested edge moves.

Usage:
    python adjust_clip.py <original.mp3> <clip_dir> <clip_name> [--start 0.1] [--end -0.1]

Deltas are seconds; positive moves the edge later, negative earlier. The value
written to segments.json is what actually landed on the frame grid, because MP3
can only be cut every 24 ms (48 kHz, 1152 samples per frame).
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path

from split_mp3 import frame_index_at, parse_frames


def nearest_frame(frames, seconds: float) -> int:
    index = frame_index_at(frames, seconds)
    if index > 0 and abs(frames[index - 1].time - seconds) < abs(frames[index].time - seconds):
        return index - 1
    return index


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("original")
    parser.add_argument("clip_dir")
    parser.add_argument("clip")
    parser.add_argument("--start", type=float, default=0.0)
    parser.add_argument("--end", type=float, default=0.0)
    args = parser.parse_args()

    data = Path(args.original).read_bytes()
    frames, _total = parse_frames(data)
    clip_dir = Path(args.clip_dir)
    manifest_path = clip_dir / "segments.json"
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    entry = next((item for item in manifest if item["file"] == args.clip), None)
    if entry is None:
        raise SystemExit(f"{args.clip} is not in {manifest_path}")

    wanted_start = entry["start"] + args.start
    wanted_end = entry["end"] + args.end
    first = nearest_frame(frames, wanted_start)
    last = nearest_frame(frames, wanted_end)
    if last <= first:
        raise SystemExit("the adjusted range is empty")

    begin = frames[first].offset
    finish = frames[last].offset if last < len(frames) else len(data)
    (clip_dir / args.clip).write_bytes(data[begin:finish])

    start = frames[first].time
    end = frames[last - 1].time + 1152 / 48000
    entry["start"] = round(start, 3)
    entry["end"] = round(end, 3)
    entry["duration"] = round(end - start, 2)
    manifest_path.write_text(
        json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )

    print(f"{args.clip}: {start:.3f}s → {end:.3f}s ({entry['duration']:.2f}s), {finish - begin} bytes")


if __name__ == "__main__":
    main()
