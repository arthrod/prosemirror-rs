import { useEffect, useRef, useState, useSyncExternalStore, type ReactNode } from "react";
import * as DropdownMenu from "@radix-ui/react-dropdown-menu";
import * as Dialog from "@radix-ui/react-dialog";
import { redo, undo } from "prosemirror-history";
import { DOMSerializer } from "prosemirror-model";
import {
  Bold, Braces, ChevronLeft, ChevronRight, Code, Columns2, Heading1, Heading2, Heading3,
  HelpCircle, Image as ImageIcon, Italic, Link2, List, ListOrdered, Minus, Moon, PanelLeft,
  PanelRight, Quote, Redo2, Search, Strikethrough, Sun, Table, Undo2, ZoomIn, ZoomOut,
} from "lucide-react";
import { EditorPane } from "@/components/editor-pane";
import { FileTree } from "@/components/file-tree";
import { ReviewPane } from "@/components/review-pane";
import { anchorsFor } from "@/lib/editor/anchors";
import {
  applyLink, findNext, insertImage, insertRule, insertTable, modLabel, removeLink,
  setHeading, setParagraph, toggleBullet, toggleCode, toggleEm, toggleOrdered, toggleQuote,
  toggleStrike, toggleStrong,
} from "@/lib/editor/commands";
import { docFromBody, joinDocument } from "@/lib/editor/markdown";
import { editorBridge, editorChrome } from "@/lib/editor/mount";
import { imagePolicy, schema } from "@/lib/editor/schema";
import { downloadText, standaloneHtml } from "@/lib/download";
import { exportReview } from "@/lib/review/export";
import type { Anchor } from "@/lib/review/types";
import { currentFile, useWorkspace } from "@/lib/store";
import type { WidthMode } from "@/lib/store-types";

export function Workspace() {
  const [booted, setBooted] = useState(false);
  const file = useWorkspace((state) => currentFile(state));
  const prefs = useWorkspace((state) => state.prefs);
  const patchPrefs = useWorkspace((state) => state.patchPrefs);
  const notice = useWorkspace((state) => state.live.notice);
  const stats = useWorkspace((state) => state.live);
  const steps = useWorkspace((state) => state.live.steps);
  const historyIndex = useWorkspace((state) => state.historyIndex);
  const historyLength = useWorkspace((state) => state.history.length);
  const back = useWorkspace((state) => state.back);
  const forward = useWorkspace((state) => state.forward);
  const selectedHash = useWorkspace((state) => state.selectedHash);
  const [help, setHelp] = useState(false);
  const [linkOpen, setLinkOpen] = useState(false);
  const [href, setHref] = useState("https://");
  const [query, setQuery] = useState("");
  const [source, setSource] = useState("");
  const findFrom = useRef(0);
  const marks = useSyncExternalStore(editorChrome.subscribe, editorChrome.getMarks, editorChrome.getMarks);

  useEffect(() => {
    let stop = false;
    void Promise.resolve(useWorkspace.persist.rehydrate()).finally(() => {
      if (!stop) setBooted(true);
    });
    return () => {
      stop = true;
    };
  }, []);

  useEffect(() => {
    imagePolicy.allowRemote = prefs.allowRemote;
    const root = document.documentElement;
    const apply = (mode: "light" | "dark") => {
      root.dataset.theme = mode;
    };
    if (prefs.theme === "system") {
      const media = window.matchMedia("(prefers-color-scheme: dark)");
      apply(media.matches ? "dark" : "light");
      const onChange = () => apply(media.matches ? "dark" : "light");
      media.addEventListener("change", onChange);
      return () => media.removeEventListener("change", onChange);
    }
    apply(prefs.theme);
  }, [prefs.theme, prefs.allowRemote]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      const typing = target && (target.tagName === "INPUT" || target.tagName === "TEXTAREA");
      const mod = event.metaKey || event.ctrlKey;
      if (mod && event.key === "\\") {
        event.preventDefault();
        patchPrefs({ showReview: !useWorkspace.getState().prefs.showReview });
      } else if (mod && event.key.toLowerCase() === "j" && !typing) {
        event.preventDefault();
        patchPrefs({ showReview: !useWorkspace.getState().prefs.showReview });
      } else if (mod && event.shiftKey && event.key.toLowerCase() === "e") {
        event.preventDefault();
        patchPrefs({ showTree: !useWorkspace.getState().prefs.showTree });
      } else if (mod && event.key === "[") {
        event.preventDefault();
        useWorkspace.getState().back();
      } else if (mod && event.key === "]") {
        event.preventDefault();
        useWorkspace.getState().forward();
      } else if (mod && (event.key === "=" || event.key === "+")) {
        event.preventDefault();
        patchPrefs({ zoom: Math.min(1.6, useWorkspace.getState().prefs.zoom + 0.1) });
      } else if (mod && event.key === "-") {
        event.preventDefault();
        patchPrefs({ zoom: Math.max(0.8, useWorkspace.getState().prefs.zoom - 0.1) });
      } else if (mod && event.key === "0" && !event.shiftKey && !event.altKey) {
        event.preventDefault();
        patchPrefs({ zoom: 1 });
      } else if (mod && event.shiftKey && (event.key === "!" || event.key === "1")) {
        event.preventDefault();
        patchPrefs({ width: "standard" });
      } else if (mod && event.shiftKey && (event.key === "@" || event.key === "2")) {
        event.preventDefault();
        patchPrefs({ width: "wide" });
      } else if (mod && event.shiftKey && (event.key === ")" || event.key === "0")) {
        event.preventDefault();
        patchPrefs({ width: "full" });
      } else if (event.key === "?" && !typing && !mod) {
        setHelp(true);
      } else if (event.key === "Escape" && !typing) {
        useWorkspace.getState().selectHash(null);
      } else if (event.altKey && ["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight"].includes(event.key) && !typing) {
        event.preventDefault();
        moveAnchor(event.key);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [patchPrefs]);

  useEffect(() => {
    const onDrop = (event: DragEvent) => {
      const list = event.dataTransfer?.files;
      if (!list?.length) return;
      event.preventDefault();
      void readFiles(list);
    };
    const onDrag = (event: DragEvent) => {
      if (event.dataTransfer?.types?.includes("Files")) event.preventDefault();
    };
    window.addEventListener("drop", onDrop);
    window.addEventListener("dragover", onDrag);
    return () => {
      window.removeEventListener("drop", onDrop);
      window.removeEventListener("dragover", onDrag);
    };
  }, []);

  const kind = file && file.kind !== "folder" ? file.kind : null;
  const showTree = prefs.showTree;
  const showReview = prefs.showReview;

  return (
    <div
      className="desk"
      data-pane={prefs.mobilePane}
      data-tree={showTree ? "on" : "off"}
      data-review={showReview ? "on" : "off"}
      style={{ ["--zoom" as string]: String(prefs.zoom), ["--measure" as string]: measure(prefs.width) }}
    >
      <header className="topbar">
        <div className="brand">
          <span className="mark">P</span>
          <div>
            <strong>ProseMirror</strong>
            <small>workspace</small>
          </div>
        </div>
        <div className="history-buttons">
          <button type="button" title="Back" onClick={back} disabled={historyIndex <= 0}><ChevronLeft size={18} /></button>
          <button type="button" title="Forward" onClick={forward} disabled={historyIndex >= historyLength - 1}><ChevronRight size={18} /></button>
        </div>
        <h1 className="doc-title">{file?.name ?? "No file"}</h1>
        <div className="top-actions">
          <div className="segment" role="group" aria-label="Reading width">
            {(["standard", "wide", "full"] as WidthMode[]).map((width) => (
              <button key={width} type="button" className={prefs.width === width ? "is-on" : ""} onClick={() => patchPrefs({ width })}>
                {width === "standard" ? "S" : width === "wide" ? "W" : "F"}
              </button>
            ))}
          </div>
          <button type="button" title="Zoom out" onClick={() => patchPrefs({ zoom: Math.max(0.8, prefs.zoom - 0.1) })}><ZoomOut size={16} /></button>
          <button type="button" title="Zoom in" onClick={() => patchPrefs({ zoom: Math.min(1.6, prefs.zoom + 0.1) })}><ZoomIn size={16} /></button>
          <button type="button" title="Theme" onClick={() => patchPrefs({ theme: prefs.theme === "light" ? "dark" : prefs.theme === "dark" ? "system" : "light" })}>
            {prefs.theme === "dark" ? <Moon size={16} /> : <Sun size={16} />}
          </button>
          <button type="button" className={prefs.collab ? "is-on" : ""} title="Pair editors" onClick={() => patchPrefs({ collab: !prefs.collab })}>
            <Columns2 size={16} />
          </button>
          <button type="button" className={prefs.showModel ? "is-on" : ""} title="Document model" onClick={() => patchPrefs({ showModel: !prefs.showModel })}>
            <Braces size={16} />
          </button>
          <button type="button" title="Library" onClick={() => patchPrefs({ showTree: !prefs.showTree, mobilePane: "tree" })}><PanelLeft size={16} /></button>
          <button type="button" title="Review" onClick={() => patchPrefs({ showReview: !prefs.showReview, mobilePane: "review" })}><PanelRight size={16} /></button>
          <FileMenu booted={booted} />
          <button type="button" title="Shortcuts" onClick={() => setHelp(true)}><HelpCircle size={16} /></button>
        </div>
      </header>
      {kind === "markdown" ? (
        <div className="toolbar">
          <Tool icon={<Undo2 size={16} />} label="Undo" keys="Mod-z" command={() => editorBridge.current?.command(undo)} />
          <Tool icon={<Redo2 size={16} />} label="Redo" keys="Mod-y" command={() => editorBridge.current?.command(redo)} />
          <span className="sep" />
          <Tool icon={<Bold size={16} />} label="Bold" keys="Mod-b" on={marks.strong} command={() => editorBridge.current?.command(toggleStrong)} />
          <Tool icon={<Italic size={16} />} label="Italic" keys="Mod-i" on={marks.em} command={() => editorBridge.current?.command(toggleEm)} />
          <Tool icon={<Strikethrough size={16} />} label="Strike" keys="Mod-Shift-x" on={marks.strikethrough} command={() => editorBridge.current?.command(toggleStrike)} />
          <Tool icon={<Code size={16} />} label="Code" keys="Mod-`" on={marks.code} command={() => editorBridge.current?.command(toggleCode)} />
          <Tool icon={<Link2 size={16} />} label="Link" on={marks.link} command={() => setLinkOpen((open) => !open)} />
          <span className="sep" />
          <Tool icon={<Heading1 size={16} />} label="Heading 1" keys="Mod-Alt-1" command={() => editorBridge.current?.command(setHeading(1))} />
          <Tool icon={<Heading2 size={16} />} label="Heading 2" keys="Mod-Alt-2" command={() => editorBridge.current?.command(setHeading(2))} />
          <Tool icon={<Heading3 size={16} />} label="Heading 3" keys="Mod-Alt-3" command={() => editorBridge.current?.command(setHeading(3))} />
          <Tool icon={<span className="mini">P</span>} label="Paragraph" keys="Mod-Alt-0" command={() => editorBridge.current?.command(setParagraph)} />
          <span className="sep" />
          <Tool icon={<List size={16} />} label="Bullet list" keys="Mod-Shift-8" command={() => editorBridge.current?.command(toggleBullet)} />
          <Tool icon={<ListOrdered size={16} />} label="Numbered list" keys="Mod-Shift-7" command={() => editorBridge.current?.command(toggleOrdered)} />
          <Tool icon={<Quote size={16} />} label="Quote" keys="Mod-Shift-." command={() => editorBridge.current?.command(toggleQuote)} />
          <Tool icon={<Minus size={16} />} label="Rule" command={() => editorBridge.current?.command(insertRule)} />
          <Tool icon={<Table size={16} />} label="Table" command={() => editorBridge.current?.command(insertTable())} />
          <label className="tool" title="Insert local image">
            <ImageIcon size={16} />
            <input
              type="file"
              accept="image/png,image/jpeg,image/gif,image/webp,image/svg+xml,image/avif"
              onChange={(event) => {
                const picked = event.target.files?.[0];
                event.target.value = "";
                if (!picked) return;
                if (picked.size > 2_000_000) {
                  useWorkspace.getState().notify("Images must be under 2 MB");
                  return;
                }
                const reader = new FileReader();
                reader.onload = () => {
                  const src = String(reader.result || "");
                  editorBridge.current?.command(insertImage(src, picked.name));
                };
                reader.readAsDataURL(picked);
              }}
            />
          </label>
          <span className="sep" />
          <label className="find">
            <Search size={14} />
            <input
              value={query}
              placeholder="Find"
              aria-label="Find in document"
              onChange={(event) => setQuery(event.target.value)}
              onKeyDown={(event) => {
                if (event.key !== "Enter") return;
                const doc = editorBridge.current?.getDoc();
                if (!doc || !query.trim()) return;
                const start = event.shiftKey ? 0 : findFrom.current;
                const found = findNext(doc, query, start);
                if (!found) {
                  findFrom.current = 0;
                  useWorkspace.getState().notify("No match");
                  return;
                }
                findFrom.current = found.to;
                editorBridge.current?.focusRange(found.from - 1, found.to);
              }}
            />
          </label>
          <button type="button" className={prefs.showSource ? "is-on tool" : "tool"} onClick={() => {
            if (!prefs.showSource && file) setSource(file.body);
            patchPrefs({ showSource: !prefs.showSource });
          }}>
            Markdown
          </button>
        </div>
      ) : null}
      {linkOpen ? (
        <form
          className="link-form"
          onSubmit={(event) => {
            event.preventDefault();
            editorBridge.current?.command(applyLink(href));
            setLinkOpen(false);
          }}
        >
          <input value={href} onChange={(event) => setHref(event.target.value)} aria-label="Link URL" />
          <button type="submit">Apply</button>
          <button type="button" onClick={() => editorBridge.current?.command(removeLink)}>Remove</button>
        </form>
      ) : null}
      <div className="workspace">
        {showTree ? <FileTree /> : <div className="tree tree-collapsed" />}
        <main className="stage">
          {kind && file ? (
            <>
              {booted ? (
                <EditorPane fileId={file.id} kind={kind} collab={prefs.collab} allowRemote={prefs.allowRemote} />
              ) : (
                <pre className="fallback">{file.body}</pre>
              )}
              {prefs.showSource ? (
                <form
                  className="source"
                  onSubmit={(event) => {
                    event.preventDefault();
                    editorBridge.current?.replaceBody(source);
                    patchPrefs({ showSource: false });
                    useWorkspace.getState().notify("Markdown applied");
                  }}
                >
                  <textarea value={source} onChange={(event) => setSource(event.target.value)} aria-label="Markdown source" />
                  <div className="icon-row">
                    <button type="submit">Apply source</button>
                    <button type="button" onClick={() => patchPrefs({ showSource: false })}>Cancel</button>
                  </div>
                </form>
              ) : null}
              {prefs.showModel ? <ModelDrawer steps={steps} selected={selectedHash} /> : null}
            </>
          ) : (
            <div className="empty-stage">
              <h2>Drop a Markdown or text file</h2>
              <p>Or create one from the library. Nothing is uploaded.</p>
            </div>
          )}
        </main>
        {showReview ? <ReviewPane /> : null}
      </div>
      <footer className="statusbar">
        <span>{stats.words} words</span>
        <span>{stats.chars} characters</span>
        <span>size {stats.size}</span>
        <span>{selectedHash ? `anchor ${selectedHash.slice(0, 8)}` : "file comment"}</span>
        <span className="notice">{notice}</span>
        {!booted ? <span>Restoring library…</span> : null}
      </footer>
      <nav className="tabbar">
        <button type="button" className={prefs.mobilePane === "tree" ? "is-on" : ""} onClick={() => patchPrefs({ mobilePane: "tree", showTree: true })}>Library</button>
        <button type="button" className={prefs.mobilePane === "write" ? "is-on" : ""} onClick={() => patchPrefs({ mobilePane: "write" })}>Write</button>
        <button type="button" className={prefs.mobilePane === "review" ? "is-on" : ""} onClick={() => patchPrefs({ mobilePane: "review", showReview: true })}>Review</button>
      </nav>
      <HelpDialog open={help} onOpenChange={setHelp} />
    </div>
  );
}

function measure(width: WidthMode) {
  if (width === "wide") return "68rem";
  if (width === "full") return "100%";
  return "42rem";
}

function Tool({
  icon, label, command, on, keys,
}: {
  icon: ReactNode;
  label: string;
  command: () => void;
  on?: boolean;
  keys?: string;
}) {
  return (
    <button type="button" className={on ? "tool is-on" : "tool"} title={keys ? `${label} (${modLabel(keys)})` : label} onMouseDown={(event) => event.preventDefault()} onClick={command}>
      {icon}
    </button>
  );
}

function FileMenu({ booted }: { booted: boolean }) {
  const file = useWorkspace((state) => currentFile(state));
  const prefs = useWorkspace((state) => state.prefs);
  const patchPrefs = useWorkspace((state) => state.patchPrefs);
  const notify = useWorkspace((state) => state.notify);
  const resetLibrary = useWorkspace((state) => state.resetLibrary);
  const importText = useWorkspace((state) => state.importText);
  return (
    <DropdownMenu.Root>
      <DropdownMenu.Trigger className="tool" title="File">File</DropdownMenu.Trigger>
      <DropdownMenu.Portal>
        <DropdownMenu.Content className="menu" align="end">
          <DropdownMenu.Item className="menu-item" onSelect={() => file && downloadText(file.name, joinDocument(file.frontmatter, file.body), "text/markdown")}>
            Download {file?.kind === "plain" ? "text" : "Markdown"}
          </DropdownMenu.Item>
          <DropdownMenu.Item className="menu-item" onSelect={() => exportHtml()}>Download HTML</DropdownMenu.Item>
          <DropdownMenu.Item className="menu-item" onSelect={() => file && downloadText(`${file.name}.json`, `${JSON.stringify(editorBridge.current?.getJSON() ?? {}, null, 2)}\n`, "application/json")}>
            Download document JSON
          </DropdownMenu.Item>
          <DropdownMenu.Item className="menu-item" onSelect={() => {
            if (!file || file.kind === "folder") return;
            const anchors = anchorsFor(docFromBody(file.kind, file.body), file.kind);
            const markdown = exportReview(file.name, anchors, file.review, new Date().toISOString());
            downloadText(file.kind === "plain" ? `${file.name}.review.md` : file.name.replace(/\.(md|markdown)$/i, ".review.md"), markdown, "text/markdown");
          }}>
            Download review
          </DropdownMenu.Item>
          <DropdownMenu.Separator className="menu-sep" />
          <DropdownMenu.Item className="menu-item" onSelect={() => patchPrefs({ allowRemote: !prefs.allowRemote })}>
            {prefs.allowRemote ? "Block remote images" : "Allow remote images"}
          </DropdownMenu.Item>
          <DropdownMenu.Item className="menu-item" onSelect={() => resetLibrary()} disabled={!booted}>
            Restore sample library
          </DropdownMenu.Item>
          <label className="menu-item">
            Import file
            <input
              type="file"
              accept=".md,.markdown,.txt,text/plain,text/markdown"
              hidden
              onChange={(event) => {
                const picked = event.target.files?.[0];
                event.target.value = "";
                if (!picked) return;
                const reader = new FileReader();
                reader.onload = () => {
                  importText(picked.name, String(reader.result || ""));
                  notify(`Imported ${picked.name}`);
                };
                reader.readAsText(picked);
              }}
            />
          </label>
        </DropdownMenu.Content>
      </DropdownMenu.Portal>
    </DropdownMenu.Root>
  );
}

function exportHtml() {
  const file = currentFile(useWorkspace.getState());
  const doc = editorBridge.current?.getDoc();
  if (!file || !doc) return;
  const holder = document.createElement("div");
  holder.appendChild(DOMSerializer.fromSchema(doc.type.schema).serializeFragment(doc.content));
  downloadText(file.name.replace(/\.(md|markdown|txt)$/i, "") + ".html", standaloneHtml(file.name, holder.innerHTML), "text/html");
}

function ModelDrawer({ steps, selected }: { steps: { label: string; json: unknown }[]; selected: string | null }) {
  const json = editorBridge.current?.getJSON();
  const text = JSON.stringify(json ?? {}, null, 2);
  return (
    <section className="model">
      <header>
        <strong>Model</strong>
        <span>{selected ? `selected ${selected}` : "no anchor"}</span>
      </header>
      <div className="model-grid">
        <pre>{text.length > 8000 ? `${text.slice(0, 8000)}\n…` : text}</pre>
        <ol>
          {steps.length === 0 ? <li>Steps show up as you edit. Pair mode sends them to the other editor.</li> : null}
          {steps.map((step, index) => (
            <li key={index}>
              <strong>{step.label}</strong> {(step.json as { stepType?: string }).stepType ?? "step"}
              <code>{JSON.stringify(step.json)}</code>
            </li>
          ))}
        </ol>
      </div>
      <p className="schema-line">
        Nodes: {Object.keys(schema.nodes).join(", ")}. Marks: {Object.keys(schema.marks).join(", ")}.
      </p>
    </section>
  );
}

function HelpDialog({ open, onOpenChange }: { open: boolean; onOpenChange: (open: boolean) => void }) {
  const rows = [
    ["Mod-b / i / `", "Bold, italic, code"],
    ["Mod-Shift-x", "Strikethrough"],
    ["Mod-Alt-1/2/3", "Heading"],
    ["Mod-z / Mod-Shift-z", "Undo / redo"],
    ["Tab / Shift-Tab", "Indent a list item, or move between table cells"],
    ["Mod-Shift-e", "Toggle the library"],
    ["Mod-j or Mod-\\", "Toggle review"],
    ["Mod-[ / Mod-]", "Back / forward"],
    ["Mod-Shift-1 / 2 / 0", "Reading width"],
    ["Mod-+ / Mod-- / Mod-0", "Zoom"],
    ["Alt-arrows", "Move between commented anchors"],
    ["Esc", "Comment on the whole file"],
    ["Mod-Enter", "Save the comment"],
  ];
  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Portal>
        <Dialog.Overlay className="overlay" />
        <Dialog.Content className="dialog">
          <Dialog.Title>Shortcuts</Dialog.Title>
          <Dialog.Description>On Windows and Linux, Ctrl replaces ⌘.</Dialog.Description>
          <ul>
            {rows.map(([keys, action]) => (
              <li key={keys}><kbd>{modLabel(keys)}</kbd><span>{action}</span></li>
            ))}
          </ul>
          <Dialog.Close className="tool">Close</Dialog.Close>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

function moveAnchor(key: string) {
  const state = useWorkspace.getState();
  const file = currentFile(state);
  if (!file || file.kind === "folder") return;
  const anchors = anchorsFor(docFromBody(file.kind, file.body), file.kind);
  const commented = new Set(file.review.blocks.filter((block) => block.comments.length).map((block) => block.hash));
  const current = anchors.find((anchor) => anchor.hash === state.selectedHash) ?? null;
  let next: Anchor | null = null;
  if (key === "ArrowLeft") {
    next = current?.parent != null ? anchors[current.parent] ?? null : null;
  } else if (key === "ArrowRight") {
    next = anchors.find((anchor) => anchor.parent != null && anchors[anchor.parent]?.hash === current?.hash && commented.has(anchor.hash)) ?? null;
  } else {
    const siblings = anchors.filter((anchor) => (anchor.parent ?? null) === (current?.parent ?? null) && commented.has(anchor.hash));
    const pool = siblings.length ? siblings : anchors.filter((anchor) => commented.has(anchor.hash));
    const index = current ? pool.findIndex((anchor) => anchor.hash === current.hash) : -1;
    next = key === "ArrowDown" ? pool[index + 1] ?? pool[0] ?? null : pool[index - 1] ?? pool[pool.length - 1] ?? null;
    if (!current && key === "ArrowDown") next = pool[0] ?? null;
  }
  if (!next) return;
  state.selectHash(next.hash);
  editorBridge.current?.focusRange(next.from, next.to);
}

async function readFiles(list: FileList) {
  for (const file of list) {
    if (!/\.(md|markdown|txt)$/i.test(file.name)) continue;
    const text = await file.text();
    useWorkspace.getState().importText(file.name, text);
  }
  useWorkspace.getState().notify("Imported");
}
