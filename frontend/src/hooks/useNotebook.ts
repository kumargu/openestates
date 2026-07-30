import { useCallback, useEffect, useState } from "react";
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
  setNotebookNoteLabels,
  showNotebookCompareLabel,
  toggleCatalogNote,
  toggleNotebookCompareId,
  updateNotebookNote,
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

export function useNotebook() {
  const [state, setState] = useState<NotebookState>(() =>
    typeof window === "undefined"
      ? { version: 3, propertyIds: [], documents: {}, notes: [], compareIds: [], hiddenCompareLabels: [] }
      : readNotebook(),
  );

  useEffect(() => {
    function refresh(event?: Event) {
      const detail = (event as CustomEvent<NotebookState> | undefined)?.detail;
      setState(isNotebookState(detail) ? detail : readNotebook());
    }
    window.addEventListener(NOTEBOOK_CHANGED_EVENT, refresh);
    window.addEventListener(SHORTLIST_CHANGED_EVENT, refresh);
    window.addEventListener("storage", refresh);
    return () => {
      window.removeEventListener(NOTEBOOK_CHANGED_EVENT, refresh);
      window.removeEventListener(SHORTLIST_CHANGED_EVENT, refresh);
      window.removeEventListener("storage", refresh);
    };
  }, []);

  const toggleFact = useCallback((input: Parameters<typeof toggleCatalogNote>[0]) => {
    setState(toggleCatalogNote(input));
  }, []);

  const rememberSelection = useCallback((input: Parameters<typeof addSelectionNote>[0]) => {
    const next = addSelectionNote(input);
    if (next) setState(next);
  }, []);

  const addHandwritten = useCallback((input: Parameters<typeof addHandwrittenNote>[0]) => {
    const next = addHandwrittenNote(input);
    if (next) setState(next);
  }, []);

  const addCommandBlock = useCallback((input: Parameters<typeof addNotebookCommandBlock>[0]) => {
    const next = addNotebookCommandBlock(input);
    if (next) setState(next);
  }, []);

  const addParagraphAfter = useCallback((input: Parameters<typeof addNotebookParagraphAfter>[0]) => {
    setState(addNotebookParagraphAfter(input));
  }, []);

  const removeNote = useCallback((noteId: string) => {
    setState(removeNotebookNote(noteId));
  }, []);

  const updateNote = useCallback((noteId: string, patch: Parameters<typeof updateNotebookNote>[1]) => {
    setState(updateNotebookNote(noteId, patch));
  }, []);

  const setNoteLabels = useCallback((noteId: string, labels: NotebookLabelId[]) => {
    setState(setNotebookNoteLabels(noteId, labels));
  }, []);

  const addNoteLabel = useCallback((noteId: string, label: NotebookLabelId) => {
    setState(addNotebookNoteLabel(noteId, label));
  }, []);

  const removeNoteLabel = useCallback((noteId: string, label: NotebookLabelId) => {
    setState(removeNotebookNoteLabel(noteId, label));
  }, []);

  const toggleCompare = useCallback((propertyId: string) => {
    setState(toggleNotebookCompareId(propertyId));
  }, []);

  const hideCompareLabel = useCallback((label: NotebookLabelId) => {
    setState(hideNotebookCompareLabel(label));
  }, []);

  const showCompareLabel = useCallback((label: NotebookLabelId) => {
    setState(showNotebookCompareLabel(label));
  }, []);

  const removeProperty = useCallback((propertyId: string) => {
    setState(removeNotebookProperty(propertyId));
  }, []);

  const anchorProperty = useCallback((propertyId: string) => {
    setState(anchorNotebookProperty(propertyId));
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
    addCommandBlock,
    addParagraphAfter,
    removeNote,
    updateNote,
    setNoteLabels,
    addNoteLabel,
    removeNoteLabel,
    toggleCompare,
    hideCompareLabel,
    showCompareLabel,
    removeProperty,
    anchorProperty,
    notesFor,
  };
}
