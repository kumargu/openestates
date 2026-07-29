import assert from "node:assert/strict";
import test from "node:test";

type MemoryStorage = {
  getItem: (key: string) => string | null;
  setItem: (key: string, value: string) => void;
  removeItem: (key: string) => void;
  clear: () => void;
};

function memoryStorage(): MemoryStorage {
  const values = new Map<string, string>();
  return {
    getItem: (key) => values.get(key) ?? null,
    setItem: (key, value) => values.set(key, value),
    removeItem: (key) => values.delete(key),
    clear: () => values.clear(),
  };
}

const storage = memoryStorage();

Object.defineProperty(globalThis, "window", {
  value: {
    localStorage: storage,
    dispatchEvent: () => true,
  },
  configurable: true,
});

const {
  NOTEBOOK_STORAGE_KEY,
  readNotebook,
  toggleNotebookCompareId,
} = await import("../src/lib/notebook.ts");
const {
  SHORTLIST_STORAGE_KEY,
} = await import("../src/lib/compare.ts");

test("workspace state includes saved homes even before notes exist", () => {
  storage.clear();
  storage.setItem(SHORTLIST_STORAGE_KEY, "saved-home,noted-home");
  storage.setItem(NOTEBOOK_STORAGE_KEY, JSON.stringify({
    propertyIds: ["noted-home"],
    notes: [],
    compareIds: [],
  }));

  const state = readNotebook();

  assert.deepEqual(state.propertyIds, ["saved-home", "noted-home"]);
  assert.deepEqual(state.compareIds, []);
});

test("compare selection is explicit and does not rewrite saved homes", () => {
  storage.clear();
  storage.setItem(SHORTLIST_STORAGE_KEY, "saved-home,noted-home");
  storage.setItem(NOTEBOOK_STORAGE_KEY, JSON.stringify({
    propertyIds: ["noted-home"],
    notes: [],
    compareIds: [],
  }));

  const state = toggleNotebookCompareId("saved-home");

  assert.deepEqual(state.propertyIds, ["saved-home", "noted-home"]);
  assert.deepEqual(state.compareIds, ["saved-home"]);
  assert.equal(storage.getItem(SHORTLIST_STORAGE_KEY), "saved-home,noted-home");
});
