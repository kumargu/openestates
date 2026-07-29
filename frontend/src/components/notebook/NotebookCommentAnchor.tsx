import { useEffect, useRef, useState } from "react";
import { useNotebook } from "../../hooks/useNotebook.ts";
import type { NotebookLabelId } from "../../lib/notebook.ts";
import { NotebookSaveIcon } from "./NotebookSaveIcon.tsx";

type NotebookCommentAnchorProps = {
  propertyId: string;
  labels: NotebookLabelId[];
  detail: string;
  source: string;
  className?: string;
};

export function NotebookCommentAnchor({
  propertyId,
  labels,
  detail,
  source,
  className = "",
}: NotebookCommentAnchorProps) {
  const { addHandwritten } = useNotebook();
  const [open, setOpen] = useState(false);
  const [draft, setDraft] = useState("");
  const rootRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    if (!open) return undefined;
    inputRef.current?.focus();

    function closeOnOutsidePointer(event: PointerEvent) {
      if (!rootRef.current?.contains(event.target as Node)) setOpen(false);
    }

    document.addEventListener("pointerdown", closeOnOutsidePointer);
    return () => document.removeEventListener("pointerdown", closeOnOutsidePointer);
  }, [open]);

  function submit() {
    const text = draft.trim();
    if (!text) return;
    addHandwritten({
      propertyId,
      text,
      labels,
      detail,
      source,
    });
    setDraft("");
    setOpen(false);
  }

  return (
    <div
      ref={rootRef}
      className={`notebook-comment-anchor ${className}`.trim()}
    >
      <button
        type="button"
        className="notebook-comment-anchor__button"
        aria-label="Add note"
        aria-expanded={open}
        title="Add note"
        onClick={(event) => {
          event.preventDefault();
          event.stopPropagation();
          setOpen((current) => !current);
        }}
      >
        <NotebookSaveIcon size={16} />
      </button>
      {open && (
        <form
          className="notebook-comment-anchor__popover"
          onSubmit={(event) => {
            event.preventDefault();
            submit();
          }}
        >
          <textarea
            ref={inputRef}
            value={draft}
            rows={3}
            onChange={(event) => setDraft(event.target.value)}
            placeholder="Add note"
            aria-label="Note"
          />
          <div>
            <button type="button" onClick={() => setOpen(false)}>
              Cancel
            </button>
            <button type="submit" disabled={!draft.trim()}>
              Add note
            </button>
          </div>
        </form>
      )}
    </div>
  );
}
