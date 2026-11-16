import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// https://vite.dev/config/
export default defineConfig({
  plugins: [react(), tailwindcss()],

  // Keep Rust compiler output visible when running under `tauri dev`.
  clearScreen: false,
  server: {
    // Tauri expects a fixed port and fails if it is taken.
    port: 1420,
    strictPort: true,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
  build: {
    target: "esnext",
  },
  test: {
    environment: "node",
    include: ["src/**/*.test.ts"],
  },
});
