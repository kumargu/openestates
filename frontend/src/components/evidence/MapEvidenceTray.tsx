import { useEffect, useRef, useState } from "react";
import { useNotebook } from "../../hooks/useNotebook.ts";
import type { NotebookLabelId } from "../../lib/notebook.ts";
import { NotebookSaveIcon } from "../notebook/NotebookSaveIcon.tsx";

export type MapEvidenceSelection = {
  id: string;
  catalogKey: string;
  title: string;
  layerLabel: string;
  meta: string[];
  summary?: string;
  sourceType: string;
  sourceUrl?: string;
  labels: NotebookLabelId[];
};

type MapEvidenceTrayProps = {
  propertyId: string;
  selection: MapEvidenceSelection;
  onClose: () => void;
};

export function MapEvidenceTray({
  propertyId,
  selection,
  onClose,
}: MapEvidenceTrayProps) {
  const { addContextual } = useNotebook();
  const [composerOpen, setComposerOpen] = useState(false);
  const [draft, setDraft] = useState("");
  const inputRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    if (composerOpen) inputRef.current?.focus();
  }, [composerOpen]);

  function submit() {
    const text = draft.trim();
    if (!text) return;
    addContextual({
      propertyId,
      catalogKey: selection.catalogKey,
      title: selection.title,
      text,
      labels: selection.labels,
      detail: [selection.layerLabel, ...selection.meta].join(" · "),
      source: selection.sourceType,
    });
    setComposerOpen(false);
    setDraft("");
  }

  return (
    <aside className="nearby-map-selection" aria-label="Selected map evidence">
      <button
        type="button"
        className="nearby-map-selection__close"
        aria-label="Close selected evidence"
        onClick={onClose}
      >
        ×
      </button>
      <div className="nearby-map-selection__copy">
        <strong>{selection.title}</strong>
        <span>{[selection.layerLabel, ...selection.meta].join(" · ")}</span>
        {selection.summary && <p>{selection.summary}</p>}
        {selection.sourceUrl ? (
          <a href={selection.sourceUrl} target="_blank" rel="noreferrer">
            Source
          </a>
        ) : selection.sourceType ? (
          <span className="nearby-map-selection__source">{selection.sourceType}</span>
        ) : null}
      </div>
      {composerOpen ? (
        <form
          className="nearby-map-selection__composer"
          onSubmit={(event) => {
            event.preventDefault();
            submit();
          }}
        >
          <textarea
            ref={inputRef}
            rows={2}
            value={draft}
            placeholder="What do you want to remember?"
            aria-label={`Note about ${selection.title}`}
            onChange={(event) => setDraft(event.target.value)}
          />
          <div>
            <button type="button" onClick={() => setComposerOpen(false)}>
              Cancel
            </button>
            <button type="submit" disabled={!draft.trim()}>
              Add note
            </button>
          </div>
        </form>
      ) : (
        <button
          type="button"
          className="nearby-map-selection__note"
          aria-label="Add note"
          title="Add note"
          onClick={() => setComposerOpen(true)}
        >
          <NotebookSaveIcon size={16} />
          <span>Add note</span>
        </button>
      )}
    </aside>
  );
}
