import { useEffect, useMemo, useState } from "react";
import { Helmet } from "react-helmet-async";
import { Link } from "react-router-dom";
import { useNotebook } from "../hooks/useNotebook.ts";
import { getProperties } from "../lib/api.ts";
import {
  ASSIGNABLE_NOTEBOOK_LABELS,
  labelDef,
  type NotebookLabelId,
  type NotebookNote,
} from "../lib/notebook.ts";
import type { PropertyCard } from "../lib/types.ts";
import "../styles/notebook.css";

function societyLabel(home: PropertyCard | undefined, id: string): string {
  if (!home) return id.slice(0, 12);
  return home.society_name?.trim() || home.title;
}

function cssLabel(id: string): string {
  if (id.startsWith("hospitals")) return "hospitals";
  if (id.startsWith("schools")) return "schools";
  if (id.startsWith("metro") || id === "tech_parks" || id === "commute") return "commute";
  if (id === "transmission" || id === "risk") return "risk";
  if (id === "open-space") return "open-space";
  if (id === "down-payment") return "down-payment";
  return id;
}

function noteIcon(note: NotebookNote): string {
  const labels = note.labels.join(" ");
  if (labels.includes("hospital")) return "🏥";
  if (labels.includes("school")) return "🎓";
  if (labels.includes("metro") || labels.includes("tech")) return "🚇";
  if (labels.includes("risk") || labels.includes("transmission")) return "⚡";
  if (labels.includes("water")) return "💧";
  if (labels.includes("approach")) return "🛤️";
  if (labels.includes("community")) return "💬";
  if (labels.includes("legal")) return "📋";
  if (labels.includes("price") || labels.includes("emi")) return "₹";
  if (note.kind === "handwritten") return "✏️";
  return "📌";
}

function LabelPicker({
  note,
  onAdd,
  onRemove,
}: {
  note: NotebookNote;
  onAdd: (id: NotebookLabelId) => void;
  onRemove: (id: NotebookLabelId) => void;
}) {
  const [open, setOpen] = useState(false);
  const available = ASSIGNABLE_NOTEBOOK_LABELS.filter((id) => !note.labels.includes(id));

  return (
    <div className="notion-row__tags">
      {note.labels.map((id) => (
        <button
          key={id}
          type="button"
          className={`notion-pill notion-pill--${cssLabel(id)}`}
          title="Remove label"
          onClick={() => onRemove(id)}
        >
          {labelDef(id).title}
        </button>
      ))}
      <div className="notion-tag-menu">
        <button
          type="button"
          className="notion-pill-add"
          aria-haspopup="listbox"
          aria-expanded={open}
          onClick={() => setOpen((v) => !v)}
        >
          {note.labels.length === 0 ? "Add label" : "+"}
        </button>
        {open && (
          <div className="notion-tag-menu__list" role="listbox">
            {available.map((id) => (
              <button
                key={id}
                type="button"
                role="option"
                onClick={() => {
                  onAdd(id);
                  setOpen(false);
                }}
              >
                {labelDef(id).title}
              </button>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

export function NotebookPage() {
  const {
    notes,
    propertyIds,
    compareIds,
    toggleCompare,
    addHandwritten,
    addNoteLabel,
    removeNoteLabel,
    removeNote,
  } = useNotebook();
  const [homes, setHomes] = useState<PropertyCard[]>([]);

  useEffect(() => {
    const controller = new AbortController();
    getProperties({ signal: controller.signal })
      .then(setHomes)
      .catch((error: unknown) => {
        if (error instanceof DOMException && error.name === "AbortError") return;
      });
    return () => controller.abort();
  }, []);

  const byId = useMemo(
    () => new Map(homes.map((home) => [home.id, home])),
    [homes],
  );

  const visible = useMemo(
    () => [...notes]
      .filter((n) => propertyIds.includes(n.propertyId))
      .sort((a, b) => b.createdAt - a.createdAt),
    [notes, propertyIds],
  );

  function quickAdd(propertyId: string, text: string, labels: NotebookLabelId[] = []) {
    if (!propertyId || !text.trim()) return;
    addHandwritten({ propertyId, text, labels });
  }

  return (
    <div className="notion-page">
      <Helmet>
        <title>Notebook | OpenEstates</title>
        <meta name="robots" content="noindex" />
      </Helmet>

      <div className="notion-cover" aria-hidden="true" />

      <header className="notion-title-block">
        <div className="notion-emoji" aria-hidden="true">📓</div>
        <h1>Home notebook</h1>
        <p className="notion-subtitle">
          {propertyIds.length} home{propertyIds.length === 1 ? "" : "s"}
          {" · "}
          {visible.length} note{visible.length === 1 ? "" : "s"}
        </p>
      </header>

      {propertyIds.length === 0 ? (
        <div className="notion-empty">
          <h2>Empty notebook</h2>
          <p>Hover a place on the map and tap the bookmark — or write a line below once you pin a home.</p>
          <Link to="/">Discover homes</Link>
        </div>
      ) : (
        <EditorialView
          propertyIds={propertyIds}
          notes={visible}
          homes={byId}
          compareIds={compareIds}
          onToggleCompare={toggleCompare}
          onAddLabel={addNoteLabel}
          onRemoveLabel={removeNoteLabel}
          onRemove={removeNote}
          onQuickAdd={(propertyId, text) => quickAdd(propertyId, text)}
        />
      )}
    </div>
  );
}

function EditorialView({
  propertyIds,
  notes,
  homes,
  compareIds,
  onToggleCompare,
  onAddLabel,
  onRemoveLabel,
  onRemove,
  onQuickAdd,
}: {
  propertyIds: string[];
  notes: NotebookNote[];
  homes: Map<string, PropertyCard>;
  compareIds: string[];
  onToggleCompare: (propertyId: string) => void;
  onAddLabel: (id: string, label: NotebookLabelId) => void;
  onRemoveLabel: (id: string, label: NotebookLabelId) => void;
  onRemove: (id: string) => void;
  onQuickAdd: (propertyId: string, text: string) => void;
}) {
  return (
    <article className="notion-editorial" aria-label="Home notebook document">
      {propertyIds.map((propertyId, index) => {
        const homeNotes = notes.filter((n) => n.propertyId === propertyId);
        const home = homes.get(propertyId);
        const inCompare = compareIds.includes(propertyId);
        return (
          <section key={propertyId} className="notion-entry">
            <header className="notion-entry__heading">
              <span className="notion-entry__number" aria-hidden="true">
                {String(index + 1).padStart(2, "0")}
              </span>
              <div className="notion-entry__title">
                <Link to={`/property/${encodeURIComponent(propertyId)}`}>
                  {societyLabel(homes.get(propertyId), propertyId)}
                </Link>
                <span>
                  {home?.area ? `${home.area} · ` : ""}
                  {homeNotes.length} note{homeNotes.length === 1 ? "" : "s"}
                </span>
              </div>
              <label className="notion-entry__compare">
                <input
                  type="checkbox"
                  checked={inCompare}
                  onChange={() => onToggleCompare(propertyId)}
                />
                <span>Compare</span>
              </label>
            </header>

            <div className="notion-entry__paper">
              {homeNotes.map((note) => (
                <div
                  key={note.id}
                  className={`notion-note${Date.now() - note.createdAt < 2_000 ? " is-fresh" : ""}`}
                >
                  <span className="notion-note__bullet" aria-hidden="true">
                    {noteIcon(note)}
                  </span>
                  <div className="notion-note__content">
                    <p>{note.title}</p>
                    {note.detail && <small>{note.detail}</small>}
                  </div>
                  <div className="notion-note__labels">
                    <LabelPicker
                      note={note}
                      onAdd={(label) => onAddLabel(note.id, label)}
                      onRemove={(label) => onRemoveLabel(note.id, label)}
                    />
                  </div>
                  <button
                    type="button"
                    className="notion-note__remove"
                    onClick={() => onRemove(note.id)}
                    aria-label="Remove"
                  >
                    ×
                  </button>
                </div>
              ))}
              <WritingBlock
                placeholder={`Continue writing about ${societyLabel(homes.get(propertyId), propertyId)}…`}
                onSubmit={(text) => onQuickAdd(propertyId, text)}
              />
            </div>
          </section>
        );
      })}
    </article>
  );
}

function WritingBlock({
  placeholder,
  onSubmit,
}: {
  placeholder: string;
  onSubmit: (text: string) => void;
}) {
  const [draft, setDraft] = useState("");
  return (
    <form
      className="notion-writing"
      onSubmit={(event) => {
        event.preventDefault();
        if (!draft.trim()) return;
        onSubmit(draft);
        setDraft("");
      }}
    >
      <textarea
        value={draft}
        onChange={(event) => setDraft(event.target.value)}
        placeholder={placeholder}
        aria-label={placeholder}
        rows={2}
      />
      {draft.trim() && (
        <button type="submit">Done</button>
      )}
    </form>
  );
}
