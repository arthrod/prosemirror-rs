import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";

const root = dirname(fileURLToPath(import.meta.url));
const engine = resolve(root, "src/engine.js");

// The npm glue is CommonJS. A named ESM import of those files fails unless
// the trailing module.exports is turned into an export list.
function npmCjs() {
  const replacements = {
    "/node/npm/patch.js": "export { patchStatics, setRawSpec, getRawSpec, schemaSpecs };",
    "/node/npm/to-dom.js": "export { createDOMSerializer };",
    "/node/npm/from-dom.js": "export { createDOMParser };",
  };
  return {
    name: "prosemirror-npm-cjs",
    transform(code, id) {
      const path = id.split("?", 1)[0];
      const key = Object.keys(replacements).find((suffix) => path.endsWith(suffix));
      if (!key || !code.includes("module.exports")) return null;
      return code.replace(/module\.exports\s*=\s*\{[^}]*\};?\s*$/, replacements[key]);
    },
  };
}

export default defineConfig({
  plugins: [npmCjs()],
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
      "prosemirror-model": engine,
      "prosemirror-transform": engine,
    },
  },
  optimizeDeps: {
    exclude: [
      "prosemirror-model",
      "prosemirror-transform",
      "prosemirror-state",
      "prosemirror-view",
      "prosemirror-commands",
      "prosemirror-keymap",
      "prosemirror-history",
      "prosemirror-schema-basic",
      "prosemirror-schema-list",
      "prosemirror-example-setup",
      "prosemirror-menu",
      "prosemirror-tables",
      "prosemirror-gapcursor",
      "prosemirror-dropcursor",
    ],
  },
  build: {
    outDir: "dist",
    emptyOutDir: true,
    target: "esnext",
  },
});
