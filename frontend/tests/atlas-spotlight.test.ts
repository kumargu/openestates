import assert from 'node:assert/strict';
import test from 'node:test';
import { mergeSpotlightOpenings, spotlightCircle, spotlightPaths, spotlightRadiusM } from '../src/lib/atlasSpotlight.ts';

test('distant subjects keep a readable spotlight as the view pulls back', () => {
  const near = spotlightRadiusM(2000, 1000, 44, 'place');
  const far = spotlightRadiusM(12000, 1000, 44, 'place');
  assert.ok(far > near * 4);
  assert.ok(spotlightRadiusM(12000, 1000, 58, 'selected') > far);
  assert.ok(spotlightRadiusM(12000, 700, 44, 'place') > far);
  assert.ok(spotlightRadiusM(2000, 1000, 44, 'place', 60) > near,
    'a wider live lens keeps the same screen-space emphasis');
});

test('area illumination preserves concave shorelines and separate parts at every zoom', () => {
  const footprints = [[{lat: 1, lng: 1}, {lat: 1, lng: 3}, {lat: 2, lng: 2},
    {lat: 3, lng: 3}, {lat: 3, lng: 1}, {lat: 1, lng: 1}],
  [{lat: 4, lng: 1}, {lat: 4, lng: 2}, {lat: 5, lng: 1}, {lat: 4, lng: 1}]];
  for (const range of [700, 12000]) {
    for (const scale of [1, 1.14, 1.3]) {
      assert.deepEqual(spotlightPaths({anchor: {lat: 2, lng: 2}, footprints, emphasis: 'selected'},
        range, 1000, 58, scale), footprints);
    }
  }
});

test('mapped home illumination stays around its site as nearby views pull back', () => {
  const extent = [{lat: 1, lng: 1}, {lat: 1, lng: 1.002}, {lat: 1.001, lng: 1.001},
    {lat: 1.002, lng: 1}, {lat: 1, lng: 1}];
  const subject = {anchor: {lat: 1.001, lng: 1.001}, extent, emphasis: 'home' as const};
  const near = spotlightPaths(subject, 700, 1000, 58, 1);
  const far = spotlightPaths(subject, 12000, 1000, 58, 1);
  assert.deepEqual(far, near, 'pulling back must not flood surrounding blocks with home light');
  assert.deepEqual(near, [extent], 'home cutout must match the mapped boundary without padding');
});

test('overlapping apertures merge transitively without losing distant openings', () => {
  const anchor = {lat: 12.98, lng: 77.74};
  const circles = [0, 0.0015, 0.003, 0.05].map(offset => spotlightCircle({...anchor, lng: anchor.lng + offset}, 120));
  const merged = mergeSpotlightOpenings(circles);
  assert.equal(merged.length, 2);
  assert.ok(merged[0].some(point => point.lng < anchor.lng));
  assert.ok(merged[0].some(point => point.lng > anchor.lng + 0.003));
  assert.deepEqual(merged[1], circles[3]);
});
