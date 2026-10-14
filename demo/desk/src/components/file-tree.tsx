import { useState } from "react";
import { FilePlus, FileText, Folder, FolderOpen, FolderPlus, Trash2 } from "lucide-react";
import { useWorkspace } from "@/lib/store";
import type { LibraryFile } from "@/lib/store-types";

export function FileTree() {
  const files = useWorkspace((state) => state.files);
  const currentId = useWorkspace((state) => state.currentId);
  const expanded = useWorkspace((state) => state.expanded);
  const open = useWorkspace((state) => state.open);
  const createFile = useWorkspace((state) => state.createFile);
  const createFolder = useWorkspace((state) => state.createFolder);
  const [query, setQuery] = useState("");
  const roots = files.filter((file) => file.parentId == null);
  const visible = query.trim()
    ? files.filter((file) => file.kind !== "folder" && file.name.toLowerCase().includes(query.trim().toLowerCase()))
    : roots;

  return (
    <aside className="tree">
      <div className="tree-head">
        <strong>Library</strong>
        <div className="icon-row">
          <button type="button" title="New Markdown file" onClick={() => createFile(null, "markdown")}>
            <FilePlus size={16} />
          </button>
          <button type="button" title="New plain text file" onClick={() => createFile(null, "plain")}>
            <FileText size={16} />
          </button>
          <button type="button" title="New folder" onClick={() => createFolder(null)}>
            <FolderPlus size={16} />
          </button>
        </div>
      </div>
      <input
        className="tree-search"
        value={query}
        onChange={(event) => setQuery(event.target.value)}
        placeholder="Filter files"
        aria-label="Filter files"
      />
      <ul className="tree-list">
        {visible.map((file) => (
          <TreeNode key={file.id} file={file} depth={0} currentId={currentId} expanded={expanded} searching={!!query.trim()} open={open} />
        ))}
      </ul>
    </aside>
  );
}

function TreeNode({
  file,
  depth,
  currentId,
  expanded,
  searching,
  open,
}: {
  file: LibraryFile;
  depth: number;
  currentId: string;
  expanded: Record<string, boolean>;
  searching: boolean;
  open: (id: string) => void;
}) {
  const files = useWorkspace((state) => state.files);
  const rename = useWorkspace((state) => state.rename);
  const remove = useWorkspace((state) => state.remove);
  const duplicate = useWorkspace((state) => state.duplicate);
  const createFile = useWorkspace((state) => state.createFile);
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(file.name);
  const [confirm, setConfirm] = useState(false);
  const children = files.filter((item) => item.parentId === file.id);
  const isOpen = expanded[file.id] !== false;
  const count = file.kind === "folder" ? 0 : file.review.blocks.reduce((n, block) => n + block.comments.length, 0) + file.review.file_comments.length;

  return (
    <li>
      <div className={file.id === currentId ? "tree-row is-current" : "tree-row"} style={{ paddingLeft: `${depth * 12 + 8}px` }}>
        {editing ? (
          <input
            className="rename"
            value={draft}
            autoFocus
            aria-label="Rename"
            onChange={(event) => setDraft(event.target.value)}
            onBlur={() => {
              rename(file.id, draft);
              setEditing(false);
            }}
            onKeyDown={(event) => {
              if (event.key === "Enter") (event.target as HTMLInputElement).blur();
              if (event.key === "Escape") {
                setDraft(file.name);
                setEditing(false);
              }
            }}
          />
        ) : (
          <button type="button" className="tree-name" onClick={() => open(file.id)} onDoubleClick={() => setEditing(true)}>
            {file.kind === "folder" ? (
              isOpen ? <FolderOpen size={15} /> : <Folder size={15} />
            ) : (
              <FileText size={15} />
            )}
            <span>{file.name}</span>
            {count > 0 ? <em>{count}</em> : null}
          </button>
        )}
        {confirm ? (
          <span className="confirm-row">
            <button type="button" onClick={() => remove(file.id)}>Delete</button>
            <button type="button" onClick={() => setConfirm(false)}>Keep</button>
          </span>
        ) : (
          <span className="row-actions">
            {file.kind === "folder" ? (
              <button type="button" title="New file in folder" onClick={() => createFile(file.id, "markdown")}>
                <FilePlus size={14} />
              </button>
            ) : (
              <button type="button" title="Duplicate" onClick={() => duplicate(file.id)}>
                <FilePlus size={14} />
              </button>
            )}
            <button type="button" title="Delete" onClick={() => setConfirm(true)}>
              <Trash2 size={14} />
            </button>
          </span>
        )}
      </div>
      {file.kind === "folder" && isOpen && !searching && children.length ? (
        <ul className="tree-list">
          {children.map((child) => (
            <TreeNode key={child.id} file={child} depth={depth + 1} currentId={currentId} expanded={expanded} searching={false} open={open} />
          ))}
        </ul>
      ) : null}
    </li>
  );
}
