import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import { fileURLToPath } from "node:url";

// The page is served by Tauri from its own asset protocol; every asset is
// bundled (no CDN, no network). The editor surface's framework-neutral logic
// is imported from ../../editor/src through the `@editor` alias so both
// shells share one implementation of the bridge-1 document rules.
// `--mode gallery` (pnpm run gallery / build:gallery) serves or builds the dev-only
// component gallery (gallery.html); every other mode builds index.html only, so the
// gallery never reaches dist (tests/gallery-excluded.test.ts).
export default defineConfig(({ mode }) => ({
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
    rollupOptions: mode === "gallery" ? { input: { gallery: fileURLToPath(new URL("gallery.html", import.meta.url)) } } : undefined,
  },
  worker: { format: "es" },
  test: {
    include: ["tests/**/*.test.ts", "tests/**/*.test.tsx"],
    environment: "node",
  },
}));
