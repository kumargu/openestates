# Nearby integration review

This is a Sites interaction mock. No production OpenEstates components were modified.

## Clear-view foundation pass

## Deployment QA pass

The OSM source extract identifies 78 metro ways as `service=yard` and one as `service=siding`. The document adapter now exposes only the 16 non-service subway ways for passenger context; the original 95-way snapshot remains untouched. This is a snapshot allow-list, to be replaced by service-tag filtering at ingestion in production.

Tour pause retains its caption, resume interpolates back to the saved viewpoint before continuing, and changing category cancels the previous animation. The deterministic application-controller checks cover these behaviors alongside group versus selected detail state. Map-dependent controls are disabled if the map becomes unavailable.

Browser QA was attempted through the supervised preview. This Worker prototype has no dev server, and installation of the preview dependency was stopped by an environment approval failure. No dependency or preview workaround was committed. Visual inspection and live Google 3D / Street View verification remain outstanding; source/controller tests are not substitutes for those checks.

- Road descent uses nearest-point projection on individual OSM segments and a segment-derived camera heading. It does not stitch ways, claim road width, or simulate ground travel. Street View stays a separate user action.
- Category scenes carry `overview` alongside their camera so the first scene actually displays the category places. `visibleFeatures` resolves group versus pair consistently. Overview suppresses individual-place details until a place is selected.
- A persistent context label identifies the selected place or category. The tour caption includes its step count. Closing Nearby returns to Waterford rather than leaving unexplained overlays behind.
- Independent `evidence.location`, `evidence.geometry`, `evidence.distance`, and `evidence.cameraElevation` records prevent mixed Google/OSM source claims. Data remain a curated snapshot; no refresh or freshness guarantee is implied.
- Human Atlas reference: `app/page.tsx` selection/isolation and member-list patterns informed the group-to-single-place interaction. No reference code was copied.

Verified with pure geometry/data checks, simulated selection/scene/cancellation checks, and a production build. Live Google rendering and visual layout still require browser review. The road quiet mask uses a padded extent solely for visual emphasis, not a road boundary.

The prototype now exposes three renderer-independent seams:

- `web/atlas-document.js` is the versioned data hand-off and provenance adapter.
- `web/atlas-core.js` owns pure selection, distance, geometry and camera math.
- `web/atlas-scenes.js` builds declarative tour scenes without knowing about Google Maps, HTML or playback state.

The current Google renderer in `web/app.js` consumes these modules. A production adapter can translate the same feature and scene shapes into OpenEstates' TypeScript contracts without copying the mock's DOM code.

## What the existing product already provides

Reviewed `frontend/src/components/property/PropertyArrivalMap.tsx`, `PropertyArrivalGoogle3DMap.tsx`, and `frontend/src/lib/nearbyPlateProjection.ts`.

- `PropertyMapContext` carries home boundary, category layers, layer lines, access lines and metro lines.
- `buildNumberedPlaces` uses stable feature/entity IDs. Preserve those identities across list, markers and playback.
- `clusterClosePlaces` already supports scale-dependent grouping (80 m nearby, 350 m area). Preserve selected places when clustering.
- `metroStationsAroundHome` selects stations around the nearest alignment and retains focus. Reuse it instead of flooding the map with every station.
- `useArrivalPlaybackController` is the shared playback owner. Category tours should become scenes in that controller rather than a second animation engine.

## Composition to port

One category opens a short distance-sorted list with matching map numbers. One selected place expands its details. Show together fits home plus the category; selecting a row returns to focused context. With Waterford isolates the home/place pair. A category tour visits each item and returns home. Keep camera controls above this surface and preserve gesture-to-pause.

For production volumes, feed existing projected clusters into this same list/map relationship. Show cluster counts at wide scale, expand on selection, and retain the selected entity's number. Do not create labels for every underlying place. This mock has 14 curated records and shows at most three category places together; it does not implement the production clustering engine.

Use the existing context geometry for roads and metro. Keep disconnected segments separate, limit rendering to the local area, and reveal only the current category. Use source-backed area polygons for societies and lakes. Keep boundaries ground-clamped; the surrounding lift effect has been removed. Quiet surroundings is optional visual emphasis, not a factual noise assessment.

## Data and verification limits

Distances are straight-line estimates, not route lengths or travel times. Roads use representative mapped points. Lake polygons describe mapped extent, not present water level or public access. OSM construction tags do not verify current project status. New camera heights reuse the nearest verified terrain sample for framing only. Street walking follows provider-linked panoramas and stops when forward coverage ends.

Checked source selectors, finite camera values, source geometry, animation cancellation, and built Worker responses. No browser visual QA was performed in this pass.
