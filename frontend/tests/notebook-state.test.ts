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
  NOTEBOOK_SCHEMA_VERSION,
  addNotebookCommandBlock,
  addNotebookParagraphAfter,
  detachNotebookPropertyFromShortlist,
  hideNotebookCompareLabel,
  readNotebook,
  showNotebookCompareLabel,
  setNotebookCompareIds,
  toggleNotebookCompareId,
  updateNotebookNote,
  upsertContextualNote,
} = await import("../src/lib/notebook.ts");
const {
  SHORTLIST_STORAGE_KEY,
  writeShortlistIds,
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

test("v2 notebook storage migrates to v3 ordered documents without losing compare state", () => {
  storage.clear();
  storage.setItem(NOTEBOOK_STORAGE_KEY, JSON.stringify({
    propertyIds: ["home-1"],
    notes: [{
      id: "note-1",
      propertyId: "home-1",
      title: "Existing thought",
      kind: "handwritten",
      catalogKey: "hand:home-1:1",
      labels: [],
      createdAt: 1,
    }, {
      id: "plan-1",
      propertyId: "home-1",
      title: "Plan summary",
      detail: "EMI looked comfortable.",
      kind: "plan",
      catalogKey: "plan:home-1:current",
      labels: ["finance", "emi"],
      createdAt: 2,
    }],
    compareIds: ["home-1"],
    hiddenCompareLabels: ["schools"],
  }));

  const state = readNotebook();
  const raw = JSON.parse(storage.getItem(NOTEBOOK_STORAGE_KEY) ?? "{}");

  assert.equal(state.version, NOTEBOOK_SCHEMA_VERSION);
  assert.equal(raw.version, NOTEBOOK_SCHEMA_VERSION);
  assert.equal(state.documents["home-1"].blocks[0].type, "paragraph");
  assert.equal(state.documents["home-1"].blocks[1].type, "financial_plan_reference");
  assert.deepEqual(state.compareIds, ["home-1"]);
  assert.deepEqual(state.hiddenCompareLabels, ["schools"]);
  assert.deepEqual(state.notes.map((note) => note.id), ["note-1", "plan-1"]);
  assert.equal(storage.getItem(SHORTLIST_STORAGE_KEY), "home-1");
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

test("a deep-linked comparison becomes active without saving every home", () => {
  storage.clear();
  storage.setItem(SHORTLIST_STORAGE_KEY, "saved-home");

  const state = setNotebookCompareIds(["saved-home", "recommended-home"]);

  assert.deepEqual(state.compareIds, ["saved-home", "recommended-home"]);
  assert.deepEqual(state.propertyIds, ["saved-home"]);
  assert.equal(storage.getItem(SHORTLIST_STORAGE_KEY), "saved-home");
});

test("removing a shortlisted home clears compare state without deleting buyer notes", () => {
  storage.clear();
  storage.setItem(SHORTLIST_STORAGE_KEY, "noted-home");
  storage.setItem(NOTEBOOK_STORAGE_KEY, JSON.stringify({
    propertyIds: ["empty-home", "noted-home"],
    notes: [{
      id: "note-1",
      propertyId: "noted-home",
      title: "Visit on Saturday",
      kind: "handwritten",
      catalogKey: "hand:noted-home:1",
      labels: [],
      createdAt: 1,
    }],
    compareIds: ["empty-home", "noted-home"],
  }));

  const emptyRemoved = detachNotebookPropertyFromShortlist("empty-home");
  assert.equal(emptyRemoved.propertyIds.includes("empty-home"), false);
  assert.deepEqual(emptyRemoved.compareIds, ["noted-home"]);

  storage.setItem(SHORTLIST_STORAGE_KEY, "");
  const notedRemoved = detachNotebookPropertyFromShortlist("noted-home");
  assert.equal(notedRemoved.propertyIds.includes("noted-home"), true);
  assert.equal(notedRemoved.notes[0]?.title, "Visit on Saturday");
  assert.deepEqual(notedRemoved.compareIds, []);
});

test("compare labels can be hidden and restored without changing notes", () => {
  storage.clear();
  storage.setItem(NOTEBOOK_STORAGE_KEY, JSON.stringify({
    propertyIds: ["home-1"],
    notes: [{
      id: "note-1",
      propertyId: "home-1",
      title: "School nearby",
      kind: "fact",
      catalogKey: "map:school",
      labels: ["schools"],
      createdAt: 1,
    }],
    compareIds: ["home-1"],
  }));

  const hidden = hideNotebookCompareLabel("schools");
  assert.deepEqual(hidden.hiddenCompareLabels, ["schools"]);
  assert.deepEqual(hidden.notes[0].labels, ["schools"]);

  const restored = showNotebookCompareLabel("schools");
  assert.deepEqual(restored.hiddenCompareLabels, []);
  assert.deepEqual(restored.notes[0].labels, ["schools"]);
});

test("saved complaint notes migrate out of generic legal labels", () => {
  storage.clear();
  storage.setItem(NOTEBOOK_STORAGE_KEY, JSON.stringify({
    propertyIds: ["home-1"],
    notes: [{
      id: "note-1",
      propertyId: "home-1",
      title: "Complaints",
      detail: "21 filed",
      kind: "selection",
      catalogKey: "sel:home-1:complaints",
      labels: ["legal"],
      createdAt: 1,
    }],
    compareIds: ["home-1"],
  }));

  const state = readNotebook();

  assert.deepEqual(state.notes[0].labels, ["complaints", "risk", "legal"]);
});

test("complaint label remains the leading visual label after migration", () => {
  storage.clear();
  storage.setItem(NOTEBOOK_STORAGE_KEY, JSON.stringify({
    propertyIds: ["home-1"],
    notes: [{
      id: "note-1",
      propertyId: "home-1",
      title: "Complaints",
      detail: "RERA",
      kind: "fact",
      catalogKey: "rera:home-1:complaints",
      labels: ["legal", "complaints"],
      createdAt: 1,
    }],
    compareIds: ["home-1"],
  }));

  const state = readNotebook();

  assert.deepEqual(state.notes[0].labels, ["complaints", "risk", "legal"]);
});

test("map notes retain property, feature, layer, distance, source, and label context", () => {
  storage.clear();

  const created = upsertContextualNote({
    propertyId: "home-1",
    catalogKey: "nearby:home-1:schools:place-school",
    title: "Green School",
    text: "Ask about the morning bus route.",
    labels: ["schools", "schools_under_1km"],
    detail: "Schools · 0.8 km · 4.3 rating",
    source: "Google",
  });

  assert.ok(created);
  assert.deepEqual(created.propertyIds, ["home-1"]);
  assert.equal(storage.getItem(SHORTLIST_STORAGE_KEY), "home-1");
  assert.equal(created.notes.length, 1);
  assert.equal(created.notes[0].catalogKey, "nearby:home-1:schools:place-school");
  assert.equal(created.notes[0].title, "Green School");
  assert.equal(created.notes[0].selectionText, "Ask about the morning bus route.");
  assert.equal(created.notes[0].detail, "Schools · 0.8 km · 4.3 rating");
  assert.equal(created.notes[0].source, "Google");
  assert.deepEqual(created.notes[0].labels, ["schools", "schools_under_1km"]);

  const updated = upsertContextualNote({
    propertyId: "home-1",
    catalogKey: "nearby:home-1:schools:place-school",
    title: "Green School",
    text: "Visit during pickup time.",
    labels: ["schools"],
    detail: "Schools · 0.8 km",
    source: "Google",
  });

  assert.ok(updated);
  assert.equal(updated.notes.length, 1);
  assert.equal(updated.notes[0].selectionText, "Visit during pickup time.");

  writeShortlistIds([]);
  upsertContextualNote({
    propertyId: "home-1",
    catalogKey: "nearby:home-1:schools:place-school",
    title: "Green School",
    text: "Keep this note, but leave the home removed.",
    labels: ["schools"],
  });
  assert.equal(storage.getItem(SHORTLIST_STORAGE_KEY), "");
});

test("an empty notebook row waits until the buyer writes before saving the home", () => {
  storage.clear();

  const draft = addNotebookParagraphAfter({ propertyId: "home-1" });
  const blockId = draft.documents["home-1"].blocks[0]?.id;
  assert.ok(blockId);
  assert.equal(storage.getItem(SHORTLIST_STORAGE_KEY), "");

  updateNotebookNote(blockId, { title: "Check traffic after school pickup." });
  assert.equal(storage.getItem(SHORTLIST_STORAGE_KEY), "home-1");
});

test("slash command blocks append and remain editable", () => {
  storage.clear();
  storage.setItem(NOTEBOOK_STORAGE_KEY, JSON.stringify({
    propertyIds: ["home-1"],
    notes: [{
      id: "existing-note",
      propertyId: "home-1",
      title: "Existing thought",
      kind: "handwritten",
      catalogKey: "hand:home-1:1",
      labels: [],
      createdAt: 1,
    }],
    compareIds: [],
  }));

  const created = addNotebookCommandBlock({ propertyId: "home-1", commandId: "visit" });
  assert.ok(created);
  assert.equal(created.notes[0].id, "existing-note");
  const blockNote = created.notes[1];
  assert.equal(blockNote.title, "Visit");
  assert.equal(blockNote.block?.type, "checklist");
  assert.equal(blockNote.block?.collapsed, false);

  if (blockNote.block?.type !== "checklist") throw new Error("expected checklist block");
  const firstItem = blockNote.block.items[0];
  const edited = updateNotebookNote(blockNote.id, {
    title: "Saturday visit",
    block: {
      ...blockNote.block,
      collapsed: true,
      items: blockNote.block.items.map((item) => (
        item.id === firstItem.id ? { ...item, checked: true } : item
      )),
    },
  });
  const editedNote = edited.notes.find((note) => note.id === blockNote.id);

  assert.equal(editedNote?.title, "Saturday visit");
  assert.equal(editedNote?.block?.collapsed, true);
  assert.equal(
    editedNote?.block?.type === "checklist" ? editedNote.block.items[0].checked : false,
    true,
  );
});

test("ordered document inserts paragraphs and slash blocks at the active position", () => {
  storage.clear();
  storage.setItem(NOTEBOOK_STORAGE_KEY, JSON.stringify({
    propertyIds: ["home-1"],
    notes: [{
      id: "first",
      propertyId: "home-1",
      title: "First paragraph",
      kind: "handwritten",
      catalogKey: "hand:home-1:first",
      labels: [],
      createdAt: 1,
    }, {
      id: "second",
      propertyId: "home-1",
      title: "Second paragraph",
      kind: "handwritten",
      catalogKey: "hand:home-1:second",
      labels: [],
      createdAt: 2,
    }],
    compareIds: [],
  }));

  const withParagraph = addNotebookParagraphAfter({ propertyId: "home-1", afterBlockId: "first" });
  const insertedParagraph = withParagraph.documents["home-1"].blocks[1];
  assert.equal(insertedParagraph.type, "paragraph");
  assert.deepEqual(
    withParagraph.documents["home-1"].blocks.map((block) => block.id),
    ["first", insertedParagraph.id, "second"],
  );

  const withCommand = addNotebookCommandBlock({
    propertyId: "home-1",
    commandId: "payment",
    replaceBlockId: insertedParagraph.id,
  });
  assert.ok(withCommand);
  assert.deepEqual(
    withCommand.documents["home-1"].blocks.map((block) => block.type),
    ["paragraph", "checklist", "paragraph"],
  );
  assert.equal(withCommand.documents["home-1"].blocks[1].type, "checklist");
});
