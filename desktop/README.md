# prosemirror-rs desktop

A [Tauri 2.12](https://v2.tauri.app/) executable for the basic and advanced demos.
The window keeps both editors:

- **Basic** — formatting, lists, and history (`example-setup`).
- **Advanced** — two editors, tables, and steps replayed after 200ms.

`prosemirror-model` and `prosemirror-transform` are aliased to the Rust
WebAssembly build. The Tauri process also links the `prosemirror` crate and
runs the same step checks natively (replace, marks, attributes, tables, undo,
collaborative replay).

`desktop/src-tauri` is excluded from the Cargo workspace so `cargo test --all`
does not need WebKit. The native checks live in `desktop/core`, which is a
workspace member.

```bash
cd desktop
npm install
npm run build:wasm
npm run desktop
```

Release bundles (deb, AppImage, rpm):

```bash
npm run desktop:build
```

Requires Rust 1.90+, wasm-pack, and the Tauri system dependencies for your OS.
