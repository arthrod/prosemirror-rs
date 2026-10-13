import MarkdownIt from "markdown-it";
import type { Node as PMNode } from "prosemirror-model";
import { MarkdownParser, MarkdownSerializer } from "prosemirror-markdown";
import { plainSchema, sanitizeHref, schema } from "./schema.ts";

const tokenizer = new MarkdownIt("default", { html: false, linkify: false, breaks: false });

function listIsTight(tokens: { type: string; hidden?: boolean }[], index: number) {
  for (let i = index + 1; i < tokens.length; i++) {
    if (tokens[i].type !== "list_item_open") return !!tokens[i].hidden;
  }
  return false;
}

export const markdownParser = new MarkdownParser(schema, tokenizer, {
  blockquote: { block: "blockquote" },
  paragraph: { block: "paragraph" },
  list_item: { block: "list_item" },
  bullet_list: {
    block: "bullet_list",
    getAttrs: (tok, tokens, i) => ({
      tight: listIsTight(tokens, i),
      bullet: tok.markup === "*" || tok.markup === "+" ? tok.markup : "-",
    }),
  },
  ordered_list: {
    block: "ordered_list",
    getAttrs: (tok, tokens, i) => ({
      order: Number(tok.attrGet("start") || 1) || 1,
      tight: listIsTight(tokens, i),
    }),
  },
  heading: { block: "heading", getAttrs: (tok) => ({ level: Number(tok.tag.slice(1)) }) },
  code_block: { block: "code_block", noCloseToken: true },
  fence: {
    block: "code_block",
    getAttrs: (tok) => ({ params: tok.info || "" }),
    noCloseToken: true,
  },
  hr: { node: "horizontal_rule" },
  image: {
    node: "image",
    getAttrs: (tok) => ({
      src: sanitizeImage(tok.attrGet("src") || ""),
      title: tok.attrGet("title") || null,
      alt: tok.children?.[0]?.content || null,
    }),
  },
  hardbreak: { node: "hard_break" },
  em: { mark: "em" },
  strong: { mark: "strong" },
  s: { mark: "strikethrough" },
  link: {
    mark: "link",
    getAttrs: (tok) => ({
      href: sanitizeHref(tok.attrGet("href") || ""),
      title: tok.attrGet("title") || null,
    }),
  },
  code_inline: { mark: "code", noCloseToken: true },
  table: { block: "table" },
  thead: { ignore: true },
  tbody: { ignore: true },
  tr: { block: "table_row" },
  th: { block: "table_header" },
  td: { block: "table_cell" },
});

function sanitizeImage(src: string): string {
  const trimmed = src.trim();
  if (/^(javascript|vbscript|file):/i.test(trimmed)) return "";
  if (/^data:/i.test(trimmed) && !/^data:image\//i.test(trimmed)) return "";
  return trimmed;
}

export const markdownSerializer = new MarkdownSerializer(
  {
    blockquote(state, node) {
      state.wrapBlock("> ", null, node, () => state.renderContent(node));
    },
    code_block(state, node) {
      const ticks = node.textContent.match(/`{3,}/gm);
      const fence = ticks ? `${ticks.sort().slice(-1)[0]}\`` : "```";
      state.write(`${fence}${node.attrs.params || ""}\n`);
      state.text(node.textContent, false);
      state.write("\n");
      state.write(fence);
      state.closeBlock(node);
    },
    heading(state, node) {
      state.write(`${"#".repeat(node.attrs.level as number)} `);
      state.renderInline(node);
      state.closeBlock(node);
    },
    horizontal_rule(state, node) {
      state.write("---");
      state.closeBlock(node);
    },
    bullet_list(state, node) {
      const bullet = (node.attrs.bullet as string) || "-";
      state.renderList(node, "  ", (i) => {
        const child = node.child(i);
        if (child.attrs.checked === true) return `${bullet} [x] `;
        if (child.attrs.checked === false) return `${bullet} [ ] `;
        return `${bullet} `;
      });
    },
    ordered_list(state, node) {
      const start = (node.attrs.order as number) || 1;
      const maxW = String(start + node.childCount - 1).length;
      state.renderList(node, " ".repeat(maxW + 2), (i) => {
        const nStr = String(start + i);
        return `${" ".repeat(maxW - nStr.length)}${nStr}. `;
      });
    },
    list_item(state, node) {
      state.renderContent(node);
    },
    paragraph(state, node) {
      state.renderInline(node);
      state.closeBlock(node);
    },
    image(state, node) {
      const alt = state.esc(String(node.attrs.alt || ""));
      const src = String(node.attrs.src || "").replace(/[()]/g, "\\$&");
      const title = node.attrs.title
        ? ` "${String(node.attrs.title).replace(/"/g, '\\"')}"`
        : "";
      state.write(`![${alt}](${src}${title})`);
    },
    hard_break(state, node, parent, index) {
      for (let i = index + 1; i < parent.childCount; i++) {
        if (parent.child(i).type !== node.type) {
          state.write("\\\n");
          return;
        }
      }
    },
    text(state, node) {
      state.text(node.text || "", true);
    },
    table(state, node) {
      const rows: string[][] = [];
      node.forEach((row) => {
        const cells: string[] = [];
        row.forEach((cell) => {
          cells.push(cell.textContent.replace(/\|/g, "\\|").replace(/\n/g, " ").trim());
        });
        rows.push(cells);
      });
      if (rows.length) {
        const width = Math.max(...rows.map((row) => row.length));
        for (const row of rows) while (row.length < width) row.push("");
        const writeRow = (row: string[]) => {
          state.write(`| ${row.join(" | ")} |`);
          state.ensureNewLine();
        };
        writeRow(rows[0]);
        state.write(`| ${rows[0].map(() => "---").join(" | ")} |`);
        state.ensureNewLine();
        for (const row of rows.slice(1)) writeRow(row);
      }
      state.closeBlock(node);
    },
    table_row() {},
    table_cell() {},
    table_header() {},
  },
  {
    em: { open: "*", close: "*", mixable: true, expelEnclosingWhitespace: true },
    strong: { open: "**", close: "**", mixable: true, expelEnclosingWhitespace: true },
    strikethrough: { open: "~~", close: "~~", mixable: true, expelEnclosingWhitespace: true },
    link: {
      open: () => "[",
      close: (_state, mark) => {
        const href = sanitizeHref(String(mark.attrs.href || "")).replace(/[()"]/g, "\\$&");
        const title = mark.attrs.title
          ? ` "${String(mark.attrs.title).replace(/"/g, '\\"')}"`
          : "";
        return `](${href}${title})`;
      },
      mixable: true,
    },
    code: {
      open: (_state, _mark, parent, index) => ticksFor(parent.child(index), -1),
      close: (_state, _mark, parent, index) => ticksFor(parent.child(index - 1), 1),
      escape: false,
    },
  },
);

function ticksFor(node: PMNode, side: number) {
  let len = 0;
  if (node.isText) {
    const found = node.text?.match(/`+/g);
    if (found) len = Math.max(...found.map((t) => t.length));
  }
  let result = len > 0 && side > 0 ? " `" : "`";
  result += "`".repeat(len);
  if (len > 0 && side < 0) result += " ";
  return result;
}

export function splitFrontmatter(text: string): { frontmatter: string; body: string } {
  const normalized = text.replace(/\r\n/g, "\n");
  if (!normalized.startsWith("---\n")) return { frontmatter: "", body: normalized };
  const rest = normalized.slice(4);
  const match = /\n(?:---|\.\.\.)[ \t]*(?:\n|$)/.exec(rest);
  if (!match) return { frontmatter: "", body: normalized };
  const end = 4 + match.index + match[0].length;
  return { frontmatter: normalized.slice(0, end), body: normalized.slice(end).replace(/^\n/, "") };
}

export function stripHtmlComments(text: string): string {
  return text.replace(/<!--[\s\S]*?-->/g, "");
}

function rewriteListItems(node: PMNode): PMNode {
  if (node.isText) return node;
  const children: PMNode[] = [];
  node.forEach((child) => children.push(rewriteListItems(child)));
  const next = node.type.create(node.attrs, children, node.marks);
  return applyTask(next);
}

function applyTask(node: PMNode): PMNode {
  if (node.type.name !== "list_item" || node.attrs.checked != null) return node;
  const first = node.firstChild;
  if (!first || first.type.name !== "paragraph") return node;
  const textNode = first.firstChild;
  if (!textNode?.isText) return node;
  const match = /^\[( |x|X)\] /.exec(textNode.text || "");
  if (!match) return node;
  const rest = (textNode.text || "").slice(match[0].length);
  const kids: PMNode[] = [];
  if (rest) kids.push(schema.text(rest, textNode.marks));
  first.forEach((child, _off, index) => {
    if (index > 0) kids.push(child);
  });
  const paragraph = first.type.create(first.attrs, kids);
  const itemKids: PMNode[] = [paragraph];
  node.forEach((child, _off, index) => {
    if (index > 0) itemKids.push(child);
  });
  return node.type.create(
    { ...node.attrs, checked: match[1].toLowerCase() === "x" },
    itemKids,
    node.marks,
  );
}

export function parseMarkdown(text: string): PMNode {
  const stripped = stripHtmlComments(text).replace(/\r\n/g, "\n").replace(/^\n+/, "");
  if (!stripped.trim()) return schema.nodes.doc.createAndFill()!;
  let doc: PMNode;
  try {
    doc = markdownParser.parse(stripped);
  } catch {
    doc = schema.nodes.doc.create(null, [
      schema.nodes.paragraph.create(null, schema.text(stripped)),
    ]);
  }
  if (!doc || !doc.childCount) return schema.nodes.doc.createAndFill()!;
  return rewriteListItems(doc);
}

export function serializeMarkdown(doc: PMNode): string {
  if (
    doc.childCount === 1 &&
    doc.firstChild?.type.name === "paragraph" &&
    doc.firstChild.content.size === 0
  ) {
    return "";
  }
  return markdownSerializer.serialize(doc).replace(/\s+$/, "");
}

export function parsePlain(text: string): PMNode {
  const normalized = text.replace(/\r\n/g, "\n");
  if (normalized === "") return plainSchema.nodes.doc.createAndFill()!;
  const paragraphs = normalized.split("\n").map((line) =>
    plainSchema.nodes.paragraph.create(null, line ? plainSchema.text(line) : null),
  );
  return plainSchema.nodes.doc.create(null, paragraphs);
}

export function serializePlain(doc: PMNode): string {
  const lines: string[] = [];
  doc.forEach((node) => lines.push(node.textContent));
  if (lines.length === 1 && lines[0] === "") return "";
  return lines.join("\n");
}

export function docFromBody(kind: "markdown" | "plain", body: string): PMNode {
  return kind === "plain" ? parsePlain(body) : parseMarkdown(body);
}

export function bodyFromDoc(kind: "markdown" | "plain", doc: PMNode): string {
  return kind === "plain" ? serializePlain(doc) : serializeMarkdown(doc);
}

export function joinDocument(frontmatter: string, body: string): string {
  if (!frontmatter) return body;
  const fm = frontmatter.endsWith("\n") ? frontmatter : `${frontmatter}\n`;
  const rest = body.replace(/^\n/, "");
  if (!rest) return fm.replace(/\n$/, "");
  return `${fm}\n${rest}`;
}

/** Serialize one block node as it would appear inside a document. */
export function serializeBlock(node: PMNode): string {
  const doc = schema.nodes.doc.create(null, [node]);
  return markdownSerializer.serialize(doc).replace(/\n+$/, "");
}
