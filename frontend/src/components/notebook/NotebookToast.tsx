import { useEffect, useRef, useState } from "react";
import {
  NOTEBOOK_CHANGED_EVENT,
  readNotebook,
  removeNotebookNote,
  type NotebookNote,
  type NotebookState,
} from "../../lib/notebook.ts";

const TOAST_DURATION_MS = 4_500;

export function NotebookToast() {
  const previous = useRef<NotebookState>(readNotebook());
  const [saved, setSaved] = useState<NotebookNote | null>(null);

  useEffect(() => {
    function handleChange(event: Event) {
      const next = (event as CustomEvent<NotebookState>).detail ?? readNotebook();
      const previousIds = new Set(previous.current.notes.map((note) => note.id));
      const added = next.notes.find((note) => !previousIds.has(note.id));
      previous.current = next;
      if (added) setSaved(added);
    }

    window.addEventListener(NOTEBOOK_CHANGED_EVENT, handleChange);
    return () => window.removeEventListener(NOTEBOOK_CHANGED_EVENT, handleChange);
  }, []);

  useEffect(() => {
    if (!saved) return;
    const timeout = window.setTimeout(() => setSaved(null), TOAST_DURATION_MS);
    return () => window.clearTimeout(timeout);
  }, [saved]);

  if (!saved) return null;

  return (
    <div className="notebook-toast" role="status" aria-live="polite">
      <span>Saved to notebook</span>
      <button
        type="button"
        onClick={() => {
          removeNotebookNote(saved.id);
          setSaved(null);
        }}
      >
        Undo
      </button>
    </div>
  );
}
