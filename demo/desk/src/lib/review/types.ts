export type FileKind = "markdown" | "plain";

export type AnchorKind = "block" | "item" | "row";

export type Comment = {
  id: string;
  text: string;
  created: string;
  updated: string;
};

/** Sidecar shape from markdown-remarkable (`ReviewDoc`, schema version 1). */
export type ReviewDoc = {
  version: 1;
  file: string;
  blocks: ReviewBlock[];
  file_comments: Comment[];
};

export type ReviewBlock = {
  hash: string;
  excerpt: string;
  kind: AnchorKind;
  comments: Comment[];
};

export type Anchor = {
  kind: AnchorKind;
  hash: string;
  source: string;
  excerpt: string;
  lineStart: number;
  lineEnd: number;
  parent: number | null;
  from: number;
  to: number;
};

export function emptyReview(fileName: string): ReviewDoc {
  return { version: 1, file: fileName, blocks: [], file_comments: [] };
}

export function commentId(): string {
  const bytes = new Uint8Array(8);
  crypto.getRandomValues(bytes);
  return (
    "c_" +
    [...bytes].map((b) => b.toString(16).padStart(2, "0")).join("")
  );
}

export function normalizeReview(raw: unknown, fileName: string): ReviewDoc {
  const base = emptyReview(fileName);
  if (!raw || typeof raw !== "object") return base;
  const doc = raw as Partial<ReviewDoc>;
  if (doc.version !== 1) return base;
  const blocks = Array.isArray(doc.blocks) ? doc.blocks.flatMap(cleanBlock) : [];
  const fileComments = Array.isArray(doc.file_comments)
    ? doc.file_comments.flatMap(cleanComment)
    : [];
  return {
    version: 1,
    file: typeof doc.file === "string" && doc.file ? doc.file : fileName,
    blocks: blocks.filter((b) => b.comments.length > 0),
    file_comments: fileComments,
  };
}

function cleanBlock(raw: unknown): ReviewBlock[] {
  if (!raw || typeof raw !== "object") return [];
  const b = raw as Partial<ReviewBlock>;
  if (typeof b.hash !== "string" || !b.hash) return [];
  const kind: AnchorKind =
    b.kind === "item" || b.kind === "row" ? b.kind : "block";
  const comments = Array.isArray(b.comments) ? b.comments.flatMap(cleanComment) : [];
  return [
    {
      hash: b.hash,
      excerpt: typeof b.excerpt === "string" ? [...b.excerpt].slice(0, 200).join("") : "",
      kind,
      comments,
    },
  ];
}

function cleanComment(raw: unknown): Comment[] {
  if (!raw || typeof raw !== "object") return [];
  const c = raw as Partial<Comment>;
  if (typeof c.id !== "string" || !/^c_[0-9a-f]{16}$/.test(c.id)) return [];
  if (typeof c.text !== "string") return [];
  const text = c.text.slice(0, 64 * 1024);
  const created = typeof c.created === "string" ? c.created : new Date().toISOString();
  const updated = typeof c.updated === "string" ? c.updated : created;
  return [{ id: c.id, text, created, updated }];
}
