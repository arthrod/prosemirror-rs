import type { Anchor, FileKind, ReviewDoc } from "./types.ts";

export function unanchored(doc: ReviewDoc, live: Anchor[]): string[] {
  const liveHashes = new Set(live.map((a) => a.hash));
  const seen = new Set<string>();
  const out: string[] = [];
  for (const block of doc.blocks) {
    if (liveHashes.has(block.hash) || seen.has(block.hash)) continue;
    seen.add(block.hash);
    out.push(block.hash);
  }
  return out;
}

/** `notes.md` → `notes.review.md`; `notes.txt` → `notes.txt.review.md`. */
export function exportPath(fileName: string, kind: FileKind): string {
  if (kind === "plain") return `${fileName}.review.md`;
  const stem = fileName.replace(/\.(md|markdown)$/i, "");
  return `${stem}.review.md`;
}

export function sidecarPath(fileName: string): string {
  return `${fileName}.review.json`;
}

/**
 * Review summary in the markdown-remarkable export layout.
 * File comments, then anchored blocks in document order, then unanchored.
 */
export function exportReview(
  fileName: string,
  live: Anchor[],
  doc: ReviewDoc,
  now: string,
): string {
  const unanchoredHashes = unanchored(doc, live).filter((hash) => {
    const block = doc.blocks.find((b) => b.hash === hash);
    return !!block && block.comments.length > 0;
  });

  const seen = new Set<string>();
  const sections: { anchor: Anchor; comments: ReviewDoc["blocks"][number]["comments"] }[] = [];
  for (const anchor of live) {
    if (seen.has(anchor.hash)) continue;
    seen.add(anchor.hash);
    const block = doc.blocks.find((b) => b.hash === anchor.hash);
    if (block && block.comments.length > 0) {
      sections.push({ anchor, comments: block.comments });
    }
  }

  const anchoredComments = sections.reduce((n, s) => n + s.comments.length, 0);
  const unanchoredComments = unanchoredHashes.reduce((n, hash) => {
    return n + (doc.blocks.find((b) => b.hash === hash)?.comments.length ?? 0);
  }, 0);
  const fileCount = doc.file_comments.length;

  let out = `# Review: ${fileName}\n\n`;
  if (fileCount > 0) {
    const total = fileCount + anchoredComments;
    out += `Exported: ${now} · ${total} comments (${fileCount} on the file, ${anchoredComments} on ${sections.length} blocks)`;
  } else {
    out += `Exported: ${now} · ${anchoredComments} comments on ${sections.length} blocks`;
  }
  if (unanchoredComments > 0) out += ` (+${unanchoredComments} unanchored)`;
  out += "\n\n";

  if (fileCount > 0) {
    out += `> (file): ${singleLine(fileName)}\n\n`;
    for (const comment of doc.file_comments) out += bullet(comment.text);
    out += "\n";
  }

  for (const section of sections) {
    out += "> ";
    out += lineLabel(section.anchor.lineStart, section.anchor.lineEnd);
    out += ": ";
    out += singleLine(section.anchor.excerpt);
    const suffix = nestedSuffix(live, section.anchor);
    if (suffix) out += ` ${suffix}`;
    out += "\n\n";
    for (const comment of section.comments) out += bullet(comment.text);
    out += "\n";
  }

  if (unanchoredHashes.length) {
    out += "## Unanchored\n\n";
    for (const hash of unanchoredHashes) {
      const block = doc.blocks.find((b) => b.hash === hash);
      if (!block) continue;
      out += `> (not found): ${singleLine(block.excerpt)}\n\n`;
      for (const comment of block.comments) out += bullet(comment.text);
      out += "\n";
    }
  }

  return `${out.trimEnd()}\n`;
}

function nestedSuffix(anchors: Anchor[], anchor: Anchor): string | null {
  if (anchor.kind === "block") return null;
  let ancestor: Anchor | undefined = anchor;
  while (ancestor && ancestor.kind !== "block") {
    if (ancestor.parent == null) return null;
    ancestor = anchors[ancestor.parent];
  }
  if (!ancestor) return null;
  return `(in ${blockKindLabel(ancestor.source)} ${lineLabel(ancestor.lineStart, ancestor.lineEnd)})`;
}

function blockKindLabel(source: string): "table" | "list" | "block" {
  const trimmed = source.trimStart();
  if (trimmed.startsWith("|")) return "table";
  if (["- ", "* ", "+ "].some((m) => trimmed.startsWith(m))) return "list";
  const digits = /^\d+/.exec(trimmed);
  if (digits) {
    const after = trimmed.slice(digits[0].length);
    if (after.startsWith(". ") || after.startsWith(") ")) return "list";
  }
  return "block";
}

function lineLabel(start: number, end: number): string {
  return start === end ? `L${start}` : `L${start}-L${end}`;
}

function singleLine(s: string): string {
  return s.replace(/[\n\r]/g, " ");
}

function bullet(text: string): string {
  const lines = text.split("\n");
  let out = `- ${lines[0] ?? ""}\n`;
  for (const line of lines.slice(1)) out += `  ${line}\n`;
  return out;
}
