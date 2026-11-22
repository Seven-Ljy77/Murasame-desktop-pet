<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";

const percent = ref(100);
let applying = false;
let pending: number | null = null;

interface AutostartState {
  enabled: boolean;
  available: boolean;
}

const autostart = ref(false);
const autostartAvailable = ref(true);
const autostartBusy = ref(false);
const autostartError = ref("");

const autostartNote = computed(() => {
  if (autostartError.value) return autostartError.value;
  if (!autostartAvailable.value) return "开发模式下不写入，安装版才生效";
  return autostart.value ? "登录 Windows 后自动出现" : "登录 Windows 后不自动启动";
});

/** Serialises slider input so a fast drag cannot interleave window resizes. */
async function pushPercent(value: number): Promise<void> {
  if (applying) {
    pending = value;
    return;
  }
  applying = true;
  try {
    const scale = await invoke<number>("set_pet_scale", { scale: value / 100 });
    percent.value = Math.round(scale * 100);
  } catch (error) {
    console.error("Unable to apply pet scale.", error);
  } finally {
    applying = false;
    if (pending !== null) {
      const next = pending;
      pending = null;
      void pushPercent(next);
    }
  }
}

function onInput(event: Event): void {
  const value = Number((event.target as HTMLInputElement).value);
  percent.value = value;
  void pushPercent(value);
}

onMounted(async () => {
  try {
    const scale = await invoke<number>("get_pet_scale");
    percent.value = Math.round(scale * 100);
  } catch (error) {
    console.error("Unable to read pet scale.", error);
  }

  try {
    const state = await invoke<AutostartState>("get_autostart");
    autostart.value = state.enabled;
    autostartAvailable.value = state.available;
  } catch (error) {
    autostartError.value = "读取开机自启状态失败";
    console.error("Unable to read autostart state.", error);
  }
});

async function onAutostartChange(event: Event): Promise<void> {  const wanted = (event.target as HTMLInputElement).checked;
  autostartBusy.value = true;
  autostartError.value = "";
  try {
    const state = await invoke<AutostartState>("set_autostart", { enabled: wanted });
    autostart.value = state.enabled;
    autostartAvailable.value = state.available;
  } catch (error) {
    autostart.value = !wanted;
    autostartError.value = typeof error === "string" ? error : "设置开机自启失败";
    console.error("Unable to change autostart.", error);
  } finally {
    autostartBusy.value = false;
  }
}
</script>

<template>
  <main class="settings">
    <h1>丛雨桌宠</h1>

    <section class="card">
      <div class="heading">
        <h2>大小</h2>
        <output class="value">{{ percent }}%</output>
      </div>
      <input
        class="slider"
        type="range"
        min="60"
        max="160"
        step="5"
        :value="percent"
        aria-label="桌宠大小"
        @input="onInput"
      />
      <div class="labels"><span>60%</span><span>160%</span></div>
    </section>

    <section class="card">
      <label class="toggle">
        <input
          type="checkbox"
          :checked="autostart"
          :disabled="!autostartAvailable || autostartBusy"
          @change="onAutostartChange"
        />
        <span class="toggle-text">
          <span class="toggle-title">开机自动启动</span>
          <span class="toggle-note" :class="{ warning: autostartError }">
            {{ autostartNote }}
          </span>
        </span>
      </label>
    </section>

    <p class="hint">拖动滑块即可缩放，右下角保持不动；关闭本窗口只是隐藏，可从托盘菜单再次打开。</p>
  </main>
</template>
