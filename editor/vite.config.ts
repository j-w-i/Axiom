import { defineConfig } from "vitest/config";

// The page is served from axiom://editor/, so every asset URL must be
// relative. Monaco is bundled in full; nothing is fetched from the network.
export default defineConfig({
  base: "./",
  build: {
    outDir: "dist",
    emptyOutDir: true,
    chunkSizeWarningLimit: 20000,
  },
  worker: { format: "es" },
  test: {
    include: ["tests/**/*.test.ts"],
    environment: "node",
  },
});
