import assert from "node:assert/strict";
import test from "node:test";
import { floorPlanForBhk } from "../src/lib/floor-plan-compare.ts";

test("floor plan compare picks the active BHK plan closest to listing carpet", () => {
  const plan = floorPlanForBhk(
    [
      {
        id: "large",
        bhk: 3,
        carpet_area_sqft: 1210,
        floor_plan_preview_url: "/media/large.png",
        plan_carpet_area_sqft: 1382,
        plan_sale_area_sqft: 2027,
        plan_configuration_type: "3BHK",
      },
      {
        id: "compact",
        bhk: 3,
        carpet_area_sqft: 1210,
        floor_plan_preview_url: "/media/compact.png",
        plan_carpet_area_sqft: 1197,
        plan_sale_area_sqft: 1775,
        plan_configuration_type: "3BHK",
      },
      {
        id: "two-bed",
        bhk: 2,
        carpet_area_sqft: 980,
        floor_plan_preview_url: "/media/2bhk.png",
        plan_carpet_area_sqft: 999,
        plan_sale_area_sqft: 1515,
        plan_configuration_type: "2BHK",
      },
    ],
    3,
  );

  assert.equal(plan?.listingId, "compact");
  assert.equal(plan?.previewUrl, "/media/compact.png");
  assert.equal(plan?.usableAreaRatio, 0.674);
});

test("floor plan compare returns null when the active BHK has no preview", () => {
  const plan = floorPlanForBhk(
    [
      {
        id: "missing-preview",
        bhk: 4,
        carpet_area_sqft: 1700,
        plan_carpet_area_sqft: 1740,
        plan_sale_area_sqft: 2525,
        plan_configuration_type: "4BHK",
      },
      {
        id: "other-bhk",
        bhk: 3,
        floor_plan_preview_url: "/media/3bhk.png",
        plan_carpet_area_sqft: 1197,
        plan_sale_area_sqft: 1775,
      },
    ],
    4,
  );

  assert.equal(plan, null);
});
