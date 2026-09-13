import assert from "node:assert/strict";
import test from "node:test";
import {
  backendReplayFixtureId,
  getBackendReplayFixture,
} from "../src/lib/dev-backend-fixtures.ts";

const propertyPath = `/api/properties/${backendReplayFixtureId}`;

test("backend replay fixture covers the normal property journey", () => {
  const property = getBackendReplayFixture(propertyPath) as {
    property?: { id?: string };
  };
  assert.equal(property.property?.id, backendReplayFixtureId);

  const around = getBackendReplayFixture(
    `${propertyPath}/surfaces/around_this_home?focus=ignored-by-replay`,
  ) as { surfaceId?: string; propertyId?: string };
  assert.equal(around.surfaceId, "around_this_home");
  assert.equal(around.propertyId, backendReplayFixtureId);

  const arrival = getBackendReplayFixture(
    `${propertyPath}/surfaces/arrival_story`,
  ) as { surfaceId?: string };
  assert.equal(arrival.surfaceId, "arrival_story");
});

test("backend replay fixture covers evidence and batch contracts", () => {
  assert.ok(getBackendReplayFixture(`${propertyPath}/evidence`));
  assert.ok(getBackendReplayFixture(`${propertyPath}/rera`));
  assert.ok(getBackendReplayFixture(`${propertyPath}/recommendations`));
  assert.ok(getBackendReplayFixture("/api/properties/surfaces/batch"));
  assert.equal(getBackendReplayFixture("/api/not-captured"), null);
});
