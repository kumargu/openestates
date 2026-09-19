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
| `src/contextLines.ts` | Renderer-neutral OSM line filtering, local clipping, and presentation handoff for metro and road overlays |
| `src/camera.ts` | Responsive place, pair, group, feature, and road camera framing |
| `src/selection.ts` | Stable ordering, numbering, and group/pair visibility |
| `src/scenes.ts` | Generic category tours with injected buyer-facing copy |
| `src/presentation.ts` | Society, estate, and township policy derived from `SurfaceSceneResponse` |
| `src/journey.ts` | Exact route projection, longest-continuous-segment selection, elapsed-time road flight, responsive aerial camera framing, Street View handoff, and camera interpolation |
| `fixtures/` | Compact Waterford interaction fixture |
| `prototype/` | Runnable Waterford and Brigade visual reference plus canonical Brigade OSM inventory |
| `screenshots/` | Review evidence captured from the running Site |
| `tests/` | Contract coverage for Waterford interactions and Brigade scale/journey behavior |

## Rules that survive integration

- OSM owns durable boundaries, roads, buildings, and mapped extents. Google may own place locations, imagery, panorama availability, and rendering.
- Every distance declares its method and target. Straight-line distance is never presented as travel time.
- Road highlighting describes mapped alignment, not measured width.
- Metro stations and metro geometry remain separate: Google Places supplies station points, OSM supplies passenger-track segments, and `fixtures/waterford.metro.json` supplies the reviewed presentation/config boundary.
- Purple is presentation data, not metro truth. `buildContextLines` accepts the configured style and never hardcodes a category or Google renderer.
- Scale behavior comes from scene geometry and configurable thresholds, never society-name branches.
- Route direction is resolved upstream from mapped direction/access and entrance facts. The camera planner preserves the supplied order; reversal is an explicit adapter option, never a coordinate or society-name heuristic.
- Buyer copy is injected into scene construction instead of embedded in geometry code.
- Camera altitude and elevation are presentation metadata, not geographic facts.
- The existing `useArrivalPlaybackController` remains the playback owner; journey builders produce scenes and pure per-frame calculations and do not start a second animation loop.
- OpenEstates' existing Street View remains the production street-level owner. `projectStreetHandoff` only transfers route position and heading back to the aerial camera.
- API keys, deployment identity, generated build output, and speculative tower/amenity labels do not belong in this package.

## Recommended wiring sequence

1. Emit the required OSM/Google evidence through the DAG and serving bundle into `SurfaceSceneResponse`.
2. Run `resolveAtlasPresentation` to select society, estate, or township layout from geometry and named config thresholds.
3. Adapt scene features into the existing nearby projections and portable selection/camera helpers.
4. Start with Waterford Metro: adapt the configured passenger segments through `buildContextLines`, map each output to one Google `Polyline3DElement`, then add the group view, numbered list, one selected station, and return home. Do not join disconnected OSM ways.
5. Add the accepted aerial road journey using `selectPrimaryAtlasRoute`, `advanceRoadDistance`, and `roadFlightCamera`; connect `projectStreetHandoff` to the existing OpenEstates Street View surface.
6. Enable the township split-context treatment for Brigade only after the same contracts pass with production scene data.
7. Fit the controls into the current property-page theme; do not copy the prototype shell pixel-for-pixel.

## Metro line renderer seam

The purple Waterford line is a presentation of reviewed OSM passenger-track
segments. The portable layer keeps those segments separate and clips them to
the configured local radius. A Google 3D adapter stays intentionally small:

```ts
const lines = buildContextLines({ segments, origin, maximumDistanceM, style });

for (const line of lines) {
  const overlay = new Polyline3DElement({
    altitudeMode: "CLAMP_TO_GROUND",
    strokeColor: line.style.strokeColor,
    strokeWidth: line.style.strokeWidth,
    drawsOccludedSegments: line.style.drawsOccludedSegments,
  });
  overlay.path = line.path;
  map.append(overlay);
}
```

`fixtures/waterford.metro.json` records the reviewed segment ids, 3.2 km
context radius, and current purple treatment. Production should emit equivalent
values from DAG facts and map-layer config instead of importing this fixture.

## Accepted Waterford road view

The current PR already preserves the latest road implementation. `src/journey.ts`
selects one continuous OSM line, applies explicit direction, advances by elapsed
time at the selected 0.5x-2x rate, and produces the accepted forward aerial
camera. `projectStreetHandoff` projects the Street View position back onto that
same route so the existing OpenEstates Street View can exit cleanly to the aerial
road position. The runnable adapter remains in `prototype/web/arrival-walk.js`.

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

The combined suite covers fourteen contracts: Waterford source ownership, ordering, visibility, responsive cameras and category tours; Brigade route fidelity, monotonic timing, heading interpolation, geometry-driven presentation policy, continuous-segment selection, speed bounds, responsive road framing, and Street View-to-aerial projection.

## Deliberately deferred

The prototype now includes the accepted calm shell and continuous ECC Road flight as a visual adapter, but this PR does not import the package into buyer-facing UI, alter backend/DAG/config behavior, add another production playback controller, or claim that experimental inventory is production evidence. Those changes should arrive as small, reviewable integration slices after this foundation is merged.
