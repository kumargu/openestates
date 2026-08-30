import assert from "node:assert/strict";
import test from "node:test";
import { buildBaselinePlanInputs } from "../src/features/home-plan/model.ts";

const values = new Map<string, string>();
Object.defineProperty(globalThis, "window", {
  value: {
    localStorage: {
      getItem: (key: string) => values.get(key) ?? null,
      setItem: (key: string, value: string) => values.set(key, value),
      removeItem: (key: string) => values.delete(key),
    },
  },
  configurable: true,
});

const {
  planDraftStorageKey,
  canPersistPlanDraft,
  clearPlanDraft,
  readPlanDraft,
  writePlanDraft,
} = await import("../src/features/home-plan/planDrafts.ts");

test("Buy vs Rent drafts remain independent per property", () => {
  values.clear();
  const first = { ...buildBaselinePlanInputs(20_000_000), monthlyEmiThousands: 175 };
  const second = { ...buildBaselinePlanInputs(30_000_000), monthlyEmiThousands: 240 };

  writePlanDraft("home/one", first, 2, "lower_emi");
  writePlanDraft("home-two", second, 4);

  assert.equal(readPlanDraft("home/one")?.inputs.monthlyEmiThousands, 175);
  assert.equal(readPlanDraft("home/one")?.extraEmisPerYear, 2);
  assert.equal(readPlanDraft("home/one")?.repaymentStrategy, "lower_emi");
  assert.equal(readPlanDraft("home-two")?.inputs.monthlyEmiThousands, 240);
  assert.equal(readPlanDraft("home-two")?.extraEmisPerYear, 4);
  assert.equal(readPlanDraft("home-two")?.repaymentStrategy, "finish_earlier");
  assert.match(planDraftStorageKey("home/one"), /home%2Fone$/);
});

test("invalid stored drafts fail closed", () => {
  values.clear();
  writePlanDraft("home-1", buildBaselinePlanInputs(20_000_000), 2);
  const stored = JSON.parse(values.get(planDraftStorageKey("home-1"))!);
  values.set(planDraftStorageKey("home-1"), JSON.stringify({
    ...stored,
    inputs: { monthlyEmiThousands: "not-a-number" },
  }));

  assert.equal(readPlanDraft("home-1"), null);
});

test("drafts without the current down-payment model are dropped", () => {
  values.clear();
  values.set(planDraftStorageKey("home-1"), JSON.stringify({
    version: 2,
    propertyId: "home-1",
    inputs: { ...buildBaselinePlanInputs(20_000_000), monthlyEmiThousands: 90 },
    extraEmisPerYear: 0,
    updatedAt: Date.now(),
  }));

  assert.equal(readPlanDraft("home-1"), null);
});

test("version 3 drafts migrate to finish-earlier repayment", () => {
  values.clear();
  values.set(planDraftStorageKey("home-1"), JSON.stringify({
    version: 3,
    propertyId: "home-1",
    inputs: buildBaselinePlanInputs(20_000_000),
    extraEmisPerYear: 2,
    updatedAt: Date.now(),
  }));

  assert.equal(readPlanDraft("home-1")?.repaymentStrategy, "finish_earlier");
});

test("version 4 drafts migrate SIP to the nearest explicit EMI multiple", () => {
  values.clear();
  const inputs = {
    ...buildBaselinePlanInputs(20_000_000),
    monthlyEmiThousands: 150,
    monthlySipThousands: 260,
  };
  values.set(planDraftStorageKey("home-1"), JSON.stringify({
    version: 4,
    propertyId: "home-1",
    inputs,
    extraEmisPerYear: 2,
    repaymentStrategy: "lower_emi",
    updatedAt: Date.now(),
  }));

  const migrated = readPlanDraft("home-1");
  assert.equal(migrated?.inputs.monthlySipThousands, 300);
  assert.equal(migrated?.repaymentStrategy, "lower_emi");
});

test("reset clears the stored draft so defaults come back", () => {
  values.clear();
  writePlanDraft("home-1", { ...buildBaselinePlanInputs(20_000_000), monthlyEmiThousands: 250 }, 3);
  assert.equal(readPlanDraft("home-1")?.inputs.monthlyEmiThousands, 250);

  clearPlanDraft("home-1");
  assert.equal(readPlanDraft("home-1"), null);
});

test("draft persistence requires the route and loaded property to match", () => {
  assert.equal(canPersistPlanDraft("home-a", "home-a", "ready"), true);
  assert.equal(canPersistPlanDraft("home-b", "home-a", "ready"), false);
  assert.equal(canPersistPlanDraft("home-a", "home-a", "loading"), false);
  assert.equal(canPersistPlanDraft(undefined, "home-a", "ready"), false);
});
