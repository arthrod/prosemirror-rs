import { history } from "prosemirror-history";
import { inputRules } from "prosemirror-inputrules";
import { gapCursor } from "prosemirror-gapcursor";
import { dropCursor } from "prosemirror-dropcursor";
import type { Node as PMNode } from "prosemirror-model";
import { EditorState, Plugin, PluginKey, TextSelection, type Command } from "prosemirror-state";
import { EditorView, Decoration, DecorationSet } from "prosemirror-view";
import { columnResizing, tableEditing } from "prosemirror-tables";
import { anchorAt, anchorsFor } from "./anchors.ts";
import { baseMap, markdownInputRules, markdownKeymap, plainKeymap } from "./commands.ts";
import { bodyFromDoc, docFromBody } from "./markdown.ts";
import { isAllowedImageSrc, plainSchema, schema } from "./schema.ts";
import type { FileKind } from "../review/types.ts";

export const editorBridge: { current: EditorHandle | null } = { current: null };

const chromeListeners = new Set<() => void>();
export const editorChrome = {
  marks: {} as Record<string, boolean>,
  subscribe(listener: () => void) {
    chromeListeners.add(listener);
    return () => chromeListeners.delete(listener);
  },
  getMarks() {
    return editorChrome.marks;
  },
  set(marks: Record<string, boolean>) {
    editorChrome.marks = marks;
    for (const listener of chromeListeners) listener();
  },
};

export type StepRecord = { label: string; json: unknown };

export type EditorHandle = {
  destroy: () => void;
  command: (command: Command) => boolean;
  focusRange: (from: number, to: number) => void;
  replaceBody: (body: string) => void;
  setAnchorState: (comments: string[], selected: string | null) => void;
  getDoc: () => PMNode | null;
  getJSON: () => unknown;
};

export function mountEditor(opts: {
  host: HTMLElement;
  peer: HTMLElement | null;
  kind: FileKind;
  body: string;
  comments: string[];
  selected: string | null;
  onBody: (body: string) => void;
  onSelect: (hash: string | null) => void;
  onSteps: (steps: StepRecord[]) => void;
  onStats: (stats: { words: number; chars: number; size: number }) => void;
}): EditorHandle {
  const activeSchema = opts.kind === "plain" ? plainSchema : schema;
  let primary: EditorView | null = null;
  let secondary: EditorView | null = null;
const queue: { steps: import("prosemirror-transform").Step[]; from: "a" | "b" }[] = [];
  let timer = 0;
  let dead = false;

  const bindings: { key: PluginKey<{ comments: Set<string>; selected: string | null }> }[] = [];
  let commentList = opts.comments;
  let selectedHash = opts.selected;

  function makePlugins() {
    const key = new PluginKey<{ comments: Set<string>; selected: string | null }>("anchors");
    bindings.push({ key });
    const anchorPlugin = new Plugin({
      key,
      state: {
        init: () => ({ comments: new Set(commentList), selected: selectedHash }),
        apply(tr, value) {
          const meta = tr.getMeta(key) as { comments?: string[]; selected?: string | null } | undefined;
          if (!meta) return value;
          return {
            comments: meta.comments ? new Set(meta.comments) : value.comments,
            selected: meta.selected === undefined ? value.selected : meta.selected,
          };
        },
      },
      props: {
        decorations(state) {
          const info = key.getState(state);
          if (!info) return null;
          const anchors = anchorsFor(state.doc, opts.kind);
          const decos = [];
          for (const anchor of anchors) {
            const classes = [];
            if (info.comments.has(anchor.hash)) classes.push("has-comment");
            if (info.selected && info.selected === anchor.hash) classes.push("is-anchor");
            if (!classes.length) continue;
            decos.push(Decoration.node(anchor.from, anchor.to, { class: classes.join(" ") }));
          }
          return DecorationSet.create(state.doc, decos);
        },
        handleDOMEvents: {
          mousedown(view, event) {
            const target = event.target;
            if (!(target instanceof HTMLInputElement) || target.type !== "checkbox") return false;
            const li = target.closest("li");
            if (!li) return false;
            const pos = view.posAtDOM(li, 0);
            const $pos = view.state.doc.resolve(pos);
            for (let depth = $pos.depth; depth > 0; depth--) {
              const node = $pos.node(depth);
              if (node.type.name !== "list_item") continue;
              view.dispatch(
                view.state.tr.setNodeMarkup($pos.before(depth), undefined, {
                  ...node.attrs,
                  checked: !node.attrs.checked,
                }),
              );
              event.preventDefault();
              return true;
            }
            return false;
          },
        },
      },
    });
    if (opts.kind === "markdown") {
      return [
        inputRules({ rules: markdownInputRules() }),
        markdownKeymap(),
        columnResizing(),
        tableEditing(),
        dropCursor(),
        gapCursor(),
        baseMap(),
        history(),
        anchorPlugin,
      ];
    }
    return [plainKeymap(), baseMap(), history(), anchorPlugin];
  }

  function makeState(body: string) {
    return EditorState.create({
      schema: activeSchema,
      doc: docFromBody(opts.kind, body),
      plugins: makePlugins(),
    });
  }

  function stats(doc: PMNode) {
    const text = doc.textContent;
    const words = text.trim() ? text.trim().split(/\s+/).length : 0;
    opts.onStats({ words, chars: text.length, size: doc.content.size });
  }

  function persistFrom(doc: PMNode) {
    opts.onBody(bodyFromDoc(opts.kind, doc));
    stats(doc);
  }

  function publishMarks(view: EditorView) {
    if (opts.kind !== "markdown") {
      editorChrome.set({});
      return;
    }
    const marks: Record<string, boolean> = {};
    for (const name of ["strong", "em", "code", "strikethrough", "link"] as const) {
      const type = schema.marks[name];
      const { from, to, empty, $from } = view.state.selection;
      marks[name] = empty
        ? !!type.isInSet(view.state.storedMarks || $from.marks())
        : view.state.doc.rangeHasMark(from, to, type);
    }
    editorChrome.set(marks);
  }

  function selectFrom(view: EditorView) {
    publishMarks(view);
    if (!view.hasFocus()) return;
    const anchors = anchorsFor(view.state.doc, opts.kind);
    const hit = anchorAt(anchors, view.state.selection.head);
    opts.onSelect(hit?.hash ?? null);
  }

  function schedule() {
    if (timer || dead) return;
    timer = window.setTimeout(() => {
      timer = 0;
      flush();
    }, 200);
  }

  function flush() {
    if (dead) return;
    const batch = queue.splice(0, queue.length);
    for (const item of batch) {
      const target = item.from === "a" ? secondary : primary;
      if (!target) continue;
      let tr = target.state.tr.setMeta("remote", true).setMeta("addToHistory", false);
      let applied = false;
      for (const step of item.steps) {
        const mapped = step.map(tr.mapping);
        if (mapped && !tr.maybeStep(mapped).failed) applied = true;
      }
      if (applied) target.dispatch(tr);
    }
  }

  function dispatchFor(view: EditorView, label: "a" | "b") {
    return (tr: import("prosemirror-state").Transaction) => {
      const remote = tr.getMeta("remote") === true;
      const next = view.state.apply(tr);
      view.updateState(next);
      if (tr.steps.length && !remote) {
        opts.onSteps(tr.steps.map((step) => ({ label, json: step.toJSON() })));
        if (opts.peer && (label === "a" ? secondary : primary)) {
          queue.push({ steps: tr.steps, from: label });
          schedule();
        }
      }
      if (tr.docChanged && !remote && label === "a") persistFrom(next.doc);
      if (tr.docChanged && remote && label === "a") persistFrom(next.doc);
      if (tr.docChanged && !remote && label === "b") {
        // The paired view applies these steps and persists the merged document.
      }
      if (!remote && (tr.selectionSet || tr.docChanged)) selectFrom(view);
    };
  }

  const nodeViews = opts.kind === "markdown"
    ? {
        image(node: PMNode) {
          return new ImageView(node);
        },
      }
    : undefined;

  primary = new EditorView(opts.host, {
    state: makeState(opts.body),
    dispatchTransaction: (tr) => dispatchFor(primary!, "a")(tr),
    nodeViews,
  });
  if (opts.peer) {
    secondary = new EditorView(opts.peer, {
      state: makeState(opts.body),
      dispatchTransaction: (tr) => dispatchFor(secondary!, "b")(tr),
      nodeViews,
    });
  }
  stats(primary.state.doc);

  function paintAnchors(comments: string[], selected: string | null) {
    commentList = comments;
    selectedHash = selected;
    const views = [primary, secondary];
    views.forEach((view, index) => {
      const key = bindings[index]?.key;
      if (!view || !key) return;
      view.dispatch(view.state.tr.setMeta(key, { comments, selected }));
    });
  }

  return {
    destroy() {
      dead = true;
      window.clearTimeout(timer);
      primary?.destroy();
      secondary?.destroy();
      primary = null;
      secondary = null;
    },
    command(command) {
      if (!primary) return false;
      return command(primary.state, primary.dispatch, primary);
    },
    focusRange(from, to) {
      if (!primary) return;
      const max = primary.state.doc.content.size;
      const pos = Math.min(Math.max(from + 1, 1), Math.max(1, max));
      const sel = TextSelection.near(primary.state.doc.resolve(pos));
      primary.dispatch(primary.state.tr.setSelection(sel).scrollIntoView());
      primary.focus();
      void to;
    },
    replaceBody(body) {
      if (!primary) return;
      const doc = docFromBody(opts.kind, body);
      for (const view of [primary, secondary]) {
        if (!view) continue;
        view.dispatch(view.state.tr.setMeta("remote", true).replaceWith(0, view.state.doc.content.size, doc.content));
      }
      persistFrom(primary.state.doc);
    },
    setAnchorState: paintAnchors,
    getDoc: () => primary?.state.doc ?? null,
    getJSON: () => primary?.state.doc.toJSON() ?? null,
  };
}

class ImageView {
  dom: HTMLElement;
  constructor(node: PMNode) {
    this.dom = document.createElement("span");
    this.dom.className = "pm-image";
    this.render(node);
  }
  render(node: PMNode) {
    const src = String(node.attrs.src || "");
    const alt = String(node.attrs.alt || "Image");
    this.dom.replaceChildren();
    if (isAllowedImageSrc(src)) {
      const img = document.createElement("img");
      img.src = src;
      img.alt = alt;
      this.dom.append(img);
      return;
    }
    const note = document.createElement("span");
    note.className = "pm-image-blocked";
    note.textContent = src
      ? `Image blocked · ${src.replace(/^https?:\/\//, "").slice(0, 48)}`
      : "Image unavailable";
    this.dom.append(note);
  }
  update(node: PMNode) {
    if (node.type !== schema.nodes.image) return false;
    this.render(node);
    return true;
  }
}
