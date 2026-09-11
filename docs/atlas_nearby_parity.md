# Atlas nearby parity — integration checkpoint

## Buyer interaction contract

- Category: home plus every API-scoped place and mapped extent. No local distance cutoff.
- Place: home-relative pair framing and an elevated straight-line relationship arc.
- Look closer: inspect that feature's actual geometry; With home restores context.
- Show together: return to the complete category, not the first item.
- Tour: overview, pair, inspection, deliberate pauses, return home. Optional whole-neighborhood scope uses the same control and playback cancellation lifecycle.
- Direct map gestures cancel playback. Pause/resume retains the current stop; reduced motion settles without flight.
- Existing sidebar, road speed control, aerial road driver and Street View exit remain in place.

## Reused learning and code

The local tangent-plane solver in `experiments/home-atlas/src/screenFit.ts` fits sourced home/feature geometry, lifted anchors, and the relationship arc into the measured unobstructed screen rectangle. It accounts for the persistent workspace edge, desktop drawer, mobile bottom sheet, category rail, and journey dock. Heading follows the home/feature bearing plus configured offsets; no category or feature identity changes the camera math. Category timing and camera tuning come from `app/config/ui/home-atlas.json`.

Reviewed Human Atlas `app/scene.tsx`: selected-geometry bounds, distinct selection/isolation states, restrained background emphasis, and user-gesture cancellation. Borrowed those interaction principles and clear-frame fitting, not anatomical meshes, explosion controls, or Three.js architecture. The prior ThreeUI research found no separately established map implementation; no unverified ThreeUI source is claimed.

Rest: category overview shows home plus every API-scoped item. Nearby map pins contain only `H` and numbers; place names stay in the drawer. Hover adds no duplicate place label. Keyboard focus and touch use the same numbered place, `With home`, and `Look closer` actions. Reduced motion applies scene cameras without animated travel. Direct manipulation, hidden-document cancellation, pause/resume position, and Street View return remain intact.

Pair focus borrows Human Atlas's screen-space selection principle: home, the selected place, their sourced extents, lifted markers, and relationship arc are contained in the clear canvas. Inspect uses a closer configured range so selected geometry gains screen presence while home remains a meaningful second anchor. PR 126's overview → pair → inspect → home pacing and 70% move / 30% dwell rhythm are preserved through `AtlasScene`; a camera arbiter makes society, road, and nearby ownership mutually exclusive. The property identity remains a corner overlay rather than a full-height exclusion.

The UI-critic pass removed the stacked `Nearby` heading, redundant place count, arc tutorial caption, source-specific control copy, map popovers, and duplicate accessible place names. Buyer controls are now `Aerial`, `Site outline`, and `Focus`. The mobile evidence drawer is a bounded bottom sheet so a usable map rectangle remains above it.

## Data contract

Overlay lines/polygons retain `entity_id` from `SceneFeature.entityId`. Selection joins shapes using explicit entity identity or exact feature identity, never label or nearest-coordinate guesses. Polygon-only features receive a deterministic extent-center display anchor, not an inferred entrance. Unlinked geometry remains visible in category overview but is not attributed to an unrelated selected pin.

The Waterford API fixture now preserves archived road segments and explicit polygon ownership from the original Atlas data. Production must supply equivalent identities; missing ownership is not repaired with project-specific heuristics.

## Regression coverage

`frontend/tests/home-atlas-integration.test.ts` verifies lake identity, road segment preservation, polygon-only discovery, distant evidence retention, relation-arc endpoints/height, camera ownership, and point/polygon/line containment at near, medium, and distant ranges under translated and rotated coordinates. It also verifies inspect geometry occupies more screen space while the home stays inside the clear frame. Existing playback tests cover cancellation, pause, and resume.

`frontend/e2e/atlas/property.spec.ts` now checks overview/pair/inspect states and relation arc creation/removal, and captures schools-together and school-with-home screenshots when run against real Google 3D.

## Verification gate

Focused real-Chrome verification covers desktop category overview, pair, inspect, tour pause/resume, road direction, Street View return, and the `390×844` bottom-sheet composition with no page errors. The raw UI-polish frames, trace, video, and three-frame contact sheet are under ignored test storage at `frontend/test-results/atlas-parity/ui-polish-check/`.

| Society rest | Schools pair | Mobile bottom sheet |
| --- | --- | --- |
| ![Society resting camera](screenshots/atlas-camera-parity/society-rest.jpg) | ![Home and selected school pair](screenshots/atlas-camera-parity/schools-pair.jpg) | ![Mobile nearby bottom sheet](screenshots/atlas-camera-parity/mobile-nearby.jpg) |

The default production build requires `VITE_API_BASE` and `VITE_SITE_URL`; configure the actual deployment origins rather than baking placeholder addresses into the application. Local review uses the Atlas build mode and backend fixture API. A configured Google Maps key and WebGL-capable browser are still required for the real map.

Two consecutive timing-enforced full journeys passed under `frontend/test-results/atlas-parity/final-parity-timing-1/` and `final-parity-timing-2/`. The trace contract keeps overview, pair, inspect, and home dwell within 5% of configured reference timing while also checking monotonic road distance, persistent map identity, exact pair marker/arc counts, and zero page errors. The local API loaded promoted catalog `catalog-71-ffb4dc50-117e-453c-b26f-41822430324e` (159 properties); the promoted Prestige Waterford response served the exact OSM home boundary and four scoped arrival features. The public API returned HTTP 503 during final verification, so local promoted-bundle verification is the recorded production-boundary check.

Attach the representative resting and focus screenshots from the recorded artifacts when the branch PR is opened. Browser DOM assertions do not by themselves prove photographic composition.

Not added: idle orbit, saved exact viewpoints, township split-layout, or synthetic building extrusion. These are optional enhancements, not prerequisites for nearby parity. Do not reintroduce Lift or map comparison.
