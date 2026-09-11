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

The accepted Site's `experiments/home-atlas/src/camera.ts` remains the shared camera implementation. The production adapter `frontend/src/lib/atlasNearbyScene.ts` now consumes it directly, with an unlimited context distance so backend-scoped evidence is not silently discarded. Category tour timing comes from the preserved Atlas scene module.

Reviewed Human Atlas `app/scene.tsx`: selected-geometry bounds, distinct selection/isolation states, restrained background emphasis, and user-gesture cancellation. Borrowed these interaction principles, not anatomical meshes or explosion/lift. Its screen-space inspector fitting is a useful follow-up but is not claimed as implemented here. The prior ThreeUI research found no separately established map implementation; no unverified ThreeUI source is claimed.

Rest: category overview. Hover: existing popover. Focus/touch: numbered place and accessible With home / Look closer actions. Reduced motion: zero-duration camera moves. No new floating captions, comparison panels, or autoplay Street View.

## Data contract

Overlay lines/polygons retain `entity_id` from `SceneFeature.entityId`. Selection joins shapes using explicit entity identity or exact feature identity, never label or nearest-coordinate guesses. Polygon-only features receive a deterministic extent-center display anchor, not an inferred entrance. Unlinked geometry remains visible in category overview but is not attributed to an unrelated selected pin.

The Waterford API fixture now preserves archived road segments and explicit polygon ownership from the original Atlas data. Production must supply equivalent identities; missing ownership is not repaired with project-specific heuristics.

## Regression coverage

`frontend/tests/home-atlas-integration.test.ts` verifies lake identity, road segment preservation, polygon-only discovery, mobile scaling, distant evidence retention, and relation-arc endpoints/height. Existing playback tests cover cancellation, pause and resume.

`frontend/e2e/atlas/property.spec.ts` now checks overview/pair/inspect states and relation arc creation/removal, and captures schools-together and school-with-home screenshots when run against real Google 3D.

## Verification gate

The frontend unit suite passed (267 tests), TypeScript and lint passed, and `vite build --mode atlas` passed (with the existing large-chunk warning). Browser verification was attempted but stopped at launch because the configured Playwright Chromium executable is absent. No new rendered screenshot or visual-parity claim is made at this checkpoint.

The default production build requires `VITE_API_BASE` and `VITE_SITE_URL`; configure the actual deployment origins rather than baking placeholder addresses into the application. Local review uses the Atlas build mode and backend fixture API. A configured Google Maps key and WebGL-capable browser are still required for the real map.

Before merge: run `npm run test:atlas-browser` with Chromium and the authorized Maps key, inspect desktop/mobile schools, lakes and road scenes, verify home/selected geometry are unobscured by the drawer, and exercise tour pause/resume plus gesture cancellation. Browser DOM assertions do not by themselves prove photographic composition.

Not added: idle orbit, saved exact viewpoints, township split-layout, or synthetic building extrusion. These are optional enhancements, not prerequisites for nearby parity. Do not reintroduce Lift or map comparison.
