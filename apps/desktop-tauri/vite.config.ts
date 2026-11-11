import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import { fileURLToPath } from "node:url";

// The page is served by Tauri from its own asset protocol; every asset is
// bundled (no CDN, no network). The editor surface's framework-neutral logic
// is imported from ../../editor/src through the `@editor` alias so both
// shells share one implementation of the bridge-1 document rules.
export default defineConfig({
  plugins: [react()],
  base: "./",
  clearScreen: false,
  resolve: {
    alias: { "@editor": fileURLToPath(new URL("../../editor/src", import.meta.url)) },
    // One Monaco instance: files under ../../editor/src must resolve to ours.
    dedupe: ["monaco-editor"],
  },
  server: { port: 5174, strictPort: true, fs: { allow: ["../.."] } },
  build: {
    outDir: "dist",
    emptyOutDir: true,
    target: "es2022",
    chunkSizeWarningLimit: 20000,
  },
  worker: { format: "es" },
  test: {
    include: ["tests/**/*.test.ts"],
    environment: "node",
  },
});
