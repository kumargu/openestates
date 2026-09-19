import assert from 'node:assert/strict';
import test from 'node:test';
import { nearbyPlacesByDistance, nearbyBrowseWindow } from '../src/lib/atlasBrowse.ts';
import detail from '../fixtures/prestige-waterford-api/property-detail.json' with {type: 'json'};
import type { PropertyDetailResponse } from '../src/lib/types.ts';
import { resolveHomeAnchor } from '../src/lib/nearbyPlateProjection.ts';
import policy from '../src/lib/atlasPolicy.ts';

const context = (detail as unknown as PropertyDetailResponse).map_context;
const home = resolveHomeAnchor(context)!;

test('all nearby categories sort by their displayed distance without mutating serving facts', () => {
  const original = structuredClone(context.places);
  for (const layer of new Set(context.places.map(place => place.layer))) {
    const places = nearbyPlacesByDistance(context.places.filter(place => place.layer === layer), home);
    assert.ok(places.every((place, index) => index === 0 || place.distance_km! >= places[index - 1].distance_km!));
    assert.deepEqual(places.map(place => place.number), places.map((_, i) => i + 1));
  }
  assert.deepEqual(context.places, original);
});

test('three-place browsing preserves stable numbers and adds a distant selection', () => {
  const places = nearbyPlacesByDistance(context.places, home);
  assert.ok(places.length > 6);
  const count = policy.nearby.initialPlaceCount;
  assert.equal(count, 3);
  assert.deepEqual(nearbyBrowseWindow(places, 0, count, null), places.slice(0, 3));
  assert.deepEqual(nearbyBrowseWindow(places, 3, count, null), places.slice(3, 6));
  const last = places.at(-1)!;
  assert.deepEqual(nearbyBrowseWindow(places, 0, count, last.feature_id ?? last.name), [...places.slice(0, 3), last]);
  assert.deepEqual(nearbyBrowseWindow(places, 999, count, null), places.slice(-3));
});


test('proof outside the opening window stays additive and missing distances use real coordinates', () => {
  const places = nearbyPlacesByDistance(context.places, home);
  const last = places.at(-1)!;
  const proof = {surfaceId: 'around_this_home', layerId: last.layer, factKey: 'nearby',
    featureId: last.feature_id, entityId: last.place_entity_id, matchedLabel: last.name, reason: 'test'};
  const focused = nearbyBrowseWindow(places, 0, 3, null, proof);
  assert.ok(focused.includes(last));
  assert.ok(places.slice(0, 3).every(place => focused.includes(place)));
  const [derived] = nearbyPlacesByDistance([{...last, distance_km: undefined}], home);
  assert.ok(Number.isFinite(derived.distance_km) && derived.distance_km! >= 0);
  assert.equal(nearbyPlacesByDistance([], home).length, 0);
});
