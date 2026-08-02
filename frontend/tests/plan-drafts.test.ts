import assert from "node:assert/strict";
import test from "node:test";
import { buildBaselinePlanInputs } from "../src/features/home-plan/model.ts";

const values = new Map<string, string>();
Object.defineProperty(globalThis, "window", {
  value: {
    localStorage: {
      getItem: (key: string) => values.get(key) ?? null,
      setItem: (key: string, value: string) => values.set(key, value),
    },
  },
  configurable: true,
});

const {
  planDraftStorageKey,
  canPersistPlanDraft,
  readPlanDraft,
  writePlanDraft,
} = await import("../src/features/home-plan/planDrafts.ts");

test("Buy vs Rent drafts remain independent per property", () => {
  values.clear();
  const first = { ...buildBaselinePlanInputs(20_000_000), monthlyEmiThousands: 175 };
  const second = { ...buildBaselinePlanInputs(30_000_000), monthlyEmiThousands: 240 };

  writePlanDraft("home/one", first, 2);
  writePlanDraft("home-two", second, 4);

  assert.equal(readPlanDraft("home/one")?.inputs.monthlyEmiThousands, 175);
  assert.equal(readPlanDraft("home/one")?.extraEmisPerYear, 2);
  assert.equal(readPlanDraft("home-two")?.inputs.monthlyEmiThousands, 240);
  assert.equal(readPlanDraft("home-two")?.extraEmisPerYear, 4);
  assert.match(planDraftStorageKey("home/one"), /home%2Fone$/);
});

test("invalid stored drafts fail closed", () => {
  values.clear();
  values.set(planDraftStorageKey("home-1"), JSON.stringify({
    version: 1,
    propertyId: "home-1",
    inputs: { monthlyEmiThousands: "not-a-number" },
    extraEmisPerYear: 2,
    updatedAt: Date.now(),
  }));

  assert.equal(readPlanDraft("home-1"), null);
});

test("draft persistence requires the route and loaded property to match", () => {
  assert.equal(canPersistPlanDraft("home-a", "home-a", "ready"), true);
  assert.equal(canPersistPlanDraft("home-b", "home-a", "ready"), false);
  assert.equal(canPersistPlanDraft("home-a", "home-a", "loading"), false);
  assert.equal(canPersistPlanDraft(undefined, "home-a", "ready"), false);
});
