import assert from "node:assert/strict";
import test from "node:test";
import {
  areaPath,
  bandPath,
  chartTickIndexes,
  linearScale,
  nearestIndex,
  smoothLinePath,
  stackedSegments,
} from "../src/features/home-plan/charts/chartGeometry.ts";
import {
  buildRepaymentChartStories,
} from "../src/features/home-plan/chartStories.ts";
import {
  buildBaselinePlanInputs,
} from "../src/features/home-plan/model.ts";
import { calculateRepaymentDashboard } from "../src/features/home-plan/repaymentModel.ts";

const inputs = buildBaselinePlanInputs(15_000_000, {
  state: "ready",
  asOfDate: "2026-01-01",
  dateSource: "not_applicable",
});

test("chart scales invert and paths close deterministically", () => {
  const scale = linearScale([10, 20], [100, 300]);
  assert.equal(scale.map(15), 200);
  assert.equal(scale.invert(200), 15);
  assert.equal(
    areaPath([{ x: 0, y: 10 }, { x: 20, y: 5 }], 30),
    "M 0 10 L 20 5 L 20 30 L 0 30 Z",
  );
  assert.equal(
    bandPath(
      [{ x: 0, y: 5 }, { x: 20, y: 4 }],
      [{ x: 0, y: 12 }, { x: 20, y: 10 }],
    ),
    "M 0 5 L 20 4 L 20 10 L 0 12 Z",
  );
  const smooth = smoothLinePath([{ x: 0, y: 10 }, { x: 10, y: 5 }, { x: 20, y: 2 }]);
  assert.ok(smooth.startsWith("M 0 10 C "));
  assert.ok(smooth.endsWith(", 20 2"));
  assert.equal(smooth.includes(" L "), false);
});

test("chart scrubbing and ticks stay bounded", () => {
  const insets = { top: 0, right: 10, bottom: 0, left: 10 };
  const bounds = { left: 100, width: 200 };
  assert.equal(nearestIndex(50, bounds, 220, insets, 5), 0);
  assert.equal(nearestIndex(200, bounds, 220, insets, 5), 2);
  assert.equal(nearestIndex(400, bounds, 220, insets, 5), 4);
  assert.deepEqual(chartTickIndexes(21, 5), [0, 5, 10, 15, 20]);
  assert.deepEqual(stackedSegments([4, -2, 3]), [
    { value: 4, start: 0, end: 4 },
    { value: 0, start: 4, end: 4 },
    { value: 3, start: 4, end: 7 },
  ]);
});

test("repayment stories expose the selected monthly schedule", () => {
  const model = calculateRepaymentDashboard(inputs, 2, "finish_earlier");
  const stories = buildRepaymentChartStories(inputs, model);
  assert.deepEqual(stories.annual, model.recurrentSchedule);
  assert.equal(stories.monthly, stories.selectedMonthly);
  assert.ok(stories.selectedMonthly.length > 0);
  assert.ok(stories.selectedMonthly.every((month) => month.paymentNumber > 0));
  assert.ok(stories.selectedMonthly.some((month) => month.extraPaid > 0));
  assert.ok(stories.baselineMonthly.every((month) => month.extraPaid === 0));
  assert.ok(stories.finishEarlierMonthly.length < stories.baselineMonthly.length);
  assert.equal(
    stories.finishEarlierMonthly.at(-1)?.closingBalance,
    0,
    "finish-earlier path must terminate at the real payoff month",
  );
});

test("repayment chart stories preserve strategy mathematics", () => {
  const model = calculateRepaymentDashboard(inputs, 4, "lower_emi");
  const stories = buildRepaymentChartStories(inputs, model);
  const finishEmis = new Set(stories.finishEarlierMonthly.map((month) => month.scheduledEmi));
  assert.equal(finishEmis.size, 1, "finish-earlier must keep its scheduled EMI constant");
  assert.ok(
    stories.lowerEmiMonthly.at(-1)!.scheduledEmi < stories.lowerEmiMonthly[0].scheduledEmi,
    "lower-EMI must reduce the scheduled payment after prepayments",
  );
  assert.ok(
    stories.lowerEmiMonthly.length >= stories.finishEarlierMonthly.length,
    "lower-EMI must not masquerade as the shorter-tenure strategy",
  );
  assert.equal(stories.lowerEmiMonthly.at(-1)?.closingBalance, 0);
});

