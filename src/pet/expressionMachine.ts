/**
 * Pure expression state machine. No DOM, no Vue, no timers of its own: the
 * clock and timer primitives are injected so the rules can be exercised in a
 * plain Node test.
 *
 * Rules
 * -----
 * - Idle: show image 1, then every `idleIntervalMs` pick a random expression
 *   from `IDLE_POOL` (never the one already showing).
 * - Hover: image 2.
 * - Clicks: consecutive clicks (each within `clickWindowMs` of the previous one)
 *   walk up a ladder: 1-2 -> 2, 3 -> 8, 4 -> 4, 5 -> 3, 6 -> 11, 7+ -> 13.
 * - `clickWindowMs` without a click resets the counter and returns to hover (2)
 *   or to idle image 1, restarting the idle countdown.
 * - Clicks inside `clickDebounceMs` after an accepted click are dropped, so a
 *   single physical click (or a long press repeating) cannot advance the ladder.
 */

export const IDLE_POOL = [1, 5, 6, 7, 8, 9, 10, 14, 15, 16] as const;
export const START_INDEX = 1;
export const HOVER_INDEX = 2;
export const MAX_CLICK_COUNT = 7;

export interface Timings {
  idleIntervalMs: number;
  clickWindowMs: number;
  clickDebounceMs: number;
}

export interface Deps {
  now: () => number;
  setTimer: (callback: () => void, delayMs: number) => number;
  clearTimer: (id: number) => void;
  random: () => number;
}

export interface Snapshot {
  index: number;
  clickCount: number;
  hovering: boolean;
}

/** Maps a consecutive-click count onto the expression it should show. */
export function expressionForClickCount(count: number): number {
  if (count <= 2) return 2;
  switch (Math.min(count, MAX_CLICK_COUNT)) {
    case 3:
      return 8;
    case 4:
      return 4;
    case 5:
      return 3;
    case 6:
      return 11;
    default:
      return 13;
  }
}

export class ExpressionMachine {
  private index = START_INDEX;
  private clickCount = 0;
  private hovering = false;
  private lastClickAt: number | null = null;
  private debounceUntil = 0;
  private idleTimer: number | null = null;
  private resetTimer: number | null = null;
  private disposed = false;

  private readonly deps: Deps;
  private readonly timings: Timings;
  private readonly onChange: (snapshot: Snapshot) => void;

  constructor(
    deps: Deps,
    timings: Timings,
    onChange: (snapshot: Snapshot) => void,
  ) {
    this.deps = deps;
    this.timings = timings;
    this.onChange = onChange;
  }

  get snapshot(): Snapshot {
    return {
      index: this.index,
      clickCount: this.clickCount,
      hovering: this.hovering,
    };
  }

  start(): void {
    this.scheduleIdle();
    this.emit();
  }

  setHovering(hovering: boolean): void {
    if (this.disposed || hovering === this.hovering) return;
    this.hovering = hovering;

    if (hovering) {
      this.clearIdleTimer();
      // A running click sequence keeps its expression until it resets.
      if (this.clickCount === 0) this.show(HOVER_INDEX);
    } else if (this.clickCount === 0) {
      // Leaving the pet always lands back on image 1, with a fresh countdown.
      this.show(START_INDEX);
      this.scheduleIdle();
    }

    this.emit();
  }

  /** Registers a click. Returns false when the click was dropped as a repeat. */
  click(): boolean {
    if (this.disposed) return false;

    const now = this.deps.now();
    if (now < this.debounceUntil) return false;
    this.debounceUntil = now + this.timings.clickDebounceMs;

    const chained =
      this.lastClickAt !== null && now - this.lastClickAt <= this.timings.clickWindowMs;
    this.clickCount = chained ? this.clickCount + 1 : 1;
    this.lastClickAt = now;

    this.clearIdleTimer();
    this.show(expressionForClickCount(this.clickCount));
    this.scheduleClickReset();
    this.emit();
    return true;
  }

  dispose(): void {
    this.disposed = true;
    this.clearIdleTimer();
    this.clearResetTimer();
  }

  private show(index: number): void {
    this.index = index;
  }

  private emit(): void {
    this.onChange(this.snapshot);
  }

  private pickIdle(): number {
    const candidates = IDLE_POOL.filter((value) => value !== this.index);
    const pool = candidates.length > 0 ? candidates : [...IDLE_POOL];
    const pick = Math.floor(this.deps.random() * pool.length);
    return pool[Math.min(Math.max(pick, 0), pool.length - 1)];
  }

  private scheduleIdle(): void {
    this.clearIdleTimer();
    if (this.disposed || this.hovering || this.clickCount > 0) return;
    this.idleTimer = this.deps.setTimer(() => {
      this.idleTimer = null;
      if (this.hovering || this.clickCount > 0) return;
      this.show(this.pickIdle());
      this.emit();
      this.scheduleIdle();
    }, this.timings.idleIntervalMs);
  }

  private scheduleClickReset(): void {
    this.clearResetTimer();
    this.resetTimer = this.deps.setTimer(() => {
      this.resetTimer = null;
      this.clickCount = 0;
      this.lastClickAt = null;
      this.show(this.hovering ? HOVER_INDEX : START_INDEX);
      this.emit();
      this.scheduleIdle();
    }, this.timings.clickWindowMs);
  }

  private clearIdleTimer(): void {
    if (this.idleTimer !== null) {
      this.deps.clearTimer(this.idleTimer);
      this.idleTimer = null;
    }
  }

  private clearResetTimer(): void {
    if (this.resetTimer !== null) {
      this.deps.clearTimer(this.resetTimer);
      this.resetTimer = null;
    }
  }
}
