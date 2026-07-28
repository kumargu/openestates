import { useNotebook } from "../../hooks/useNotebook.ts";
import type { NotebookLabelId, NotebookNoteKind } from "../../lib/notebook.ts";

type NotebookPinButtonProps = {
  propertyId: string;
  catalogKey: string;
  title: string;
  labels: NotebookLabelId[];
  detail?: string;
  source?: string;
  kind?: NotebookNoteKind;
  className?: string;
};

/** Quiet hover comment anchor. Labels never show on Property, only on Notebook/Compare. */
export function NotebookPinButton({
  propertyId,
  catalogKey,
  title,
  labels,
  detail,
  source,
  kind = "fact",
  className = "",
}: NotebookPinButtonProps) {
  const { isPinned, toggleFact } = useNotebook();
  const filled = isPinned(catalogKey);

  return (
    <button
      type="button"
      className={`notebook-pin${filled ? " is-filled" : ""} ${className}`.trim()}
      aria-label={filled ? "Remove note" : "Add note"}
      aria-pressed={filled}
      title={filled ? "Remove note" : "Add note"}
      onClick={(event) => {
        event.preventDefault();
        event.stopPropagation();
        toggleFact({
          propertyId,
          catalogKey,
          title,
          labels,
          detail,
          source,
          kind,
        });
      }}
    >
      <svg viewBox="0 0 24 24" width="15" height="15" aria-hidden="true">
        <path
          d="M6.2 5.4h11.6A2.7 2.7 0 0 1 20.5 8v5.8a2.7 2.7 0 0 1-2.7 2.7H13l-4.4 3.1v-3.1H6.2a2.7 2.7 0 0 1-2.7-2.7V8a2.7 2.7 0 0 1 2.7-2.6Z"
          fill={filled ? "currentColor" : "none"}
          stroke="currentColor"
          strokeWidth="1.7"
          strokeLinejoin="round"
        />
      </svg>
    </button>
  );
}
