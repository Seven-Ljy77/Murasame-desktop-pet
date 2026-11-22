/**
 * Browser side of the voice feature: turns clip numbers into URLs and plays
 * them through a single audio element, which is what keeps clips from
 * interrupting each other.
 */

import segments from "../assets/voice/segments.json";
import { VoiceTrigger } from "./voiceTrigger.ts";

const CLIP_URLS = import.meta.glob("../assets/voice/*.mp3", {
  eager: true,
  query: "?url",
  import: "default",
}) as Record<string, string>;

const BY_INDEX = new Map<number, string>();
for (const entry of segments) {
  const url = CLIP_URLS[`../assets/voice/${entry.file}`];
  if (url) BY_INDEX.set(entry.index, url);
}

class ElementSink {
  private readonly audio = new Audio();
  private muted = false;

  constructor() {
    this.audio.preload = "auto";
    this.audio.volume = 0.6;
  }

  busy(): boolean {
    return !this.audio.paused && !this.audio.ended;
  }

  play(clip: number): void {
    if (this.muted) return;
    const url = BY_INDEX.get(clip);
    if (!url) return;
    this.audio.src = url;
    void this.audio.play().catch((error) => {
      console.error("Unable to play voice clip.", error);
    });
  }

  setVolume(value: number): void {
    this.audio.volume = Math.min(1, Math.max(0, value));
  }

  setMuted(value: boolean): void {
    this.muted = value;
    if (value) this.audio.pause();
  }

  pause(): void {
    this.audio.pause();
  }
}

export class VoicePlayer {
  private readonly sink = new ElementSink();
  private readonly trigger = new VoiceTrigger(this.sink);

  setVolume(value: number): void {
    this.sink.setVolume(value);
  }

  setMuted(value: boolean): void {
    this.sink.setMuted(value);
  }

  pause(): void {
    this.sink.pause();
  }

  onIdleExpression(index: number): void {
    this.trigger.onIdleExpression(index);
  }

  onClickCount(count: number): void {
    this.trigger.onClickCount(count);
  }
}
