/** Voice trigger rules, kept free of DOM and asset imports so they stay testable. */

/** Idle rotation landing on these expressions speaks. */
export const IDLE_CLIPS: Record<number, number> = {
  15: 7,
  10: 8,
};

/** The first two clicks of a streak pick one of these at random. */
export const CLICK_POOL = [1, 2, 3, 5];

/** Later clicks of the streak are fixed lines. */
export const CLICK_CLIPS: Record<number, number> = {
  3: 4,
  6: 9,
};

/** From this many consecutive clicks on, the streak keeps repeating one line. */
export const STREAK_CLIP_FROM = 7;
export const STREAK_CLIP = 10;

export type ClickTrigger = { kind: "fixed"; clip: number } | { kind: "random" } | null;

export function clipForClick(count: number): ClickTrigger {
  if (count >= STREAK_CLIP_FROM) return { kind: "fixed", clip: STREAK_CLIP };
  const fixed = CLICK_CLIPS[count];
  if (fixed !== undefined) return { kind: "fixed", clip: fixed };
  if (count <= 2) return { kind: "random" };
  return null;
}

export function clipForIdle(index: number): number | null {
  const clip = IDLE_CLIPS[index];
  return clip === undefined ? null : clip;
}

/** Never returns `previous`, so the same line cannot fire twice in a row. */
export function pickRandomClip(previous: number | null, random: () => number = Math.random): number {
  const pool = CLICK_POOL.filter((clip) => clip !== previous);
  const source = pool.length > 0 ? pool : CLICK_POOL;
  const index = Math.min(source.length - 1, Math.floor(random() * source.length));
  return source[index];
}
