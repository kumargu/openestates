import { useEffect, useId, useMemo, useRef, useState, type KeyboardEvent } from "react";
import { Helmet } from "react-helmet-async";
import { Link, useLocation, useSearchParams } from "react-router-dom";
import { useNotebook } from "../hooks/useNotebook.ts";
import { FOCUS_STORAGE_KEY, readShortlistIds } from "../lib/compare.ts";
import { SocietyComparisonMatrix } from "../components/compare/SocietyComparisonMatrix.tsx";
import { WorkspaceHeader } from "../components/workspace/WorkspaceHeader.tsx";
import { LabelPill } from "../components/ui/LabelPill.tsx";
import { getProperties, getProperty } from "../lib/api.ts";
import {
  matchingNotebookCommands,
  slashQuery,
  type NotebookCommand,
} from "../lib/notebookCommands.ts";
import {
  ASSIGNABLE_NOTEBOOK_LABELS,
  labelDef,
  type NotebookCommandBlock,
  type NotebookChecklistItem,
  type NotebookFieldItem,
  type NotebookLabelId,
  type NotebookNote,
} from "../lib/notebook.ts";
import { LabelVisualIcon } from "../lib/LabelVisualIcon.tsx";
import {
  activeWorkspaceCompareIds,
  workspaceBuyVsRentHref,
  workspaceCompareHref,
  workspaceFocusedHomeId,
} from "../lib/workspaceNav.ts";
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

function noteIcon(note: NotebookNote) {
  return <LabelVisualIcon id={note.labels[0] ?? (note.kind === "handwritten" ? "visit" : "other")} size={22} />;
}

function workspaceMode(pathname: string): WorkspaceMode {
  return pathname === "/workspace/compare" ? "compare" : "notes";
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
        <LabelPill
          key={id}
          labelId={id}
          surface="notebook"
          title="Remove label"
          onClick={() => onRemove(id)}
        />
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
    setCompareIds,
    toggleCompare,
    addHandwritten,
    addCommandBlock,
    addParagraphAfter,
    updateNote,
    addNoteLabel,
    removeNoteLabel,
    removeNote,
  } = useNotebook();
  const [homes, setHomes] = useState<PropertyCard[]>([]);
  const [catalogStatus, setCatalogStatus] = useState<
    "loading" | "ready" | "error"
  >("loading");
  const [compareState, setCompareState] = useState<CompareState>({
    key: "",
    status: "idle",
    details: [],
  });

  useEffect(() => {
    const controller = new AbortController();
    getProperties({ signal: controller.signal })
      .then((properties) => {
        setHomes(properties);
        setCatalogStatus("ready");
      })
      .catch((error: unknown) => {
        if (error instanceof DOMException && error.name === "AbortError") return;
        setCatalogStatus("error");
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
    () => activeWorkspaceCompareIds(requestedCompareIds, compareIds),
    [compareIds, requestedCompareIds],
  );
  const selectedHomes = useMemo(
    () => activeCompareIds
      .map((id) => byId.get(id))
      .filter((home): home is PropertyCard => Boolean(home)),
    [activeCompareIds, byId],
  );
  const compareKey = selectedHomes.map((home) => home.id).join(",");

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
  const shortlistedWorkspaceIds = readShortlistIds()
    .filter((id) => orderedPropertyIds.includes(id));
  const focusCandidates = mode === "compare"
    ? activeCompareIds
    : shortlistedWorkspaceIds.length > 0
      ? shortlistedWorkspaceIds
      : orderedPropertyIds;
  const focusedWorkspaceId = workspaceFocusedHomeId(
    searchParams.get("focus"),
    window.localStorage.getItem(FOCUS_STORAGE_KEY),
    focusCandidates,
  );
  const compareHref = workspaceCompareHref(activeCompareIds, focusedWorkspaceId);
  const buyVsRentHref = workspaceBuyVsRentHref(focusedWorkspaceId);
  const compareViewStatus: CompareStatus =
    selectedHomes.length < 2
      ? catalogStatus === "loading" && activeCompareIds.length >= 2
        ? "loading"
        : catalogStatus === "error"
          ? "error"
          : "idle"
      : compareState.key === compareKey
        ? compareState.status
        : "loading";

  function quickAdd(propertyId: string, text: string, labels: NotebookLabelId[] = []) {
    if (!propertyId || !text.trim()) return;
    addHandwritten({ propertyId, text, labels });
  }

  useEffect(() => {
    if (mode !== "compare" || requestedCompareIds.length === 0) return;
    const selectionChanged = activeCompareIds.length !== compareIds.length
      || activeCompareIds.some((id, index) => id !== compareIds[index]);
    if (selectionChanged) setCompareIds(activeCompareIds);
  }, [activeCompareIds, compareIds, mode, requestedCompareIds.length, setCompareIds]);

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

  function removeCompareHomes(propertyIdsToRemove: string[]) {
    const removeSet = new Set(propertyIdsToRemove);
    const nextIds = activeCompareIds.filter((id) => !removeSet.has(id));
    setCompareIds(nextIds);

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

      <WorkspaceHeader
        mode={mode}
        compareHref={compareHref}
        buyVsRentHref={buyVsRentHref}
        compareCount={activeCompareIds.length}
      />
      <h1 className="visually-hidden">Workspace</h1>

      {mode === "compare" ? (
        <CompareWorkspaceView
          selectedHomes={selectedHomes}
          catalog={homes}
          details={compareState.key === compareKey ? compareState.details : []}
          status={compareViewStatus}
          onRemoveHome={removeCompareHomes}
        />
      ) : propertyIds.length === 0 ? (
        <div className="notion-empty">
          <h2>Empty workspace</h2>
          <p>Save a home or add a note from a property page to start your decision workspace.</p>
          <Link to="/">Explore</Link>
        </div>
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
          onAddParagraphAfter={(propertyId, blockId) => addParagraphAfter({ propertyId, afterBlockId: blockId })}
          onCommandAt={(propertyId, blockId, commandId) =>
            addCommandBlock({ propertyId, commandId, replaceBlockId: blockId })}
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
  onRemoveHome,
}: {
  selectedHomes: PropertyCard[];
  catalog: PropertyCard[];
  details: PropertyDetailResponse[];
  status: CompareStatus;
  onRemoveHome: (propertyIds: string[]) => void;
}) {
  if (status === "loading") {
    return (
      <section className="workspace-compare-view" aria-label="Compare homes">
        <div className="workspace-compare-loading" aria-label="Loading comparison">
          <div />
          <div />
        </div>
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

  return (
    <section className="workspace-compare-view" aria-label="Compare homes">
      <SocietyComparisonMatrix
        selectedHomes={selectedHomes}
        catalog={catalog}
        details={details}
        onRemoveColumn={onRemoveHome}
      />
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
  onAddParagraphAfter,
  onCommandAt,
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
  onAddParagraphAfter: (propertyId: string, blockId: string) => void;
  onCommandAt: (propertyId: string, blockId: string, commandId: NotebookCommand["id"]) => void;
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
                  onAddParagraphAfter={onAddParagraphAfter}
                  onCommandAt={onCommandAt}
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
  onAddParagraphAfter,
  onCommandAt,
}: {
  note: NotebookNote;
  onAddLabel: (id: string, label: NotebookLabelId) => void;
  onRemoveLabel: (id: string, label: NotebookLabelId) => void;
  onRemove: (id: string) => void;
  onUpdate: (id: string, patch: Partial<Pick<NotebookNote, "title" | "block">>) => void;
  onAddParagraphAfter: (propertyId: string, blockId: string) => void;
  onCommandAt: (propertyId: string, blockId: string, commandId: NotebookCommand["id"]) => void;
}) {
  const [mountedAt] = useState(() => Date.now());
  const labelPicker = (
    <LabelPicker
      note={note}
      onAdd={(label) => onAddLabel(note.id, label)}
      onRemove={(label) => onRemoveLabel(note.id, label)}
    />
  );
  return (
    <div className={`notion-note${note.kind === "plan" ? " notion-note--plan" : ""}${mountedAt - note.createdAt < 2_000 ? " is-fresh" : ""}`}>
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
        ) : note.kind === "handwritten" ? (
          <NotebookParagraph
            note={note}
            onChange={(text) => onUpdate(note.id, { title: text })}
            onAddNext={() => onAddParagraphAfter(note.propertyId, note.id)}
            onCommand={(commandId) => onCommandAt(note.propertyId, note.id, commandId)}
          />
        ) : (
          <>
            <div className="notion-note__head">
              <div>
                <p>{note.title}</p>
                {note.source && <span className="notion-note__meta">{note.source}</span>}
              </div>
              {note.kind === "plan" && (
                <div className="notion-note__head-tags">
                  {note.planHref && (
                    <Link className="notion-plan-link" to={note.planHref}>
                      Open Plan
                    </Link>
                  )}
                  {labelPicker}
                </div>
              )}
            </div>
            {note.detail && <small>{note.detail}</small>}
          </>
        )}
      </div>
      {note.kind !== "plan" && <div className="notion-note__labels">{labelPicker}</div>}
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

function commandIdForSlash(value: string): NotebookCommand["id"] | null {
  const normalized = value.trim().toLowerCase();
  const command = matchingNotebookCommands(normalized.replace(/^\//, ""))
    .find((item) => item.slash === normalized);
  return command?.id ?? null;
}

function NotebookParagraph({
  note,
  onChange,
  onAddNext,
  onCommand,
}: {
  note: NotebookNote;
  onChange: (text: string) => void;
  onAddNext: () => void;
  onCommand: (commandId: NotebookCommand["id"]) => void;
}) {
  function handleKeyDown(event: KeyboardEvent<HTMLTextAreaElement>) {
    if (event.key !== "Enter" || event.shiftKey) return;
    event.preventDefault();
    const commandId = commandIdForSlash(note.title);
    if (commandId) {
      onCommand(commandId);
      return;
    }
    onAddNext();
  }

  return (
    <textarea
      className="notion-paragraph"
      value={note.title}
      aria-label="Notebook paragraph"
      rows={1}
      placeholder="Write a note"
      onChange={(event) => onChange(event.target.value)}
      onKeyDown={handleKeyDown}
    />
  );
}

function updateChecklistItem(
  block: Extract<NotebookCommandBlock, { type: "checklist" }>,
  itemId: string,
  patch: Partial<NotebookChecklistItem>,
): NotebookCommandBlock {
  return {
    ...block,
    items: block.items.map((item) => (
      item.id === itemId ? { ...item, ...patch } : item
    )),
  };
}

function updateFieldItem(
  block: Extract<NotebookCommandBlock, { type: "fields" }>,
  fieldId: string,
  patch: Partial<NotebookFieldItem>,
): NotebookCommandBlock {
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
  block: NotebookCommandBlock;
  onUpdate: (patch: Partial<Pick<NotebookNote, "title" | "block">>) => void;
}) {
  function updateBlock(nextBlock: NotebookCommandBlock) {
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
  block: Extract<NotebookCommandBlock, { type: "checklist" }>;
  onUpdate: (block: NotebookCommandBlock) => void;
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
  block: Extract<NotebookCommandBlock, { type: "fields" }>;
  onUpdate: (block: NotebookCommandBlock) => void;
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
