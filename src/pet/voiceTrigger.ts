import { CLICK_POOL, clipForClick, clipForIdle, pickRandomClip } from "./voiceRules.ts";

/**
 * Decides *whether* a voice line should play. The sink owns the actual audio
 * element, so the rules can be exercised without a browser.
 */
export interface ClipSink {
  /** True while a clip is still playing: new triggers are dropped. */
  busy(): boolean;
  play(clip: number): void;
}

export class VoiceTrigger {
  private lastRandom: number | null = null;
  private readonly sink: ClipSink;
  private readonly random: () => number;

  constructor(sink: ClipSink, random: () => number = Math.random) {
    this.sink = sink;
    this.random = random;
  }

  /** The 10s rotation moved to a new expression. */
  onIdleExpression(index: number): void {
    const clip = clipForIdle(index);
    if (clip !== null) this.trigger(clip);
  }

  /** A click brought the streak to `count`. */
  onClickCount(count: number): void {
    const trigger = clipForClick(count);
    if (trigger === null) return;
    if (trigger.kind === "fixed") {
      this.trigger(trigger.clip);
      return;
    }
    this.trigger(pickRandomClip(this.lastRandom, this.random));
  }

  private trigger(clip: number): void {
    if (this.sink.busy()) return;
    if (CLICK_POOL.includes(clip)) this.lastRandom = clip;
    this.sink.play(clip);
  }
}
