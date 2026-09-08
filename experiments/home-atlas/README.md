# Home Atlas foundation

This experiment preserves the renderer-independent foundation proven in the
[Waterford Home Atlas](https://openestates-home-atlas.guluuu3.chatgpt.site).
It is intentionally not imported by the current OpenEstates frontend.

The goal is to let a later integration reuse the hard parts without copying the
prototype UI:

- source-aware feature contracts;
- neighborhood-scale geometry and nearest-segment calculations;
- responsive camera framing;
- stable category numbering and group/pair selection;
- declarative tour scenes that state both camera and visible context.

## Boundaries

- OSM owns durable geometry. Google may render the scene and provide panorama
  availability, but renderer state is never stored as map truth.
- Distances declare their method and target. A straight-line distance is not a
  route or travel time.
- Road highlighting describes alignment, not measured width.
- Camera elevation is presentation metadata only.
- `fixtures/waterford.sample.json` is a compact test fixture, not production
  config or serving data.
- There is no API key or Google Maps loader in this folder.

## Modules

| Module | Responsibility |
| --- | --- |
| `src/types.ts` | Portable feature, evidence, camera, and scene contracts |
| `src/geometry.ts` | Distance, footprint, feature points, nearest road segment |
| `src/camera.ts` | Responsive place, pair, group, feature, and road cameras |
| `src/selection.ts` | Stable distance ordering, numbering, and scene visibility |
| `src/scenes.ts` | Generic category-tour construction with injected buyer copy |

## Later OpenEstates integration

1. Adapt DAG/serving facts into `AtlasDocument`; do not load the fixture.
2. Reuse `buildNumberedPlaces`, `clusterClosePlaces`, and
   `metroStationsAroundHome` from `frontend/src/lib/nearbyPlateProjection.ts`.
3. Translate `AtlasScene.camera` into the existing Google 3D map adapter.
4. Translate `AtlasScene.visibility` into `PropertyMapContext` layers.
5. Run scenes through `useArrivalPlaybackController`; do not add a second
   playback owner.

The first integration slice should be Metro around Waterford: group view,
numbered list, one selected station, then return home. Roads and schools should
reuse the same interaction grammar.

## Checks

From this folder, with Node 22 or newer:

```bash
node --experimental-strip-types --test tests/homeAtlas.test.ts
```

For strict type checking when TypeScript is available:

```bash
tsc --noEmit --strict --target ES2022 --module NodeNext --moduleResolution NodeNext src/*.ts
```

