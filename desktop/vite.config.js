import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";

const root = dirname(fileURLToPath(import.meta.url));
const wasmEntry = resolve(root, "../node/npm/wasm/index.js");

export default defineConfig({
  root,
  base: "./",
  server: {
    host: "127.0.0.1",
    port: 5173,
    strictPort: true,
    fs: { allow: [resolve(root, "..")] },
  },
  resolve: {
    alias: {
      "prosemirror-model": wasmEntry,
      "prosemirror-transform": wasmEntry,
    },
  },
  build: {
    outDir: "dist",
    emptyOutDir: true,
  },
});
