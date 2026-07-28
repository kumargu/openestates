import {
  createContext,
  useCallback,
  useContext,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import {
  CATALOG,
  PLAN_PINS,
  PROPERTIES,
  guessTag,
  type CatalogFact,
  type NoteKind,
  type NoteMark,
  type PropertyId,
  type TagId,
} from "./data.ts";

export type NotebookNote = {
  id: string;
  propertyId: PropertyId;
  kind: NoteKind;
  mark: NoteMark;
  tag: TagId;
  label: string;
  detail?: string;
  source?: string;
  catalogId?: string;
  /** Full selected text when created via select-text Remember. */
  selectionText?: string;
  createdAt: number;
};

export type Toast = {
  id: number;
  text: string;
  undo?: () => void;
};

export type NotebookView = "list" | "board" | "by-home";

type NotebookContextValue = {
  notes: NotebookNote[];
  propertyIds: PropertyId[];
  compareIds: PropertyId[];
  focusedId: PropertyId;
  pulse: boolean;
  toast: Toast | null;
  notebookView: NotebookView;
  compareStyle: "columns" | "matrix" | "mobile";
  setFocusedId: (id: PropertyId) => void;
  setNotebookView: (s: NotebookView) => void;
  setCompareStyle: (s: "columns" | "matrix" | "mobile") => void;
  isPropertyInNotebook: (id: PropertyId) => boolean;
  isCatalogSaved: (catalogId: string) => boolean;
  toggleProperty: (id: PropertyId) => void;
  toggleCatalog: (fact: CatalogFact) => void;
  /** Handwritten notes — Notebook page only. */
  addHandwritten: (propertyId: PropertyId, text: string, tag?: TagId) => void;
  /** Select-text Remember from RERA-style blocks (structured pin, not free composer). */
  addSelectionNote: (args: {
    propertyId: PropertyId;
    text: string;
    tag: TagId;
    source: string;
    mark?: NoteMark;
  }) => void;
  setNoteTag: (noteId: string, tag: TagId) => void;
  removeNote: (noteId: string) => void;
  toggleCompare: (id: PropertyId) => void;
  clearCompare: () => void;
  seedDemo: () => void;
  resetAll: () => void;
};

const NotebookContext = createContext<NotebookContextValue | null>(null);

let toastSeq = 0;
let noteSeq = 0;

function nextNoteId(prefix: string): string {
  noteSeq += 1;
  return `${prefix}-${noteSeq}`;
}

function summarizeSelection(text: string): string {
  const cleaned = text.replace(/\s+/g, " ").trim();
  if (cleaned.length <= 72) return cleaned;
  return `${cleaned.slice(0, 70).trimEnd()}…`;
}

export function NotebookProvider({ children }: { children: ReactNode }) {
  const [notes, setNotes] = useState<NotebookNote[]>([]);
  const [anchors, setAnchors] = useState<PropertyId[]>([]);
  const [compareIds, setCompareIds] = useState<PropertyId[]>([]);
  const [focusedId, setFocusedId] = useState<PropertyId>("waterford");
  const [pulse, setPulse] = useState(false);
  const [toast, setToast] = useState<Toast | null>(null);
  const [notebookView, setNotebookView] = useState<NotebookView>("list");
  const [compareStyle, setCompareStyle] = useState<"columns" | "matrix" | "mobile">(
    "columns",
  );

  const flash = useCallback((text: string, undo?: () => void) => {
    const id = ++toastSeq;
    setToast({ id, text, undo });
    setPulse(true);
    window.setTimeout(() => setPulse(false), 700);
    window.setTimeout(() => {
      setToast((t) => (t?.id === id ? null : t));
    }, 2800);
  }, []);

  const ensureAnchor = useCallback((propertyId: PropertyId) => {
    setAnchors((prev) => (prev.includes(propertyId) ? prev : [...prev, propertyId]));
  }, []);

  const isPropertyInNotebook = useCallback(
    (id: PropertyId) => anchors.includes(id),
    [anchors],
  );

  const isCatalogSaved = useCallback(
    (catalogId: string) => notes.some((n) => n.catalogId === catalogId),
    [notes],
  );

  const toggleProperty = useCallback(
    (id: PropertyId) => {
      setAnchors((prev) => {
        if (prev.includes(id)) {
          const next = prev.filter((x) => x !== id);
          setNotes((ns) => ns.filter((n) => n.propertyId !== id));
          setCompareIds((c) => c.filter((x) => x !== id));
          flash("Removed from notebook · Undo", () => {
            setAnchors((a) => (a.includes(id) ? a : [...a, id]));
          });
          return next;
        }
        flash("Added to your notebook");
        return [...prev, id];
      });
      setFocusedId(id);
    },
    [flash],
  );

  const toggleCatalog = useCallback(
    (fact: CatalogFact) => {
      setNotes((prev) => {
        const existing = prev.find((n) => n.catalogId === fact.id);
        if (existing) {
          flash("Removed · Undo", () => {
            setNotes((ns) =>
              ns.some((n) => n.catalogId === fact.id) ? ns : [...ns, existing],
            );
            ensureAnchor(fact.propertyId);
          });
          return prev.filter((n) => n.id !== existing.id);
        }
        ensureAnchor(fact.propertyId);
        setFocusedId(fact.propertyId);
        flash(`Saved · ${fact.tag.replace(/-/g, " ")}`);
        return [
          ...prev,
          {
            id: nextNoteId("ui"),
            propertyId: fact.propertyId,
            kind: fact.kind,
            mark: fact.mark,
            tag: fact.tag,
            label: fact.label,
            detail: fact.detail,
            source: fact.source,
            catalogId: fact.id,
            createdAt: Date.now(),
          },
        ];
      });
    },
    [ensureAnchor, flash],
  );

  const addHandwritten = useCallback(
    (propertyId: PropertyId, text: string, tag?: TagId) => {
      const trimmed = text.trim();
      if (!trimmed) return;
      const resolved = tag ?? guessTag(trimmed);
      ensureAnchor(propertyId);
      setFocusedId(propertyId);
      setNotes((prev) => [
        ...prev,
        {
          id: nextNoteId("hand"),
          propertyId,
          kind: "handwritten",
          mark: trimmed.endsWith("?") ? "question" : "note",
          tag: resolved,
          label: trimmed,
          source: "You",
          createdAt: Date.now(),
        },
      ]);
      flash(`Note added · ${resolved.replace(/-/g, " ")}`);
    },
    [ensureAnchor, flash],
  );

  const addSelectionNote = useCallback(
    ({
      propertyId,
      text,
      tag,
      source,
      mark = "concern",
    }: {
      propertyId: PropertyId;
      text: string;
      tag: TagId;
      source: string;
      mark?: NoteMark;
    }) => {
      const trimmed = text.replace(/\s+/g, " ").trim();
      if (trimmed.length < 8) return;
      const summary = summarizeSelection(trimmed);
      const catalogId = `sel:${propertyId}:${summary.slice(0, 40)}`;
      ensureAnchor(propertyId);
      setFocusedId(propertyId);
      setNotes((prev) => {
        if (prev.some((n) => n.catalogId === catalogId)) {
          flash("Already in notebook");
          return prev;
        }
        flash(`Remembered · ${tag.replace(/-/g, " ")}`);
        return [
          ...prev,
          {
            id: nextNoteId("sel"),
            propertyId,
            kind: "fact",
            mark,
            tag,
            label: summary,
            detail: "Selected from evidence",
            source,
            catalogId,
            selectionText: trimmed,
            createdAt: Date.now(),
          },
        ];
      });
    },
    [ensureAnchor, flash],
  );

  const setNoteTag = useCallback((noteId: string, tag: TagId) => {
    setNotes((prev) => prev.map((n) => (n.id === noteId ? { ...n, tag } : n)));
  }, []);

  const removeNote = useCallback(
    (noteId: string) => {
      setNotes((prev) => {
        const victim = prev.find((n) => n.id === noteId);
        if (!victim) return prev;
        flash("Removed · Undo", () => {
          setNotes((ns) => (ns.some((n) => n.id === noteId) ? ns : [...ns, victim]));
          ensureAnchor(victim.propertyId);
        });
        return prev.filter((n) => n.id !== noteId);
      });
    },
    [ensureAnchor, flash],
  );

  const toggleCompare = useCallback((id: PropertyId) => {
    setCompareIds((prev) => {
      if (prev.includes(id)) return prev.filter((x) => x !== id);
      if (prev.length >= 3) return prev;
      return [...prev, id];
    });
  }, []);

  const clearCompare = useCallback(() => setCompareIds([]), []);

  const seedDemo = useCallback(() => {
    const picks = [
      "wf-schools",
      "wf-water",
      "wf-oc",
      "wf-price",
      "wf-layout",
      "wf-complaint",
      "wf-completion",
      "da-schools",
      "da-storage",
      "da-water",
      "da-price",
      "pr-park",
      "pr-commute",
      "pr-schools",
      "money-down",
      "money-emi",
      "wf-gap",
      "da-buffer",
    ];
    const fromCatalog = [...CATALOG, ...PLAN_PINS]
      .filter((f) => picks.includes(f.id))
      .map((fact) => ({
        id: nextNoteId("seed"),
        propertyId: fact.propertyId,
        kind: fact.kind,
        mark: fact.mark,
        tag: fact.tag,
        label: fact.label,
        detail: fact.detail,
        source: fact.source,
        catalogId: fact.id,
        createdAt: Date.now() - Math.floor(Math.random() * 86_400_000),
      }));

    const handwritten: NotebookNote[] = [
      {
        id: nextNoteId("seed"),
        propertyId: "waterford",
        kind: "handwritten",
        mark: "note",
        tag: "layout",
        label: "Kitchen felt larger than Dream Acres.",
        source: "You",
        createdAt: Date.now() - 3600_000,
      },
      {
        id: nextNoteId("seed"),
        propertyId: "waterford",
        kind: "handwritten",
        mark: "question",
        tag: "water",
        label: "Is Kaveri operational for this tower?",
        source: "You",
        createdAt: Date.now() - 1800_000,
      },
      {
        id: nextNoteId("seed"),
        propertyId: "waterford",
        kind: "handwritten",
        mark: "note",
        tag: "down-payment",
        label: "Can stretch ₹4 L from PF if this wins.",
        source: "You",
        createdAt: Date.now() - 900_000,
      },
      {
        id: nextNoteId("seed"),
        propertyId: "dream-acres",
        kind: "handwritten",
        mark: "note",
        tag: "layout",
        label: "Storage better — utility felt compact.",
        source: "You",
        createdAt: Date.now() - 7200_000,
      },
    ];

    setAnchors(PROPERTIES.map((p) => p.id));
    setNotes([...fromCatalog, ...handwritten]);
    setCompareIds(["waterford", "dream-acres"]);
    setFocusedId("waterford");
    setNotebookView("list");
    flash("Demo notebook loaded · try Board by tag");
  }, [flash]);

  const resetAll = useCallback(() => {
    setNotes([]);
    setAnchors([]);
    setCompareIds([]);
    flash("Notebook cleared");
  }, [flash]);

  const value = useMemo<NotebookContextValue>(
    () => ({
      notes,
      propertyIds: anchors,
      compareIds,
      focusedId,
      pulse,
      toast,
      notebookView,
      compareStyle,
      setFocusedId,
      setNotebookView,
      setCompareStyle,
      isPropertyInNotebook,
      isCatalogSaved,
      toggleProperty,
      toggleCatalog,
      addHandwritten,
      addSelectionNote,
      setNoteTag,
      removeNote,
      toggleCompare,
      clearCompare,
      seedDemo,
      resetAll,
    }),
    [
      notes,
      anchors,
      compareIds,
      focusedId,
      pulse,
      toast,
      notebookView,
      compareStyle,
      isPropertyInNotebook,
      isCatalogSaved,
      toggleProperty,
      toggleCatalog,
      addHandwritten,
      addSelectionNote,
      setNoteTag,
      removeNote,
      toggleCompare,
      clearCompare,
      seedDemo,
      resetAll,
    ],
  );

  return <NotebookContext.Provider value={value}>{children}</NotebookContext.Provider>;
}

export function useNotebook(): NotebookContextValue {
  const ctx = useContext(NotebookContext);
  if (!ctx) throw new Error("useNotebook outside provider");
  return ctx;
}
