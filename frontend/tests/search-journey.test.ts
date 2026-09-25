import assert from "node:assert/strict";
import test from "node:test";
import { journeyFixture, retainedFixture } from "./fixtures/search-journey.ts";
import { projectSearchJourney, readSavedJourney, saveJourney, selectJourneyProperty, journeyEditTargets, journeyUrl, readSearchCheckpoint } from "../src/lib/search-journey.ts";
import { primaryProofFocus, resolvedProofFocus } from "../src/lib/proof-focus.ts";
import { searchResultReasonLabels } from "../src/lib/search.ts";
import { getDiscovery, getProperty, getPropertyContextsBatch, resumeSearch, resumeSearchCheckpoint, reviseSearch, searchProperties, resolveSearchProof } from "../src/lib/api.ts";

test("wire envelope projects ranked cards and server-selected proof without reparsing the query", () => {
  const envelope = journeyFixture();
  const response = projectSearchJourney(envelope);
  const result = response.resultSets[0].results[0];
  assert.equal(result.matchTier, "exact");
  assert.equal("matchScore" in result, false);
  assert.equal("description_summary" in result, false);
  assert.deepEqual(response.orderedResultIds, envelope.active.results.orderedResultIds);
  assert.deepEqual(searchResultReasonLabels(result), ["Near Fixture School"]);
  assert.equal(primaryProofFocus(result)?.proofToken, "signed:exact-receipt");
  assert.equal(journeyEditTargets(envelope.active.intent)[0].target.kind, "predicate");
});

test("retained replies keep cards only when the parent fingerprint matches", () => {
  const parent = projectSearchJourney(journeyFixture());
  const retained = projectSearchJourney(retainedFixture(), parent);
  assert.strictEqual(retained.resultSets, parent.resultSets);
  assert.equal(retained.journey?.attempt.outcome, "preservedParent");
  assert.throws(() => projectSearchJourney(retainedFixture()), /unavailable/);
  const wrong = retainedFixture();
  if (wrong.active.results.kind === "retained") wrong.active.results.resultFingerprint = "different";
  assert.throws(() => projectSearchJourney(wrong, parent), /unavailable/);
});

test("saved journeys carry the selected home and tolerate unavailable storage", () => {
  const records = new Map<string, string>();
  Object.defineProperty(globalThis, "localStorage", { configurable: true, value: {
    getItem: (key: string) => records.get(key) ?? null,
    setItem: (key: string, value: string) => records.set(key, value),
  } });
  const response = projectSearchJourney(journeyFixture());
  saveJourney("3BHK", response, undefined, "earlier-revision");
  selectJourneyProperty("/?q=3BHK&journey=revision-1", response.orderedResultIds[0]);
  assert.equal(readSavedJourney("revision-1")?.selectedId, response.orderedResultIds[0]);
  assert.equal(readSavedJourney("revision-1")?.previousId, "earlier-revision");
  selectJourneyProperty("/?q=3BHK&journey=revision-1", "not-a-result");
  assert.equal(readSavedJourney("revision-1")?.selectedId, response.orderedResultIds[0]);

  const resumedEnvelope = journeyFixture();
  resumedEnvelope.attempt.kind = "resume";
  resumedEnvelope.active.revision.id = "revision-resumed";
  resumedEnvelope.active.revision.stateToken = "signed:revision-resumed";
  const resumed = projectSearchJourney(resumedEnvelope);
  saveJourney("3BHK", resumed, response.orderedResultIds[0], "earlier-revision");
  assert.equal(readSavedJourney("revision-1"), undefined);
  assert.equal(readSavedJourney("revision-resumed")?.selectedId, response.orderedResultIds[0]);
  const persistedSize = records.get("openestates:search-journeys:v1")?.length;
  saveJourney("3BHK", resumed, response.orderedResultIds[0], "earlier-revision");
  assert.equal(records.get("openestates:search-journeys:v1")?.length, persistedSize);

  Object.defineProperty(globalThis, "localStorage", { configurable: true, get() { throw new Error("disabled"); } });
  assert.doesNotThrow(() => saveJourney("3BHK", response));
  assert.equal(readSavedJourney("revision-1"), undefined);
});

test("portable links restore signed state without parsing the buyer summary or needing local storage", async () => {
  const response = projectSearchJourney(journeyFixture());
  response.orderedResultIds = ["home:ಬೆಂಗಳೂರು"];
  const url = new URL(journeyUrl("3 BHK under ₹2.4Cr", response), "https://example.test");
  assert.equal(url.searchParams.get("q"), "3 BHK under ₹2.4Cr");
  assert.ok(!url.search.includes("signed:"));
  const checkpoint = readSearchCheckpoint(url.hash)!;
  assert.deepEqual(checkpoint, { token: "signed:revision-1", ids: response.orderedResultIds });
  assert.equal(readSearchCheckpoint("#search=%not-json"), undefined);
  assert.equal(readSearchCheckpoint('#search={"token":"x","ids":[12]}'), undefined);
  const original = globalThis.fetch;
  globalThis.fetch = async (path, options) => {
    assert.ok(String(path).endsWith("/search/resume"));
    assert.deepEqual(JSON.parse(String(options?.body)), { parentToken: checkpoint.token, knownResultIds: checkpoint.ids });
    return Response.json(journeyFixture());
  };
  try {
    assert.equal((await resumeSearchCheckpoint(checkpoint)).journey?.active.revision.id, "revision-1");
  } finally { globalThis.fetch = original; }
});

test("API chain sends signed parents, stable mutation IDs, selection, and exact proof identity", async () => {
  const calls: Array<{ path: string; body: Record<string, unknown> }> = [];
  const proof = {
    propertyId: "home", factKey: "nearby_schools", targetEntityId: "school",
    targetLabel: "Fixture School", value: { type: "Numeric", data: 0.8 }, unit: "km",
    contractVersion: 1, snapshotIdentity: "fixture", semanticFingerprint: "fixture", branchId: "branch", predicateId: "predicate", subjectEntityId: "society:home", relation: "near", resolutionStatus: "resolved", derivationChain: [],
    sourceObservations: [{ provider: "fixture", providerObservationId: "exact", subjectEntityId: "society:home", observedAt: "2026-07-14T12:00:00Z", assetLineage: ["fixture/v1"], observationId: "exact", sourceUrl: "https://example.test/exact" }],
  };
  const original = globalThis.fetch;
  globalThis.fetch = async (input, options) => {
    const path = String(input);
    calls.push({ path, body: options?.body ? JSON.parse(String(options.body)) : {} });
    return Response.json(path.endsWith("/resolve") ? proof : path.endsWith("/revisions") ? retainedFixture() : journeyFixture());
  };
  try {
    const initial = await searchProperties("3BHK");
    const target = { kind: "predicate" as const, branchId: "branch-1", predicateId: "budget" };
    const revised = await reviseSearch(initial, "cheaper", "stable-mutation", target, initial.orderedResultIds[0]);
    await resumeSearch(revised);
    const resolved = await resolveSearchProof("signed:exact-receipt", "home");
    assert.equal(calls[1].body.parentToken, "signed:revision-1");
    assert.equal(calls[1].body.clientMutationId, "stable-mutation");
    assert.deepEqual(calls[1].body.target, target);
    assert.equal(calls[1].body.selectedPropertyId, initial.orderedResultIds[0]);
    assert.deepEqual(calls[2].body.knownResultIds, initial.orderedResultIds);
    assert.deepEqual(calls[3].body, { proofToken: "signed:exact-receipt", propertyId: "home" });
    assert.equal(resolvedProofFocus(resolved, "signed:exact-receipt")?.sourceUrl, "https://example.test/exact");
  } finally { globalThis.fetch = original; }
});

test("cancelling one initial-search consumer does not cancel the other", async () => {
  const original = globalThis.fetch;
  let finish: ((response: Response) => void) | undefined;
  globalThis.fetch = () => new Promise<Response>((resolve) => { finish = resolve; });
  try {
    const controller = new AbortController();
    const cancelled = searchProperties("shared query", { signal: controller.signal });
    const live = searchProperties("shared query");
    const rejected = assert.rejects(cancelled, { name: "AbortError" });
    controller.abort();
    await rejected;
    finish?.(Response.json(journeyFixture()));
    assert.equal((await live).journey?.active.revision.id, "revision-1");
  } finally { globalThis.fetch = original; }
});

test("concurrent resume effects share one stateless rebase request", async () => {
  const original = globalThis.fetch;
  let calls = 0;
  let finish: ((response: Response) => void) | undefined;
  globalThis.fetch = () => {
    calls += 1;
    return new Promise<Response>((resolve) => { finish = resolve; });
  };
  try {
    const checkpoint = { token: "signed:revision-1", ids: ["home"] };
    const first = resumeSearchCheckpoint(checkpoint);
    const second = resumeSearchCheckpoint(checkpoint);
    await new Promise((resolve) => setTimeout(resolve, 0));
    assert.equal(calls, 1);
    finish!(Response.json(journeyFixture()));
    assert.equal((await first).journey?.active.revision.id, "revision-1");
    assert.equal((await second).journey?.active.revision.id, "revision-1");
  } finally { globalThis.fetch = original; }
});

test("concurrent context reads share one bounded batch", async () => {
  const original = globalThis.fetch;
  let calls = 0;
  let finish: ((response: Response) => void) | undefined;
  globalThis.fetch = () => {
    calls += 1;
    return new Promise<Response>((resolve) => { finish = resolve; });
  };
  try {
    const first = getPropertyContextsBatch(["home"]);
    const second = getPropertyContextsBatch(["home"]);
    await new Promise((resolve) => setTimeout(resolve, 0));
    assert.equal(calls, 1);
    finish!(Response.json({ contractVersion: 1, snapshotIdentity: "fixture", items: [] }));
    assert.deepEqual(await first, { contractVersion: 1, snapshotIdentity: "fixture", items: [] });
    assert.deepEqual(await second, { contractVersion: 1, snapshotIdentity: "fixture", items: [] });
  } finally { globalThis.fetch = original; }
});

test("concurrent landing discovery effects share one catalog request", async () => {
  const original = globalThis.fetch;
  let calls = 0;
  let finish: ((response: Response) => void) | undefined;
  globalThis.fetch = () => {
    calls += 1;
    return new Promise<Response>((resolve) => { finish = resolve; });
  };
  try {
    const first = getDiscovery();
    const second = getDiscovery();
    await new Promise((resolve) => setTimeout(resolve, 0));
    assert.equal(calls, 1);
    finish!(Response.json({ product_promise: "proof", quotes: [], shelves: [] }));
    assert.deepEqual(await first, { product_promise: "proof", quotes: [], shelves: [] });
    assert.deepEqual(await second, { product_promise: "proof", quotes: [], shelves: [] });
  } finally { globalThis.fetch = original; }
});

test("snapshot conflicts prevent an older discovery request from restoring the cache", async () => {
  const original = globalThis.fetch;
  const discoveryResolvers: Array<(response: Response) => void> = [];
  let discoveryCalls = 0;
  globalThis.fetch = (input) => {
    const path = String(input);
    if (path.includes("/api/properties/")) {
      return Promise.resolve(Response.json(
        { code: "stale_snapshot" },
        { status: 409 },
      ));
    }
    discoveryCalls += 1;
    return new Promise<Response>((resolve) => discoveryResolvers.push(resolve));
  };
  try {
    await assert.rejects(getProperty("reset-cache"));
    const retired = getDiscovery();
    await new Promise((resolve) => setTimeout(resolve, 0));
    assert.equal(discoveryCalls, 1);

    await assert.rejects(getProperty("detect-conflict"));
    const current = getDiscovery();
    await new Promise((resolve) => setTimeout(resolve, 0));
    assert.equal(discoveryCalls, 2);

    discoveryResolvers[0](Response.json({ product_promise: "retired", quotes: [], shelves: [] }));
    discoveryResolvers[1](Response.json({ product_promise: "current", quotes: [], shelves: [] }));
    assert.equal((await retired).product_promise, "retired");
    assert.equal((await current).product_promise, "current");
    assert.equal((await getDiscovery()).product_promise, "current");
    assert.equal(discoveryCalls, 2);
  } finally {
    globalThis.fetch = original;
  }
});
