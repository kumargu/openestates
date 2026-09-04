import { useCallback, useSyncExternalStore } from "react";
import { SHORTLIST_CHANGED_EVENT } from "../lib/compare.ts";
import {
  NOTEBOOK_CHANGED_EVENT,
  addHandwrittenNote,
  addNotebookCommandBlock,
  addNotebookParagraphAfter,
  addNotebookNoteLabel,
  addSelectionNote,
  anchorNotebookProperty,
  hideNotebookCompareLabel,
  isCatalogPinned,
  readNotebook,
  removeNotebookNote,
  removeNotebookNoteLabel,
  removeNotebookProperty,
  setNotebookCompareIds,
  setNotebookNoteLabels,
  showNotebookCompareLabel,
  toggleCatalogNote,
  toggleNotebookCompareId,
  updateNotebookNote,
  upsertContextualNote,
  type NotebookLabelId,
  type NotebookNote,
  type NotebookState,
} from "../lib/notebook.ts";

function isNotebookState(value: unknown): value is NotebookState {
  if (typeof value !== "object" || value == null) return false;
  const candidate = value as Partial<NotebookState>;
  return Array.isArray(candidate.propertyIds)
    && typeof candidate.documents === "object"
    && candidate.documents != null
    && Array.isArray(candidate.notes)
    && Array.isArray(candidate.compareIds)
    && (candidate.hiddenCompareLabels == null || Array.isArray(candidate.hiddenCompareLabels));
}

const EMPTY_NOTEBOOK: NotebookState = {
  version: 3,
  propertyIds: [],
  documents: {},
  notes: [],
  compareIds: [],
  hiddenCompareLabels: [],
};
const notebookSubscribers = new Set<() => void>();
let notebookSnapshot: NotebookState | null = null;
let listening = false;

function currentNotebookSnapshot(): NotebookState {
  if (!notebookSnapshot) {
    notebookSnapshot = typeof window === "undefined" ? EMPTY_NOTEBOOK : readNotebook();
  }
  return notebookSnapshot;
}

function publishNotebookSnapshot(next: NotebookState): void {
  if (next === notebookSnapshot) return;
  notebookSnapshot = next;
  for (const subscriber of notebookSubscribers) subscriber();
}

function refreshNotebookSnapshot(event?: Event): void {
  const detail = (event as CustomEvent<NotebookState> | undefined)?.detail;
  publishNotebookSnapshot(isNotebookState(detail) ? detail : readNotebook());
}

function startNotebookListeners(): void {
  if (listening) return;
  window.addEventListener(NOTEBOOK_CHANGED_EVENT, refreshNotebookSnapshot);
  window.addEventListener(SHORTLIST_CHANGED_EVENT, refreshNotebookSnapshot);
  window.addEventListener("storage", refreshNotebookSnapshot);
  listening = true;
}

function stopNotebookListeners(): void {
  if (!listening) return;
  window.removeEventListener(NOTEBOOK_CHANGED_EVENT, refreshNotebookSnapshot);
  window.removeEventListener(SHORTLIST_CHANGED_EVENT, refreshNotebookSnapshot);
  window.removeEventListener("storage", refreshNotebookSnapshot);
  listening = false;
}

function subscribeToNotebook(subscriber: () => void): () => void {
  const restarting = notebookSubscribers.size === 0;
  notebookSubscribers.add(subscriber);
  startNotebookListeners();
  if (restarting) refreshNotebookSnapshot();
  return () => {
    notebookSubscribers.delete(subscriber);
    if (notebookSubscribers.size === 0) stopNotebookListeners();
  };
}

/** @internal Coherent external-store boundary shared by React consumers. */
export const notebookExternalStore = {
  getSnapshot: currentNotebookSnapshot,
  subscribe: subscribeToNotebook,
};

export function useNotebook() {
  const state = useSyncExternalStore(
    notebookExternalStore.subscribe,
    notebookExternalStore.getSnapshot,
    () => EMPTY_NOTEBOOK,
  );

  const toggleFact = useCallback((input: Parameters<typeof toggleCatalogNote>[0]) => {
    publishNotebookSnapshot(toggleCatalogNote(input));
  }, []);

  const rememberSelection = useCallback((input: Parameters<typeof addSelectionNote>[0]) => {
    const next = addSelectionNote(input);
    if (next) publishNotebookSnapshot(next);
  }, []);

  const addHandwritten = useCallback((input: Parameters<typeof addHandwrittenNote>[0]) => {
    const next = addHandwrittenNote(input);
    if (next) publishNotebookSnapshot(next);
  }, []);

  const addContextual = useCallback((input: Parameters<typeof upsertContextualNote>[0]) => {
    const next = upsertContextualNote(input);
    if (next) publishNotebookSnapshot(next);
  }, []);

  const addCommandBlock = useCallback((input: Parameters<typeof addNotebookCommandBlock>[0]) => {
    const next = addNotebookCommandBlock(input);
    if (next) publishNotebookSnapshot(next);
  }, []);

  const addParagraphAfter = useCallback((input: Parameters<typeof addNotebookParagraphAfter>[0]) => {
    publishNotebookSnapshot(addNotebookParagraphAfter(input));
  }, []);

  const removeNote = useCallback((noteId: string) => {
    publishNotebookSnapshot(removeNotebookNote(noteId));
  }, []);

  const updateNote = useCallback((noteId: string, patch: Parameters<typeof updateNotebookNote>[1]) => {
    publishNotebookSnapshot(updateNotebookNote(noteId, patch));
  }, []);

  const setNoteLabels = useCallback((noteId: string, labels: NotebookLabelId[]) => {
    publishNotebookSnapshot(setNotebookNoteLabels(noteId, labels));
  }, []);

  const addNoteLabel = useCallback((noteId: string, label: NotebookLabelId) => {
    publishNotebookSnapshot(addNotebookNoteLabel(noteId, label));
  }, []);

  const removeNoteLabel = useCallback((noteId: string, label: NotebookLabelId) => {
    publishNotebookSnapshot(removeNotebookNoteLabel(noteId, label));
  }, []);

  const toggleCompare = useCallback((propertyId: string) => {
    publishNotebookSnapshot(toggleNotebookCompareId(propertyId));
  }, []);

  const setCompareIds = useCallback((propertyIds: string[]) => {
    publishNotebookSnapshot(setNotebookCompareIds(propertyIds));
  }, []);

  const hideCompareLabel = useCallback((label: NotebookLabelId) => {
    publishNotebookSnapshot(hideNotebookCompareLabel(label));
  }, []);

  const showCompareLabel = useCallback((label: NotebookLabelId) => {
    publishNotebookSnapshot(showNotebookCompareLabel(label));
  }, []);

  const removeProperty = useCallback((propertyId: string) => {
    publishNotebookSnapshot(removeNotebookProperty(propertyId));
  }, []);

  const anchorProperty = useCallback((propertyId: string) => {
    publishNotebookSnapshot(anchorNotebookProperty(propertyId));
  }, []);

  const notesFor = useCallback(
    (propertyId: string): NotebookNote[] => state.notes.filter((n) => n.propertyId === propertyId),
    [state.notes],
  );

  return {
    state,
    notes: state.notes,
    documents: state.documents,
    propertyIds: state.propertyIds,
    compareIds: state.compareIds,
    hiddenCompareLabels: state.hiddenCompareLabels ?? [],
    isPinned: (catalogKey: string) => isCatalogPinned(catalogKey, state),
    toggleFact,
    rememberSelection,
    addHandwritten,
    addContextual,
    addCommandBlock,
    addParagraphAfter,
    removeNote,
    updateNote,
    setNoteLabels,
    addNoteLabel,
    removeNoteLabel,
    toggleCompare,
    setCompareIds,
    hideCompareLabel,
    showCompareLabel,
    removeProperty,
    anchorProperty,
    notesFor,
  };
}
