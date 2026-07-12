const STORAGE_KEY = "openestates_shortlist";

export type DecisionTag = "watching" | "finalist" | "verify" | "stretch";

export type ShortlistItem = {
  id: string;
  tag: DecisionTag;
  note: string;
  addedAt: string;
  updatedAt: string;
};

type StoredShortlist = {
  version: 2;
  items: ShortlistItem[];
};

const DEFAULT_TAG: DecisionTag = "watching";

function nowIso(): string {
  return new Date().toISOString();
}

function makeItem(id: string): ShortlistItem {
  const ts = nowIso();
  return {
    id,
    tag: DEFAULT_TAG,
    note: "",
    addedAt: ts,
    updatedAt: ts,
  };
}

function parseStoredShortlist(): StoredShortlist {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return { version: 2, items: [] };

    const parsed = JSON.parse(raw);

    if (Array.isArray(parsed)) {
      return {
        version: 2,
        items: parsed
          .filter((id): id is string => typeof id === "string" && id.length > 0)
          .map(makeItem),
      };
    }

    if (parsed && Array.isArray(parsed.items)) {
      return {
        version: 2,
        items: parsed.items
          .filter((item: unknown): item is Partial<ShortlistItem> & { id: string } => {
            return typeof item === "object" && item !== null && typeof (item as ShortlistItem).id === "string";
          })
          .map((item: Partial<ShortlistItem> & { id: string }) => ({
            id: item.id,
            tag: isDecisionTag(item.tag) ? item.tag : DEFAULT_TAG,
            note: typeof item.note === "string" ? item.note : "",
            addedAt: typeof item.addedAt === "string" ? item.addedAt : nowIso(),
            updatedAt: typeof item.updatedAt === "string" ? item.updatedAt : nowIso(),
          })),
      };
    }
  } catch {
    return { version: 2, items: [] };
  }

  return { version: 2, items: [] };
}

function isDecisionTag(value: unknown): value is DecisionTag {
  return value === "watching" || value === "finalist" || value === "verify" || value === "stretch";
}

function persistShortlist(shortlist: StoredShortlist): void {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(shortlist));
}

export function getShortlistItems(): ShortlistItem[] {
  return parseStoredShortlist().items;
}

export function getShortlistedIds(): string[] {
  return getShortlistItems().map((item) => item.id);
}

export function isShortlisted(id: string): boolean {
  return getShortlistedIds().includes(id);
}

export function addToShortlist(id: string): void {
  const shortlist = parseStoredShortlist();
  if (!shortlist.items.some((item) => item.id === id)) {
    shortlist.items.push(makeItem(id));
    persistShortlist(shortlist);
  }
}

export function removeFromShortlist(id: string): void {
  const shortlist = parseStoredShortlist();
  shortlist.items = shortlist.items.filter((item) => item.id !== id);
  persistShortlist(shortlist);
}

export function toggleShortlist(id: string): boolean {
  const shortlist = parseStoredShortlist();
  const index = shortlist.items.findIndex((item) => item.id === id);
  if (index >= 0) {
    shortlist.items.splice(index, 1);
    persistShortlist(shortlist);
    return false; // removed
  } else {
    shortlist.items.push(makeItem(id));
    persistShortlist(shortlist);
    return true; // added
  }
}

export function updateShortlistItem(id: string, patch: Partial<Pick<ShortlistItem, "tag" | "note">>): ShortlistItem[] {
  const shortlist = parseStoredShortlist();
  shortlist.items = shortlist.items.map((item) => {
    if (item.id !== id) return item;
    return {
      ...item,
      ...patch,
      updatedAt: nowIso(),
    };
  });
  persistShortlist(shortlist);
  return shortlist.items;
}
