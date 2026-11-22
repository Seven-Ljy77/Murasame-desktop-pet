"""Split a CBR/VBR MP3 into one clip per transcript line, without re-encoding.

The clips are cut on MPEG frame boundaries, so they keep the original audio
quality and stay valid MP3 streams. Timestamps come from the transcript that
sits next to the audio, in the form:

    00:00:02
    日：起きたか?
    中：醒了吗？

Usage:
    python split_mp3.py <audio.mp3> <transcript.txt> <output_dir> [--min-gap 0.6]

Each clip is written as 01.mp3, 02.mp3, ... and a segments.json next to them
records the file name, start/end time and both transcript lines.
"""

from __future__ import annotations

import argparse
import json
import re
from dataclasses import dataclass, field
from pathlib import Path

BITRATES = {
    (1, 1): [0, 32, 64, 96, 128, 160, 192, 224, 256, 288, 320, 352, 384, 416, 448],
    (1, 2): [0, 32, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 384],
    (1, 3): [0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320],
    (2, 1): [0, 32, 48, 56, 64, 80, 96, 112, 128, 144, 160, 176, 192, 224, 256],
    (2, 2): [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160],
    (2, 3): [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160],
}
SAMPLE_RATES = {1: [44100, 48000, 32000], 2: [22050, 24000, 16000], 25: [11025, 12000, 8000]}


@dataclass
class Frame:
    offset: int
    length: int
    time: float


@dataclass
class Line:
    start: float
    end: float | None = None
    ja: str = ""
    zh: str = ""
    notes: list[str] = field(default_factory=list)


def parse_frames(data: bytes) -> tuple[list[Frame], float]:
    """Walk MPEG audio frames and return their offsets plus total duration."""
    offset = 0
    if data[:3] == b"ID3":
        size = (
            ((data[6] & 0x7F) << 21)
            | ((data[7] & 0x7F) << 14)
            | ((data[8] & 0x7F) << 7)
            | (data[9] & 0x7F)
        )
        offset = 10 + size

    frames: list[Frame] = []
    time = 0.0
    while offset + 4 <= len(data):
        if data[offset] != 0xFF or (data[offset + 1] & 0xE0) != 0xE0:
            offset += 1
            continue
        version_bits = (data[offset + 1] >> 3) & 0x3
        layer_bits = (data[offset + 1] >> 1) & 0x3
        if version_bits == 1 or layer_bits == 0:
            offset += 1
            continue

        version = {3: 1, 2: 2, 0: 25}[version_bits]
        layer = {3: 1, 2: 2, 1: 3}[layer_bits]
        bitrate_index = (data[offset + 2] >> 4) & 0xF
        rate_index = (data[offset + 2] >> 2) & 0x3
        if bitrate_index == 0xF or rate_index == 3:
            offset += 1
            continue

        bitrate = BITRATES[(1 if version == 1 else 2, layer)][bitrate_index] * 1000
        sample_rate = SAMPLE_RATES[version][rate_index]
        padding = (data[offset + 2] >> 1) & 1

        if layer == 1:
            frame_length = (12 * bitrate // sample_rate + padding) * 4
            samples = 384
        elif layer == 3 and version != 1:
            frame_length = 72 * bitrate // sample_rate + padding
            samples = 576
        else:
            frame_length = 144 * bitrate // sample_rate + padding
            samples = 1152

        if frame_length < 24:
            offset += 1
            continue

        frames.append(Frame(offset, frame_length, time))
        time += samples / sample_rate
        offset += frame_length

    return frames, time


def parse_transcript(path: Path) -> list[Line]:
    """Read either ``HH:MM:SS`` markers or explicit ``start - end`` ranges."""
    lines: list[Line] = []
    for raw in path.read_text(encoding="utf-8").splitlines():
        text = raw.strip()

        range_match = re.match(r"^(\d+(?:\.\d+)?)\s*-\s*(\d+(?:\.\d+)?)$", text)
        if range_match:
            lines.append(Line(start=float(range_match.group(1)), end=float(range_match.group(2))))
            continue

        stamp = re.match(r"^(\d{2}):(\d{2}):(\d{2})$", text)
        if stamp:
            h, m, s = (int(part) for part in stamp.groups())
            lines.append(Line(start=h * 3600 + m * 60 + s))
            continue

        if not lines:
            continue
        for prefix, field_name in (("日：", "ja"), ("中：", "zh"), ("注：", "notes")):
            if text.startswith(prefix):
                value = text[len(prefix) :].strip()
                if field_name == "notes":
                    lines[-1].notes.append(value)
                else:
                    setattr(lines[-1], field_name, value)
                break
    return lines


def merge_close(lines: list[Line], min_gap: float) -> list[Line]:
    """Fold lines whose timestamps are too close to cut between (duplicates)."""
    merged: list[Line] = []
    for line in lines:
        if merged and line.start - merged[-1].start < min_gap:
            merged[-1].ja = (merged[-1].ja + " " + line.ja).strip()
            merged[-1].zh = (merged[-1].zh + " " + line.zh).strip()
            merged[-1].notes.extend(line.notes)
            continue
        merged.append(
            Line(
                start=line.start,
                end=line.end,
                ja=line.ja,
                zh=line.zh,
                notes=list(line.notes),
            )
        )
    return merged


def frame_index_at(frames: list[Frame], seconds: float) -> int:
    """First frame starting at or after ``seconds``."""
    low, high = 0, len(frames)
    while low < high:
        mid = (low + high) // 2
        if frames[mid].time < seconds:
            low = mid + 1
        else:
            high = mid
    return low


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("audio")
    parser.add_argument("transcript")
    parser.add_argument("output_dir")
    parser.add_argument("--min-gap", type=float, default=0.6)
    parser.add_argument(
        "--pre-roll",
        type=float,
        default=0.15,
        help="start this far before each timestamp so the first syllable is not clipped",
    )
    args = parser.parse_args()

    audio_path = Path(args.audio)
    output_dir = Path(args.output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)

    data = audio_path.read_bytes()
    frames, total = parse_frames(data)
    lines = merge_close(parse_transcript(Path(args.transcript)), args.min_gap)
    print(f"{audio_path.name}: {len(frames)} frames, {total:.2f}s, {len(lines)} lines")

    manifest = []
    for index, line in enumerate(lines, start=1):
        start = max(0.0, line.start - args.pre_roll)
        if line.end is not None:
            end = line.end
        elif index < len(lines):
            end = lines[index].start
        else:
            end = total
        first = frame_index_at(frames, start)
        last = frame_index_at(frames, end)
        if last <= first:
            print(f"  skip {index:02d} ({start:.1f}s): empty range")
            continue

        begin = frames[first].offset
        finish = frames[last].offset if last < len(frames) else len(data)
        name = f"{len(manifest) + 1:02d}.mp3"
        (output_dir / name).write_bytes(data[begin:finish])

        length = frames[last - 1].time + 1152 / 48000 - frames[first].time
        manifest.append(
            {
                "file": name,
                "index": len(manifest) + 1,
                "start": round(frames[first].time, 3),
                "end": round(frames[last - 1].time + 1152 / 48000, 3),
                "duration": round(frames[last - 1].time + 1152 / 48000 - frames[first].time, 2),
                "ja": line.ja,
                "zh": line.zh,
                "notes": line.notes,
            }
        )
        print(f"  {name}  {start:5.1f}s → {end:5.1f}s  ({length:.2f}s)  {line.zh or line.ja}")

    (output_dir / "segments.json").write_text(
        json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    print(f"wrote {len(manifest)} clips + segments.json to {output_dir}")


if __name__ == "__main__":
    main()
