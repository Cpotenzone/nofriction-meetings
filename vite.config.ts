import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [react()],

  // Isolate the Lessons surface into its own chunk so the authoritative
  // offline guard (scripts/check-lessons-offline.mjs, owner-decision D5) can
  // scan ONLY the Lessons bytes for remote references rather than the whole
  // app's URLs. The surface is src/components/lessons/** + the LessonsView and
  // its preview-only logic mirror.
  build: {
    rollupOptions: {
      output: {
        manualChunks(id: string) {
          if (
            id.includes("/components/lessons/") ||
            id.includes("/views/LessonsView") ||
            id.includes("/lib/lessonLogic") ||
            id.includes("/lib/lessons.ts")
          ) {
            return "lessons";
          }
          return undefined;
        },
      },
    },
  },

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
