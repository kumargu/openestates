import assert from "node:assert/strict";
import test from "node:test";

const values = new Map<string, string>();
const events = new EventTarget();
const localStorage = {
  getItem: (key: string) => values.get(key) ?? null,
  setItem: (key: string, value: string) => values.set(key, value),
  removeItem: (key: string) => values.delete(key),
};

Object.defineProperty(globalThis, "window", {
  value: {
    localStorage,
    addEventListener: events.addEventListener.bind(events),
    removeEventListener: events.removeEventListener.bind(events),
    dispatchEvent: events.dispatchEvent.bind(events),
  },
  configurable: true,
});

const { NOTEBOOK_STORAGE_KEY, NOTEBOOK_SCHEMA_VERSION } = await import(
  "../src/lib/notebook.ts"
);
const { notebookExternalStore } = await import("../src/hooks/useNotebook.ts");

function writeNotebookSnapshot(compareIds: string[]) {
  localStorage.setItem(NOTEBOOK_STORAGE_KEY, JSON.stringify({
    version: NOTEBOOK_SCHEMA_VERSION,
    propertyIds: [],
    documents: {},
    compareIds,
    hiddenCompareLabels: [],
  }));
}

test("notebook subscribers refresh storage after an idle interval", () => {
  writeNotebookSnapshot(["first-home"]);
  const unsubscribe = notebookExternalStore.subscribe(() => undefined);
  assert.deepEqual(notebookExternalStore.getSnapshot().compareIds, ["first-home"]);
  unsubscribe();

  writeNotebookSnapshot(["second-home"]);
  let publications = 0;
  const unsubscribeAfterIdle = notebookExternalStore.subscribe(() => {
    publications += 1;
  });

  assert.deepEqual(notebookExternalStore.getSnapshot().compareIds, ["second-home"]);
  assert.equal(publications, 1);
  unsubscribeAfterIdle();
});
