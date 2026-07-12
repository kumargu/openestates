const STORAGE_KEY = "openestates_shortlist";
export const SHEET_UPDATED_EVENT = "oe-sheet-changed";

export type SheetTag = "watching" | "finalist" | "verify" | "stretch";

export type SheetItem = {
  id: string;
  tag: SheetTag;
  note: string;
  addedAt: string;
  updatedAt: string;
};

type StoredSheet = {
  version: 2;
  items: SheetItem[];
};

const DEFAULT_TAG: SheetTag = "watching";

function nowIso(): string {
  return new Date().toISOString();
}

function makeItem(id: string): SheetItem {
  const ts = nowIso();
  return {
    id,
    tag: DEFAULT_TAG,
    note: "",
    addedAt: ts,
    updatedAt: ts,
  };
}

function isSheetTag(value: unknown): value is SheetTag {
  return value === "watching" || value === "finalist" || value === "verify" || value === "stretch";
}

function parseStoredSheet(): StoredSheet {
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
          .filter((item: unknown): item is Partial<SheetItem> & { id: string } => {
            return typeof item === "object" && item !== null && typeof (item as SheetItem).id === "string";
          })
          .map((item: Partial<SheetItem> & { id: string }) => ({
            id: item.id,
            tag: isSheetTag(item.tag) ? item.tag : DEFAULT_TAG,
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

function persistSheet(sheet: StoredSheet): void {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(sheet));
  if (typeof window !== "undefined") {
    window.dispatchEvent(new CustomEvent(SHEET_UPDATED_EVENT));
  }
}

export function getSheetItems(): SheetItem[] {
  return parseStoredSheet().items;
}

export function isOnSheet(id: string): boolean {
  return getSheetItems().some((item) => item.id === id);
}

export function removeFromSheet(id: string): void {
  const sheet = parseStoredSheet();
  sheet.items = sheet.items.filter((item) => item.id !== id);
  persistSheet(sheet);
}

export function toggleSheetItem(id: string): boolean {
  const sheet = parseStoredSheet();
  const index = sheet.items.findIndex((item) => item.id === id);
  if (index >= 0) {
    sheet.items.splice(index, 1);
    persistSheet(sheet);
    return false;
  }

  sheet.items.push(makeItem(id));
  persistSheet(sheet);
  return true;
}
