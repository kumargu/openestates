import { useEffect, useMemo, useState } from "react";
import { Helmet } from "react-helmet-async";
import { Link, useLocation, useSearchParams } from "react-router-dom";
import { useNotebook } from "../hooks/useNotebook.ts";
import { SocietyComparisonMatrix } from "../components/compare/SocietyComparisonMatrix.tsx";
import { getProperties, getProperty } from "../lib/api.ts";
import {
  ASSIGNABLE_NOTEBOOK_LABELS,
  labelDef,
  type NotebookLabelId,
  type NotebookNote,
} from "../lib/notebook.ts";
import type { PropertyCard, PropertyDetailResponse } from "../lib/types.ts";
import "../styles/notebook.css";

const MAX_WORKSPACE_COMPARE_HOMES = 4;

type WorkspaceMode = "notes" | "compare";
type CompareStatus = "idle" | "loading" | "ready" | "error";

type CompareState = {
  key: string;
  status: CompareStatus;
  details: PropertyDetailResponse[];
};

function parseComparedIds(value: string | null): string[] {
  if (!value) return [];
  return [...new Set(value.split(",").map((id) => id.trim()).filter(Boolean))]
    .slice(0, MAX_WORKSPACE_COMPARE_HOMES);
}

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

function workspaceMode(pathname: string): WorkspaceMode {
  return pathname === "/workspace/compare" ? "compare" : "notes";
}

function workspaceCompareHref(ids: string[], focusId?: string): string {
  if (ids.length < 2) return "/workspace/compare";
  const params = new URLSearchParams();
  params.set("ids", ids.slice(0, MAX_WORKSPACE_COMPARE_HOMES).join(","));
  if (focusId) params.set("focus", focusId);
  return `/workspace/compare?${params.toString()}`;
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

export function WorkspacePage() {
  const location = useLocation();
  const [searchParams] = useSearchParams();
  const mode = workspaceMode(location.pathname);
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
  const [compareState, setCompareState] = useState<CompareState>({
    key: "",
    status: "idle",
    details: [],
  });
  const [copied, setCopied] = useState(false);

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
  const requestedCompareIds = useMemo(
    () => parseComparedIds(searchParams.get("ids")),
    [searchParams],
  );
  const activeCompareIds = useMemo(
    () => requestedCompareIds.length > 0
      ? requestedCompareIds
      : compareIds.slice(0, MAX_WORKSPACE_COMPARE_HOMES),
    [compareIds, requestedCompareIds],
  );
  const selectedHomes = useMemo(
    () => activeCompareIds
      .map((id) => byId.get(id))
      .filter((home): home is PropertyCard => Boolean(home)),
    [activeCompareIds, byId],
  );
  const compareKey = selectedHomes.map((home) => home.id).join(",");
  const compareHref = workspaceCompareHref(
    activeCompareIds,
    searchParams.get("focus") ?? selectedHomes[0]?.id,
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

  useEffect(() => {
    if (mode !== "compare") return undefined;
    if (selectedHomes.length < 2) {
      setCompareState({ key: compareKey, status: "idle", details: [] });
      return undefined;
    }

    const controller = new AbortController();
    setCompareState({ key: compareKey, status: "loading", details: [] });
    Promise.allSettled(
      selectedHomes.map((home) => getProperty(home.id, { signal: controller.signal })),
    )
      .then((results) => {
        if (controller.signal.aborted) return;
        const details = results
          .filter((result): result is PromiseFulfilledResult<PropertyDetailResponse> =>
            result.status === "fulfilled"
          )
          .map((result) => result.value);
        setCompareState({ key: compareKey, status: "ready", details });
      })
      .catch((error: unknown) => {
        if (error instanceof DOMException && error.name === "AbortError") return;
        setCompareState({ key: compareKey, status: "error", details: [] });
      });

    return () => controller.abort();
  }, [compareKey, mode, selectedHomes]);

  function copyComparisonLink() {
    const href = workspaceCompareHref(activeCompareIds, searchParams.get("focus") ?? selectedHomes[0]?.id);
    void navigator.clipboard.writeText(`${window.location.origin}${href}`)
      .then(() => setCopied(true));
  }

  return (
    <div className="notion-page workspace-document">
      <Helmet>
        <title>Workspace | OpenEstates</title>
        <meta name="robots" content="noindex" />
      </Helmet>

      <div className="notion-cover" aria-hidden="true" />

      <header className="notion-title-block">
        <div className="notion-emoji" aria-hidden="true">▦</div>
        <h1>Workspace</h1>
        <p className="notion-subtitle">
          {propertyIds.length} home{propertyIds.length === 1 ? "" : "s"}
          {" · "}
          {visible.length} note{visible.length === 1 ? "" : "s"}
        </p>
      </header>

      <nav className="workspace-mode-tabs" aria-label="Workspace views">
        <Link
          to="/workspace"
          className={mode === "notes" ? "is-active" : undefined}
          aria-current={mode === "notes" ? "page" : undefined}
        >
          Notes
        </Link>
        <Link
          to={compareHref}
          className={mode === "compare" ? "is-active" : undefined}
          aria-current={mode === "compare" ? "page" : undefined}
        >
          Compare
          {activeCompareIds.length > 0 && <span>{activeCompareIds.length}</span>}
        </Link>
      </nav>

      {propertyIds.length === 0 ? (
        <div className="notion-empty">
          <h2>Empty workspace</h2>
          <p>Save a home or add a note from a property page to start your decision workspace.</p>
          <Link to="/">Discover homes</Link>
        </div>
      ) : mode === "compare" ? (
        <CompareWorkspaceView
          selectedHomes={selectedHomes}
          catalog={homes}
          details={compareState.key === compareKey ? compareState.details : []}
          status={compareState.key === compareKey ? compareState.status : "loading"}
          copied={copied}
          onCopy={copyComparisonLink}
        />
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

function CompareWorkspaceView({
  selectedHomes,
  catalog,
  details,
  status,
  copied,
  onCopy,
}: {
  selectedHomes: PropertyCard[];
  catalog: PropertyCard[];
  details: PropertyDetailResponse[];
  status: CompareStatus;
  copied: boolean;
  onCopy: () => void;
}) {
  if (selectedHomes.length < 2) {
    return (
      <section className="workspace-compare-empty">
        <span>Compare</span>
        <h2>Add one more home to compare.</h2>
        <p>Use the compare toggle beside saved homes in Notes. The workspace keeps the same notes and labels when you switch views.</p>
        <Link to="/workspace">Back to notes</Link>
      </section>
    );
  }

  if (status === "error") {
    return (
      <section className="workspace-compare-empty">
        <span>Compare</span>
        <h2>Comparison is unavailable.</h2>
        <p>Property details could not be loaded for this comparison.</p>
        <Link to="/workspace">Back to notes</Link>
      </section>
    );
  }

  return (
    <section className="workspace-compare-view" aria-label="Compare saved homes">
      <header className="workspace-compare-view__head">
        <div>
          <span>Side by side</span>
          <h2>Same shortlist. Sharper tradeoffs.</h2>
          <p>Compare uses the saved labels that stay decision-worthy.</p>
        </div>
        <button type="button" onClick={onCopy}>
          {copied ? "Link copied" : "Share"}
        </button>
      </header>

      {status === "loading" ? (
        <div className="workspace-compare-loading" aria-label="Loading comparison">
          <div />
          <div />
        </div>
      ) : (
        <SocietyComparisonMatrix
          selectedHomes={selectedHomes}
          catalog={catalog}
          details={details}
        />
      )}
    </section>
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
