import { useMemo, useRef, useState } from "react";
import { Copy, Download, Trash2 } from "lucide-react";
import { anchorsFor } from "@/lib/editor/anchors";
import { docFromBody } from "@/lib/editor/markdown";
import { editorBridge } from "@/lib/editor/mount";
import { exportPath, exportReview, sidecarPath, unanchored } from "@/lib/review/export";
import type { Anchor, Comment } from "@/lib/review/types";
import { currentFile, useWorkspace } from "@/lib/store";
import { downloadText } from "@/lib/download";

export function ReviewPane() {
  const file = useWorkspace((state) => currentFile(state));
  const selectedHash = useWorkspace((state) => state.selectedHash);
  const selectHash = useWorkspace((state) => state.selectHash);
  const addComment = useWorkspace((state) => state.addComment);
  const addFileComment = useWorkspace((state) => state.addFileComment);
  const updateComment = useWorkspace((state) => state.updateComment);
  const deleteComment = useWorkspace((state) => state.deleteComment);
  const reattach = useWorkspace((state) => state.reattach);
  const dropUnanchored = useWorkspace((state) => state.dropUnanchored);
  const notify = useWorkspace((state) => state.notify);
  const [draft, setDraft] = useState("");
  const [editing, setEditing] = useState<string | null>(null);
  const editingHash = useRef<string | null>(null);

  const anchors = useMemo(() => {
    if (!file || file.kind === "folder") return [] as Anchor[];
    return anchorsFor(docFromBody(file.kind, file.body), file.kind);
  }, [file]);

  if (!file || file.kind === "folder") {
    return <aside className="review"><p className="empty-copy">Open a file to review it.</p></aside>;
  }

  const open = file;
  const selected = anchors.find((anchor) => anchor.hash === selectedHash) ?? null;
  const lost = unanchored(open.review, anchors);
  const activeComments = selected
    ? open.review.blocks.find((block) => block.hash === selected.hash)?.comments ?? []
    : open.review.file_comments;

  function save() {
    if (!draft.trim()) return;
    if (editing) {
      updateComment(editingHash.current, editing, draft);
      setEditing(null);
    } else if (selected) {
      addComment(selected.hash, selected.excerpt, selected.kind, draft);
    } else {
      addFileComment(draft);
    }
    setDraft("");
  }

  const exportReviewFile = () => {
    if (open.kind === "folder") return;
    const markdown = exportReview(open.name, anchors, open.review, new Date().toISOString());
    downloadText(exportPath(open.name, open.kind), markdown, "text/markdown");
    notify("Review exported");
  };

  return (
    <aside className="review">
      <div className="review-head">
        <strong>Review</strong>
        <div className="icon-row">
          <button
            type="button"
            title="Copy review Markdown"
            onClick={async () => {
              const markdown = exportReview(open.name, anchors, open.review, new Date().toISOString());
              try {
                await navigator.clipboard.writeText(markdown);
                notify("Review copied");
              } catch {
                notify("Could not copy");
              }
            }}
          >
            <Copy size={16} />
          </button>
          <button type="button" title="Download review Markdown" onClick={exportReviewFile}>
            <Download size={16} />
          </button>
          <button
            type="button"
            title="Download review sidecar"
            onClick={() => {
              downloadText(sidecarPath(open.name), `${JSON.stringify(open.review, null, 2)}\n`, "application/json");
              notify("Sidecar downloaded");
            }}
          >
            <Download size={16} />
          </button>
        </div>
      </div>
      <button type="button" className={selected ? "crumb" : "crumb is-on"} onClick={() => selectHash(null)}>
        File
      </button>
      {selected ? <p className="anchor-excerpt">{selected.excerpt || "Empty block"}</p> : <p className="anchor-excerpt">Comments on the whole file</p>}
      <ul className="comments">
        {activeComments.map((comment) => (
          <CommentCard
            key={comment.id}
            comment={comment}
            onEdit={() => {
              editingHash.current = selected ? selected.hash : null;
              setEditing(comment.id);
              setDraft(comment.text);
            }}
            onDelete={() => deleteComment(selected ? selected.hash : null, comment.id)}
          />
        ))}
      </ul>
      <label className="composer">
        <span>{editing ? "Edit comment" : "New comment"}</span>
        <textarea
          value={draft}
          onChange={(event) => setDraft(event.target.value)}
          placeholder="Write a note. It never goes into the document."
          onKeyDown={(event) => {
            if ((event.metaKey || event.ctrlKey) && event.key === "Enter") {
              event.preventDefault();
              save();
            }
          }}
        />
        <button type="button" onClick={save}>{editing ? "Update" : "Save"}</button>
      </label>
      <h3>In the document</h3>
      <ul className="anchor-index">
        {anchors.filter((anchor) => open.review.blocks.some((block) => block.hash === anchor.hash && block.comments.length)).map((anchor) => (
          <li key={`${anchor.kind}-${anchor.from}`}>
            <button
              type="button"
              className={anchor.hash === selectedHash ? "is-on" : ""}
              onClick={() => {
                selectHash(anchor.hash);
                editorBridge.current?.focusRange(anchor.from, anchor.to);
              }}
            >
              <small>{anchor.kind} · L{anchor.lineStart}</small>
              {anchor.excerpt}
            </button>
          </li>
        ))}
      </ul>
      {lost.length ? (
        <>
          <h3>Unanchored</h3>
          <ul className="anchor-index">
            {lost.map((hash) => {
              const block = open.review.blocks.find((item) => item.hash === hash);
              if (!block) return null;
              return (
                <li key={hash}>
                  <p>{block.excerpt || "Missing block"}</p>
                  <div className="icon-row">
                    <button
                      type="button"
                      onClick={() => {
                        if (!selected) {
                          notify("Select a block first");
                          return;
                        }
                        reattach(hash, { hash: selected.hash, excerpt: selected.excerpt, kind: selected.kind });
                      }}
                    >
                      Attach to selection
                    </button>
                    <button type="button" onClick={() => dropUnanchored(hash)}>
                      <Trash2 size={14} />
                    </button>
                  </div>
                </li>
              );
            })}
          </ul>
        </>
      ) : null}
    </aside>
  );
}

function CommentCard({
  comment,
  onEdit,
  onDelete,
}: {
  comment: Comment;
  onEdit: () => void;
  onDelete: () => void;
}) {
  return (
    <li>
      <p>{comment.text}</p>
      <div className="comment-meta">
        <time dateTime={comment.updated}>{comment.updated.slice(0, 16).replace("T", " ")}</time>
        <button type="button" onClick={onEdit}>Edit</button>
        <button type="button" onClick={onDelete}>Delete</button>
      </div>
    </li>
  );
}
