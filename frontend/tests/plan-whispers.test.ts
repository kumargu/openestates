import assert from "node:assert/strict";
import test from "node:test";
import { planWhisperFor, planWhispersFor } from "../src/features/home-plan/planWhispers.ts";

test("plan whispers keep humor contextual without celebrity claims", () => {
  const rent = planWhispersFor("rent").join(" ");
  const buy = planWhispersFor("buy").join(" ");
  const prepay = planWhispersFor("prepay").join(" ");

  assert.match(rent, /geyser/);
  assert.match(buy, /maintenance WhatsApp group/);
  assert.match(prepay, /Extra EMIs now/);
  assert.doesNotMatch(`${rent} ${buy}`, /Zuckerberg|Buffett|Kamath/);
});

test("plan whisper stays deterministic and marks the loan-free milestone", () => {
  const context = { theme: "rent" as const, activeYear: 7, loanFreeYear: 14 };
  assert.equal(planWhisperFor(context), planWhisperFor(context));
  assert.match(
    planWhisperFor({ theme: "prepay", activeYear: 14, loanFreeYear: 14 }),
    /left the group chat/,
  );
});
