import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Isolated prototype. Reads real repository source (?raw) for realistic
// editor content; never imported by the product.
export default defineConfig({
  plugins: [react()],
  base: "./",
  clearScreen: false,
  server: { port: 5190, strictPort: true, fs: { allow: ["../.."] } },
  build: { outDir: "dist", emptyOutDir: true, target: "es2022", chunkSizeWarningLimit: 20000 },
  worker: { format: "es" },
});
