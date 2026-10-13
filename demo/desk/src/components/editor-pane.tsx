import { memo, useEffect, useRef, useState } from "react";
import { editorBridge, mountEditor } from "@/lib/editor/mount";
import { currentFile, useWorkspace } from "@/lib/store";
import type { FileKind } from "@/lib/review/types";

type Props = {
  fileId: string;
  kind: FileKind;
  collab: boolean;
  allowRemote: boolean;
};

export const EditorPane = memo(function EditorPane({ fileId, kind, collab, allowRemote }: Props) {
  const host = useRef<HTMLDivElement>(null);
  const peer = useRef<HTMLDivElement>(null);
  const [ready, setReady] = useState(false);
  const fallback = useWorkspace((state) => state.files.find((file) => file.id === fileId)?.body ?? "");

  useEffect(() => {
    const node = host.current;
    if (!node) return;
    const file = currentFile(useWorkspace.getState());
    if (!file || file.id !== fileId) return;
    let handle: ReturnType<typeof mountEditor> | null = null;
    let cancelled = false;
    setReady(false);
    handle = mountEditor({
      host: node,
      peer: collab ? peer.current : null,
      kind,
      body: file.body,
      comments: file.review.blocks.filter((block) => block.comments.length).map((block) => block.hash),
      selected: useWorkspace.getState().selectedHash,
      onBody: (body) => useWorkspace.getState().updateBody(fileId, body),
      onSelect: (hash) => useWorkspace.getState().selectHash(hash),
      onSteps: (steps) => useWorkspace.getState().noteSteps(steps),
      onStats: (stats) => useWorkspace.getState().noteStats(stats),
    });
    if (cancelled) {
      handle.destroy();
      return;
    }
    editorBridge.current = handle;
    setReady(true);
    const unsub = useWorkspace.subscribe((state, prev) => {
      const next = state.files.find((item) => item.id === fileId);
      const previous = prev.files.find((item) => item.id === fileId);
      const comments = next?.review.blocks.filter((block) => block.comments.length).map((block) => block.hash) ?? [];
      const prevComments =
        previous?.review.blocks.filter((block) => block.comments.length).map((block) => block.hash) ?? [];
      if (state.selectedHash !== prev.selectedHash || comments.join("|") !== prevComments.join("|")) {
        handle?.setAnchorState(comments, state.selectedHash);
      }
    });
    return () => {
      cancelled = true;
      unsub();
      handle?.destroy();
      if (editorBridge.current === handle) editorBridge.current = null;
    };
  }, [fileId, kind, collab, allowRemote]);

  return (
    <div className={collab ? "pair" : "single"}>
      <section className="sheet-wrap">
        {collab ? <p className="pair-label">Editor A</p> : null}
        {!ready ? <pre className="fallback" hidden>{fallback || "Empty document"}</pre> : null}
        <div ref={host} className="sheet" />
      </section>
      {collab ? (
        <section className="sheet-wrap">
          <p className="pair-label">Editor B · steps arrive after 200ms</p>
          <div ref={peer} className="sheet" />
        </section>
      ) : null}
    </div>
  );
});
