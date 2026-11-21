import { onBeforeUnmount, ref } from "vue";
import { CLICK_DEBOUNCE_MS, CLICK_WINDOW_MS, IDLE_INTERVAL_MS } from "./config";
import { ExpressionMachine, type Snapshot } from "./expressionMachine";

/** Binds the pure expression machine to Vue reactivity and browser timers. */
export function useExpression() {
  const index = ref(1);
  const clickCount = ref(0);

  const machine = new ExpressionMachine(
    {
      now: () => performance.now(),
      setTimer: (callback, delayMs) => window.setTimeout(callback, delayMs),
      clearTimer: (id) => window.clearTimeout(id),
      random: Math.random,
    },
    {
      idleIntervalMs: IDLE_INTERVAL_MS,
      clickWindowMs: CLICK_WINDOW_MS,
      clickDebounceMs: CLICK_DEBOUNCE_MS,
    },
    (snapshot: Snapshot) => {
      index.value = snapshot.index;
      clickCount.value = snapshot.clickCount;
    },
  );

  machine.start();
  onBeforeUnmount(() => machine.dispose());

  return {
    index,
    clickCount,
    setHovering: (hovering: boolean) => machine.setHovering(hovering),
    click: () => machine.click(),
  };
}
