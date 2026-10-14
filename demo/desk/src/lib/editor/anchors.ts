import type { Node as PMNode } from "prosemirror-model";
import { excerptOf, hashSource, itemExcerpt, rowExcerpt } from "../review/hash.ts";
import type { Anchor } from "../review/types.ts";
import { markdownSerializer, serializePlain } from "./markdown.ts";
import { schema } from "./schema.ts";

export function anchorsFor(doc: PMNode, kind: "markdown" | "plain"): Anchor[] {
  return kind === "plain" ? plainAnchors(doc) : markdownAnchors(doc);
}

/** Innermost anchor covering a document position. */
export function anchorAt(anchors: Anchor[], pos: number): Anchor | null {
  let best: Anchor | null = null;
  for (const anchor of anchors) {
    if (pos >= anchor.from && pos <= anchor.to) {
      if (!best || anchor.to - anchor.from <= best.to - best.from) best = anchor;
    }
  }
  return best;
}

function markdownAnchors(doc: PMNode): Anchor[] {
  const anchors: Anchor[] = [];
  let line = 1;
  doc.forEach((block, offset) => {
    const source = serializeBlock(block);
    const blockIndex = anchors.length;
    const lineStart = line;
    const lineEnd = line + Math.max(0, newlineCount(source));
    pushAnchor(anchors, {
      kind: "block",
      source,
      excerpt: excerptOf(source.trim()),
      lineStart,
      lineEnd,
      parent: null,
      from: offset,
      to: offset + block.nodeSize,
    });
    if (block.type.name === "bullet_list" || block.type.name === "ordered_list") {
      collectItems(block, offset, blockIndex, anchors, source, lineStart);
    } else if (block.type.name === "table") {
      collectRows(block, offset, blockIndex, anchors, source, lineStart);
    } else {
      collectNested(block, offset, blockIndex, anchors, source, lineStart);
    }
    line = lineEnd + 2;
  });
  return anchors;
}

function collectNested(
  node: PMNode,
  nodePos: number,
  parent: number,
  anchors: Anchor[],
  parentSource: string,
  parentLine: number,
) {
  let pos = nodePos + 1;
  node.forEach((child) => {
    if (child.type.name === "bullet_list" || child.type.name === "ordered_list") {
      collectItems(child, pos, parent, anchors, parentSource, parentLine);
    } else if (child.type.name === "table") {
      collectRows(child, pos, parent, anchors, parentSource, parentLine);
    } else if (!child.isText && child.childCount) {
      collectNested(child, pos, parent, anchors, parentSource, parentLine);
    }
    pos += child.nodeSize;
  });
}

function collectItems(
  list: PMNode,
  listPos: number,
  parent: number,
  anchors: Anchor[],
  parentSource: string,
  parentLine: number,
) {
  let pos = listPos + 1;
  let searchFrom = 0;
  list.forEach((item, _off, index) => {
    if (item.type.name !== "list_item") {
      pos += item.nodeSize;
      return;
    }
    const source = itemSource(list, item, index);
    const located = locate(parentSource, source, searchFrom);
    if (located) searchFrom = located.index + Math.max(1, source.length);
    const lineStart = parentLine + (located?.line ?? 0);
    const lineEnd = lineStart + newlineCount(source);
    const itemIndex = anchors.length;
    pushAnchor(anchors, {
      kind: "item",
      source,
      excerpt: itemExcerpt(source),
      lineStart,
      lineEnd,
      parent,
      from: pos,
      to: pos + item.nodeSize,
    });
    let inner = pos + 1;
    item.forEach((child) => {
      if (child.type.name === "bullet_list" || child.type.name === "ordered_list") {
        collectItems(child, inner, itemIndex, anchors, source, lineStart);
      } else if (child.type.name === "table") {
        collectRows(child, inner, itemIndex, anchors, source, lineStart);
      } else if (child.type.name === "blockquote") {
        collectNested(child, inner, itemIndex, anchors, source, lineStart);
      }
      inner += child.nodeSize;
    });
    pos += item.nodeSize;
    void index;
  });
}

function collectRows(
  table: PMNode,
  tablePos: number,
  parent: number,
  anchors: Anchor[],
  parentSource: string,
  parentLine: number,
) {
  let pos = tablePos + 1;
  let searchFrom = 0;
  table.forEach((row) => {
    if (row.type.name !== "table_row") {
      pos += row.nodeSize;
      return;
    }
    const cells: string[] = [];
    row.forEach((cell) => cells.push(cell.textContent.replace(/\n/g, " ").trim()));
    const source = `| ${cells.join(" | ")} |`;
    const located = locate(parentSource, source, searchFrom);
    if (located) searchFrom = located.index + source.length;
    const line = parentLine + (located?.line ?? 0);
    pushAnchor(anchors, {
      kind: "row",
      source,
      excerpt: rowExcerpt(cells),
      lineStart: line,
      lineEnd: line,
      parent,
      from: pos,
      to: pos + row.nodeSize,
    });
    pos += row.nodeSize;
  });
}

function locate(haystack: string, needle: string, from: number) {
  if (!needle) return null;
  const index = haystack.indexOf(needle, from);
  if (index < 0) return null;
  const line = haystack.slice(0, index).split("\n").length - 1;
  return { index, line };
}

function newlineCount(source: string) {
  if (!source) return 0;
  return source.split("\n").length - 1;
}

function itemSource(list: PMNode, item: PMNode, index: number) {
  const wrapper =
    list.type.name === "ordered_list"
      ? schema.nodes.ordered_list.create(
          { order: listStart(list) + index, tight: true },
          [item],
        )
      : schema.nodes.bullet_list.create({ tight: true, bullet: list.attrs.bullet || "-" }, [item]);
  const doc = schema.nodes.doc.create(null, [wrapper]);
  return markdownSerializer.serialize(doc).replace(/\n+$/, "").trim();
}

function serializeBlock(node: PMNode) {
  const doc = schema.nodes.doc.create(null, [node]);
  return markdownSerializer.serialize(doc).replace(/\n+$/, "");
}

function pushAnchor(anchors: Anchor[], draft: Omit<Anchor, "hash">) {
  const source = draft.source.trim();
  anchors.push({ ...draft, source, hash: hashWithOccurrence(anchors, source) });
}

function plainAnchors(doc: PMNode): Anchor[] {
  const anchors: Anchor[] = [];
  const text = serializePlain(doc);
  if (!text) return anchors;
  doc.forEach((node, offset) => {
    const source = node.textContent.trim();
    if (!source) return;
    const line = text.slice(0, lineOffset(doc, offset)).split("\n").length;
    anchors.push({
      kind: "block",
      hash: hashWithOccurrence(anchors, source),
      source,
      excerpt: [...source].slice(0, 80).join(""),
      lineStart: line,
      lineEnd: line,
      parent: null,
      from: offset,
      to: offset + node.nodeSize,
    });
  });
  return anchors;
}

/** Same text keeps the original hash; later copies add a stable occurrence index. */
function hashWithOccurrence(anchors: { source: string }[], source: string) {
  let occurrence = 0;
  for (const anchor of anchors) if (anchor.source === source) occurrence += 1;
  return hashSource(occurrence === 0 ? source : `${source}\0${occurrence}`);
}

function listStart(list: PMNode) {
  const order = Number(list.attrs.order);
  return Number.isFinite(order) ? order : 1;
}

function lineOffset(doc: PMNode, offset: number) {
  let chars = 0;
  doc.forEach((node, nodeOffset) => {
    if (nodeOffset < offset) chars += node.textContent.length + 1;
  });
  return chars;
}
