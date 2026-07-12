const STORAGE_KEY = "openestates_shortlist";

export type KeptHomeTag = "watching" | "finalist" | "verify" | "stretch";

export type KeptHomeItem = {
  id: string;
  tag: KeptHomeTag;
  note: string;
  addedAt: string;
  updatedAt: string;
};

type StoredKeptHomes = {
  version: 2;
  items: KeptHomeItem[];
};

const DEFAULT_TAG: KeptHomeTag = "watching";

function nowIso(): string {
  return new Date().toISOString();
}

function makeItem(id: string): KeptHomeItem {
  const ts = nowIso();
  return {
    id,
    tag: DEFAULT_TAG,
    note: "",
    addedAt: ts,
    updatedAt: ts,
  };
}

function isKeptHomeTag(value: unknown): value is KeptHomeTag {
  return value === "watching" || value === "finalist" || value === "verify" || value === "stretch";
}

function parseStoredKeptHomes(): StoredKeptHomes {
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
          .filter((item: unknown): item is Partial<KeptHomeItem> & { id: string } => {
            return typeof item === "object" && item !== null && typeof (item as KeptHomeItem).id === "string";
          })
          .map((item: Partial<KeptHomeItem> & { id: string }) => ({
            id: item.id,
            tag: isKeptHomeTag(item.tag) ? item.tag : DEFAULT_TAG,
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

function persistKeptHomes(keptHomes: StoredKeptHomes): void {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(keptHomes));
}

export function getKeptHomeItems(): KeptHomeItem[] {
  return parseStoredKeptHomes().items;
}

export function isKeptHome(id: string): boolean {
  return getKeptHomeItems().some((item) => item.id === id);
}

export function removeKeptHome(id: string): void {
  const keptHomes = parseStoredKeptHomes();
  keptHomes.items = keptHomes.items.filter((item) => item.id !== id);
  persistKeptHomes(keptHomes);
}

export function toggleKeptHome(id: string): boolean {
  const keptHomes = parseStoredKeptHomes();
  const index = keptHomes.items.findIndex((item) => item.id === id);
  if (index >= 0) {
    keptHomes.items.splice(index, 1);
    persistKeptHomes(keptHomes);
    return false;
  }

  keptHomes.items.push(makeItem(id));
  persistKeptHomes(keptHomes);
  return true;
}
