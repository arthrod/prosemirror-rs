import {
  baseKeymap,
  chainCommands,
  createParagraphNear,
  liftEmptyBlock,
  newlineInCode,
  selectParentNode,
  setBlockType,
  splitBlock,
  toggleMark,
  wrapIn,
} from "prosemirror-commands";
import { undo, redo } from "prosemirror-history";
import { InputRule, textblockTypeInputRule, undoInputRule, wrappingInputRule } from "prosemirror-inputrules";
import { keymap } from "prosemirror-keymap";
import type { Command } from "prosemirror-state";
import type { Node as PMNode } from "prosemirror-model";
import { liftListItem, sinkListItem, splitListItem, wrapInList } from "prosemirror-schema-list";
import { goToNextCell } from "prosemirror-tables";
import { sanitizeHref, schema } from "./schema.ts";

const mac = typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.platform);

export function modLabel(key: string) {
  const mod = mac ? "⌘" : "Ctrl";
  return key.replaceAll("Mod-", `${mod}+`).replaceAll("Shift-", "Shift+").replaceAll("Alt-", mac ? "⌥" : "Alt+");
}

export const toggleStrong: Command = toggleMark(schema.marks.strong);
export const toggleEm: Command = toggleMark(schema.marks.em);
export const toggleCode: Command = toggleMark(schema.marks.code);
export const toggleStrike: Command = toggleMark(schema.marks.strikethrough);
export const setParagraph: Command = setBlockType(schema.nodes.paragraph);
export function setHeading(level: number): Command {
  return setBlockType(schema.nodes.heading, { level });
}
export const toggleBullet: Command = wrapInList(schema.nodes.bullet_list);
export const toggleOrdered: Command = wrapInList(schema.nodes.ordered_list);
export const toggleQuote: Command = wrapIn(schema.nodes.blockquote);

export const insertRule: Command = (state, dispatch) => {
  if (dispatch) {
    dispatch(state.tr.replaceSelectionWith(schema.nodes.horizontal_rule.create()).scrollIntoView());
  }
  return true;
};

export function insertTable(rows = 3, cols = 3): Command {
  return (state, dispatch) => {
    const header = schema.nodes.table_row.create(
      null,
      Array.from({ length: cols }, () => schema.nodes.table_header.createAndFill()!),
    );
    const body = Array.from({ length: Math.max(0, rows - 1) }, () =>
      schema.nodes.table_row.create(
        null,
        Array.from({ length: cols }, () => schema.nodes.table_cell.createAndFill()!),
      ),
    );
    const node = schema.nodes.table.create(null, [header, ...body]);
    if (dispatch) dispatch(state.tr.replaceSelectionWith(node).scrollIntoView());
    return true;
  };
}

export function applyLink(href: string): Command {
  return (state, dispatch) => {
    const { from, to } = state.selection;
    if (from === to) return false;
    if (dispatch) {
      dispatch(state.tr.addMark(from, to, schema.marks.link.create({ href: sanitizeHref(href) })).scrollIntoView());
    }
    return true;
  };
}

export const removeLink: Command = (state, dispatch) => {
  const { from, to } = state.selection;
  if (dispatch) dispatch(state.tr.removeMark(from, to, schema.marks.link).scrollIntoView());
  return true;
};

export function insertImage(src: string, alt: string): Command {
  return (state, dispatch) => {
    const node = schema.nodes.image.create({ src, alt: alt || null });
    if (dispatch) dispatch(state.tr.replaceSelectionWith(node).scrollIntoView());
    return true;
  };
}

export function findNext(doc: PMNode, query: string, start: number): { from: number; to: number } | null {
  const q = query.trim().toLowerCase();
  if (!q) return null;
  let wrapped: { from: number; to: number } | null = null;
  let found: { from: number; to: number } | null = null;
  doc.descendants((node, pos) => {
    if (!node.isText || !node.text || found) return;
    const text = node.text.toLowerCase();
    let i = text.indexOf(q);
    while (i >= 0) {
      const from = pos + i;
      const hit = { from, to: from + q.length };
      if (!wrapped) wrapped = hit;
      if (from >= start) {
        found = hit;
        return false;
      }
      i = text.indexOf(q, i + 1);
    }
  });
  return found ?? wrapped;
}

export function markdownKeymap() {
  return keymap({
  "Mod-z": undo,
  "Mod-y": redo,
  "Shift-Mod-z": redo,
  Backspace: undoInputRule,
  "Mod-b": toggleStrong,
  "Mod-i": toggleEm,
  "Mod-`": toggleCode,
  "Mod-Shift-x": toggleStrike,
  "Mod-Shift-8": toggleBullet,
  "Mod-Shift-7": toggleOrdered,
  "Mod-Shift-.": toggleQuote,
  Enter: chainCommands(
    newlineInCode,
    splitListItem(schema.nodes.list_item),
    createParagraphNear,
    liftEmptyBlock,
    splitBlock,
  ),
  Tab: chainCommands(goToNextCell(1), sinkListItem(schema.nodes.list_item)),
  "Shift-Tab": chainCommands(goToNextCell(-1), liftListItem(schema.nodes.list_item)),
  "Mod-Alt-0": setParagraph,
  "Mod-Alt-1": setHeading(1),
  "Mod-Alt-2": setHeading(2),
  "Mod-Alt-3": setHeading(3),
  Escape: selectParentNode,
  });
}

export function plainKeymap() {
  return keymap({
  "Mod-z": undo,
  "Mod-y": redo,
  "Shift-Mod-z": redo,
  Enter: splitBlock,
  });
}

export function baseMap() {
  return keymap(baseKeymap);
}

export function markdownInputRules() {
  return [
    wrappingInputRule(/^\s*>\s$/, schema.nodes.blockquote),
    wrappingInputRule(
      /^(\d+)\.\s$/,
      schema.nodes.ordered_list,
      (match) => ({ order: Number(match[1]) }),
      (match, node) => node.childCount + node.attrs.order === Number(match[1]),
    ),
    wrappingInputRule(/^\s*([-+*])\s$/, schema.nodes.bullet_list, (match) => ({
      bullet: match[1] === "*" || match[1] === "+" ? match[1] : "-",
    })),
    textblockTypeInputRule(/^```(.*)$/, schema.nodes.code_block, (match) => ({
      params: (match[1] || "").trim(),
    })),
    textblockTypeInputRule(/^(#{1,6})\s$/, schema.nodes.heading, (match) => ({
      level: match[1].length,
    })),
    new InputRule(/^(?:---|___|\*\*\*)$/, (state, _match, start, end) => {
      const $start = state.doc.resolve(start);
      if (
        !$start.node(-1).canReplaceWith(
          $start.index(-1),
          $start.indexAfter(-1),
          schema.nodes.horizontal_rule,
        )
      ) {
        return null;
      }
      return state.tr.delete(start, end).replaceSelectionWith(schema.nodes.horizontal_rule.create());
    }),
    new InputRule(/^\[( |x|X)\] $/, (state, match, start, end) => {
      const $pos = state.doc.resolve(start);
      for (let depth = $pos.depth; depth > 0; depth--) {
        if ($pos.node(depth).type.name !== "list_item") continue;
        const item = $pos.node(depth);
        return state.tr.delete(start, end).setNodeMarkup($pos.before(depth), undefined, {
          ...item.attrs,
          checked: match[1].toLowerCase() === "x",
        });
      }
      return null;
    }),
  ];
}
