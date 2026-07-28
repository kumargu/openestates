import { TAGS, propertyById, tagById, type PropertyId, type TagId } from "../data.ts";
import { useNotebook, type NotebookNote } from "../store.tsx";

function formatWhen(ts: number): string {
  const mins = Math.round((Date.now() - ts) / 60_000);
  if (mins < 2) return "Just now";
  if (mins < 60) return `${mins}m ago`;
  const hrs = Math.round(mins / 60);
  if (hrs < 24) return `Today · ${hrs}h ago`;
  return "Yesterday";
}

function TagPill({
  tag,
  onClick,
}: {
  tag: TagId;
  onClick?: () => void;
}) {
  const def = tagById(tag);
  return (
    <button
      type="button"
      className="tag-pill"
      style={{ background: def.color, color: def.ink }}
      onClick={onClick}
      title="Change tag"
    >
      {def.label}
    </button>
  );
}

export function NotebookPage({
  onOpenCompare,
  onOpenProperty,
  onOpenPlan,
}: {
  onOpenCompare: () => void;
  onOpenProperty: (id: PropertyId) => void;
  onOpenPlan: () => void;
}) {
  const {
    notes,
    propertyIds,
    compareIds,
    notebookView,
    setNotebookView,
    toggleCompare,
    addHandwritten,
    setNoteTag,
    removeNote,
    focusedId,
  } = useNotebook();

  const visible = notes.filter((n) => propertyIds.includes(n.propertyId));

  return (
    <div className="page page--notebook notion-page">
      <div className="notion-cover" aria-hidden />

      <header className="notion-title-block">
        <div className="notion-emoji" aria-hidden>
          📓
        </div>
        <h1>Home notebook</h1>
        <p className="notion-subtitle">
          {propertyIds.length} homes · {visible.length} notes · handwritten only here · tags drive
          Compare
        </p>
        <div className="oe-cross-links" style={{ marginTop: 12 }}>
          <button type="button" className="oe-cross-link" onClick={() => onOpenProperty(focusedId)}>
            Property
          </button>
          <button type="button" className="oe-cross-link" onClick={onOpenPlan}>
            Plan
          </button>
          {compareIds.length >= 2 && (
            <button
              type="button"
              className="oe-cross-link oe-cross-link--accent"
              onClick={onOpenCompare}
            >
              Compare {compareIds.length}
            </button>
          )}
        </div>
      </header>

      <div className="notion-toolbar">
        <div className="notion-views">
          {(
            [
              ["list", "List"],
              ["board", "By tag"],
              ["by-home", "By home"],
            ] as const
          ).map(([id, label]) => (
            <button
              key={id}
              type="button"
              className={`notion-view-btn${notebookView === id ? " is-active" : ""}`}
              onClick={() => setNotebookView(id)}
            >
              {label}
            </button>
          ))}
        </div>
        <div className="notion-toolbar__right">
          <span className="notion-tool">Properties</span>
          <span className="notion-tool">Filter</span>
          <span className="notion-tool">Sort</span>
          <button
            type="button"
            className="btn-compare"
            disabled={compareIds.length < 2}
            onClick={onOpenCompare}
          >
            Compare {compareIds.length >= 2 ? compareIds.length : ""}
          </button>
        </div>
      </div>

      <div className="compare-pick-row">
        <span>Select homes for compare</span>
        {propertyIds.map((id) => {
          const p = propertyById(id);
          const on = compareIds.includes(id);
          return (
            <button
              key={id}
              type="button"
              className={`select-pill${on ? " is-on" : ""}`}
              onClick={() => toggleCompare(id)}
            >
              {on ? "●" : "○"} {p.short}
            </button>
          );
        })}
      </div>

      {propertyIds.length === 0 ? (
        <div className="empty-notebook">
          <h2>Empty notebook</h2>
          <p>
            Save a home, pin a school theme, or write a note. UI selections and handwriting share
            one tagged stream — like Notion rows with properties.
          </p>
        </div>
      ) : notebookView === "board" ? (
        <BoardView notes={visible} onTag={setNoteTag} onRemove={removeNote} />
      ) : notebookView === "by-home" ? (
        <ByHomeView
          propertyIds={propertyIds}
          notes={visible}
          onTag={setNoteTag}
          onRemove={removeNote}
          onNote={(id, text) => addHandwritten(id, text)}
        />
      ) : (
        <ListView
          notes={visible}
          onTag={setNoteTag}
          onRemove={removeNote}
          onQuickAdd={(text) => addHandwritten(focusedId, text)}
        />
      )}
    </div>
  );
}

function ListView({
  notes,
  onTag,
  onRemove,
  onQuickAdd,
}: {
  notes: NotebookNote[];
  onTag: (id: string, tag: TagId) => void;
  onRemove: (id: string) => void;
  onQuickAdd: (text: string) => void;
}) {
  const sorted = [...notes].sort((a, b) => b.createdAt - a.createdAt);

  return (
    <div className="notion-list">
      <div className="notion-list__head">
        <span>Note</span>
        <span>Tag</span>
        <span>Home</span>
        <span>Updated</span>
      </div>
      {sorted.map((n) => (
        <NoteListRow key={n.id} note={n} onTag={onTag} onRemove={onRemove} />
      ))}
      <form
        className="notion-new-row"
        onSubmit={(e) => {
          e.preventDefault();
          const fd = new FormData(e.currentTarget);
          onQuickAdd(String(fd.get("note") ?? ""));
          e.currentTarget.reset();
        }}
      >
        <span className="notion-new-row__plus">+</span>
        <input
          name="note"
          placeholder="New note… try “down payment 1 Cr from PF” — auto-tags Down payment"
          autoComplete="off"
        />
      </form>
    </div>
  );
}

function NoteListRow({
  note,
  onTag,
  onRemove,
}: {
  note: NotebookNote;
  onTag: (id: string, tag: TagId) => void;
  onRemove: (id: string) => void;
}) {
  const p = propertyById(note.propertyId);
  const kindLabel =
    note.kind === "handwritten"
      ? "You"
      : note.selectionText
        ? "Selected text"
        : note.kind === "theme"
          ? "Theme"
          : note.kind === "plan"
            ? "Plan"
            : "Fact";

  return (
    <div className="notion-row">
      <div className="notion-row__title">
        <span className="notion-row__icon" aria-hidden>
          {note.kind === "handwritten" ? "✏️" : p.icon}
        </span>
        <div>
          <p className="notion-row__name">{note.label}</p>
          <p className="notion-row__meta">
            {kindLabel}
            {note.detail ? ` · ${note.detail}` : ""}
          </p>
        </div>
      </div>
      <div className="notion-row__tag">
        <TagPill tag={note.tag} />
        <select
          className="tag-select"
          aria-label="Change tag"
          value={note.tag}
          onChange={(e) => onTag(note.id, e.target.value as TagId)}
        >
          {TAGS.map((t) => (
            <option key={t.id} value={t.id}>
              {t.label}
            </option>
          ))}
        </select>
      </div>
      <div className="notion-row__home">{p.short}</div>
      <div className="notion-row__when">
        <span>{formatWhen(note.createdAt)}</span>
        <button type="button" className="note-row__remove" onClick={() => onRemove(note.id)}>
          ×
        </button>
      </div>
    </div>
  );
}

function BoardView({
  notes,
  onTag,
  onRemove,
}: {
  notes: NotebookNote[];
  onTag: (id: string, tag: TagId) => void;
  onRemove: (id: string) => void;
}) {
  const used = TAGS.filter((t) => notes.some((n) => n.tag === t.id));

  return (
    <div className="notion-board">
      {used.map((tag) => {
        const cards = notes.filter((n) => n.tag === tag.id);
        return (
          <section key={tag.id} className="notion-column">
            <header className="notion-column__head">
              <span className="tag-pill" style={{ background: tag.color, color: tag.ink }}>
                {tag.label}
              </span>
              <em>{cards.length}</em>
            </header>
            {cards.map((n) => {
              const p = propertyById(n.propertyId);
              return (
                <article key={n.id} className="notion-card">
                  <p className="notion-card__title">
                    <span aria-hidden>{n.kind === "handwritten" ? "✏️" : p.icon}</span> {n.label}
                  </p>
                  <div className="notion-card__foot">
                    <span className="notion-card__home">{p.short}</span>
                    <select
                      className="tag-select"
                      value={n.tag}
                      onChange={(e) => onTag(n.id, e.target.value as TagId)}
                      aria-label="Retag"
                    >
                      {TAGS.map((t) => (
                        <option key={t.id} value={t.id}>
                          {t.label}
                        </option>
                      ))}
                    </select>
                    <button type="button" className="note-row__remove" onClick={() => onRemove(n.id)}>
                      ×
                    </button>
                  </div>
                </article>
              );
            })}
            <p className="notion-column__hint">Compare uses this tag as a row</p>
          </section>
        );
      })}
    </div>
  );
}

function ByHomeView({
  propertyIds,
  notes,
  onTag,
  onRemove,
  onNote,
}: {
  propertyIds: PropertyId[];
  notes: NotebookNote[];
  onTag: (id: string, tag: TagId) => void;
  onRemove: (id: string) => void;
  onNote: (propertyId: PropertyId, text: string) => void;
}) {
  return (
    <div className="by-home">
      {propertyIds.map((id) => {
        const p = propertyById(id);
        const mine = notes.filter((n) => n.propertyId === id);
        return (
          <section key={id} className="by-home__section">
            <h2>
              <span aria-hidden>{p.icon}</span> {p.name}
            </h2>
            {mine.map((n) => (
              <div key={n.id} className="by-home__row">
                <p>{n.label}</p>
                <TagPill tag={n.tag} />
                <select
                  className="tag-select"
                  value={n.tag}
                  onChange={(e) => onTag(n.id, e.target.value as TagId)}
                >
                  {TAGS.map((t) => (
                    <option key={t.id} value={t.id}>
                      {t.label}
                    </option>
                  ))}
                </select>
                <button type="button" className="note-row__remove" onClick={() => onRemove(n.id)}>
                  ×
                </button>
              </div>
            ))}
            <form
              className="notion-new-row"
              onSubmit={(e) => {
                e.preventDefault();
                const fd = new FormData(e.currentTarget);
                onNote(id, String(fd.get("note") ?? ""));
                e.currentTarget.reset();
              }}
            >
              <span className="notion-new-row__plus">+</span>
              <input name="note" placeholder={`Note on ${p.short}…`} autoComplete="off" />
            </form>
          </section>
        );
      })}
    </div>
  );
}
