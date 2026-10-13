import { Schema } from "prosemirror-model";
import { tableNodes } from "prosemirror-tables";

const tables = tableNodes({
  tableGroup: "block",
  cellContent: "inline*",
  cellAttributes: {},
});

export const schema = new Schema({
  nodes: {
    doc: { content: "block+" },
    paragraph: {
      content: "inline*",
      group: "block",
      parseDOM: [{ tag: "p" }],
      toDOM: () => ["p", 0],
    },
    blockquote: {
      content: "block+",
      group: "block",
      defining: true,
      parseDOM: [{ tag: "blockquote" }],
      toDOM: () => ["blockquote", 0],
    },
    horizontal_rule: {
      group: "block",
      parseDOM: [{ tag: "hr" }],
      toDOM: () => ["hr"],
    },
    heading: {
      attrs: { level: { default: 1 } },
      content: "inline*",
      group: "block",
      defining: true,
      parseDOM: [1, 2, 3, 4, 5, 6].map((level) => ({
        tag: `h${level}`,
        attrs: { level },
      })),
      toDOM: (node) => [`h${node.attrs.level}`, 0],
    },
    code_block: {
      content: "text*",
      marks: "",
      group: "block",
      code: true,
      defining: true,
      attrs: { params: { default: "" } },
      parseDOM: [
        {
          tag: "pre",
          preserveWhitespace: "full",
          getAttrs: (dom) => ({
            params: (dom as HTMLElement).getAttribute("data-params") || "",
          }),
        },
      ],
      toDOM: (node) => [
        "pre",
        node.attrs.params ? { "data-params": node.attrs.params } : {},
        ["code", 0],
      ],
    },
    text: { group: "inline" },
    image: {
      inline: true,
      attrs: {
        src: { default: "" },
        alt: { default: null },
        title: { default: null },
      },
      group: "inline",
      draggable: true,
      parseDOM: [
        {
          tag: "img[src]",
          getAttrs: (dom) => {
            const el = dom as HTMLElement;
            return {
              src: el.getAttribute("src"),
              title: el.getAttribute("title"),
              alt: el.getAttribute("alt"),
            };
          },
        },
      ],
      toDOM: (node) => [
        "img",
        {
          src: displayImageSrc(String(node.attrs.src || "")),
          alt: node.attrs.alt || undefined,
          title: node.attrs.title || undefined,
        },
      ],
    },
    hard_break: {
      inline: true,
      group: "inline",
      selectable: false,
      parseDOM: [{ tag: "br" }],
      toDOM: () => ["br"],
    },
    ordered_list: {
      content: "list_item+",
      group: "block",
      attrs: { order: { default: 1 }, tight: { default: false } },
      parseDOM: [
        {
          tag: "ol",
          getAttrs: (dom) => ({
            order: (dom as HTMLOListElement).hasAttribute("start")
              ? Number((dom as HTMLOListElement).getAttribute("start"))
              : 1,
            tight: (dom as HTMLElement).hasAttribute("data-tight"),
          }),
        },
      ],
      toDOM: (node) => [
        "ol",
        {
          start: node.attrs.order === 1 ? undefined : node.attrs.order,
          "data-tight": node.attrs.tight ? "true" : undefined,
        },
        0,
      ],
    },
    bullet_list: {
      content: "list_item+",
      group: "block",
      attrs: { tight: { default: false }, bullet: { default: "-" } },
      parseDOM: [
        {
          tag: "ul",
          getAttrs: (dom) => ({
            tight: (dom as HTMLElement).hasAttribute("data-tight"),
            bullet: "-",
          }),
        },
      ],
      toDOM: (node) => [
        "ul",
        { "data-tight": node.attrs.tight ? "true" : undefined },
        0,
      ],
    },
    list_item: {
      content: "paragraph block*",
      defining: true,
      attrs: { checked: { default: null } },
      parseDOM: [
        {
          tag: "li",
          getAttrs: (dom) => {
            const el = dom as HTMLElement;
            const box = el.querySelector("input[type=checkbox]") as HTMLInputElement | null;
            if (!box && el.dataset.task == null) return { checked: null };
            const on = el.dataset.task === "done" || !!box?.checked;
            return { checked: on };
          },
        },
      ],
      toDOM: (node) => {
        if (node.attrs.checked == null) return ["li", 0];
        const done = node.attrs.checked === true;
        return [
          "li",
          { class: "task", "data-task": done ? "done" : "open" },
          [
            "input",
            {
              type: "checkbox",
              ...(done ? { checked: "checked" } : {}),
            },
          ],
          ["div", { class: "task-body" }, 0],
        ];
      },
    },
    ...tables,
  },
  marks: {
    em: {
      parseDOM: [{ tag: "i" }, { tag: "em" }, { style: "font-style=italic" }],
      toDOM: () => ["em", 0],
    },
    strong: {
      parseDOM: [
        { tag: "strong" },
        { tag: "b" },
        { style: "font-weight=bold" },
        { style: "font-weight=700" },
      ],
      toDOM: () => ["strong", 0],
    },
    link: {
      attrs: { href: { default: "" }, title: { default: null } },
      inclusive: false,
      parseDOM: [
        {
          tag: "a[href]",
          getAttrs: (dom) => {
            const el = dom as HTMLElement;
            return {
              href: sanitizeHref(el.getAttribute("href") || ""),
              title: el.getAttribute("title"),
            };
          },
        },
      ],
      toDOM: (mark) => [
        "a",
        {
          href: sanitizeHref(String(mark.attrs.href || "")),
          title: mark.attrs.title || undefined,
          rel: "noreferrer noopener",
        },
        0,
      ],
    },
    code: {
      parseDOM: [{ tag: "code" }],
      toDOM: () => ["code", 0],
    },
    strikethrough: {
      parseDOM: [{ tag: "s" }, { tag: "del" }, { style: "text-decoration=line-through" }],
      toDOM: () => ["s", 0],
    },
  },
});

/** Paragraphs of plain text. No marks, no markdown. */
export const plainSchema = new Schema({
  nodes: {
    doc: { content: "paragraph+" },
    paragraph: {
      content: "text*",
      parseDOM: [{ tag: "p" }],
      toDOM: () => ["p", 0],
    },
    text: {},
  },
  marks: {},
});

export const imagePolicy = { allowRemote: false };

const BAD_LINK = /^(javascript|vbscript|file|data):/i;

export function sanitizeHref(href: string): string {
  const trimmed = href.trim();
  if (!trimmed) return "";
  if (BAD_LINK.test(trimmed)) return "#";
  return trimmed;
}

export function isRemoteHttp(src: string): boolean {
  return /^https?:\/\//i.test(src.trim());
}

export function isAllowedImageSrc(src: string, allowRemote = imagePolicy.allowRemote): boolean {
  const trimmed = src.trim();
  if (!trimmed || BAD_LINK.test(trimmed)) {
    return /^data:image\/(png|jpe?g|gif|webp|bmp|svg\+xml|avif|x-icon);/i.test(trimmed);
  }
  if (isRemoteHttp(trimmed)) return allowRemote;
  if (trimmed.startsWith("data:")) {
    return /^data:image\/(png|jpe?g|gif|webp|bmp|svg\+xml|avif|x-icon);/i.test(trimmed);
  }
  // Relative paths are kept in the source but not fetched (no local filesystem).
  return false;
}

function displayImageSrc(src: string): string {
  return isAllowedImageSrc(src) ? src : "";
}
