import { anchorsFor } from "./editor/anchors.ts";
import { docFromBody } from "./editor/markdown.ts";
import { emptyReview, type Comment, type ReviewBlock, type ReviewDoc } from "./review/types.ts";
import type { LibraryFile } from "./store-types.ts";

const NOW = "2026-04-02T15:00:00.000Z";

function note(id: string, text: string): Comment {
  return { id, text, created: NOW, updated: NOW };
}

function reviewFor(
  fileName: string,
  kind: "markdown" | "plain",
  body: string,
  picks: { match: string; kind?: ReviewBlock["kind"]; text: string; id: string }[],
  fileComments: Comment[] = [],
): ReviewDoc {
  const anchors = anchorsFor(docFromBody(kind, body), kind);
  const blocks: ReviewBlock[] = [];
  for (const pick of picks) {
    const anchor = anchors.find(
      (item) =>
        (pick.kind == null || item.kind === pick.kind) &&
        (item.excerpt.includes(pick.match) || item.source.includes(pick.match)),
    );
    if (!anchor) continue;
    blocks.push({
      hash: anchor.hash,
      excerpt: anchor.excerpt,
      kind: anchor.kind,
      comments: [note(pick.id, pick.text)],
    });
  }
  return { version: 1, file: fileName, blocks, file_comments: fileComments };
}

const welcome = `---
title: Welcome
---

# A writing desk for structured text

This is an editable ProseMirror document. The model, steps, and schema are the same ones [prosemirror-rs](https://github.com/arthrod/prosemirror-rs) implements in Rust — here they run in the browser so you can write, review, and export without a native build.

Select any block and leave a comment in the review pane. Comments live beside the file, matched by a hash of the block, the way markdown-remarkable keeps a sidecar.

## What you can edit

- **Marks** — bold, italic, strike, \`code\`, and links
- Lists, including tasks
- Tables, quotes, rules, and fenced code
- Local images. Remote images stay blocked until you allow them

- [ ] Try a task
- [x] Open the review on the sample note

| Piece | Where it lives |
| --- | --- |
| Document | ProseMirror document |
| Review | \`notes.review.json\` sidecar |
| Export | Markdown, HTML, or a review summary |

> Comments follow the block if the wording stays the same, and become unanchored when it changes.

\`\`\`ts
type Step = { stepType: string; from: number; to: number }
\`\`\`
`;

const design = `## Design notes

The editor serializes back to Markdown, so the file you export is the file you can open again.

- Keep the schema small enough to round-trip
- Hash anchors on the trimmed Markdown of each block
- Nested list items and table rows can be reviewed on their own

| Decision | Status |
| --- | --- |
| Sidecar stays out of the Markdown | Kept |
| Pair editing copies steps across a short delay | Kept |

A changed paragraph drops its comment into Unanchored instead of deleting it.
`;

const scratch = `Plain text is not Markdown.

Each non-blank line can take a comment.
Line numbers in the export match the text file.
`;

export function sampleFiles(): LibraryFile[] {
  const welcomeBody = welcome.replace(/^---\ntitle: Welcome\n---\n\n/, "");
  return [
    {
      id: "folder-notes",
      parentId: null,
      name: "notes",
      kind: "folder",
      body: "",
      frontmatter: "",
      review: emptyReview("notes"),
      created: NOW,
      updated: NOW,
    },
    {
      id: "file-welcome",
      parentId: null,
      name: "Welcome.md",
      kind: "markdown",
      body: welcomeBody,
      frontmatter: "---\ntitle: Welcome\n---\n",
      review: reviewFor("Welcome.md", "markdown", welcomeBody, [
        {
          match: "A writing desk",
          text: "Lead with the job of the desk, not the library name.",
          id: "c_a11ce00000000001",
        },
      ]),
      created: NOW,
      updated: NOW,
    },
    {
      id: "file-design",
      parentId: "folder-notes",
      name: "Design notes.md",
      kind: "markdown",
      body: design,
      frontmatter: "",
      review: reviewFor(
        "Design notes.md",
        "markdown",
        design,
        [
          {
            match: "Hash anchors",
            kind: "item",
            text: "Say which bytes are hashed — trimmed source, SHA-256, first 16 hex.",
            id: "c_a11ce00000000002",
          },
          {
            match: "Sidecar stays",
            kind: "row",
            text: "Call out that the Markdown file itself is never rewritten by a comment.",
            id: "c_a11ce00000000003",
          },
        ],
        [note("c_a11ce00000000004", "File-wide: this note is the one to hand to a reviewer.")],
      ),
      created: NOW,
      updated: NOW,
    },
    {
      id: "file-scratch",
      parentId: "folder-notes",
      name: "scratch.txt",
      kind: "plain",
      body: scratch.trim(),
      frontmatter: "",
      review: reviewFor("scratch.txt", "plain", scratch.trim(), [
        {
          match: "Each non-blank line",
          text: "Line comments should survive a reload.",
          id: "c_a11ce00000000005",
        },
      ]),
      created: NOW,
      updated: NOW,
    },
  ];
}
