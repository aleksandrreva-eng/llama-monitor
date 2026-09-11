import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// https://vitejs.dev/config/
export default defineConfig({
  plugins: [svelte()],
  // Tauri serves the frontend from a custom scheme (tauri://localhost), so
  // assets MUST be referenced relatively. Vite's default `base: "/"` emits
  // absolute `/assets/...` URLs, which never resolve inside the webview and
  // produce a blank/black window in the packaged app.
  base: "./",
  // Tauri expects a clean build output.
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: "127.0.0.1",
  },
  build: {
    target: "es2021",
    minify: process.env.TAURI_ENV_DEBUG ? false : "esbuild",
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
  },
});
