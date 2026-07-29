import { useEffect, useRef, useState } from "react";
import { useNotebook } from "../../hooks/useNotebook.ts";
import type { NotebookLabelId } from "../../lib/notebook.ts";

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
        <svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true">
          <path d="M7 7.5h10M7 11h7" />
          <path d="M5.8 4.5h12.4A2.8 2.8 0 0 1 21 7.3v6.4a2.8 2.8 0 0 1-2.8 2.8H13l-4.7 3.2v-3.2H5.8A2.8 2.8 0 0 1 3 13.7V7.3a2.8 2.8 0 0 1 2.8-2.8Z" />
        </svg>
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
