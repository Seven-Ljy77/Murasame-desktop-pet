import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import { fileURLToPath } from "node:url";

const entry = (path: string) => fileURLToPath(new URL(path, import.meta.url));

// Tauri expects a fixed dev-server port and must not clear the terminal output.
// Two HTML entries: the pet overlay and the settings dialog.
export default defineConfig({
  plugins: [vue()],
  clearScreen: false,
  build: {
    rollupOptions: {
      input: {
        main: entry("./index.html"),
        settings: entry("./settings.html"),
      },
    },
  },
  server: {
    host: "127.0.0.1",
    port: 1420,
    strictPort: true,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
});
