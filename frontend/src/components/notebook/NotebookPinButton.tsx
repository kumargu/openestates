import { useNotebook } from "../../hooks/useNotebook.ts";
import type { NotebookLabelId, NotebookNoteKind } from "../../lib/notebook.ts";
import { NotebookSaveIcon } from "./NotebookSaveIcon.tsx";

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
      aria-label={filled ? "Remove from notebook" : "Save to notebook"}
      aria-pressed={filled}
      title={filled ? "Saved" : "Save"}
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
      <NotebookSaveIcon filled={filled} size={15} />
    </button>
  );
}
