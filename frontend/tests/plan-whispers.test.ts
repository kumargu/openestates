import assert from "node:assert/strict";
import test from "node:test";
import { planWhispersFor } from "../src/features/home-plan/planWhispers.ts";

test("plan whispers preserve curated rent, buy, and prepay themes", () => {
  const rent = planWhispersFor("rent").join(" ");
  const buy = planWhispersFor("buy").join(" ");
  const prepay = planWhispersFor("prepay").join(" ");

  assert.match(rent, /Zuckerberg rented for years/);
  assert.match(rent, /Rent buys you the option to leave/);
  assert.match(buy, /A house is a decision\. A home is everything after it/);
  assert.match(buy, /spreadsheet can't price/);
  assert.match(prepay, /Freedom from EMI/);
});
