import assert from "node:assert/strict";
import test from "node:test";

const {
  NOTEBOOK_COMMANDS,
  matchingNotebookCommands,
  slashQuery,
} = await import("../src/lib/notebookCommands.ts");

test("slash query only opens for a leading command draft", () => {
  assert.equal(slashQuery("/"), "");
  assert.equal(slashQuery("/vi"), "vi");
  assert.equal(slashQuery("normal note /vi"), null);
  assert.equal(slashQuery("/visit\nextra"), null);
});

test("command filtering matches slash names and keywords", () => {
  assert.deepEqual(
    matchingNotebookCommands("vis").map((command) => command.id),
    ["visit"],
  );
  assert.deepEqual(
    matchingNotebookCommands("emi").map((command) => command.id),
    ["budget"],
  );
  assert.deepEqual(
    matchingNotebookCommands("token").map((command) => command.id),
    ["payment"],
  );
});

test("commands describe appendable notebook blocks", () => {
  const visit = NOTEBOOK_COMMANDS.find((command) => command.id === "visit");
  const budget = NOTEBOOK_COMMANDS.find((command) => command.id === "budget");
  assert.ok(visit);
  assert.ok(budget);

  assert.equal(visit.blockType, "checklist");
  assert.equal(visit.items?.includes("Check water pressure"), true);
  assert.equal(budget.blockType, "fields");
  assert.equal(budget.fields?.includes("Comfortable EMI"), true);
  assert.equal(NOTEBOOK_COMMANDS.some((command) => command.slash === "/buying-cost"), false);
});
