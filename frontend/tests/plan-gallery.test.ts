import assert from "node:assert/strict";
import test from "node:test";
import { hasPlanGalleryItems, planGalleryItems } from "../src/lib/planGallery.ts";
import type { ProjectPlansView } from "../src/lib/types.ts";

test("filed previews work when legacy floor plans are omitted", () => {
  const plans: ProjectPlansView = {
    provider: "RERA",
    coverage_quality: "usable",
    filed_plan_previews: [{
      artifact_id: "brigade-laguna:site-plan",
      kind: "site_plan",
      label: "Site plan",
      preview_url: "/media/previews/site-plan.png",
      confidence: 0.85,
    }],
  };

  assert.equal(hasPlanGalleryItems(plans), true);
  assert.deepEqual(planGalleryItems(plans), [{
    id: "brigade-laguna:site-plan",
    kind: "site_plan",
    label: "Site plan",
    previewUrl: "/media/previews/site-plan.png",
    thumbnailUrl: "/media/previews/site-plan.png",
  }]);
});

test("plans without usable preview URLs stay hidden", () => {
  const plans: ProjectPlansView = {
    provider: "RERA",
    coverage_quality: "unavailable",
    filed_plan_previews: [{
      artifact_id: "private-plan",
      kind: "site_plan",
      label: "Site plan",
      preview_url: "file:///private/plan.png",
      confidence: 0.85,
    }],
  };

  assert.equal(hasPlanGalleryItems(plans), false);
  assert.deepEqual(planGalleryItems(plans), []);
});
