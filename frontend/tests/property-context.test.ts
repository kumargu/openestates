import assert from "node:assert/strict";
import test from "node:test";
import { atlasFixtureContext } from "../src/lib/dev-atlas-fixtures.ts";
import { projectPropertyContext } from "../src/lib/property-context.ts";
import { validateWire } from "../src/lib/wire.ts";
import type { PropertyContext } from "../src/generated/PropertyContext.ts";

test("presentation caps expand for the exact receipt without replacing default context", () => {
  const context = atlasFixtureContext();
  const source = context.features.find(
    (feature) => feature.fact.factKey === "nearby_schools",
  )!;
  assert.ok(source?.target);
  context.features = Array.from({ length: 6 }, (_, index) => ({
    ...structuredClone(source),
    fact: {
      ...structuredClone(source.fact),
      id: `school:${index}`,
      evidence: {
        ...source.fact.evidence,
        evidence_id: {
          kind: "observation" as const,
          id: `observation:${index}`,
        },
      },
    },
    target: {
      ...source.target!,
      entityId: `place:school-${index}`,
      name: `School ${index}`,
    },
  }));
  const normal = projectPropertyContext(context, "around_this_home")!;
  assert.equal(normal.features.length, 5);
  const matched = context.features[5];
  context.matchedProof = {
    contractVersion: 1,
    snapshotIdentity: context.snapshotIdentity,
    semanticFingerprint: "fixture",
    propertyId: context.propertyId,
    branchId: "branch",
    predicateId: "school-predicate",
    subjectEntityId: context.anchor.entityId,
    targetEntityId: matched.target!.entityId,
    targetLabel: matched.target!.name,
    factKey: matched.fact.factKey,
    relation: "supports",
    value: matched.fact.value,
    sourceObservations: [
      {
        observationId: matched.fact.evidence.evidence_id.id,
        provider: matched.fact.sourceType,
        providerObservationId: matched.fact.id,
        subjectEntityId: context.anchor.entityId,
        observedAt: matched.fact.observedAt,
        assetLineage: ["fixture/archived-geometry"],
      },
    ],
    derivationChain: [],
    resolutionStatus: "resolved",
  };
  const decoded = validateWire<PropertyContext>("context", context);
  const focused = projectPropertyContext(
    decoded,
    "around_this_home",
    "signed-proof",
  )!;
  assert.equal(focused.features.length, 6);
  assert.ok(
    normal.features.every((feature) =>
      focused.features.some((kept) => kept.id === feature.id),
    ),
  );
  assert.equal(focused.proofFocus?.entityId, matched.target!.entityId);
  assert.equal(focused.proofFocus?.receiptId, matched.fact.id);
  assert.deepEqual(
    focused.receipts.find((receipt) => receipt.id === matched.fact.id)
      ?.evidence,
    matched.fact.evidence,
  );
  assert.equal(
    context.features.length,
    6,
    "presentation must not mutate domain context",
  );
});


test("configured geographic spread adds distant choices without replacing the nearest defaults", () => {
  const context = atlasFixtureContext();
  const school = context.features.find(feature => feature.fact.factKey === "nearby_schools")!;
  context.features = Array.from({ length: 6 }, (_, index) => ({
    ...structuredClone(school),
    fact: { ...structuredClone(school.fact), id: `spread:${index}` },
    target: { ...structuredClone(school.target!), entityId: `place:spread-${index}`, name: `School ${index}`,
      geometry: { type: "Point" as const, coordinates: [77.6 + (index === 5 ? 0.1 : 0), 13] as [number, number] } },
  }));
  const scene = projectPropertyContext(context, "around_this_home")!;
  assert.equal(scene.features.length, 6);
  assert.ok(context.features.every(feature => scene.receipts.some(receipt => receipt.id === feature.fact.id)));
});
