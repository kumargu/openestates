import assert from "node:assert/strict";
import test from "node:test";
import { reraFactCount, reraFactGroups } from "../src/lib/reraProjectFacts.ts";
import type { ReraInfo } from "../src/lib/types.ts";

const GODREJ_RERA: ReraInfo = {
  registered: true,
  registration_number: "PRM/KA/RERA/1251/446/PR/160622/005000",
  status: "APPROVED",
  start_date: "2022-03-21",
  completion_date: "2027-03-20",
  original_completion_date: "2027-03-20",
  total_units: 1161,
  total_land_area_sqm: 30898,
  total_land_area_acres: 7.635,
  open_area_pct: 46.94,
  units_per_acre: 152.06,
  complaints_count: 10,
  complaints_resolved_pct: 100,
  builder_total_projects: 13,
  builder_revocations: 0,
  land_litigation: false,
  escrow_bank: "Axis Bank",
  has_borrowing: false,
  has_mortgage: false,
};

test("RERA groups expose the complete buyer-relevant record", () => {
  const groups = reraFactGroups(GODREJ_RERA);

  assert.deepEqual(groups.map((group) => group.label), [
    "Registration",
    "Schedule",
    "Project scale",
    "Buyer checks",
  ]);
  assert.equal(reraFactCount(GODREJ_RERA), 15);
  assert.equal(
    groups.find((group) => group.id === "scale")?.rows.find((row) => row.label === "Homes")?.value,
    "1,161",
  );
  assert.equal(
    groups.find((group) => group.id === "checks")?.rows.find((row) => row.label === "Complaints")?.value,
    "10 filed · 100% resolved",
  );
});

test("RERA groups preserve unknowns instead of inventing clean checks", () => {
  const groups = reraFactGroups({
    registered: true,
    registration_number: "PRM/TEST",
    completion_date: "2028-12-31",
  });
  const labels = groups.flatMap((group) => group.rows.map((row) => row.label));

  assert.equal(labels.includes("Land litigation"), false);
  assert.equal(labels.includes("Builder revocations"), false);
  assert.equal(labels.includes("Project borrowing"), false);
  assert.equal(labels.includes("Project mortgage"), false);
});

test("RERA registration status stays conservative for missing and sentinel values", () => {
  const unconfirmed = reraFactGroups({ registered: false });
  const unconfirmedStatus = unconfirmed[0].rows[0];
  assert.equal(unconfirmedStatus.value, "Registration not confirmed");
  assert.equal(unconfirmedStatus.tone, "watch");
  assert.equal(reraFactCount({ registered: false }), 1);

  const registered = reraFactGroups({ registered: true, status: "UNKNOWN" });
  assert.equal(registered[0].rows[0].value, "Registered");
  assert.equal(registered[0].rows[0].tone, "positive");

  const rejected = reraFactGroups({ registered: false, status: "REJECTED" });
  assert.equal(rejected[0].rows[0].value, "Rejected");
  assert.equal(rejected[0].rows[0].tone, "watch");

  const expired = reraFactGroups({ registered: true, status: "REGISTRATION EXPIRED" });
  assert.equal(expired[0].rows[0].tone, "watch");
});

test("RERA groups omit sentinel text across the structured record", () => {
  const groups = reraFactGroups({
    registered: true,
    status: "UNKNOWN",
    registration_number: "N/A",
    start_date: "not specified",
    completion_date: "UNKNOWN",
    escrow_bank: "none",
  });
  const rows = groups.flatMap((group) => group.rows);

  assert.deepEqual(rows, [
    { label: "Status", value: "Registered", tone: "positive" },
  ]);
});
