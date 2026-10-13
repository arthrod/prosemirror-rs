# ProseMirror desk

A browser writing desk for [prosemirror-rs](https://github.com/arthrod/prosemirror-rs).
The editor is the JavaScript ProseMirror model that crate implements. The library,
review pane, and sidecar export follow [markdown-remarkable](https://github.com/arthrod/markdown-remarkable).

```bash
npm install
npm run dev
```

Open the printed local address. The sample library is already loaded.

- **Write** — marks, lists, tasks, tables, quotes, code, images, undo, find.
- **Review** — comments on a block, list item, table row, or the whole file. Each note is anchored by a SHA-256 of the trimmed passage (first 16 hex characters), the same idea as a remarkable sidecar. Change the wording and the note becomes unanchored instead of disappearing.
- **Export** — Markdown, HTML, a review summary (`notes.md` → `notes.review.md`, `notes.txt` → `notes.txt.review.md`), and `*.review.json`.
- **Pair** — a second editor receives steps after 200ms.

Documents stay in this browser (`localStorage`). Remote images stay blocked until you allow them from the File menu.

```bash
npm test
npm run build
```
