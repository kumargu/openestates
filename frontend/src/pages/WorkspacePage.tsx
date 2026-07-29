import { useEffect, useId, useMemo, useRef, useState, type KeyboardEvent } from "react";
import { Helmet } from "react-helmet-async";
import { Link, useLocation, useSearchParams } from "react-router-dom";
import { useNotebook } from "../hooks/useNotebook.ts";
import { SocietyComparisonMatrix } from "../components/compare/SocietyComparisonMatrix.tsx";
import { getProperties, getProperty } from "../lib/api.ts";
import {
  matchingNotebookCommands,
  slashQuery,
  type NotebookCommand,
} from "../lib/notebookCommands.ts";
import {
  ASSIGNABLE_NOTEBOOK_LABELS,
  labelDef,
  type NotebookBlock,
  type NotebookChecklistItem,
  type NotebookFieldItem,
  type NotebookLabelId,
  type NotebookNote,
} from "../lib/notebook.ts";
import { LabelVisualIcon } from "../lib/LabelVisualIcon.tsx";
import { labelClassToken } from "../lib/labelVisuals.ts";
import type { PropertyCard, PropertyDetailResponse } from "../lib/types.ts";
import "../styles/notebook.css";

const MAX_WORKSPACE_COMPARE_HOMES = 4;
const NOTEBOOK_COMPOSER_PLACEHOLDER = "Write a note, /visit, /budget, /payment";

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
  return labelClassToken(id);
}

function noteIcon(note: NotebookNote) {
  return <LabelVisualIcon id={note.labels[0] ?? (note.kind === "handwritten" ? "visit" : "other")} size={22} />;
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

function propertyIdsWithNotesFirst(propertyIds: string[], notes: NotebookNote[]): string[] {
  const latestByProperty = new Map<string, number>();
  for (const note of notes) {
    latestByProperty.set(
      note.propertyId,
      Math.max(latestByProperty.get(note.propertyId) ?? 0, note.createdAt),
    );
  }
  const originalIndex = new Map(propertyIds.map((id, index) => [id, index]));
  return [...propertyIds].sort((a, b) => {
    const aLatest = latestByProperty.get(a) ?? 0;
    const bLatest = latestByProperty.get(b) ?? 0;
    if (aLatest && bLatest) return bLatest - aLatest;
    if (aLatest) return -1;
    if (bLatest) return 1;
    return (originalIndex.get(a) ?? 0) - (originalIndex.get(b) ?? 0);
  });
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
          <LabelVisualIcon id={id} size={18} />
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
                <LabelVisualIcon id={id} size={18} />
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
  const [searchParams, setSearchParams] = useSearchParams();
  const mode = workspaceMode(location.pathname);
  const {
    notes,
    propertyIds,
    compareIds,
    toggleCompare,
    addHandwritten,
    addCommandBlock,
    updateNote,
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
      .sort((a, b) => a.createdAt - b.createdAt),
    [notes, propertyIds],
  );
  const orderedPropertyIds = useMemo(
    () => propertyIdsWithNotesFirst(propertyIds, visible),
    [propertyIds, visible],
  );

  function quickAdd(propertyId: string, text: string, labels: NotebookLabelId[] = []) {
    if (!propertyId || !text.trim()) return;
    addHandwritten({ propertyId, text, labels });
  }

  useEffect(() => {
    if (mode !== "compare") return undefined;
    if (selectedHomes.length < 2) return undefined;

    const controller = new AbortController();
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

  function removeCompareHomes(propertyIdsToRemove: string[]) {
    const removeSet = new Set(propertyIdsToRemove);
    const nextIds = activeCompareIds.filter((id) => !removeSet.has(id));
    for (const id of propertyIdsToRemove) {
      if (compareIds.includes(id)) toggleCompare(id);
    }

    const next = new URLSearchParams(searchParams);
    if (nextIds.length > 0) {
      next.set("ids", nextIds.join(","));
    } else {
      next.delete("ids");
    }
    if (!nextIds.includes(next.get("focus") ?? "")) {
      if (nextIds[0]) next.set("focus", nextIds[0]);
      else next.delete("focus");
    }
    setSearchParams(next, { replace: true });
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
          onRemoveHome={removeCompareHomes}
          onRemoveNoteLabel={removeNoteLabel}
        />
      ) : (
        <EditorialView
          propertyIds={orderedPropertyIds}
          notes={visible}
          homes={byId}
          compareIds={compareIds}
          onToggleCompare={toggleCompare}
          onAddLabel={addNoteLabel}
          onRemoveLabel={removeNoteLabel}
          onRemove={removeNote}
          onUpdate={updateNote}
          onQuickAdd={(propertyId, text) => quickAdd(propertyId, text)}
          onCommand={(propertyId, commandId) => addCommandBlock({ propertyId, commandId })}
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
  onRemoveHome,
  onRemoveNoteLabel,
}: {
  selectedHomes: PropertyCard[];
  catalog: PropertyCard[];
  details: PropertyDetailResponse[];
  status: CompareStatus;
  copied: boolean;
  onCopy: () => void;
  onRemoveHome: (propertyIds: string[]) => void;
  onRemoveNoteLabel: (noteId: string, label: NotebookLabelId) => void;
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
          onRemoveColumn={onRemoveHome}
          onRemoveNoteLabel={onRemoveNoteLabel}
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
  onUpdate,
  onQuickAdd,
  onCommand,
}: {
  propertyIds: string[];
  notes: NotebookNote[];
  homes: Map<string, PropertyCard>;
  compareIds: string[];
  onToggleCompare: (propertyId: string) => void;
  onAddLabel: (id: string, label: NotebookLabelId) => void;
  onRemoveLabel: (id: string, label: NotebookLabelId) => void;
  onRemove: (id: string) => void;
  onUpdate: (id: string, patch: Partial<Pick<NotebookNote, "title" | "block">>) => void;
  onQuickAdd: (propertyId: string, text: string) => void;
  onCommand: (propertyId: string, commandId: NotebookCommand["id"]) => void;
}) {
  const notedPropertyIds = propertyIds.filter((propertyId) =>
    notes.some((note) => note.propertyId === propertyId),
  );
  const savedOnlyPropertyIds = propertyIds.filter((propertyId) =>
    !notes.some((note) => note.propertyId === propertyId),
  );

  return (
    <article className="notion-editorial" aria-label="Home notebook document">
      {notedPropertyIds.map((propertyId, index) => {
        const homeNotes = notes.filter((n) => n.propertyId === propertyId);
        const home = homes.get(propertyId);
        const inCompare = compareIds.includes(propertyId);
        return (
          <section key={propertyId} className="notion-entry">
            <header className="notion-entry__heading">
              <span className="notion-entry__number" aria-hidden="true">
                {String(index + 1).padStart(2, "0")}
              </span>
              <CompareCheckbox
                checked={inCompare}
                label={`Include ${societyLabel(homes.get(propertyId), propertyId)} in compare`}
                onChange={() => onToggleCompare(propertyId)}
              />
              <div className="notion-entry__title">
                <Link to={`/property/${encodeURIComponent(propertyId)}`}>
                  {societyLabel(homes.get(propertyId), propertyId)}
                </Link>
                <span>
                  {home?.area ? `${home.area} · ` : ""}
                  {homeNotes.length} note{homeNotes.length === 1 ? "" : "s"}
                </span>
              </div>
            </header>

            <div className="notion-entry__paper">
              {homeNotes.map((note) => (
                <NotebookNoteRow
                  key={note.id}
                  note={note}
                  onAddLabel={onAddLabel}
                  onRemoveLabel={onRemoveLabel}
                  onRemove={onRemove}
                  onUpdate={onUpdate}
                />
              ))}
              <NotebookComposer
                ariaLabel={`Continue writing about ${societyLabel(homes.get(propertyId), propertyId)}`}
                onSubmit={(text) => onQuickAdd(propertyId, text)}
                onCommand={(command) => onCommand(propertyId, command.id)}
              />
            </div>
          </section>
        );
      })}
      {savedOnlyPropertyIds.length > 0 && (
        <section className="notion-saved-stack" aria-label="Saved homes without notes">
          <header>
            <span>Saved without notes</span>
            <p>Blank notebook spaces for homes you have saved.</p>
          </header>
          <div className="notion-saved-stack__list">
            {savedOnlyPropertyIds.map((propertyId) => {
              const home = homes.get(propertyId);
              const inCompare = compareIds.includes(propertyId);
              return (
                <SavedHomeRow
                  key={propertyId}
                  propertyId={propertyId}
                  title={societyLabel(home, propertyId)}
                  area={home?.area}
                  inCompare={inCompare}
                  onToggleCompare={() => onToggleCompare(propertyId)}
                  onQuickAdd={(text) => onQuickAdd(propertyId, text)}
                  onCommand={(command) => onCommand(propertyId, command.id)}
                />
              );
            })}
          </div>
        </section>
      )}
    </article>
  );
}

function SavedHomeRow({
  propertyId,
  title,
  area,
  inCompare,
  onToggleCompare,
  onQuickAdd,
  onCommand,
}: {
  propertyId: string;
  title: string;
  area?: string;
  inCompare: boolean;
  onToggleCompare: () => void;
  onQuickAdd: (text: string) => void;
  onCommand: (command: NotebookCommand) => void;
}) {
  return (
    <div className="notion-saved-home">
      <div className="notion-saved-home__heading">
        <CompareCheckbox
          checked={inCompare}
          label={`Include ${title} in compare`}
          onChange={onToggleCompare}
        />
        <div>
          <Link to={`/property/${encodeURIComponent(propertyId)}`}>{title}</Link>
          {area && <span>{area}</span>}
        </div>
      </div>
      <div className="notion-saved-home__writer">
        <NotebookComposer
          ariaLabel={`Start writing about ${title}`}
          onSubmit={onQuickAdd}
          onCommand={onCommand}
        />
      </div>
    </div>
  );
}

function CompareCheckbox({
  checked,
  label,
  onChange,
}: {
  checked: boolean;
  label: string;
  onChange: () => void;
}) {
  return (
    <label className="notion-compare-check" title={checked ? "In compare" : "Add to compare"}>
      <input
        type="checkbox"
        checked={checked}
        onChange={onChange}
      />
      <span className="sr-only">{label}</span>
    </label>
  );
}

function NotebookNoteRow({
  note,
  onAddLabel,
  onRemoveLabel,
  onRemove,
  onUpdate,
}: {
  note: NotebookNote;
  onAddLabel: (id: string, label: NotebookLabelId) => void;
  onRemoveLabel: (id: string, label: NotebookLabelId) => void;
  onRemove: (id: string) => void;
  onUpdate: (id: string, patch: Partial<Pick<NotebookNote, "title" | "block">>) => void;
}) {
  const [mountedAt] = useState(() => Date.now());
  return (
    <div className={`notion-note${mountedAt - note.createdAt < 2_000 ? " is-fresh" : ""}`}>
      <span className="notion-note__bullet" aria-hidden="true">
        {noteIcon(note)}
      </span>
      <div className="notion-note__content">
        {note.block ? (
          <NotebookBlockEditor
            note={note}
            block={note.block}
            onUpdate={(patch) => onUpdate(note.id, patch)}
          />
        ) : (
          <>
            <p>{note.title}</p>
            {note.detail && <small>{note.detail}</small>}
          </>
        )}
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
  );
}

function updateChecklistItem(
  block: Extract<NotebookBlock, { type: "checklist" }>,
  itemId: string,
  patch: Partial<NotebookChecklistItem>,
): NotebookBlock {
  return {
    ...block,
    items: block.items.map((item) => (
      item.id === itemId ? { ...item, ...patch } : item
    )),
  };
}

function updateFieldItem(
  block: Extract<NotebookBlock, { type: "fields" }>,
  fieldId: string,
  patch: Partial<NotebookFieldItem>,
): NotebookBlock {
  return {
    ...block,
    fields: block.fields.map((field) => (
      field.id === fieldId ? { ...field, ...patch } : field
    )),
  };
}

function NotebookBlockEditor({
  note,
  block,
  onUpdate,
}: {
  note: NotebookNote;
  block: NotebookBlock;
  onUpdate: (patch: Partial<Pick<NotebookNote, "title" | "block">>) => void;
}) {
  function updateBlock(nextBlock: NotebookBlock) {
    onUpdate({ block: nextBlock });
  }

  const itemCount = block.type === "checklist" ? block.items.length : block.fields.length;
  const checkedCount = block.type === "checklist"
    ? block.items.filter((item) => item.checked).length
    : 0;
  const summary = block.type === "checklist"
    ? `${checkedCount} / ${itemCount} done`
    : `${itemCount} fields`;

  return (
    <div className="notion-block">
      <div className="notion-block__head">
        <button
          type="button"
          className="notion-block__collapse"
          aria-label={block.collapsed ? "Expand block" : "Collapse block"}
          aria-expanded={!block.collapsed}
          onClick={() => updateBlock({ ...block, collapsed: !block.collapsed })}
        >
          {block.collapsed ? "+" : "-"}
        </button>
        <input
          className="notion-block__title"
          value={note.title}
          aria-label="Block title"
          onChange={(event) => onUpdate({ title: event.target.value })}
        />
        <small>{summary}</small>
      </div>
      {!block.collapsed && block.type === "checklist" && (
        <ChecklistBlock block={block} onUpdate={updateBlock} />
      )}
      {!block.collapsed && block.type === "fields" && (
        <FieldBlock block={block} onUpdate={updateBlock} />
      )}
    </div>
  );
}

function ChecklistBlock({
  block,
  onUpdate,
}: {
  block: Extract<NotebookBlock, { type: "checklist" }>;
  onUpdate: (block: NotebookBlock) => void;
}) {
  function addItem() {
    onUpdate({
      ...block,
      items: [
        ...block.items,
        {
          id: `i_${Date.now().toString(36)}_${Math.random().toString(36).slice(2, 7)}`,
          text: "",
          checked: false,
        },
      ],
    });
  }

  return (
    <div className="notion-checklist">
      {block.items.map((item) => (
        <label key={item.id} className="notion-checklist__item">
          <input
            type="checkbox"
            checked={item.checked}
            onChange={(event) => onUpdate(updateChecklistItem(block, item.id, { checked: event.target.checked }))}
          />
          <input
            type="text"
            value={item.text}
            aria-label="Checklist item"
            placeholder="List item"
            onChange={(event) => onUpdate(updateChecklistItem(block, item.id, { text: event.target.value }))}
          />
          <button
            type="button"
            aria-label="Remove item"
            onClick={() => onUpdate({ ...block, items: block.items.filter((candidate) => candidate.id !== item.id) })}
          >
            ×
          </button>
        </label>
      ))}
      <button type="button" className="notion-block__add" onClick={addItem}>
        + Add item
      </button>
    </div>
  );
}

function FieldBlock({
  block,
  onUpdate,
}: {
  block: Extract<NotebookBlock, { type: "fields" }>;
  onUpdate: (block: NotebookBlock) => void;
}) {
  return (
    <div className="notion-fields">
      {block.fields.map((field) => (
        <label key={field.id} className="notion-fields__row">
          <input
            type="text"
            value={field.label}
            aria-label="Field label"
            onChange={(event) => onUpdate(updateFieldItem(block, field.id, { label: event.target.value }))}
          />
          <input
            type="text"
            value={field.value}
            aria-label={`${field.label} value`}
            placeholder="Add value"
            onChange={(event) => onUpdate(updateFieldItem(block, field.id, { value: event.target.value }))}
          />
        </label>
      ))}
    </div>
  );
}

function NotebookComposer({
  ariaLabel,
  onSubmit,
  onCommand,
}: {
  ariaLabel: string;
  onSubmit: (text: string) => void;
  onCommand: (command: NotebookCommand) => void;
}) {
  const [draft, setDraft] = useState("");
  const [menuOpen, setMenuOpen] = useState(false);
  const [activeIndex, setActiveIndex] = useState(0);
  const menuBaseId = useId();
  const formRef = useRef<HTMLFormElement | null>(null);
  const textareaRef = useRef<HTMLTextAreaElement | null>(null);
  const query = slashQuery(draft);
  const commands = menuOpen ? matchingNotebookCommands(query ?? "") : [];
  const menuVisible = menuOpen && commands.length > 0;
  const listboxId = `${menuBaseId}-commands`;
  const activeCommandIndex = commands.length === 0
    ? 0
    : Math.min(activeIndex, commands.length - 1);
  const activeOptionId = menuVisible
    ? `${listboxId}-${commands[activeCommandIndex].id}`
    : undefined;

  useEffect(() => {
    if (!menuOpen) return undefined;
    function closeOnOutsidePress(event: MouseEvent) {
      if (event.target instanceof Node && formRef.current?.contains(event.target)) return;
      setMenuOpen(false);
    }
    document.addEventListener("mousedown", closeOnOutsidePress);
    return () => document.removeEventListener("mousedown", closeOnOutsidePress);
  }, [menuOpen]);

  function focusComposer() {
    window.requestAnimationFrame(() => textareaRef.current?.focus());
  }

  function selectCommand(command: NotebookCommand) {
    onCommand(command);
    if (slashQuery(draft) != null) setDraft("");
    setMenuOpen(false);
    focusComposer();
  }

  function openCommandMenu() {
    setMenuOpen(true);
    setActiveIndex(0);
    focusComposer();
  }

  function updateDraft(value: string) {
    setDraft(value);
    const nextQuery = slashQuery(value);
    if (nextQuery == null) {
      setMenuOpen(false);
      return;
    }
    setActiveIndex(0);
    setMenuOpen(true);
  }

  function handleKeyDown(event: KeyboardEvent<HTMLTextAreaElement>) {
    if (!menuOpen) return;
    if (event.key === "ArrowDown") {
      event.preventDefault();
      setActiveIndex((index) => commands.length === 0 ? 0 : (index + 1) % commands.length);
      return;
    }
    if (event.key === "ArrowUp") {
      event.preventDefault();
      setActiveIndex((index) => commands.length === 0 ? 0 : (index - 1 + commands.length) % commands.length);
      return;
    }
    if (event.key === "Enter" && commands[activeCommandIndex]) {
      event.preventDefault();
      selectCommand(commands[activeCommandIndex]);
      return;
    }
    if (event.key === "Escape") {
      event.preventDefault();
      setMenuOpen(false);
      return;
    }
    if (event.key === "Backspace" && draft === "/") {
      event.preventDefault();
      setDraft("");
      setMenuOpen(false);
    }
  }

  return (
    <form
      ref={formRef}
      className="notion-writing"
      onSubmit={(event) => {
        event.preventDefault();
        if (!draft.trim()) return;
        onSubmit(draft);
        setDraft("");
      }}
    >
      <button
        type="button"
        className="notion-writing__insert"
        aria-label="Insert notebook block"
        aria-haspopup="listbox"
        aria-expanded={menuVisible}
        aria-controls={menuVisible ? listboxId : undefined}
        onClick={openCommandMenu}
      >
        +
      </button>
      <textarea
        ref={textareaRef}
        value={draft}
        onChange={(event) => updateDraft(event.target.value)}
        onKeyDown={handleKeyDown}
        placeholder={NOTEBOOK_COMPOSER_PLACEHOLDER}
        aria-label={ariaLabel}
        role="combobox"
        aria-autocomplete="list"
        aria-expanded={menuVisible}
        aria-controls={menuVisible ? listboxId : undefined}
        aria-activedescendant={activeOptionId}
        rows={2}
      />
      {menuVisible && (
        <div
          id={listboxId}
          className="notion-command-menu"
          role="listbox"
          aria-label="Notebook commands"
        >
          {commands.map((command, index) => (
            <button
              id={`${listboxId}-${command.id}`}
              key={command.id}
              type="button"
              role="option"
              aria-selected={index === activeCommandIndex}
              className={index === activeCommandIndex ? "is-active" : undefined}
              onMouseDown={(event) => event.preventDefault()}
              onClick={() => selectCommand(command)}
            >
              <span>{command.title}</span>
              <small>{command.slash}</small>
            </button>
          ))}
        </div>
      )}
      {draft.trim() && (
        <button type="submit">Done</button>
      )}
    </form>
  );
}
