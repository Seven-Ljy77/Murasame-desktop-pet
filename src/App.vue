<script setup lang="ts">
import { onBeforeUnmount, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { EXPRESSION_SOURCES, preloadExpressions } from "./pet/expressions";
import { useExpression } from "./pet/useExpression";
import { VoicePlayer } from "./pet/voice";

/**
 * A press that moves further than this (or is held longer than the hold
 * timeout) becomes a native window drag, so it must not also count as a click.
 */
const DRAG_DISTANCE_PX = 7;
const DRAG_HOLD_MS = 220;

const dragging = ref(false);
const hovering = ref(false);
const popping = ref(false);
const { index, clickCount, setHovering, click } = useExpression();

const voice = new VoicePlayer();

// A click hands the streak over to the voice rules; a silent change while idle
// can only come from the 10s rotation.
watch(clickCount, (count, previous) => {
  if (count > previous) voice.onClickCount(count);
});

watch(index, (current) => {
  if (clickCount.value === 0 && !hovering.value) voice.onIdleExpression(current);
});

void invoke<{ volume: number; muted: boolean }>("get_audio_settings")
  .then((settings) => {
    voice.setVolume(settings.volume);
    voice.setMuted(settings.muted);
  })
  .catch((error) => console.error("Unable to read audio settings.", error));

const stopVoiceListener = listen<{ volume: number; muted: boolean }>(
  "voice-settings",
  (event) => {
    voice.setVolume(event.payload.volume);
    voice.setMuted(event.payload.muted);
  },
);

preloadExpressions();

let activePointer: number | null = null;
let originX = 0;
let originY = 0;
let holdTimer: number | undefined;
let popTimer: number | undefined;
let pressBecameDrag = false;

function pop(): void {
  popping.value = true;
  window.clearTimeout(popTimer);
  popTimer = window.setTimeout(() => {
    popping.value = false;
  }, 140);
}

function enterHover(): void {
  hovering.value = true;
  setHovering(true);
}

function leaveHover(): void {
  hovering.value = false;
  setHovering(false);
}

function clearHoldTimer(): void {
  if (holdTimer !== undefined) {
    window.clearTimeout(holdTimer);
    holdTimer = undefined;
  }
}

async function beginDrag(): Promise<void> {
  if (dragging.value) return;
  dragging.value = true;
  pressBecameDrag = true;
  clearHoldTimer();
  try {
    // Resolves once the native drag loop ends (mouse released).
    await invoke("begin_drag");
  } catch (error) {
    console.error("Unable to start window drag.", error);
  } finally {
    dragging.value = false;
    activePointer = null;
    // Throttled saves during the drag can lag behind the final position.
    void invoke("save_pet_position").catch((error) => {
      console.error("Unable to persist the pet position.", error);
    });
  }
}

function onPointerDown(event: PointerEvent): void {
  if (event.button !== 0) return;
  event.preventDefault();
  activePointer = event.pointerId;
  originX = event.clientX;
  originY = event.clientY;
  pressBecameDrag = false;
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
  if (!pressBecameDrag) {
    pop();
    click();
  }
  pressBecameDrag = false;
}

function onPointerCancel(): void {
  activePointer = null;
  clearHoldTimer();
  pressBecameDrag = false;
}

/** Right-click hides the pet; the tray icon brings it back. */
function onContextMenu(): void {
  clearHoldTimer();
  activePointer = null;
  pressBecameDrag = false;
  hovering.value = false;
  setHovering(false);
  voice.pause();
  void invoke("hide_pet").catch((error) => {
    console.error("Unable to hide the pet.", error);
  });
}

onBeforeUnmount(() => {
  clearHoldTimer();
  window.clearTimeout(popTimer);
  void stopVoiceListener.then((stop) => stop());
});
</script>

<template>
  <main
    class="pet-stage"
    :data-dragging="dragging ? 'true' : 'false'"
    :data-hovering="hovering ? 'true' : 'false'"
    :data-popping="popping ? 'true' : 'false'"
    @pointerdown="onPointerDown"
    @pointermove="onPointerMove"
    @pointerup="onPointerUp"
    @pointercancel="onPointerCancel"
    @pointerenter="enterHover"
    @pointerleave="leaveHover"
    @contextmenu.prevent="onContextMenu"
  >
    <div class="pet-breathe">
      <img
        class="pet-art"
        :src="EXPRESSION_SOURCES[index]"
        alt="丛雨"
        draggable="false"
      />
    </div>
  </main>
</template>
