import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";
import { sampleFiles } from "./sample.ts";
import { splitFrontmatter } from "./editor/markdown.ts";
import { commentId, emptyReview, type AnchorKind, type ReviewDoc } from "./review/types.ts";
import {
  defaultPrefs,
  type LibraryFile,
  type Prefs,
  type StepRecord,
} from "./store-types.ts";

type Live = {
  words: number;
  chars: number;
  size: number;
  steps: StepRecord[];
  notice: string;
};

type State = {
  files: LibraryFile[];
  currentId: string;
  history: string[];
  historyIndex: number;
  expanded: Record<string, boolean>;
  prefs: Prefs;
  selectedHash: string | null;
  live: Live;
  open: (id: string) => void;
  back: () => void;
  forward: () => void;
  patchPrefs: (patch: Partial<Prefs>) => void;
  selectHash: (hash: string | null) => void;
  updateBody: (id: string, body: string) => void;
  setFrontmatter: (id: string, frontmatter: string) => void;
  noteSteps: (steps: StepRecord[]) => void;
  noteStats: (stats: Pick<Live, "words" | "chars" | "size">) => void;
  notify: (notice: string) => void;
  createFile: (parentId: string | null, kind: "markdown" | "plain") => void;
  createFolder: (parentId: string | null) => void;
  rename: (id: string, name: string) => void;
  duplicate: (id: string) => void;
  remove: (id: string) => void;
  toggleExpanded: (id: string) => void;
  importText: (name: string, text: string) => void;
  addComment: (hash: string, excerpt: string, kind: AnchorKind, text: string) => void;
  addFileComment: (text: string) => void;
  updateComment: (hash: string | null, commentId: string, text: string) => void;
  deleteComment: (hash: string | null, commentId: string) => void;
  reattach: (fromHash: string, anchor: { hash: string; excerpt: string; kind: AnchorKind }) => void;
  dropUnanchored: (hash: string) => void;
  resetLibrary: () => void;
};

const initialFiles = sampleFiles();

function stamp(file: LibraryFile, review: ReviewDoc): LibraryFile {
  return { ...file, review: { ...review, file: file.name }, updated: new Date().toISOString() };
}

function uniqueName(files: LibraryFile[], parentId: string | null, name: string) {
  const siblings = new Set(files.filter((file) => file.parentId === parentId).map((file) => file.name));
  if (!siblings.has(name)) return name;
  const dot = name.lastIndexOf(".");
  const stem = dot > 0 ? name.slice(0, dot) : name;
  const ext = dot > 0 ? name.slice(dot) : "";
  let n = 2;
  while (siblings.has(`${stem} ${n}${ext}`)) n += 1;
  return `${stem} ${n}${ext}`;
}

let noticeTimer = 0;

export const useWorkspace = create<State>()(
  persist(
    (set, get) => ({
      files: initialFiles,
      currentId: "file-welcome",
      history: ["file-welcome"],
      historyIndex: 0,
      expanded: { "folder-notes": true },
      prefs: defaultPrefs,
      selectedHash: null,
      live: { words: 0, chars: 0, size: 0, steps: [], notice: "" },
      open: (id) => {
        const file = get().files.find((item) => item.id === id);
        if (!file || file.kind === "folder") {
          if (file?.kind === "folder") {
            set((state) => ({ expanded: { ...state.expanded, [id]: !state.expanded[id] } }));
          }
          return;
        }
        set((state) => {
          const history = state.history.slice(0, state.historyIndex + 1);
          if (history[history.length - 1] !== id) history.push(id);
          return {
            currentId: id,
            history,
            historyIndex: history.length - 1,
            selectedHash: null,
            prefs: { ...state.prefs, mobilePane: "write", showSource: false },
          };
        });
      },
      back: () => {
        const { history, historyIndex } = get();
        if (historyIndex <= 0) return;
        const next = historyIndex - 1;
        set({ historyIndex: next, currentId: history[next], selectedHash: null });
      },
      forward: () => {
        const { history, historyIndex } = get();
        if (historyIndex >= history.length - 1) return;
        const next = historyIndex + 1;
        set({ historyIndex: next, currentId: history[next], selectedHash: null });
      },
      patchPrefs: (patch) => set((state) => ({ prefs: { ...state.prefs, ...patch } })),
      selectHash: (hash) => set((state) => (state.selectedHash === hash ? state : { selectedHash: hash })),
      updateBody: (id, body) =>
        set((state) => ({
          files: state.files.map((file) =>
            file.id === id && file.body !== body ? { ...file, body, updated: new Date().toISOString() } : file,
          ),
        })),
      setFrontmatter: (id, frontmatter) =>
        set((state) => ({
          files: state.files.map((file) => (file.id === id ? { ...file, frontmatter } : file)),
        })),
      noteSteps: (steps) =>
        set((state) => ({
          live: { ...state.live, steps: [...steps.reverse(), ...state.live.steps].slice(0, 16) },
        })),
      noteStats: (stats) => set((state) => ({ live: { ...state.live, ...stats } })),
      notify: (notice) => {
        set((state) => ({ live: { ...state.live, notice } }));
        window.clearTimeout(noticeTimer);
        noticeTimer = window.setTimeout(() => {
          set((state) => ({ live: { ...state.live, notice: "" } }));
        }, 2600);
      },
      createFile: (parentId, kind) => {
        const name = uniqueName(get().files, parentId, kind === "plain" ? "Untitled.txt" : "Untitled.md");
        const id = crypto.randomUUID();
        const file: LibraryFile = {
          id,
          parentId,
          name,
          kind,
          body: kind === "plain" ? "" : "# Untitled\n\n",
          frontmatter: "",
          review: emptyReview(name),
          created: new Date().toISOString(),
          updated: new Date().toISOString(),
        };
        set((state) => ({
          files: [...state.files, file],
          expanded: parentId ? { ...state.expanded, [parentId]: true } : state.expanded,
        }));
        get().open(id);
      },
      createFolder: (parentId) => {
        const name = uniqueName(get().files, parentId, "Folder");
        const id = crypto.randomUUID();
        const file: LibraryFile = {
          id,
          parentId,
          name,
          kind: "folder",
          body: "",
          frontmatter: "",
          review: emptyReview(name),
          created: new Date().toISOString(),
          updated: new Date().toISOString(),
        };
        set((state) => ({
          files: [...state.files, file],
          expanded: { ...state.expanded, ...(parentId ? { [parentId]: true } : {}), [id]: true },
        }));
      },
      rename: (id, name) => {
        const clean = name.trim();
        if (!clean || /[\\/]/.test(clean)) return;
        set((state) => ({
          files: state.files.map((file) => {
            if (file.id !== id) return file;
            const next = uniqueName(
              state.files.filter((item) => item.id !== id),
              file.parentId,
              clean,
            );
            return {
              ...file,
              name: next,
              review: { ...file.review, file: next },
              updated: new Date().toISOString(),
            };
          }),
        }));
      },
      duplicate: (id) => {
        const file = get().files.find((item) => item.id === id);
        if (!file || file.kind === "folder") return;
        const copy: LibraryFile = {
          ...structuredClone(file),
          id: crypto.randomUUID(),
          name: uniqueName(get().files, file.parentId, file.name),
          created: new Date().toISOString(),
          updated: new Date().toISOString(),
        };
        copy.review = { ...copy.review, file: copy.name };
        set((state) => ({ files: [...state.files, copy] }));
        get().open(copy.id);
      },
      remove: (id) => {
        const doomed = new Set<string>();
        const walk = (pid: string) => {
          doomed.add(pid);
          for (const child of get().files) if (child.parentId === pid) walk(child.id);
        };
        walk(id);
        set((state) => {
          const files = state.files.filter((file) => !doomed.has(file.id));
          let currentId = state.currentId;
          let history = state.history.filter((item) => !doomed.has(item));
          if (doomed.has(currentId)) {
            currentId = files.find((file) => file.kind !== "folder")?.id ?? currentId;
          }
          if (!history.length && currentId) history = [currentId];
          const historyIndex = Math.max(0, history.lastIndexOf(currentId));
          return { files, currentId, history, historyIndex, selectedHash: null };
        });
      },
      toggleExpanded: (id) =>
        set((state) => ({ expanded: { ...state.expanded, [id]: !state.expanded[id] } })),
      importText: (name, text) => {
        const kind = /\.txt$/i.test(name) ? "plain" : "markdown";
        const safe = name.replace(/[\\/]/g, "") || (kind === "plain" ? "Untitled.txt" : "Untitled.md");
        const id = crypto.randomUUID();
        const fileName = uniqueName(get().files, null, safe);
        const file: LibraryFile = {
          id,
          parentId: null,
          name: fileName,
          kind,
          body: text.replace(/\r\n/g, "\n"),
          frontmatter: "",
          review: emptyReview(fileName),
          created: new Date().toISOString(),
          updated: new Date().toISOString(),
        };
        if (kind === "markdown") {
          const split = splitFrontmatter(file.body);
          file.frontmatter = split.frontmatter;
          file.body = split.body;
        }
        set((state) => ({ files: [...state.files, file] }));
        get().open(id);
      },
      addComment: (hash, excerpt, kind, text) => {
        const trimmed = text.trim();
        if (!trimmed) return;
        const id = get().currentId;
        const comment = {
          id: commentId(),
          text: trimmed,
          created: new Date().toISOString(),
          updated: new Date().toISOString(),
        };
        set((state) => ({
          files: state.files.map((file) => {
            if (file.id !== id) return file;
            const blocks = file.review.blocks.map((block) => ({ ...block, comments: [...block.comments] }));
            const existing = blocks.find((block) => block.hash === hash);
            if (existing) existing.comments.push(comment);
            else blocks.push({ hash, excerpt, kind, comments: [comment] });
            return stamp(file, { ...file.review, blocks });
          }),
        }));
      },
      addFileComment: (text) => {
        const trimmed = text.trim();
        if (!trimmed) return;
        const id = get().currentId;
        const comment = {
          id: commentId(),
          text: trimmed,
          created: new Date().toISOString(),
          updated: new Date().toISOString(),
        };
        set((state) => ({
          files: state.files.map((file) =>
            file.id === id
              ? stamp(file, { ...file.review, file_comments: [...file.review.file_comments, comment] })
              : file,
          ),
        }));
      },
      updateComment: (hash, id, text) => {
        const trimmed = text.trim();
        if (!trimmed) return;
        const fileId = get().currentId;
        set((state) => ({
          files: state.files.map((file) => {
            if (file.id !== fileId) return file;
            const touch = (comment: (typeof file.review.file_comments)[number]) =>
              comment.id === id ? { ...comment, text: trimmed, updated: new Date().toISOString() } : comment;
            if (hash == null) {
              return stamp(file, { ...file.review, file_comments: file.review.file_comments.map(touch) });
            }
            return stamp(file, {
              ...file.review,
              blocks: file.review.blocks.map((block) =>
                block.hash === hash ? { ...block, comments: block.comments.map(touch) } : block,
              ),
            });
          }),
        }));
      },
      deleteComment: (hash, id) => {
        const fileId = get().currentId;
        set((state) => ({
          files: state.files.map((file) => {
            if (file.id !== fileId) return file;
            if (hash == null) {
              return stamp(file, {
                ...file.review,
                file_comments: file.review.file_comments.filter((comment) => comment.id !== id),
              });
            }
            const blocks = file.review.blocks
              .map((block) =>
                block.hash === hash
                  ? { ...block, comments: block.comments.filter((comment) => comment.id !== id) }
                  : block,
              )
              .filter((block) => block.comments.length > 0);
            return stamp(file, { ...file.review, blocks });
          }),
        }));
      },
      reattach: (fromHash, anchor) => {
        const fileId = get().currentId;
        set((state) => ({
          files: state.files.map((file) => {
            if (file.id !== fileId) return file;
            const source = file.review.blocks.find((block) => block.hash === fromHash);
            if (!source) return file;
            const blocks = file.review.blocks.filter((block) => block.hash !== fromHash);
            const target = blocks.find((block) => block.hash === anchor.hash);
            if (target) target.comments = [...target.comments, ...source.comments];
            else {
              blocks.push({
                hash: anchor.hash,
                excerpt: anchor.excerpt,
                kind: anchor.kind,
                comments: source.comments,
              });
            }
            return stamp(file, { ...file.review, blocks });
          }),
          selectedHash: anchor.hash,
        }));
      },
      dropUnanchored: (hash) => {
        const fileId = get().currentId;
        set((state) => ({
          files: state.files.map((file) =>
            file.id === fileId
              ? stamp(file, {
                  ...file.review,
                  blocks: file.review.blocks.filter((block) => block.hash !== hash),
                })
              : file,
          ),
        }));
      },
      resetLibrary: () =>
        set({
          files: sampleFiles(),
          currentId: "file-welcome",
          history: ["file-welcome"],
          historyIndex: 0,
          expanded: { "folder-notes": true },
          selectedHash: null,
          live: { words: 0, chars: 0, size: 0, steps: [], notice: "Sample library restored" },
        }),
    }),
    {
      name: "prosemirror-workspace-v1",
      skipHydration: true,
      partialize: (state) => ({
        files: state.files,
        currentId: state.currentId,
        history: state.history,
        historyIndex: state.historyIndex,
        expanded: state.expanded,
        prefs: state.prefs,
      }),
      merge: (persisted, current) => {
        const saved = persisted as Partial<State> | undefined;
        if (!saved?.files?.length) return current;
        return {
          ...current,
          files: saved.files,
          currentId: saved.currentId || current.currentId,
          history: saved.history?.length ? saved.history : [saved.currentId || current.currentId],
          historyIndex: saved.historyIndex ?? 0,
          expanded: saved.expanded ?? {},
          prefs: { ...defaultPrefs, ...saved.prefs },
        };
      },
      storage: createJSONStorage(() => ({
        getItem: (name) => {
          try {
            return localStorage.getItem(name);
          } catch {
            return null;
          }
        },
        setItem: (name, value) => {
          pendingKey = name;
          pendingWrite = value;
          window.clearTimeout(writeTimer);
          writeTimer = window.setTimeout(flushPendingStorage, 250);
        },
        removeItem: (name) => {
          try {
            localStorage.removeItem(name);
          } catch {
            // Storage can be disabled.
          }
        },
      })),
    },
  ),
);

let pendingWrite: string | null = null;
let pendingKey: string | null = null;
let writeTimer = 0;

function flushPendingStorage() {
  if (pendingWrite == null || pendingKey == null) return;
  window.clearTimeout(writeTimer);
  writeTimer = 0;
  try {
    localStorage.setItem(pendingKey, pendingWrite);
    pendingWrite = null;
  } catch {
    // Quota exceeded or storage blocked. Leave the pending value for a later flush.
  }
}

if (typeof window !== "undefined") {
  window.addEventListener("pagehide", flushPendingStorage);
  window.addEventListener("beforeunload", flushPendingStorage);
}

export function currentFile(state: Pick<State, "files" | "currentId">) {
  return state.files.find((file) => file.id === state.currentId) ?? null;
}
