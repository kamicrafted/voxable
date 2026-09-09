import { defineConfig } from "vite";
import { fileURLToPath } from "url";

const entry = (p) => fileURLToPath(new URL(p, import.meta.url));

export default defineConfig({
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  envPrefix: ["VITE_", "TAURI_ENV_"],
  build: {
    target: "esnext",
    minify: !process.env.TAURI_ENV_DEBUG ? "esbuild" : false,
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
    outDir: "dist",
    // Multi-page: the Hub (index.html) and the Flow Bar (flowbar.html).
    rollupOptions: {
      input: {
        main: entry("./index.html"),
        flowbar: entry("./flowbar.html"),
      },
    },
  },
});
