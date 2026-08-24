/**
 * Buyer notebook — local persistence until a transactional API exists.
 *
 * Rule: labels are v2 compatibility metadata, not the durable cross-surface contract.
 * - UI still mints labels from structured card data while Notebook remains local.
 * - Handwritten notes start with no labels → Add-note only.
 * - Shared DecisionFacet projections carry the semantic Compare contract.
 * - Some labels organize only (community, legal) and never join Compare.
 */

import { readShortlistIds, writeShortlistIds } from "./compare.ts";
import { NOTEBOOK_COMMANDS, type NotebookCommand } from "./notebookCommands.ts";

export const NOTEBOOK_STORAGE_KEY = "openestates:buyer-notebook-v2";
export const NOTEBOOK_CHANGED_EVENT = "openestates:notebook-changed";
export const MAX_NOTEBOOK_NOTES = 200;
export const MAX_COMPARE_FROM_NOTEBOOK = 4;
export const MAX_LABELS_PER_NOTE = 4;
export const NOTEBOOK_SCHEMA_VERSION = 3;

export type NotebookNoteKind = "fact" | "plan" | "selection" | "handwritten";

/** Stable v2 label ids retained for Notebook organization and migration. */
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

export type ParagraphBlock = {
  id: string;
  type: "paragraph";
  text: string;
  createdAt: number;
};

export type ChecklistBlock = {
  id: string;
  type: "checklist";
  title: string;
  collapsed: boolean;
  items: NotebookChecklistItem[];
  labels: NotebookLabelId[];
  catalogKey: string;
  createdAt: number;
};

export type FieldBlock = {
  id: string;
  type: "fields";
  title: string;
  collapsed: boolean;
  fields: NotebookFieldItem[];
  labels: NotebookLabelId[];
  catalogKey: string;
  createdAt: number;
};

export type EvidenceReferenceBlock = {
  id: string;
  type: "evidence_reference";
  title: string;
  detail?: string;
  source?: string;
  catalogKey: string;
  selectionText?: string;
  labels: NotebookLabelId[];
  createdAt: number;
};

export type FinancialPlanReferenceBlock = {
  id: string;
  type: "financial_plan_reference";
  title: string;
  detail?: string;
  source?: string;
  catalogKey: string;
  labels: NotebookLabelId[];
  createdAt: number;
  planHref: string;
};

export type NotebookBlock =
  | ParagraphBlock
  | ChecklistBlock
  | FieldBlock
  | EvidenceReferenceBlock
  | FinancialPlanReferenceBlock;

export type NotebookCommandBlock =
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
  block?: NotebookCommandBlock;
  planHref?: string;
  createdAt: number;
};

export type NotebookDocument = {
  propertyId: string;
  blocks: NotebookBlock[];
};

export type NotebookState = {
  version: typeof NOTEBOOK_SCHEMA_VERSION;
  propertyIds: string[];
  documents: Record<string, NotebookDocument>;
  /** Compatibility projection for current Compare and note adapters. */
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

const EMPTY: NotebookState = {
  version: NOTEBOOK_SCHEMA_VERSION,
  propertyIds: [],
  documents: {},
  notes: [],
  compareIds: [],
  hiddenCompareLabels: [],
};
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

function normalizeCommandBlock(raw: unknown): NotebookCommandBlock | undefined {
  if (typeof raw !== "object" || raw == null) return undefined;
  const block = raw as Partial<NotebookCommandBlock>;
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
    block: normalizeCommandBlock(raw.block),
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
    block: normalizeCommandBlock(migrated.block),
    createdAt: migrated.createdAt ?? Date.now(),
  };
}

function commandBlockFromNote(note: NotebookNote): ChecklistBlock | FieldBlock | null {
  if (!note.block) return null;
  if (note.block.type === "checklist") {
    return {
      id: note.id,
      type: "checklist",
      title: note.title,
      collapsed: note.block.collapsed,
      items: note.block.items,
      labels: note.labels,
      catalogKey: note.catalogKey,
      createdAt: note.createdAt,
    };
  }
  return {
    id: note.id,
    type: "fields",
    title: note.title,
    collapsed: note.block.collapsed,
    fields: note.block.fields,
    labels: note.labels,
    catalogKey: note.catalogKey,
    createdAt: note.createdAt,
  };
}

function blockFromNote(note: NotebookNote): NotebookBlock {
  const commandBlock = commandBlockFromNote(note);
  if (commandBlock) return commandBlock;
  if (note.kind === "plan") {
    return {
      id: note.id,
      type: "financial_plan_reference",
      title: note.title,
      detail: note.detail,
      source: note.source,
      catalogKey: note.catalogKey,
      labels: note.labels,
      createdAt: note.createdAt,
      planHref: `/workspace/buy-vs-rent/${encodeURIComponent(note.propertyId)}`,
    };
  }
  if (note.kind === "fact" || note.kind === "selection") {
    return {
      id: note.id,
      type: "evidence_reference",
      title: note.title,
      detail: note.detail,
      source: note.source,
      catalogKey: note.catalogKey,
      selectionText: note.selectionText,
      labels: note.labels,
      createdAt: note.createdAt,
    };
  }
  return {
    id: note.id,
    type: "paragraph",
    text: note.title,
    createdAt: note.createdAt,
  };
}

function noteFromBlock(propertyId: string, block: NotebookBlock): NotebookNote {
  if (block.type === "paragraph") {
    return {
      id: block.id,
      propertyId,
      title: block.text,
      kind: "handwritten",
      catalogKey: `hand:${propertyId}:${block.id}`,
      labels: [],
      createdAt: block.createdAt,
    };
  }
  if (block.type === "financial_plan_reference") {
    return {
      id: block.id,
      propertyId,
      title: block.title,
      detail: block.detail,
      source: block.source,
      kind: "plan",
      catalogKey: block.catalogKey,
      labels: uniqueLabels(block.labels),
      planHref: block.planHref,
      createdAt: block.createdAt,
    };
  }
  if (block.type === "evidence_reference") {
    return {
      id: block.id,
      propertyId,
      title: block.title,
      detail: block.detail,
      source: block.source,
      kind: block.selectionText ? "selection" : "fact",
      catalogKey: block.catalogKey,
      selectionText: block.selectionText,
      labels: uniqueLabels(block.labels),
      createdAt: block.createdAt,
    };
  }
  const noteBlock: NotebookCommandBlock = block.type === "checklist"
    ? { type: "checklist", collapsed: block.collapsed, items: block.items }
    : { type: "fields", collapsed: block.collapsed, fields: block.fields };
  return {
    id: block.id,
    propertyId,
    title: block.title,
    kind: "handwritten",
    catalogKey: block.catalogKey,
    labels: uniqueLabels(block.labels),
    block: noteBlock,
    createdAt: block.createdAt,
  };
}

function projectNotes(documents: Record<string, NotebookDocument>): NotebookNote[] {
  return Object.values(documents)
    .flatMap((document) => document.blocks.map((block) => noteFromBlock(document.propertyId, block)))
    .slice(0, MAX_NOTEBOOK_NOTES);
}

function documentsFromNotes(notes: NotebookNote[]): Record<string, NotebookDocument> {
  const documents: Record<string, NotebookDocument> = {};
  for (const note of notes) {
    documents[note.propertyId] ??= { propertyId: note.propertyId, blocks: [] };
    documents[note.propertyId].blocks.push(blockFromNote(note));
  }
  return documents;
}

function normalizeNotebookBlock(raw: unknown): NotebookBlock | null {
  if (typeof raw !== "object" || raw == null) return null;
  const block = raw as Partial<NotebookBlock> & Record<string, unknown>;
  const id = typeof block.id === "string" ? block.id : noteId("b");
  const createdAt = typeof block.createdAt === "number" ? block.createdAt : Date.now();
  if (block.type === "paragraph" && typeof block.text === "string") {
    return { id, type: "paragraph", text: block.text, createdAt };
  }
  if (block.type === "checklist" && Array.isArray(block.items)) {
    return {
      id,
      type: "checklist",
      title: typeof block.title === "string" ? block.title : "Checklist",
      collapsed: block.collapsed === true,
      items: block.items.map(normalizeChecklistItem).filter((item): item is NotebookChecklistItem => item != null),
      labels: uniqueLabels(Array.isArray(block.labels) ? block.labels.filter((item): item is string => typeof item === "string") : []),
      catalogKey: typeof block.catalogKey === "string" ? block.catalogKey : `block:${id}`,
      createdAt,
    };
  }
  if (block.type === "fields" && Array.isArray(block.fields)) {
    return {
      id,
      type: "fields",
      title: typeof block.title === "string" ? block.title : "Fields",
      collapsed: block.collapsed === true,
      fields: block.fields.map(normalizeFieldItem).filter((field): field is NotebookFieldItem => field != null),
      labels: uniqueLabels(Array.isArray(block.labels) ? block.labels.filter((item): item is string => typeof item === "string") : []),
      catalogKey: typeof block.catalogKey === "string" ? block.catalogKey : `block:${id}`,
      createdAt,
    };
  }
  if (block.type === "evidence_reference" && typeof block.title === "string" && typeof block.catalogKey === "string") {
    return {
      id,
      type: "evidence_reference",
      title: block.title,
      detail: typeof block.detail === "string" ? block.detail : undefined,
      source: typeof block.source === "string" ? block.source : undefined,
      catalogKey: block.catalogKey,
      selectionText: typeof block.selectionText === "string" ? block.selectionText : undefined,
      labels: uniqueLabels(Array.isArray(block.labels) ? block.labels.filter((item): item is string => typeof item === "string") : []),
      createdAt,
    };
  }
  if (block.type === "financial_plan_reference" && typeof block.title === "string" && typeof block.catalogKey === "string") {
    return {
      id,
      type: "financial_plan_reference",
      title: block.title,
      detail: typeof block.detail === "string" ? block.detail : undefined,
      source: typeof block.source === "string" ? block.source : undefined,
      catalogKey: block.catalogKey,
      labels: uniqueLabels(Array.isArray(block.labels) ? block.labels.filter((item): item is string => typeof item === "string") : []),
      createdAt,
      planHref: typeof block.planHref === "string" ? block.planHref : "",
    };
  }
  return null;
}

function normalizeDocuments(raw: unknown): Record<string, NotebookDocument> {
  if (typeof raw !== "object" || raw == null) return {};
  const documents: Record<string, NotebookDocument> = {};
  for (const [propertyId, value] of Object.entries(raw as Record<string, unknown>)) {
    if (typeof value !== "object" || value == null) continue;
    const candidate = value as Partial<NotebookDocument>;
    const id = typeof candidate.propertyId === "string" ? candidate.propertyId : propertyId;
    const blocks = Array.isArray(candidate.blocks)
      ? candidate.blocks
        .map(normalizeNotebookBlock)
        .filter((block): block is NotebookBlock => block != null)
      : [];
    documents[id] = { propertyId: id, blocks };
  }
  return documents;
}

function readRawState(): NotebookState {
  try {
    const raw = window.localStorage.getItem(NOTEBOOK_STORAGE_KEY)
      ?? window.localStorage.getItem("openestates:buyer-notebook-v1");
    if (!raw) return EMPTY;
    const parsed = JSON.parse(raw) as Partial<NotebookState>;
    const propertyIds = Array.isArray(parsed.propertyIds) ? parsed.propertyIds.filter(Boolean) : [];
    const compareIds = Array.isArray(parsed.compareIds)
      ? [...new Set(parsed.compareIds.map((id) => id.trim()).filter(Boolean))]
        .slice(0, MAX_COMPARE_FROM_NOTEBOOK)
      : [];
    const hiddenCompareLabels = Array.isArray(parsed.hiddenCompareLabels)
      ? parsed.hiddenCompareLabels.filter((item): item is NotebookLabelId => typeof item === "string")
      : [];
    const documents = parsed.version === NOTEBOOK_SCHEMA_VERSION
      ? normalizeDocuments(parsed.documents)
      : documentsFromNotes(Array.isArray(parsed.notes)
        ? parsed.notes
          .map((note) => normalizeNote(note as Record<string, unknown>))
          .filter((note): note is NotebookNote => note != null)
          .slice(0, MAX_NOTEBOOK_NOTES)
        : []);
    const state: NotebookState = {
      version: NOTEBOOK_SCHEMA_VERSION,
      propertyIds: [...new Set([...propertyIds, ...Object.keys(documents)])],
      documents,
      notes: projectNotes(documents),
      compareIds,
      hiddenCompareLabels,
    };
    if (parsed.version !== NOTEBOOK_SCHEMA_VERSION) {
      window.localStorage.setItem(NOTEBOOK_STORAGE_KEY, JSON.stringify({
        version: NOTEBOOK_SCHEMA_VERSION,
        propertyIds: state.propertyIds,
        documents: state.documents,
        compareIds: state.compareIds,
        hiddenCompareLabels: state.hiddenCompareLabels,
      }));
    }
    return state;
  } catch {
    return EMPTY;
  }
}

function completeState(state: Partial<NotebookState>): NotebookState {
  const documents = state.documents ?? documentsFromNotes(state.notes ?? []);
  return {
    version: NOTEBOOK_SCHEMA_VERSION,
    propertyIds: [...new Set([...(state.propertyIds ?? []), ...Object.keys(documents)])],
    documents,
    notes: projectNotes(documents),
    compareIds: state.compareIds ?? [],
    hiddenCompareLabels: state.hiddenCompareLabels ?? [],
  };
}

export function readNotebook(): NotebookState {
  const state = readRawState();
  const shortlist = readShortlistIds();
  const propertyIds = [...new Set([...shortlist, ...state.propertyIds])];
  if (shortlist.length === 0 && propertyIds.length === state.propertyIds.length) return state;
  return {
    ...state,
    propertyIds,
    compareIds: [...new Set(state.compareIds)].slice(0, MAX_COMPARE_FROM_NOTEBOOK),
  };
}

export function writeNotebook(state: Partial<NotebookState>): NotebookState {
  const completed = completeState(state);
  const next: NotebookState = {
    version: NOTEBOOK_SCHEMA_VERSION,
    propertyIds: [...new Set(completed.propertyIds)],
    documents: completed.documents,
    notes: completed.notes.slice(0, MAX_NOTEBOOK_NOTES),
    compareIds: [...new Set(completed.compareIds.map((id) => id.trim()).filter(Boolean))]
      .slice(0, MAX_COMPARE_FROM_NOTEBOOK),
    hiddenCompareLabels: [...new Set(completed.hiddenCompareLabels ?? [])],
  };
  window.localStorage.setItem(NOTEBOOK_STORAGE_KEY, JSON.stringify({
    version: NOTEBOOK_SCHEMA_VERSION,
    propertyIds: next.propertyIds,
    documents: next.documents,
    compareIds: next.compareIds,
    hiddenCompareLabels: next.hiddenCompareLabels,
  }));
  emit(next);
  return next;
}

function ensureProperty(state: NotebookState, propertyId: string): NotebookState {
  const documents = state.documents[propertyId]
    ? state.documents
    : {
        ...state.documents,
        [propertyId]: { propertyId, blocks: [] },
      };
  const propertyIds = state.propertyIds.includes(propertyId)
    ? state.propertyIds
    : [propertyId, ...state.propertyIds];
  return { ...state, propertyIds, documents, notes: projectNotes(documents) };
}

function updateDocument(
  state: NotebookState,
  propertyId: string,
  updater: (blocks: NotebookBlock[]) => NotebookBlock[],
): NotebookState {
  const withProp = ensureProperty(state, propertyId);
  const document = withProp.documents[propertyId] ?? { propertyId, blocks: [] };
  const documents = {
    ...withProp.documents,
    [propertyId]: {
      propertyId,
      blocks: updater(document.blocks),
    },
  };
  return { ...withProp, documents, notes: projectNotes(documents) };
}

function insertBlock(
  blocks: NotebookBlock[],
  block: NotebookBlock,
  afterBlockId?: string,
  replaceBlockId?: string,
): NotebookBlock[] {
  const withoutReplacement = replaceBlockId
    ? blocks.filter((item) => item.id !== replaceBlockId)
    : blocks;
  const anchor = replaceBlockId ?? afterBlockId;
  const originalIndex = anchor ? blocks.findIndex((item) => item.id === anchor) : -1;
  const replacementAdjustment = replaceBlockId && originalIndex >= 0 ? 0 : 1;
  const insertAt = originalIndex >= 0
    ? Math.min(withoutReplacement.length, originalIndex + replacementAdjustment)
    : withoutReplacement.length;
  return [
    ...withoutReplacement.slice(0, insertAt),
    block,
    ...withoutReplacement.slice(insertAt),
  ];
}

function blockByCatalogKey(state: NotebookState, catalogKey: string): { propertyId: string; block: NotebookBlock } | null {
  for (const document of Object.values(state.documents)) {
    const block = document.blocks.find((item) =>
      "catalogKey" in item && item.catalogKey === catalogKey
    );
    if (block) return { propertyId: document.propertyId, block };
  }
  return null;
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
  const existing = blockByCatalogKey(state, input.catalogKey);
  if (existing) {
    return writeNotebook(updateDocument(state, existing.propertyId, (blocks) =>
      blocks.filter((block) => block.id !== existing.block.id)
    ));
  }

  const createdAt = Date.now();
  const block: NotebookBlock = input.kind === "plan"
    ? {
        id: noteId(),
        type: "financial_plan_reference",
        title: input.title,
        detail: input.detail,
        source: input.source,
        catalogKey: input.catalogKey,
        labels: uniqueLabels(input.labels),
        createdAt,
        planHref: `/workspace/buy-vs-rent/${encodeURIComponent(input.propertyId)}`,
      }
    : {
        id: noteId(),
        type: "evidence_reference",
        title: input.title,
        detail: input.detail,
        source: input.source,
        catalogKey: input.catalogKey,
        labels: uniqueLabels(input.labels),
        createdAt,
      };
  return writeNotebook(updateDocument(state, input.propertyId, (blocks) => [...blocks, block]));
}

export function upsertCatalogNote(input: {
  propertyId: string;
  catalogKey: string;
  title: string;
  labels: NotebookLabelId[];
  detail?: string;
  source?: string;
  kind?: NotebookNoteKind;
}): NotebookState {
  const state = readNotebook();
  const existing = blockByCatalogKey(state, input.catalogKey);
  const createdAt = existing?.block.createdAt ?? Date.now();
  const nextBlock: NotebookBlock = input.kind === "plan"
    ? {
        id: existing?.block.id ?? noteId(),
        type: "financial_plan_reference",
        title: input.title,
        detail: input.detail,
        source: input.source,
        catalogKey: input.catalogKey,
        labels: uniqueLabels(input.labels),
        createdAt,
        planHref: `/workspace/buy-vs-rent/${encodeURIComponent(input.propertyId)}`,
      }
    : {
        id: existing?.block.id ?? noteId(),
        type: "evidence_reference",
        title: input.title,
        detail: input.detail,
        source: input.source,
        catalogKey: input.catalogKey,
        labels: uniqueLabels(input.labels),
        createdAt,
      };

  return writeNotebook(updateDocument(state, input.propertyId, (blocks) => (
    existing
      ? blocks.map((block) => block.id === existing.block.id ? nextBlock : block)
      : [...blocks, nextBlock]
  )));
}

export function upsertContextualNote(input: {
  propertyId: string;
  catalogKey: string;
  title: string;
  text: string;
  labels: NotebookLabelId[];
  detail?: string;
  source?: string;
}): NotebookState | null {
  const selectionText = input.text.trim();
  if (!selectionText) return null;

  const state = readNotebook();
  const existing = blockByCatalogKey(state, input.catalogKey);
  const block: EvidenceReferenceBlock = {
    id: existing?.block.id ?? noteId(),
    type: "evidence_reference",
    title: input.title,
    detail: input.detail,
    source: input.source,
    catalogKey: input.catalogKey,
    selectionText,
    labels: uniqueLabels(input.labels),
    createdAt: existing?.block.createdAt ?? Date.now(),
  };

  return writeNotebook(updateDocument(state, input.propertyId, (blocks) => (
    existing
      ? blocks.map((current) => current.id === existing.block.id ? block : current)
      : [...blocks, block]
  )));
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
  if (blockByCatalogKey(state, catalogKey)) return state;
  const block: EvidenceReferenceBlock = {
    id: noteId(),
    title: summary,
    type: "evidence_reference",
    detail: "Selected from evidence",
    source: input.source ?? "Selection",
    catalogKey,
    selectionText: trimmed,
    labels,
    createdAt: Date.now(),
  };
  return writeNotebook(updateDocument(state, input.propertyId, (blocks) => [...blocks, block]));
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
  const block: ParagraphBlock = {
    id: noteId(),
    type: "paragraph",
    text: trimmed,
    createdAt: Date.now(),
  };
  const state = readNotebook();
  return writeNotebook(updateDocument(state, input.propertyId, (blocks) => [...blocks, block]));
}

function commandLabels(command: NotebookCommand): NotebookLabelId[] {
  if (command.id === "budget") return ["finance"];
  if (command.id === "payment") return ["legal"];
  if (command.id === "visit") return ["visit"];
  return [];
}

function blockFromCommand(command: NotebookCommand, propertyId: string): ChecklistBlock | FieldBlock {
  const createdAt = Date.now();
  const labels = uniqueLabels(commandLabels(command));
  const catalogKey = `block:${propertyId}:${command.id}:${createdAt}`;
  if (command.blockType === "fields") {
    return {
      id: noteId(),
      type: "fields",
      title: command.title,
      collapsed: false,
      fields: (command.fields ?? []).map((label) => ({
        id: noteId("f"),
        label,
        value: "",
      })),
      labels,
      catalogKey,
      createdAt,
    };
  }
  return {
    id: noteId(),
    type: "checklist",
    title: command.title,
    collapsed: false,
    items: (command.items ?? ["New item"]).map((text) => ({
      id: noteId("i"),
      text,
      checked: false,
    })),
    labels,
    catalogKey,
    createdAt,
  };
}

export function addNotebookCommandBlock(input: {
  propertyId: string;
  commandId: NotebookCommand["id"];
  afterBlockId?: string;
  replaceBlockId?: string;
}): NotebookState | null {
  const command = NOTEBOOK_COMMANDS.find((item) => item.id === input.commandId);
  if (!command) return null;
  const state = readNotebook();
  return writeNotebook(updateDocument(state, input.propertyId, (blocks) =>
    insertBlock(blocks, blockFromCommand(command, input.propertyId), input.afterBlockId, input.replaceBlockId)
  ));
}

export function addNotebookParagraphAfter(input: {
  propertyId: string;
  afterBlockId?: string;
}): NotebookState {
  const state = readNotebook();
  const block: ParagraphBlock = {
    id: noteId(),
    type: "paragraph",
    text: "",
    createdAt: Date.now(),
  };
  return writeNotebook(updateDocument(state, input.propertyId, (blocks) =>
    insertBlock(blocks, block, input.afterBlockId)
  ));
}

function updateBlock(block: NotebookBlock, patch: Partial<Pick<NotebookNote, "title" | "block">>): NotebookBlock {
  if (block.type === "paragraph") {
    return { ...block, text: patch.title ?? block.text };
  }
  if (block.type === "checklist" && patch.block?.type === "checklist") {
    return {
      ...block,
      title: patch.title ?? block.title,
      collapsed: patch.block.collapsed,
      items: patch.block.items,
    };
  }
  if (block.type === "fields" && patch.block?.type === "fields") {
    return {
      ...block,
      title: patch.title ?? block.title,
      collapsed: patch.block.collapsed,
      fields: patch.block.fields,
    };
  }
  if (block.type === "evidence_reference" || block.type === "financial_plan_reference") {
    return { ...block, title: patch.title ?? block.title };
  }
  return block;
}

export function updateNotebookNote(
  noteIdToUpdate: string,
  patch: Partial<Pick<NotebookNote, "title" | "block">>,
): NotebookState {
  const state = readNotebook();
  for (const document of Object.values(state.documents)) {
    if (!document.blocks.some((block) => block.id === noteIdToUpdate)) continue;
    return writeNotebook(updateDocument(state, document.propertyId, (blocks) =>
      blocks.map((block) => block.id === noteIdToUpdate ? updateBlock(block, patch) : block)
    ));
  }
  return state;
}

export function removeNotebookNote(noteId: string): NotebookState {
  const state = readNotebook();
  for (const document of Object.values(state.documents)) {
    if (!document.blocks.some((block) => block.id === noteId)) continue;
    return writeNotebook(updateDocument(state, document.propertyId, (blocks) =>
      blocks.filter((block) => block.id !== noteId)
    ));
  }
  return state;
}

export function setNotebookNoteLabels(noteId: string, labels: NotebookLabelId[]): NotebookState {
  const state = readNotebook();
  return writeNotebook({
    ...state,
    documents: Object.fromEntries(Object.entries(state.documents).map(([propertyId, document]) => [
      propertyId,
      {
        ...document,
        blocks: document.blocks.map((block) =>
          block.id === noteId && "labels" in block ? { ...block, labels: uniqueLabels(labels) } : block
        ),
      },
    ])),
  });
}

export function addNotebookNoteLabel(noteId: string, label: NotebookLabelId): NotebookState {
  const state = readNotebook();
  const note = state.notes.find((item) => item.id === noteId);
  return setNotebookNoteLabels(noteId, uniqueLabels([...(note?.labels ?? []), label]));
}

export function removeNotebookNoteLabel(noteId: string, label: NotebookLabelId): NotebookState {
  const state = readNotebook();
  const note = state.notes.find((item) => item.id === noteId);
  return setNotebookNoteLabels(noteId, (note?.labels ?? []).filter((item) => item !== label));
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

export function setNotebookCompareIds(propertyIds: string[]): NotebookState {
  const state = readNotebook();
  return writeNotebook({ ...state, compareIds: propertyIds });
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
  const documents = { ...state.documents };
  delete documents[propertyId];
  return writeNotebook({
    propertyIds: state.propertyIds.filter((id) => id !== propertyId),
    documents,
    compareIds: state.compareIds.filter((id) => id !== propertyId),
    hiddenCompareLabels: state.hiddenCompareLabels,
  });
}

export function detachNotebookPropertyFromShortlist(propertyId: string): NotebookState {
  const state = readNotebook();
  const documents = { ...state.documents };
  const hasBuyerNotes = (documents[propertyId]?.blocks.length ?? 0) > 0;
  if (!hasBuyerNotes) delete documents[propertyId];
  return writeNotebook({
    ...state,
    propertyIds: hasBuyerNotes
      ? state.propertyIds
      : state.propertyIds.filter((id) => id !== propertyId),
    documents,
    compareIds: state.compareIds.filter((id) => id !== propertyId),
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
