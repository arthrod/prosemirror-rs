import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { hashSource } from "./hash.ts";
import { exportPath, exportReview, unanchored } from "./export.ts";
import { emptyReview, type Anchor, type ReviewDoc } from "./types.ts";
import { parseMarkdown, serializeMarkdown, splitFrontmatter } from "../editor/markdown.ts";
import { anchorsFor } from "../editor/anchors.ts";
import { findNext } from "../editor/commands.ts";
import { sanitizeHref } from "../editor/schema.ts";
import { TextSelection } from "prosemirror-state";

describe("review hashes", () => {
  it("matches SHA-256 truncated to 16 hex chars", () => {
    const expected = createHash("sha256").update("Hello").digest("hex").slice(0, 16);
    assert.equal(hashSource("Hello"), expected);
    assert.equal(hashSource("Hello").length, 16);
    assert.notEqual(hashSource("Hello"), hashSource("Hello!"));
  });
});

describe("markdown round trip", () => {
  it("keeps headings, marks, tasks, tables, and quotes", () => {
    const source = [
      "# Title",
      "",
      "A **bold** and *em* and ~~gone~~ word.",
      "",
      "- [ ] todo",
      "- [x] done",
      "",
      "| a | b |",
      "| --- | --- |",
      "| 1 | 2 |",
      "",
      "> quoted",
      "",
      "```ts",
      "const x = 1",
      "```",
    ].join("\n");
    const out = serializeMarkdown(parseMarkdown(source));
    assert.match(out, /^# Title/m);
    assert.match(out, /\*\*bold\*\*/);
    assert.match(out, /~~gone~~/);
    assert.match(out, /- \[ \] todo/);
    assert.match(out, /- \[x\] done/);
    assert.match(out, /\| a \| b \|/);
    assert.match(out, /\| 1 \| 2 \|/);
    assert.match(out, /> quoted/);
    assert.match(out, /```ts/);
  });

  it("neutralizes javascript links and strips a leading frontmatter block", () => {
    const doc = parseMarkdown("See [x](javascript:alert(1)).");
    const serialized = serializeMarkdown(doc);
    assert.equal(JSON.stringify(doc.toJSON()).includes('"type":"link"'), false);
    assert.match(serialized, /\\\[x\\\]\(javascript:alert\(1\)\)/);
    assert.doesNotMatch(serialized, /(?<!\\)\]\(javascript:/);
    assert.equal(sanitizeHref("javascript:alert(1)"), "#");
    assert.equal(sanitizeHref("java\tscript:alert(1)"), "#");
    assert.equal(sanitizeHref("java\nscript:alert(1)"), "#");
    assert.equal(sanitizeHref("https://example.com/a b"), "https://example.com/a b");
    const split = splitFrontmatter("---\ntitle: Hi\n---\n\n# Body\n");
    assert.match(split.frontmatter, /^---/);
    assert.match(split.body, /^# Body/);
    const dotted = splitFrontmatter("---\ntitle: Hi\n...\n\n# Body\n");
    assert.match(dotted.frontmatter, /^\s*---[\s\S]*\.\.\.\s*$/);
    assert.match(dotted.body, /^# Body/);
  });

  it("keeps ordered-list starts, task markers, table marks, and fenced comments", () => {
    const ordered = serializeMarkdown(parseMarkdown("0. zero\n1. one\n"));
    assert.match(ordered, /^0\. zero/m);
    assert.match(ordered, /^1\. one/m);
    const tasks = serializeMarkdown(parseMarkdown("1. [x] done\n2. [ ] todo\n"));
    assert.match(tasks, /1\. \[x\] done/);
    assert.match(tasks, /2\. \[ \] todo/);
    const table = serializeMarkdown(parseMarkdown("| **a** | b |\n| --- | --- |\n| *c* | `d` |\n"));
    assert.match(table, /\*\*a\*\*/);
    assert.match(table, /\*c\*/);
    assert.match(table, /`d`/);
    const fenced = "```\n<!-- keep -->\n```";
    assert.match(serializeMarkdown(parseMarkdown(fenced)), /<!-- keep -->/);
  });

  it("searches across adjacent text nodes and selects the match", () => {
    const doc = parseMarkdown("hello **world** there");
    const hit = findNext(doc, "hello world", 0);
    assert.ok(hit);
    assert.equal(doc.textBetween(hit.from, hit.to), "hello world");
    const sel = TextSelection.between(doc.resolve(hit.from), doc.resolve(hit.to), 1);
    assert.equal(sel.from, hit.from);
    assert.equal(sel.to, hit.to);
    const wrapped = findNext(doc, "hello", hit.to);
    assert.equal(wrapped && doc.textBetween(wrapped.from, wrapped.to), "hello");
  });

  it("gives identical blocks distinct review hashes", () => {
    const doc = parseMarkdown("Same\n\nSame\n");
    const blocks = anchorsFor(doc, "markdown").filter((anchor) => anchor.kind === "block");
    assert.equal(blocks.length, 2);
    assert.notEqual(blocks[0].hash, blocks[1].hash);
    assert.equal(blocks[0].hash, hashSource(blocks[0].source));
  });

  it("numbers ordered-list items on their own lines", () => {
    const doc = parseMarkdown("1. one\n2. two\n");
    const anchors = anchorsFor(doc, "markdown");
    const items = anchors.filter((anchor) => anchor.kind === "item");
    assert.equal(items.length, 2);
    assert.equal(items[0].lineStart, 1);
    assert.equal(items[1].lineStart, 2);
    assert.equal(items[1].excerpt, "two");
  });
});

describe("review export", () => {
  it("matches the empty remarkable header", () => {
    const out = exportReview("notes.md", [], emptyReview("notes.md"), "2026-08-22T07:10:00.000Z");
    assert.equal(out, "# Review: notes.md\n\nExported: 2026-08-22T07:10:00.000Z · 0 comments on 0 blocks\n");
  });

  it("quotes anchored comments and parks missing ones", () => {
    const anchors: Anchor[] = [
      {
        kind: "block",
        hash: "abc",
        source: "## Design",
        excerpt: "## Design",
        lineStart: 1,
        lineEnd: 1,
        parent: null,
        from: 0,
        to: 4,
      },
      {
        kind: "item",
        hash: "def",
        source: "- nested",
        excerpt: "nested",
        lineStart: 3,
        lineEnd: 3,
        parent: 0,
        from: 5,
        to: 8,
      },
    ];
    const doc: ReviewDoc = {
      version: 1,
      file: "notes.md",
      file_comments: [
        { id: "c_aaaaaaaaaaaaaaaa", text: "File note\ncontinued", created: "", updated: "" },
      ],
      blocks: [
        {
          hash: "def",
          excerpt: "nested",
          kind: "item",
          comments: [{ id: "c_bbbbbbbbbbbbbbbb", text: "Look here", created: "", updated: "" }],
        },
        {
          hash: "missing",
          excerpt: "Old paragraph",
          kind: "block",
          comments: [{ id: "c_cccccccccccccccc", text: "Gone", created: "", updated: "" }],
        },
      ],
    };
    const out = exportReview("notes.md", anchors, doc, "2026-08-22T07:10:00.000Z");
    assert.match(out, /> \(file\): notes.md/);
    assert.match(out, /- File note\n {2}continued/);
    assert.match(out, /> L3: nested \(in block L1\)/);
    assert.match(out, /## Unanchored/);
    assert.match(out, /> \(not found\): Old paragraph/);
    assert.deepEqual(unanchored(doc, anchors), ["missing"]);
    assert.equal(exportPath("notes.md", "markdown"), "notes.review.md");
    assert.equal(exportPath("notes.txt", "plain"), "notes.txt.review.md");
  });
});
