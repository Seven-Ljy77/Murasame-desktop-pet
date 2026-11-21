/**
 * Rule-level test for the expression state machine, with a hand-rolled clock so
 * the 10s idle tick / 3s click window / 150ms debounce can be exercised
 * instantly.
 *
 * Run: node tools/test-expression-machine.ts
 */

import {
  ExpressionMachine,
  IDLE_POOL,
  expressionForClickCount,
} from "../src/pet/expressionMachine.ts";

let failures = 0;

function check(label: string, actual: unknown, expected: unknown): void {
  const ok = JSON.stringify(actual) === JSON.stringify(expected);
  if (!ok) failures += 1;
  console.log(`${ok ? "PASS" : "FAIL"}  ${label}  actual=${JSON.stringify(actual)} expected=${JSON.stringify(expected)}`);
}

// --- fake clock -----------------------------------------------------------
let now = 0;
let nextTimerId = 1;
const timers = new Map<number, { at: number; callback: () => void }>();

const deps = {
  now: () => now,
  setTimer: (callback: () => void, delayMs: number) => {
    const id = nextTimerId++;
    timers.set(id, { at: now + delayMs, callback });
    return id;
  },
  clearTimer: (id: number) => {
    timers.delete(id);
  },
  random: () => 0, // deterministic: always the first candidate
};

function advance(ms: number): void {
  const target = now + ms;
  for (;;) {
    let dueId: number | null = null;
    let dueAt = Number.POSITIVE_INFINITY;
    for (const [id, timer] of timers) {
      if (timer.at <= target && timer.at < dueAt) {
        dueAt = timer.at;
        dueId = id;
      }
    }
    if (dueId === null) break;
    now = dueAt;
    const timer = timers.get(dueId)!;
    timers.delete(dueId);
    timer.callback();
  }
  now = target;
}

// --- harness --------------------------------------------------------------
let snapshot = { index: 1, clickCount: 0, hovering: false };
const machine = new ExpressionMachine(
  deps,
  { idleIntervalMs: 10_000, clickWindowMs: 3_000, clickDebounceMs: 150 },
  (next) => {
    snapshot = next;
  },
);

// --- click ladder ---------------------------------------------------------
check("clicks 1 -> 2", expressionForClickCount(1), 2);
check("clicks 2 -> 2", expressionForClickCount(2), 2);
check("clicks 3 -> 8", expressionForClickCount(3), 8);
check("clicks 4 -> 4", expressionForClickCount(4), 4);
check("clicks 5 -> 3", expressionForClickCount(5), 3);
check("clicks 6 -> 11", expressionForClickCount(6), 11);
check("clicks 7 -> 13", expressionForClickCount(7), 13);
check("clicks 8 -> 13 (clamped)", expressionForClickCount(8), 13);

// --- start / idle rotation ------------------------------------------------
machine.start();
check("start shows image 1", snapshot.index, 1);

advance(10_000);
check("after 10s idle tick switches", snapshot.index !== 1, true);
check("idle pick comes from the pool", (IDLE_POOL as readonly number[]).includes(snapshot.index), true);

// --- hover ----------------------------------------------------------------
machine.setHovering(true);
check("hover shows image 2", snapshot.index, 2);

machine.setHovering(false);
check("leaving hover returns to image 1", snapshot.index, 1);
advance(10_000);
check("idle countdown restarted after leaving hover", (IDLE_POOL as readonly number[]).includes(snapshot.index), true);
check("first idle tick moves off image 1", snapshot.index !== 1, true);

// --- click ladder through the machine -------------------------------------
const ladder: Array<[number, number]> = [
  [1, 2],
  [2, 2],
  [3, 8],
  [4, 4],
  [5, 3],
  [6, 11],
  [7, 13],
];
for (const [clicks, expected] of ladder) {
  const accepted = machine.click();
  check(`machine click #${clicks} accepted`, accepted, true);
  check(`machine click #${clicks} shows ${expected}`, snapshot.index, expected);
  advance(200); // past the 150ms debounce, still inside the 3s window
}
check("machine reports clickCount 7", snapshot.clickCount, 7);

// --- debounce -------------------------------------------------------------
advance(500);
machine.click(); // accepted -> count 8
check("click #8 clamps to image 13", snapshot.index, 13);
check("click #8 rejected inside debounce window", machine.click(), false);
check("rejected click did not advance count", snapshot.clickCount, 8);
advance(1000); // idle tick must not fire while a sequence is running
check("idle timer stays paused during sequence", snapshot.index, 13);

// --- reset after 3s -------------------------------------------------------
machine.setHovering(true);
advance(3_000);
check("sequence reset while hovering", snapshot.index, 2);
check("sequence reset clears count", snapshot.clickCount, 0);

machine.setHovering(false);
check("after reset, back to image 1", snapshot.index, 1);

// --- a slow click does not chain ------------------------------------------
machine.click();
check("slow click #1 shows 2", snapshot.index, 2);
advance(3_500); // > 3s: sequence reset in between
machine.click();
check("click after reset restarts at #1 -> 2", snapshot.index, 2);
check("click after reset resets count", snapshot.clickCount, 1);

// --- idle rotation keeps running when idle --------------------------------
const before = snapshot.index;
advance(10_000);
check("idle tick still rotates", snapshot.index !== before, true);

machine.dispose();
console.log(failures === 0 ? "\nALL PASSED" : `\n${failures} FAILURE(S)`);
process.exit(failures === 0 ? 0 : 1);
