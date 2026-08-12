import assert from "node:assert/strict";
import test from "node:test";
import {
  builderHealthSummary,
  builderProjectMilestones,
  hasRelatedBuilderEvidence,
  uniqueBuilderProjects,
} from "../src/lib/builderHealth.ts";
import type { BuilderPortfolio, BuilderProjectRecord } from "../src/lib/types.ts";

function project(overrides: Partial<BuilderProjectRecord> = {}): BuilderProjectRecord {
  return {
    property_id: "property:waterford-3bhk",
    project_name: "Prestige Waterford",
    area: "Whitefield",
    rera_registered: false,
    current: false,
    ...overrides,
  };
}

function portfolio(projects: BuilderProjectRecord[]): BuilderPortfolio {
  return {
    builder_name: "Prestige Estates Projects Limited",
    tracked_projects: projects.length,
    rera_registered_projects: projects.filter((item) => item.rera_number).length,
    delayed_projects: projects.filter((item) => (item.delay_months ?? 0) > 0).length,
    complaint_projects: projects.filter((item) => (item.complaints_count ?? 0) > 0).length,
    revocations: 0,
    projects,
  };
}

test("builder health stays hidden without another project artifact", () => {
  const current = project({
    current: true,
    rera_number: "PR/003528",
    rera_status: "Registered",
  });
  assert.equal(hasRelatedBuilderEvidence(portfolio([current])), false);
  assert.equal(hasRelatedBuilderEvidence(null), false);
});

test("another RERA project enables builder health", () => {
  const current = project({ current: true, rera_number: "PR/003528" });
  const related = project({
    property_id: "property:park-grove",
    project_name: "Prestige Park Grove",
    rera_number: "PR/005736",
  });
  assert.equal(hasRelatedBuilderEvidence(portfolio([current, related])), true);
});

test("duplicate configurations collapse to one project and retain current marker", () => {
  const duplicate = project({
    property_id: "property:waterford-4bhk",
    rera_number: "PR/003528",
  });
  const current = project({
    current: true,
    rera_number: "PR/003528",
  });
  assert.deepEqual(uniqueBuilderProjects(portfolio([duplicate, current])), [current]);
});

test("milestone rail distinguishes active construction from delivered projects", () => {
  const active = builderProjectMilestones(project({
    rera_number: "PR/005736",
    rera_registered: true,
    rera_status: "Registered",
    project_status_display: "Under construction",
    completion_date: "2027-12-31",
  }));
  assert.deepEqual(active.map((item) => item.state), [
    "complete",
    "current",
    "pending",
    "pending",
  ]);

  const delivered = builderProjectMilestones(project({
    rera_number: "PR/003528",
    rera_registered: true,
    project_status_display: "Delivered",
    completion_date: "2023-12-31",
  }));
  assert.deepEqual(delivered.map((item) => item.state), [
    "complete",
    "complete",
    "complete",
    "complete",
  ]);
});

test("registration and negative delivery states do not imply progress", () => {
  const registered = builderProjectMilestones(project({
    rera_number: "PR/005736",
    rera_registered: true,
    rera_status: "Registered",
    project_status_display: "Not completed",
    completion_date: "2025-12-31",
  }));
  assert.deepEqual(registered.map((item) => item.state), [
    "complete",
    "pending",
    "pending",
    "pending",
  ]);

  const pendingOccupancy = builderProjectMilestones(project({
    rera_number: "PR/005737",
    project_status_display: "Occupancy certificate pending",
  }));
  assert.equal(pendingOccupancy[0].state, "pending");
  assert.equal(pendingOccupancy.at(-1)?.state, "pending");
});

test("health summary reports evidence-backed project flags", () => {
  const delayed = project({
    property_id: "property:serenity",
    project_name: "Prestige Serenity Shores",
    rera_number: "PR/005503",
    delay_months: 4,
    complaints_count: 1,
  });
  const clear = project({
    property_id: "property:park-grove",
    project_name: "Prestige Park Grove",
    rera_number: "PR/005736",
  });
  const summary = builderHealthSummary(portfolio([delayed, clear]));
  assert.equal(summary.flaggedProjects, 1);
  assert.equal(summary.label, "1 project needs review");
  assert.match(summary.read, /2 related projects/);
  assert.match(summary.read, /1 delayed/);
});

test("health summary preserves unknown revocations and uses returned rows", () => {
  const data = portfolio([
    project({ rera_number: "PR/003528", rera_registered: true }),
    project({
      property_id: "property:park-grove",
      project_name: "Prestige Park Grove",
      rera_number: "PR/005736",
      rera_registered: true,
    }),
  ]);
  data.revocations = undefined;
  data.tracked_projects = 10;
  data.rera_registered_projects = 9;

  const summary = builderHealthSummary(data);
  assert.equal(summary.metrics.projects, 2);
  assert.equal(summary.metrics.reraLinked, 2);
  assert.equal(summary.metrics.revocations, null);
  assert.equal(summary.label, "Review available records");
  assert.equal(summary.tone, "neutral");
});
