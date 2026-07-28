import type { NoteMark } from "../data.ts";
import { markGlyph } from "../data.ts";

type NotebookIconProps = {
  filled: boolean;
  onClick: () => void;
  label: string;
  size?: "sm" | "md";
  flying?: boolean;
};

export function NotebookIcon({
  filled,
  onClick,
  label,
  size = "md",
  flying = false,
}: NotebookIconProps) {
  return (
    <button
      type="button"
      className={`nb-icon nb-icon--${size}${filled ? " is-filled" : ""}${flying ? " is-flying" : ""}`}
      onClick={(e) => {
        e.stopPropagation();
        onClick();
      }}
      aria-label={label}
      title={filled ? "Remove from notebook" : "Add to notebook"}
    >
      <svg viewBox="0 0 24 24" aria-hidden="true">
        <path
          d="M7 3.5h8.5A2.5 2.5 0 0 1 18 6v14.2l-5.2-2.6L7.5 20.2V6A2.5 2.5 0 0 1 10 3.5"
          fill={filled ? "currentColor" : "none"}
          stroke="currentColor"
          strokeWidth="1.7"
          strokeLinejoin="round"
        />
      </svg>
    </button>
  );
}

export function NoteRow({
  mark,
  label,
  meta,
  onRemove,
  compact = false,
}: {
  mark: NoteMark;
  label: string;
  meta?: string;
  onRemove?: () => void;
  compact?: boolean;
}) {
  return (
    <div className={`note-row${compact ? " note-row--compact" : ""}`}>
      <span className={`note-row__mark note-row__mark--${mark}`} aria-hidden>
        {markGlyph(mark)}
      </span>
      <div className="note-row__body">
        <p className="note-row__label">{label}</p>
        {meta && <p className="note-row__meta">{meta}</p>}
      </div>
      {onRemove && (
        <button type="button" className="note-row__remove" onClick={onRemove} aria-label="Remove">
          ×
        </button>
      )}
    </div>
  );
}

export function InlineNoteComposer({
  onSubmit,
  placeholder = "Add a note…",
}: {
  onSubmit: (text: string) => void;
  placeholder?: string;
}) {
  return (
    <form
      className="note-composer"
      onSubmit={(e) => {
        e.preventDefault();
        const fd = new FormData(e.currentTarget);
        const text = String(fd.get("note") ?? "");
        onSubmit(text);
        e.currentTarget.reset();
      }}
    >
      <input name="note" className="note-composer__input" placeholder={placeholder} autoComplete="off" />
      <button type="submit" className="note-composer__go">
        Save
      </button>
    </form>
  );
}
