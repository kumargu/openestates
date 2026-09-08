# Home Atlas experiment

This package preserves the renderer-independent foundations and the complete visual prototypes proven in the [Waterford and Brigade Home Atlas](https://openestates-home-atlas.guluuu3.chatgpt.site/). It is intentionally isolated from the current OpenEstates frontend until the property-page integration is designed and reviewed.

Everything lives under one package so a later Codex session can reuse the accepted behavior without reconstructing geometry, selection, camera, tour, or scale-policy decisions.

## One production contract

`SurfaceSceneResponse` remains the production scene boundary:

```text
OSM and Google collectors
  -> DAG facts and config
  -> serving bundle
  -> Rust SurfaceSceneResponse
  -> presentation, selection, and journey planners
  -> Google 3D renderer and existing React playback controls
```

`AtlasDocument` in `src/types.ts` is an experiment-only normalized interaction model used by the compact Waterford fixture. It must not become another API, storage schema, or source of truth. Production integration should consume `SurfaceSceneResponse` and adapt only the minimum view state needed by the portable helpers.

The archived `prototype/web/atlas-document.js` has the same constraint: it preserves the working Site, but is not a production import surface.

## Package map

| Path | Responsibility |
| --- | --- |
| `src/types.ts` | Portable feature, evidence, camera, visibility, and scene contracts |
| `src/geometry.ts` | Distance, footprint, feature-point, and nearest-segment calculations |
| `src/camera.ts` | Responsive place, pair, group, feature, and road camera framing |
| `src/selection.ts` | Stable ordering, numbering, and group/pair visibility |
| `src/scenes.ts` | Generic category tours with injected buyer-facing copy |
| `src/presentation.ts` | Society, estate, and township policy derived from `SurfaceSceneResponse` |
| `src/journey.ts` | Exact route projection, timed journey scenes, and camera interpolation |
| `fixtures/` | Compact Waterford interaction fixture |
| `prototype/` | Runnable Waterford and Brigade visual reference plus canonical Brigade OSM inventory |
| `screenshots/` | Review evidence captured from the running Site |
| `tests/` | Contract coverage for Waterford interactions and Brigade scale/journey behavior |

## Rules that survive integration

- OSM owns durable boundaries, roads, buildings, and mapped extents. Google may own place locations, imagery, panorama availability, and rendering.
- Every distance declares its method and target. Straight-line distance is never presented as travel time.
- Road highlighting describes mapped alignment, not measured width.
- Scale behavior comes from scene geometry and configurable thresholds, never society-name branches.
- Route direction is resolved upstream from mapped direction/access and entrance facts. The camera planner preserves the supplied order.
- Buyer copy is injected into scene construction instead of embedded in geometry code.
- Camera altitude and elevation are presentation metadata, not geographic facts.
- The existing `useArrivalPlaybackController` remains the playback owner; journey builders produce scenes and do not start a second animation loop.
- API keys, deployment identity, generated build output, and speculative tower/amenity labels do not belong in this package.

## Recommended wiring sequence

1. Emit the required OSM/Google evidence through the DAG and serving bundle into `SurfaceSceneResponse`.
2. Run `resolveAtlasPresentation` to select society, estate, or township layout from geometry and named config thresholds.
3. Adapt scene features into the existing nearby projections and portable selection/camera helpers.
4. Start with Waterford Metro: group view, numbered list, one selected station, and return home.
5. Add road descent/walk using the ordered route and `buildAerialJourney`.
6. Enable the township split-context treatment for Brigade only after the same contracts pass with production scene data.
7. Fit the controls into the current property-page theme; do not copy the prototype shell pixel-for-pixel.

## Validation

From the repository root:

```bash
frontend/node_modules/.bin/tsc -p experiments/home-atlas/tsconfig.json
node --experimental-strip-types --test experiments/home-atlas/tests/*.test.ts
```

From `experiments/home-atlas/prototype/`:

```bash
npm run check
```

The combined suite covers ten contracts: Waterford source ownership, ordering, visibility, responsive cameras and category tours; Brigade route fidelity, monotonic timing, heading interpolation, and geometry-driven presentation policy.

## Deliberately deferred

This PR does not import the package into buyer-facing UI, alter backend/DAG/config behavior, add another playback controller, or claim that experimental inventory is production evidence. Those changes should arrive as small, reviewable integration slices after this foundation is merged.
