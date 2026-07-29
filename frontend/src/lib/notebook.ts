/**
 * Buyer notebook — local persistence until a transactional API exists.
 *
 * Rule: labels are the join key.
 * - UI picks mint labels from structured card data (layer + distance).
 * - Handwritten notes start with no labels → Add-note only.
 * - Compare joins homes on shared compare-join labels.
 * - Some labels organize only (community, legal) and never join Compare.
 */

import { readShortlistIds, writeShortlistIds } from "./compare.ts";
import { NOTEBOOK_COMMANDS, type NotebookCommand } from "./notebookCommands.ts";

export const NOTEBOOK_STORAGE_KEY = "openestates:buyer-notebook-v2";
export const NOTEBOOK_CHANGED_EVENT = "openestates:notebook-changed";
export const MAX_NOTEBOOK_NOTES = 200;
export const MAX_COMPARE_FROM_NOTEBOOK = 4;
export const MAX_LABELS_PER_NOTE = 4;

export type NotebookNoteKind = "fact" | "plan" | "selection" | "handwritten";

/** Stable label ids used as Compare join keys. */
export type NotebookLabelId = string;

export type NotebookLabelDef = {
  id: NotebookLabelId;
  title: string;
  /** False = notebook organization only; never joins Compare. */
  compareJoin: boolean;
  /** Optional Compare row; keeps label-specific grouping out of Compare UI code. */
  compareGroup?: string;
};

export type NotebookChecklistItem = {
  id: string;
  text: string;
  checked: boolean;
};

export type NotebookFieldItem = {
  id: string;
  label: string;
  value: string;
};

export type NotebookBlock =
  | {
      type: "checklist";
      collapsed: boolean;
      items: NotebookChecklistItem[];
    }
  | {
      type: "fields";
      collapsed: boolean;
      fields: NotebookFieldItem[];
    };

export type NotebookNote = {
  id: string;
  propertyId: string;
  /** Buyer-facing note text. */
  title: string;
  detail?: string;
  source?: string;
  kind: NotebookNoteKind;
  catalogKey: string;
  selectionText?: string;
  /** Join / organize keys. Empty means the note stays out of Compare. */
  labels: NotebookLabelId[];
  block?: NotebookBlock;
  createdAt: number;
};

export type NotebookState = {
  propertyIds: string[];
  notes: NotebookNote[];
  compareIds: string[];
  hiddenCompareLabels: NotebookLabelId[];
};

/** Catalog — expand later via config; keep frontend-deterministic for MVP. */
export const NOTEBOOK_LABELS: NotebookLabelDef[] = [
  { id: "schools", title: "Schools", compareJoin: true, compareGroup: "access_notes" },
  { id: "schools_under_1km", title: "School under 1 km", compareJoin: true, compareGroup: "nearby_access" },
  { id: "schools_under_3km", title: "School under 3 km", compareJoin: true, compareGroup: "nearby_access" },
  { id: "schools_under_5km", title: "School under 5 km", compareJoin: true, compareGroup: "nearby_access" },
  { id: "hospitals", title: "Hospitals", compareJoin: true, compareGroup: "access_notes" },
  { id: "hospitals_under_1km", title: "Hospital under 1 km", compareJoin: true, compareGroup: "nearby_access" },
  { id: "hospitals_under_3km", title: "Hospital under 3 km", compareJoin: true, compareGroup: "nearby_access" },
  { id: "hospitals_under_5km", title: "Hospital under 5 km", compareJoin: true, compareGroup: "nearby_access" },
  { id: "commute", title: "Commute", compareJoin: true, compareGroup: "commute_anchors" },
  { id: "metro", title: "Metro", compareJoin: true, compareGroup: "commute_anchors" },
  { id: "metro_under_1km", title: "Metro under 1 km", compareJoin: true, compareGroup: "nearby_access" },
  { id: "metro_under_3km", title: "Metro under 3 km", compareJoin: true, compareGroup: "nearby_access" },
  { id: "tech_parks", title: "Tech parks", compareJoin: true, compareGroup: "commute_anchors" },
  { id: "water", title: "Water", compareJoin: true, compareGroup: "water" },
  { id: "risk", title: "Risk", compareJoin: true, compareGroup: "red_flags" },
  { id: "complaints", title: "Complaints", compareJoin: true, compareGroup: "red_flags" },
  { id: "transmission", title: "High-tension line", compareJoin: true, compareGroup: "red_flags" },
  { id: "approach", title: "Approach road", compareJoin: true, compareGroup: "approach" },
  { id: "open-space", title: "Open space", compareJoin: true, compareGroup: "open_spaces" },
  { id: "price", title: "Price proof", compareJoin: true, compareGroup: "money" },
  { id: "layout", title: "Layout", compareJoin: true, compareGroup: "layout" },
  { id: "down-payment", title: "Down payment", compareJoin: true, compareGroup: "money" },
  { id: "emi", title: "EMI", compareJoin: true, compareGroup: "money" },
  { id: "finance", title: "Finance", compareJoin: false },
  { id: "legal", title: "Legal", compareJoin: false },
  { id: "community", title: "Community", compareJoin: false },
  { id: "visit", title: "Visit", compareJoin: false },
  { id: "other", title: "Other", compareJoin: false },
];

/** Labels a buyer can attach to handwritten notes (keep short). */
export const ASSIGNABLE_NOTEBOOK_LABELS: NotebookLabelId[] = [
  "schools",
  "hospitals",
  "commute",
  "metro",
  "tech_parks",
  "water",
  "risk",
  "complaints",
  "transmission",
  "approach",
  "open-space",
  "price",
  "layout",
  "down-payment",
  "emi",
  "finance",
  "legal",
  "community",
  "visit",
  "other",
];

const EMPTY: NotebookState = { propertyIds: [], notes: [], compareIds: [], hiddenCompareLabels: [] };
const LABEL_BY_ID = new Map(NOTEBOOK_LABELS.map((item) => [item.id, item]));

function emit(state: NotebookState) {
  window.dispatchEvent(new CustomEvent(NOTEBOOK_CHANGED_EVENT, { detail: state }));
}

export function labelDef(id: NotebookLabelId): NotebookLabelDef {
  return LABEL_BY_ID.get(id) ?? { id, title: id, compareJoin: false };
}

export function isCompareJoinLabel(id: NotebookLabelId): boolean {
  return labelDef(id).compareJoin;
}

/** Simple rule: has at least one compare-join label. */
export function noteIsCompareable(note: Pick<NotebookNote, "labels">): boolean {
  return note.labels.some(isCompareJoinLabel);
}

export function compareJoinLabels(note: Pick<NotebookNote, "labels">): NotebookLabelId[] {
  return note.labels.filter(isCompareJoinLabel);
}

function uniqueLabels(labels: NotebookLabelId[]): NotebookLabelId[] {
  return [...new Set(labels.filter(Boolean))].slice(0, MAX_LABELS_PER_NOTE);
}

function noteId(prefix = "n"): string {
  return `${prefix}_${Date.now().toString(36)}_${Math.random().toString(36).slice(2, 7)}`;
}

function normalizeChecklistItem(raw: unknown): NotebookChecklistItem | null {
  if (typeof raw !== "object" || raw == null) return null;
  const item = raw as Partial<NotebookChecklistItem>;
  if (typeof item.text !== "string") return null;
  return {
    id: typeof item.id === "string" ? item.id : noteId("i"),
    text: item.text,
    checked: item.checked === true,
  };
}

function normalizeFieldItem(raw: unknown): NotebookFieldItem | null {
  if (typeof raw !== "object" || raw == null) return null;
  const item = raw as Partial<NotebookFieldItem>;
  if (typeof item.label !== "string") return null;
  return {
    id: typeof item.id === "string" ? item.id : noteId("f"),
    label: item.label,
    value: typeof item.value === "string" ? item.value : "",
  };
}

function normalizeBlock(raw: unknown): NotebookBlock | undefined {
  if (typeof raw !== "object" || raw == null) return undefined;
  const block = raw as Partial<NotebookBlock>;
  if (block.type === "checklist" && Array.isArray(block.items)) {
    const items = block.items
      .map(normalizeChecklistItem)
      .filter((item): item is NotebookChecklistItem => item != null);
    return {
      type: "checklist",
      collapsed: block.collapsed === true,
      items,
    };
  }
  if (block.type === "fields" && Array.isArray(block.fields)) {
    const fields = block.fields
      .map(normalizeFieldItem)
      .filter((field): field is NotebookFieldItem => field != null);
    return {
      type: "fields",
      collapsed: block.collapsed === true,
      fields,
    };
  }
  return undefined;
}

/** Distance → bucket label for a base dimension (hospitals → hospitals_under_3km). */
export function distanceBucketLabel(base: NotebookLabelId, km?: number | null): NotebookLabelId | null {
  if (km == null || !Number.isFinite(km)) return null;
  if (km <= 1) return `${base}_under_1km`;
  if (km <= 3) return `${base}_under_3km`;
  if (km <= 5) return `${base}_under_5km`;
  return null;
}

export function labelsForNearbyPlace(layer: string, distanceKm?: number | null): NotebookLabelId[] {
  const key = layer.toLowerCase();
  let base: NotebookLabelId = "other";
  if (key === "schools") base = "schools";
  else if (key === "hospitals") base = "hospitals";
  else if (key === "metro") base = "metro";
  else if (key.includes("tech")) base = "tech_parks";
  else if (key === "red_flags") base = "risk";
  else if (key === "water") base = "water";
  else if (key.includes("park") || key.includes("green")) base = "open-space";
  else base = "commute";

  const labels: NotebookLabelId[] = [base];
  if (base === "metro") labels.push("commute");
  if (base === "tech_parks") labels.push("commute");
  if (base === "risk") {
    // red-flag places stay on risk; lines add transmission separately
  }
  const bucketBase = base === "metro" || base === "schools" || base === "hospitals" ? base : null;
  if (bucketBase) {
    const bucket = distanceBucketLabel(bucketBase, distanceKm);
    if (bucket) labels.push(bucket);
  }
  return uniqueLabels(labels);
}

export function labelsForRedFlagLine(title: string): NotebookLabelId[] {
  const hay = title.toLowerCase();
  if (hay.includes("transmission") || hay.includes("voltage") || hay.includes("tension")) {
    return ["risk", "transmission"];
  }
  if (hay.includes("drain") || hay.includes("flood")) return ["risk", "water"];
  return ["risk"];
}

export function labelsFromEvidenceSection(kind: string, text: string): NotebookLabelId[] {
  const hay = `${kind} ${text}`.toLowerCase();
  if (hay.includes("school")) return ["schools"];
  if (hay.includes("hospital")) return ["hospitals"];
  if (hay.includes("water") || hay.includes("flood") || hay.includes("groundwater")) return ["water"];
  if (hay.includes("complaint")) return ["complaints", "risk"];
  if (hay.includes("rera") || hay.includes("legal") || hay.includes("registration")) {
    return ["legal"];
  }
  if (hay.includes("price") || hay.includes("market") || hay.includes("asking")) return ["price"];
  if (hay.includes("plan") || hay.includes("layout") || hay.includes("carpet")) return ["layout"];
  if (hay.includes("metro")) return ["metro", "commute"];
  if (hay.includes("tech park")) return ["tech_parks", "commute"];
  if (hay.includes("commute") || hay.includes("traffic") || hay.includes("road")) return ["commute"];
  if (hay.includes("park") || hay.includes("green") || hay.includes("open")) return ["open-space"];
  if (hay.includes("community") || hay.includes("review") || hay.includes("pulse")) return ["community"];
  if (hay.includes("approach") || hay.includes("gate")) return ["approach"];
  if (hay.includes("transmission") || hay.includes("tension")) return ["risk", "transmission"];
  return ["other"];
}

function migrateLegacyNote(raw: Record<string, unknown>): Partial<NotebookNote> {
  const title = typeof raw.title === "string"
    ? raw.title
    : typeof raw.label === "string"
      ? raw.label
      : "";
  const detail = typeof raw.detail === "string" ? raw.detail : undefined;
  const source = typeof raw.source === "string" ? raw.source : undefined;
  let labels: NotebookLabelId[] = [];
  if (Array.isArray(raw.labels)) {
    labels = raw.labels.filter((item): item is string => typeof item === "string");
  } else if (typeof raw.tag === "string") {
    labels = [raw.tag];
    // Old compareEligible=false with a join tag → keep label but denylist already handles community/legal
    if (raw.compareEligible === false && !["legal", "community", "visit", "other"].includes(raw.tag)) {
      // Keep labels; joinability now comes from catalog
    }
  }
  labels = normalizeMigratedLabels(labels, `${title} ${detail ?? ""} ${source ?? ""}`);
  return {
    id: typeof raw.id === "string" ? raw.id : undefined,
    propertyId: typeof raw.propertyId === "string" ? raw.propertyId : undefined,
    title,
    detail,
    source,
    kind: (raw.kind as NotebookNoteKind) ?? "fact",
    catalogKey: typeof raw.catalogKey === "string" ? raw.catalogKey : undefined,
    selectionText: typeof raw.selectionText === "string" ? raw.selectionText : undefined,
    labels,
    block: normalizeBlock(raw.block),
    createdAt: typeof raw.createdAt === "number" ? raw.createdAt : Date.now(),
  };
}

function normalizeMigratedLabels(labels: NotebookLabelId[], text: string): NotebookLabelId[] {
  if (!/complaint/i.test(text) && !labels.includes("complaints")) return labels;
  return [
    "complaints",
    "risk",
    ...labels.filter((label) => label !== "complaints" && label !== "risk"),
  ];
}

function normalizeNote(raw: Partial<NotebookNote> | Record<string, unknown>): NotebookNote | null {
  const migrated = migrateLegacyNote(raw as Record<string, unknown>);
  if (!migrated.id || !migrated.propertyId || !migrated.catalogKey || !migrated.title) return null;
  return {
    id: migrated.id,
    propertyId: migrated.propertyId,
    title: migrated.title,
    detail: migrated.detail,
    source: migrated.source,
    kind: migrated.kind ?? "fact",
    catalogKey: migrated.catalogKey,
    selectionText: migrated.selectionText,
    labels: uniqueLabels(migrated.labels ?? []),
    block: normalizeBlock(migrated.block),
    createdAt: migrated.createdAt ?? Date.now(),
  };
}

function readRawState(): NotebookState {
  try {
    const raw = window.localStorage.getItem(NOTEBOOK_STORAGE_KEY)
      ?? window.localStorage.getItem("openestates:buyer-notebook-v1");
    if (!raw) return EMPTY;
    const parsed = JSON.parse(raw) as Partial<NotebookState>;
    const notes = Array.isArray(parsed.notes)
      ? parsed.notes
        .map((note) => normalizeNote(note as Record<string, unknown>))
        .filter((note): note is NotebookNote => note != null)
        .slice(0, MAX_NOTEBOOK_NOTES)
      : [];
    return {
      propertyIds: Array.isArray(parsed.propertyIds) ? parsed.propertyIds.filter(Boolean) : [],
      notes,
      compareIds: Array.isArray(parsed.compareIds) ? parsed.compareIds.filter(Boolean) : [],
      hiddenCompareLabels: Array.isArray(parsed.hiddenCompareLabels)
        ? parsed.hiddenCompareLabels.filter((item): item is NotebookLabelId => typeof item === "string")
        : [],
    };
  } catch {
    return EMPTY;
  }
}

export function readNotebook(): NotebookState {
  const state = readRawState();
  const shortlist = readShortlistIds();
  const propertyIds = [...new Set([...shortlist, ...state.propertyIds])];
  if (shortlist.length === 0 && propertyIds.length === state.propertyIds.length) return state;
  return {
    ...state,
    propertyIds,
    compareIds: state.compareIds
      .filter((id) => propertyIds.includes(id))
      .slice(0, MAX_COMPARE_FROM_NOTEBOOK),
  };
}

export function writeNotebook(state: NotebookState): NotebookState {
  const next: NotebookState = {
    propertyIds: [...new Set(state.propertyIds)],
    notes: state.notes.slice(0, MAX_NOTEBOOK_NOTES),
    compareIds: state.compareIds
      .filter((id) => state.propertyIds.includes(id))
      .slice(0, MAX_COMPARE_FROM_NOTEBOOK),
    hiddenCompareLabels: [...new Set(state.hiddenCompareLabels ?? [])],
  };
  window.localStorage.setItem(NOTEBOOK_STORAGE_KEY, JSON.stringify(next));
  emit(next);
  return next;
}

function ensureProperty(state: NotebookState, propertyId: string): NotebookState {
  if (state.propertyIds.includes(propertyId)) return state;
  return { ...state, propertyIds: [propertyId, ...state.propertyIds] };
}

export function isCatalogPinned(catalogKey: string, state = readNotebook()): boolean {
  return state.notes.some((n) => n.catalogKey === catalogKey);
}

export function toggleCatalogNote(input: {
  propertyId: string;
  catalogKey: string;
  title: string;
  labels: NotebookLabelId[];
  detail?: string;
  source?: string;
  kind?: NotebookNoteKind;
}): NotebookState {
  const state = readNotebook();
  const existing = state.notes.find((n) => n.catalogKey === input.catalogKey);
  if (existing) {
    return writeNotebook({
      ...state,
      notes: state.notes.filter((n) => n.id !== existing.id),
    });
  }

  const note: NotebookNote = {
    id: noteId(),
    propertyId: input.propertyId,
    title: input.title,
    detail: input.detail,
    source: input.source,
    kind: input.kind ?? "fact",
    catalogKey: input.catalogKey,
    labels: uniqueLabels(input.labels),
    createdAt: Date.now(),
  };
  const withProp = ensureProperty(state, input.propertyId);
  return writeNotebook({
    ...withProp,
    notes: [...withProp.notes, note],
  });
}

export function addSelectionNote(input: {
  propertyId: string;
  text: string;
  labels?: NotebookLabelId[];
  source?: string;
}): NotebookState | null {
  const trimmed = input.text.replace(/\s+/g, " ").trim();
  if (trimmed.length < 8) return null;
  const summary = trimmed.length <= 72 ? trimmed : `${trimmed.slice(0, 70).trimEnd()}…`;
  const labels = uniqueLabels(input.labels ?? labelsFromEvidenceSection("selection", trimmed));
  const catalogKey = `sel:${input.propertyId}:${summary.slice(0, 48)}`;
  const state = readNotebook();
  if (state.notes.some((n) => n.catalogKey === catalogKey)) return state;

  const note: NotebookNote = {
    id: noteId(),
    propertyId: input.propertyId,
    title: summary,
    detail: "Selected from evidence",
    source: input.source ?? "Selection",
    kind: "selection",
    catalogKey,
    selectionText: trimmed,
    labels,
    createdAt: Date.now(),
  };
  const withProp = ensureProperty(state, input.propertyId);
  return writeNotebook({ ...withProp, notes: [...withProp.notes, note] });
}

/** Handwritten starts unlabeled unless caller passes labels (e.g. approach compose). */
export function addHandwrittenNote(input: {
  propertyId: string;
  text: string;
  labels?: NotebookLabelId[];
  source?: string;
  detail?: string;
}): NotebookState | null {
  const trimmed = input.text.trim();
  if (!trimmed) return null;
  const note: NotebookNote = {
    id: noteId(),
    propertyId: input.propertyId,
    title: trimmed,
    detail: input.detail,
    source: input.source ?? "You",
    kind: "handwritten",
    catalogKey: `hand:${input.propertyId}:${Date.now()}`,
    labels: uniqueLabels(input.labels ?? []),
    createdAt: Date.now(),
  };
  const state = ensureProperty(readNotebook(), input.propertyId);
  return writeNotebook({ ...state, notes: [...state.notes, note] });
}

function commandLabels(command: NotebookCommand): NotebookLabelId[] {
  if (command.id === "budget") return ["finance"];
  if (command.id === "payment") return ["legal"];
  if (command.id === "visit") return ["visit"];
  return [];
}

function blockFromCommand(command: NotebookCommand): NotebookBlock {
  if (command.blockType === "fields") {
    return {
      type: "fields",
      collapsed: false,
      fields: (command.fields ?? []).map((label) => ({
        id: noteId("f"),
        label,
        value: "",
      })),
    };
  }
  return {
    type: "checklist",
    collapsed: false,
    items: (command.items ?? ["New item"]).map((text) => ({
      id: noteId("i"),
      text,
      checked: false,
    })),
  };
}

export function addNotebookCommandBlock(input: {
  propertyId: string;
  commandId: NotebookCommand["id"];
}): NotebookState | null {
  const command = NOTEBOOK_COMMANDS.find((item) => item.id === input.commandId);
  if (!command) return null;
  const note: NotebookNote = {
    id: noteId(),
    propertyId: input.propertyId,
    title: command.title,
    source: "You",
    kind: "handwritten",
    catalogKey: `block:${input.propertyId}:${command.id}:${Date.now()}`,
    labels: uniqueLabels(commandLabels(command)),
    block: blockFromCommand(command),
    createdAt: Date.now(),
  };
  const state = ensureProperty(readNotebook(), input.propertyId);
  return writeNotebook({ ...state, notes: [...state.notes, note] });
}

export function updateNotebookNote(
  noteIdToUpdate: string,
  patch: Partial<Pick<NotebookNote, "title" | "block">>,
): NotebookState {
  const state = readNotebook();
  return writeNotebook({
    ...state,
    notes: state.notes.map((note) => (
      note.id === noteIdToUpdate
        ? {
            ...note,
            ...patch,
            title: patch.title ?? note.title,
            block: patch.block === undefined ? note.block : patch.block,
          }
        : note
    )),
  });
}

export function removeNotebookNote(noteId: string): NotebookState {
  const state = readNotebook();
  return writeNotebook({
    ...state,
    notes: state.notes.filter((n) => n.id !== noteId),
  });
}

export function setNotebookNoteLabels(noteId: string, labels: NotebookLabelId[]): NotebookState {
  const state = readNotebook();
  return writeNotebook({
    ...state,
    notes: state.notes.map((n) => (
      n.id === noteId ? { ...n, labels: uniqueLabels(labels) } : n
    )),
  });
}

export function addNotebookNoteLabel(noteId: string, label: NotebookLabelId): NotebookState {
  const state = readNotebook();
  return writeNotebook({
    ...state,
    notes: state.notes.map((n) => (
      n.id === noteId ? { ...n, labels: uniqueLabels([...n.labels, label]) } : n
    )),
  });
}

export function removeNotebookNoteLabel(noteId: string, label: NotebookLabelId): NotebookState {
  const state = readNotebook();
  return writeNotebook({
    ...state,
    notes: state.notes.map((n) => (
      n.id === noteId ? { ...n, labels: n.labels.filter((item) => item !== label) } : n
    )),
  });
}

export function toggleNotebookCompareId(propertyId: string): NotebookState {
  const state = readNotebook();
  if (!state.propertyIds.includes(propertyId) && !state.notes.some((n) => n.propertyId === propertyId)) {
    return state;
  }
  const withProp = ensureProperty(state, propertyId);
  const currentIds = withProp.compareIds.slice(0, MAX_COMPARE_FROM_NOTEBOOK);
  const on = currentIds.includes(propertyId);
  const compareIds = on
    ? currentIds.filter((id) => id !== propertyId)
    : currentIds.length >= MAX_COMPARE_FROM_NOTEBOOK
      ? currentIds
      : [...currentIds, propertyId];
  return writeNotebook({ ...withProp, compareIds });
}

export function hideNotebookCompareLabel(label: NotebookLabelId): NotebookState {
  const state = readNotebook();
  if (state.hiddenCompareLabels.includes(label)) return state;
  return writeNotebook({
    ...state,
    hiddenCompareLabels: [...state.hiddenCompareLabels, label],
  });
}

export function showNotebookCompareLabel(label: NotebookLabelId): NotebookState {
  const state = readNotebook();
  return writeNotebook({
    ...state,
    hiddenCompareLabels: state.hiddenCompareLabels.filter((item) => item !== label),
  });
}

export function removeNotebookProperty(propertyId: string): NotebookState {
  const state = readNotebook();
  writeShortlistIds(readShortlistIds().filter((id) => id !== propertyId));
  return writeNotebook({
    propertyIds: state.propertyIds.filter((id) => id !== propertyId),
    notes: state.notes.filter((n) => n.propertyId !== propertyId),
    compareIds: state.compareIds.filter((id) => id !== propertyId),
    hiddenCompareLabels: state.hiddenCompareLabels,
  });
}

export function anchorNotebookProperty(propertyId: string): NotebookState {
  return writeNotebook(ensureProperty(readNotebook(), propertyId));
}

export function compareEligibleNotes(state = readNotebook()): NotebookNote[] {
  return state.notes.filter(noteIsCompareable);
}

/** Labels that appear as compare joins across the given notes. */
export function sharedCompareLabelRows(notes: NotebookNote[]): NotebookLabelId[] {
  const counts = new Map<NotebookLabelId, number>();
  for (const note of notes) {
    for (const label of new Set(compareJoinLabels(note))) {
      counts.set(label, (counts.get(label) ?? 0) + 1);
    }
  }
  return [...counts.entries()]
    .sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]))
    .map(([id]) => id);
}

export function compareHrefFromNotebook(state = readNotebook(), focusId?: string): string | null {
  if (state.compareIds.length < 2) return null;
  const ids = state.compareIds;
  const focus = focusId && ids.includes(focusId) ? focusId : ids[0];
  return `/workspace/compare?ids=${encodeURIComponent(ids.join(","))}&focus=${encodeURIComponent(focus)}`;
}
