import { useEffect, useRef, useState } from "react";
import { useNotebook } from "../../hooks/useNotebook.ts";
import type { NotebookLabelId } from "../../lib/notebook.ts";
import { NotebookSaveIcon } from "./NotebookSaveIcon.tsx";

type NotebookCommentAnchorProps = {
  propertyId: string;
  labels: NotebookLabelId[];
  detail: string;
  source: string;
  label?: string;
  className?: string;
  onOpen?: () => void;
};

export function NotebookCommentAnchor({
  propertyId,
  labels,
  detail,
  source,
  label,
  className = "",
  onOpen,
}: NotebookCommentAnchorProps) {
  const { addHandwritten } = useNotebook();
  const [open, setOpen] = useState(false);
  const [draft, setDraft] = useState("");
  const rootRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);

  function close() {
    setOpen(false);
    triggerRef.current?.focus({ preventScroll: true });
  }

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
    close();
  }

  return (
    <div
      ref={rootRef}
      className={`notebook-comment-anchor ${className}`.trim()}
      onKeyDown={event => {
        if (open && event.key === "Escape") {
          event.preventDefault();
          event.stopPropagation();
          close();
        }
      }}
    >
      <button
        ref={triggerRef}
        type="button"
        className="notebook-comment-anchor__button"
        aria-label="Add note"
        aria-expanded={open}
        title="Add note"
        onClick={(event) => {
          event.preventDefault();
          event.stopPropagation();
          if (!open) onOpen?.();
          setOpen((current) => !current);
        }}
      >
        <NotebookSaveIcon size={16} />
        {label && <span>{label}</span>}
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
            <button type="button" onClick={close}>
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
