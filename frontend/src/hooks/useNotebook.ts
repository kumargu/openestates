import { useCallback, useEffect, useState } from "react";
import { SHORTLIST_CHANGED_EVENT } from "../lib/compare.ts";
import {
  NOTEBOOK_CHANGED_EVENT,
  addHandwrittenNote,
  addNotebookNoteLabel,
  addSelectionNote,
  anchorNotebookProperty,
  isCatalogPinned,
  readNotebook,
  removeNotebookNote,
  removeNotebookNoteLabel,
  removeNotebookProperty,
  setNotebookNoteLabels,
  toggleCatalogNote,
  toggleNotebookCompareId,
  type NotebookLabelId,
  type NotebookNote,
  type NotebookState,
} from "../lib/notebook.ts";

function isNotebookState(value: unknown): value is NotebookState {
  if (typeof value !== "object" || value == null) return false;
  const candidate = value as Partial<NotebookState>;
  return Array.isArray(candidate.propertyIds)
    && Array.isArray(candidate.notes)
    && Array.isArray(candidate.compareIds);
}

export function useNotebook() {
  const [state, setState] = useState<NotebookState>(() =>
    typeof window === "undefined" ? { propertyIds: [], notes: [], compareIds: [] } : readNotebook(),
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

  const removeNote = useCallback((noteId: string) => {
    setState(removeNotebookNote(noteId));
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
    propertyIds: state.propertyIds,
    compareIds: state.compareIds,
    isPinned: (catalogKey: string) => isCatalogPinned(catalogKey, state),
    toggleFact,
    rememberSelection,
    addHandwritten,
    removeNote,
    setNoteLabels,
    addNoteLabel,
    removeNoteLabel,
    toggleCompare,
    removeProperty,
    anchorProperty,
    notesFor,
  };
}
