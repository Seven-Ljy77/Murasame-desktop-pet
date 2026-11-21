<script setup lang="ts">
import { onBeforeUnmount, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import petIdle from "./assets/pet/idle.png";

/**
 * A press that moves further than this (or is held longer than the hold
 * timeout) is treated as a drag instead of a click. The native window drag
 * takes over from there, so the webview stops receiving pointer events.
 */
const DRAG_DISTANCE_PX = 7;
const DRAG_HOLD_MS = 220;

const dragging = ref(false);

let activePointer: number | null = null;
let originX = 0;
let originY = 0;
let holdTimer: number | undefined;

function clearHoldTimer(): void {
  if (holdTimer !== undefined) {
    window.clearTimeout(holdTimer);
    holdTimer = undefined;
  }
}

async function beginDrag(): Promise<void> {
  if (dragging.value) return;
  dragging.value = true;
  clearHoldTimer();
  try {
    await invoke("begin_drag");
  } catch (error) {
    console.error("Unable to start window drag.", error);
  } finally {
    dragging.value = false;
    activePointer = null;
  }
}

function onPointerDown(event: PointerEvent): void {
  if (event.button !== 0) return;
  event.preventDefault();
  activePointer = event.pointerId;
  originX = event.clientX;
  originY = event.clientY;
  clearHoldTimer();
  holdTimer = window.setTimeout(() => void beginDrag(), DRAG_HOLD_MS);
}

function onPointerMove(event: PointerEvent): void {
  if (activePointer !== event.pointerId || dragging.value) return;
  const distance = Math.hypot(event.clientX - originX, event.clientY - originY);
  if (distance >= DRAG_DISTANCE_PX) void beginDrag();
}

function onPointerUp(event: PointerEvent): void {
  if (activePointer !== event.pointerId) return;
  activePointer = null;
  clearHoldTimer();
}

onBeforeUnmount(() => {
  clearHoldTimer();
});
</script>

<template>
  <main
    class="pet-stage"
    :data-dragging="dragging ? 'true' : 'false'"
    @pointerdown="onPointerDown"
    @pointermove="onPointerMove"
    @pointerup="onPointerUp"
    @pointercancel="onPointerUp"
    @contextmenu.prevent
  >
    <img class="pet-art" :src="petIdle" alt="丛雨" draggable="false" />
  </main>
</template>
