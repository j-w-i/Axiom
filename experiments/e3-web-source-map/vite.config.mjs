import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import axiomSource from "./axiom-source-plugin.mjs";
export default defineConfig({ plugins: [axiomSource(), react()], server: { port: 5199, strictPort: true }, clearScreen: false });
