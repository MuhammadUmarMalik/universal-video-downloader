import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import path from "node:path";

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },
  // Electron loads the packaged renderer from file://; keep assets relative to index.html.
  base: "./",
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
});
