import { resolve } from "node:path";
import { defineConfig } from "vite";

export default defineConfig({
  root: "ui",
  clearScreen: false,
  server: { port: 5173, strictPort: true },
  build: {
    outDir: "../dist",
    emptyOutDir: true,
    target: "safari16",
    rollupOptions: {
      input: {
        popover: resolve(import.meta.dirname, "ui/popover.html"),
        settings: resolve(import.meta.dirname, "ui/settings.html"),
      },
    },
  },
});
