/**
 * Node test for the voice trigger rules (no browser needed):
 *
 *   node --test tools/test-voice-trigger.ts
 */

import assert from "node:assert/strict";
import test from "node:test";

import { CLICK_POOL, clipForClick, clipForIdle, pickRandomClip } from "../src/pet/voiceRules.ts";
import { VoiceTrigger, type ClipSink } from "../src/pet/voiceTrigger.ts";

class FakeSink implements ClipSink {
  played: number[] = [];
  playing = false;

  busy(): boolean {
    return this.playing;
  }

  play(clip: number): void {
    this.played.push(clip);
    this.playing = true;
  }

  finish(): void {
    this.playing = false;
  }
}

test("idle rotation speaks only for 15 and 10", () => {
  assert.equal(clipForIdle(15), 7);
  assert.equal(clipForIdle(10), 8);
  assert.equal(clipForIdle(1), null);
  assert.equal(clipForIdle(13), null);
});

test("click ladder maps the counts the owner asked for", () => {
  assert.deepEqual(clipForClick(3), { kind: "fixed", clip: 4 });
  assert.deepEqual(clipForClick(6), { kind: "fixed", clip: 9 });
  assert.deepEqual(clipForClick(1), { kind: "random" });
  assert.deepEqual(clipForClick(2), { kind: "random" });
  assert.equal(clipForClick(4), null);
  assert.equal(clipForClick(5), null);
});

test("every click from 7 on keeps playing clip 10", () => {
  for (const count of [7, 8, 9, 20]) {
    assert.deepEqual(clipForClick(count), { kind: "fixed", clip: 10 });
  }
});

test("clip 10 replays on later clicks once the previous one finished", () => {
  const sink = new FakeSink();
  const trigger = new VoiceTrigger(sink, () => 0);

  for (const count of [7, 8, 9]) {
    trigger.onClickCount(count);
    sink.finish();
  }
  assert.deepEqual(sink.played, [10, 10, 10]);
});

test("random pick stays inside the pool and never repeats", () => {
  for (const value of [0, 0.24, 0.5, 0.75, 0.99]) {
    const pick = pickRandomClip(null, () => value);
    assert.ok(CLICK_POOL.includes(pick));
  }
  for (const previous of CLICK_POOL) {
    for (const value of [0, 0.5, 0.99]) {
      assert.notEqual(pickRandomClip(previous, () => value), previous);
    }
  }
});

test("a trigger is dropped while a clip is still playing", () => {
  const sink = new FakeSink();
  const trigger = new VoiceTrigger(sink, () => 0);

  trigger.onClickCount(1);
  assert.deepEqual(sink.played, [CLICK_POOL[0]]);

  trigger.onClickCount(3);
  trigger.onIdleExpression(15);
  assert.deepEqual(sink.played, [CLICK_POOL[0]], "busy sink must swallow new triggers");

  sink.finish();
  trigger.onClickCount(3);
  assert.deepEqual(sink.played, [CLICK_POOL[0], 4]);
});

test("consecutive random picks never repeat a line", () => {
  const sink = new FakeSink();
  const trigger = new VoiceTrigger(sink, () => 0);

  for (let i = 0; i < 8; i += 1) {
    trigger.onClickCount(1);
    sink.finish();
  }
  assert.equal(sink.played.length, 8);
  for (let i = 1; i < sink.played.length; i += 1) {
    assert.notEqual(sink.played[i], sink.played[i - 1]);
  }
});

test("idle clips follow the same busy rule and never queue up", () => {
  const sink = new FakeSink();
  const trigger = new VoiceTrigger(sink, () => 0);

  trigger.onIdleExpression(10);
  trigger.onIdleExpression(15);
  assert.deepEqual(sink.played, [8], "second idle line is skipped while the first plays");

  sink.finish();
  trigger.onIdleExpression(15);
  assert.deepEqual(sink.played, [8, 7]);
});
