# Home Atlas → property page integration

## Baseline and scope

Branch: `feat/home-atlas-property-integration`. Prepared from `origin/main` at
`b4c2ccc` (PR #128), with PR #126 (`b36ccae`) merged at `7797111`.
The archived experiment remains intact. This integration changes the real
property page, not the Sites prototype. When an `arrival_story` scene carries a
valid anchor and experience policy, it joins the normal property journey: a
three-frame cinematic hero, the existing `Around this home` chapter frame, the
existing `The way in` chapter frame, and reviews. The two chapter frames now
render the sourced 3D geometry and approach journey. The workspace sidebar
remains the sole navigation shell.
Properties without that immersive scene keep the existing 2D evidence path. The
Rust serving contract is unchanged.

## Run locally

Use the repository's Node 22 environment, then:

```bash
cd frontend
npm ci
# Configure VITE_GOOGLE_MAPS_API_KEY in your untracked .env.local.
npm run dev:atlas
```

Open `/property/fixture-prestige-waterford-3bhk` on the local server. This uses
the existing fixture API seam and the real React property page, real Google 3D,
real elevation service, and existing OpenEstates Street View implementation.
Nothing substitutes a painted canvas or mock Google SDK.

The fixture is explicitly named **local demo**. Listing values are illustrative.
Its geometry and nearby points are copied from PR #126's archived Waterford
inventory; the metro is restricted to its 16 reviewed passenger segments.
It does not claim these archived values are fresh production facts. The fixture
does not invent an entrance. No fixture is promoted to the DAG or Parquet.

For the real backend, run `npm run dev` without fixture mode, use the normal API
configuration, and open a real property ID. No component changes are necessary.
Fixture responses are disabled in production deployments. Vercel preview
deployments can opt into the review fixture only by opening the fixture property
URL with `?fixture=atlas`; ordinary preview URLs continue to use the configured
API. Production builds retain the repository's existing `VITE_API_BASE` and
`VITE_SITE_URL` requirements.

Google rendering still requires an enabled/billed Google project, the relevant
Maps/3D/Street View/elevation access, an authorized browser referrer, internet
access, and a supported WebGL browser. The supplied key is not committed.
Missing Google rendering produces the existing fallback, not invented imagery.
The deployment CSP explicitly permits the Google Maps loader, Google-hosted map
imagery, 3D/Street View data requests, and Google fonts while retaining the
existing self-only defaults for unrelated resources.

## Runtime ownership

| Concern | Owner |
| --- | --- |
| Durable geometry, places, receipts | Existing `SurfaceSceneResponse` from Rust/DAG |
| Scene → existing map model | `frontend/src/lib/surfaceSceneProjection.ts` |
| Continuous OSM route and per-frame camera maths | Preserved `experiments/home-atlas/src/journey.ts` |
| Independent metro segments | Preserved `contextLines.ts` through `homeAtlasProjection.ts` |
| Playback state, waits, pause, resume, cancellation | Existing `ArrivalPlaybackController` |
| Aerial animation frames | `useAtlasRoadFlight`, subordinate to that controller |
| Street-level rendering | Existing `useGuidedStreetViewTour` |
| `rest` / `browse` / `selected` / `tour` UI state | `PropertyArrivalMap` via `atlasNearbyUi` |
| Google elements, overlays, focus camera | Existing `PropertyArrivalGoogle3DMap` |
| Road speed/camera tuning and overlay styling | `app/config/ui/home-atlas.json` |
| Property identity and save/note actions | The cinematic property hero |

The renderer consumes scene-provided coordinates. It has no Waterford-name branch.
Society camera framing continues to fit the mapped footprint through
`societyCameraComposition`; large boundaries therefore widen the view. The
portable estate/township classification and Brigade-specific reference experience
are preserved in PR #126, but a production split-context locator is **not** wired
by this change. Do not describe that separate layout as integrated.

## Buyer experience

- The cinematic hero remains its own first chapter and owns the property name,
  facts, Save, and Note.
- `Around this home` remains a separate location chapter. Its map is replaced
  by the sourced 3D Atlas without changing the surrounding page composition.
- `The way in` remains the next arrival chapter and uses the dynamic approach
  geometry.
- Atlas opens on the home inside the original `Around this home` shell. Its
  narrow left rail keeps Schools, Hospitals, Tech parks, Parks, Lakes, and
  Metro outside the map instead of covering rendered geometry.
- Choosing a layer fits its scoped evidence with the home; choosing a
  destination fits the home-to-place pair.
- The hero is intentionally shorter than the evidence chapters. Nearby has a
  taller resting frame and can grow vertically for active layers, selections,
  and tours.
- The restored layer rail is slightly narrower than its pre-126 width. Its
  active row ends in an arrow. During tour playback it folds to icon width so
  the map gains space; pause or completion restores labels and radius controls.
- A segmented 2 km / 5 km / All control scopes every Nearby point layer.
  Changing it updates the map markers and tour. A selected or search-matched
  place remains visible outside the chosen radius. The radius is visually
  inactive until a layer is selected.
- Scene-derived Schools, Hospitals, Tech parks, Parks, Lakes, Breweries, and
  Metro remain payload-driven. One layer is shown at a time so eligibility and
  proof stay legible.
- Layer rows remain stable when a radius has no matching point. A quiet map
  state explains the empty scope; line or polygon geometry still renders.
- Search proof focus is forwarded to the arrival scene and opens the matching
  layer or place. Selected places retain Directions, Source, and Add note
  actions, and closing a selection returns keyboard focus to its layer row.
- The existing full-screen map action remains available with the 3D renderer.
- Metro points and the actual API-provided track segments. Segments stay separate;
  no straight line is invented between stations. Browse, selected place, and
  category tour reuse one camera owner.
- Nearby categories are derived from the existing payload. Polygon overlays
  preserve supplied holes and multipolygon parts; missing polygons are not
  manufactured.
- Approach Road is a shorter chapter below Atlas. It starts with context,
  descends to the accepted forward aerial
  camera, holds briefly for orientation, then moves continuously. One speed
  slider controls 0.5–2× progression. This is an aerial inspection of mapped road
  alignment, not a claim about walking/driving permissions, widths, or travel time.
- Street View is opt-in. It starts near the current aerial route position using
  the existing panorama sequence. **Back to aerial** is available even while
  Street View loads or has no coverage. Escape also exits. The returned panorama
  position is projected onto the same aerial route, preserving heading; replay
  is explicit after returning.
- Direct map gestures stop a Nearby tour immediately and preserve the current
  camera. `Reset view` appears only after that manual takeover. Marker selection
  is not treated as a map gesture. Pause/resume uses the existing controller.
  Reduced-motion users do not receive the automatic aerial flight.

## Integration gaps fixed

1. Scene metro lines previously went into `access_lines`, while the arrival
   renderer only read `metro_lines`. They now reach the metro view.
2. Scene-scoped lines are not re-clipped by an unrelated frontend radius; this
   preserves the backend's scope and any expanded proof context.
3. Road mode no longer automatically replaces the aerial canvas with Street View.
4. The old road camera branch was removed to avoid competing camera ownership.
5. Camera code no longer assumes `flyCameraTo` returns a Promise; current Google
   documentation declares no return value. See [Google 3D reference](https://developers.google.com/maps/documentation/javascript/reference/3d-map).
6. Scene-driven map rebuilds reattach overlays; property changes reset local
   view state. Inactive Street View/aerial panes are inert.
7. Fixture code is loaded only on demand, not in the normal initial app bundle.

## Interaction/design note

[#137](https://github.com/kumargu/openestates/issues/137) restores the exact
pre-126 `Around this home` composition: a narrow layer rail on the left and the
map in its own uninterrupted slot. Only the renderer and camera behavior change;
the old OSM map is replaced by sourced Google 3D geometry. The anatomical
dashboard, multi-layer combinations, explode slider, auto-rotation, and
decorative orbit were not borrowed.

At rest, the cinematic hero remains the first chapter and `Around this home`
opens on the home-focused map with all layer switches off. Choosing a layer
fits its scoped evidence; selecting a destination fits the pair. The layer rail
never enters the map footprint, so camera framing no longer needs drawer
collision behavior. Reduced motion skips automatic flights. Touch/mobile
adaptation keeps the full horizontal layer controls instead of using the
desktop tour fold.

Nearby motion reuses the existing sourced chapter sequence through one rail
control that becomes Pause and Resume. No timeline, speed control, or decorative
orbit was added.

The page uses the same warm background as the rest of the property experience.
The hero is limited to three cinematic frames while the full gallery remains
available from its existing action.

The selected place remains selected while the inspector is visible. A selected
home-to-destination relationship rotates onto the map's wide axis before fitting,
so bearing alone cannot force a much wider camera range. Camera fitting reserves
the camera rail; the layer rail sits outside the rendered map. A direct gesture interrupts a tour
without moving the camera. Pair scenes use sourced markers and geometry only;
the straight-line distance remains in the selected row instead of drawing a
synthetic connector that could be mistaken for a route.

ThreeUI's Animated Top Dock was checked for its pointer, focus, and
reduced-motion treatment. Its proximity spring, item growth, shader/glass
variants, and decorative motion were intentionally not borrowed: this stage
needs stable targets and the map already supplies motion.

Rest: shorter cinematic hero followed by the taller home-focused location
chapter. Browse: the persistent side rail, direct radius choices, and one
arrow-marked active layer. Selected: a compact map card adds distance and
Directions. Tour: one play/pause action appears on the camera rail; the rail
folds only while playback is running and unfolds on pause. Save and Note stay
on the hero.

## Verification and remaining gate

Commands:

```bash
cd frontend
npm run lint
npm test
VITE_API_BASE=https://api.example.com VITE_SITE_URL=https://example.com npm run build
npx playwright install chromium
npm run test:atlas-browser
```

The browser check runs the actual property route with only fixture API data and
captures the hero, Home + Nearby Atlas, selected pair, approach road, and reviews in
`frontend/test-results/atlas/`. It requires the Google key and working real 3D;
it does not silently pass using a mocked renderer.

The localhost browser pass used real Google 3D. It confirmed the restored
hero → Around this home → The way in → reviews composition, the pre-126 layer
rail, home and category framing, 2 km scope, layer toggles, the purple Metro
track, and Approach Road.
Mobile was not treated as an acceptance target for this iteration.

Observed in this environment: lint, TypeScript, the production build, all 280
frontend tests, and `git diff --check` pass.

The real promoted Parquet bundle/Rust API was not available for end-to-end
validation in this pass. Sparse production scenes will remain sparse: they do
not silently borrow fixture evidence. Validate one real Waterford ID against your
local backend before treating this as production-ready.

## PR 138 — immersive explorer refinement (2026-09-16)

The buyer can now read nearby place names before moving the camera, inspect a
place at its own scale, and expand the whole explorer without losing controls.
The native dialog promotes the existing Google map into the browser's top layer;
closing it preserves the scene and restores focus. The inspector sits below the
map, so it never covers the geometry the camera is fitting. The category rail
and map dimensions remain stable across play/pause.

### Camera and geometry contract

- `pair` contains the home, destination and explicitly associated OSM geometry.
  It compares horizontal and vertical fits to use the available viewport.
- `inspect` contains the destination's real polygon/line extents and marker.
  Home is deliberately allowed outside the frame; `Show with home` restores the
  relationship. This supersedes the former inspect-must-contain-home contract.
- The invisible synthetic relationship arc has been removed from camera fitting.
  No routes, entrance connectors, buildings or source facts are manufactured.
- Inspection asks the existing cached Google Elevation service for the destination
  terrain, including tour inspection. Google `flyCameraTo` presents the scene;
  OSM geometry identity, polygon holes and separate route segments are preserved.
- `View another side` refits the same geometry at a 90-degree heading increment.
  `Top view` and `Focus surroundings` are explicit controls; close-ups show the
  unmasked photorealistic scene. Manual pointer and keyboard gestures stop motion.
- Tours expose previous/next view, progress, pause/resume and end. Null selection
  in overview/return-home really clears the previous destination.

### Interaction note / UI critic

ThreeUI's current catalog was checked (https://threeui.com/). Its decorative
WebGL backgrounds do not solve this explorer's evidence-navigation problem; no
new ThreeUI implementation was copied. The existing narrow rail is developed
into persistent category navigation plus a named place list. Google camera API
reference: https://developers.google.com/maps/documentation/javascript/3d/animate-camera.

Rest: home-focused scene, named categories and one explicit expand action.
Hover: quiet row highlight. Selection: place details and actions below the map.
Focus: visible rings, selected inspector focus without scrolling, native modal
focus containment and return to the expand control. Touch: horizontal category
and place rows; actions have 44px minimum height. Reduced motion: existing camera
flights settle immediately. Playback never folds the rail or resizes the stage.
No proximity magnification, decorative orbit, floating identity card or new
buyer-facing renderer terminology was borrowed.

UI critic source/code pass: no duplicate property identity, no card obscuring
map evidence, no invented facts, and no decorative autoplay added. A visual
approval is still required: this environment's managed browser blocks localhost;
the repository browser test cannot launch because Chrome is absent. Installing
Chrome was attempted and failed on the environment's OS permission restrictions.
No new screenshots or real-Google camera smoothness approval are claimed.

### Verification for this refinement

- Frontend lint and production TypeScript/Vite build.
- All 281 frontend tests pass, including 15 focused Atlas tests with the new
  home-reset regression and revised destination-only inspection contract.
- Browser specs updated for the actual category rail, inspection controls,
  native fullscreen, stable pause/resume, mobile selection and separate approach
  chapter. They retain real Google 3D; there is no mock renderer fallback.
- Run `npm run test:atlas-browser` with Chrome and `VITE_GOOGLE_MAPS_API_KEY`
  available to capture home, inspection, fullscreen, mobile and road screenshots.
- The key is runtime-only and is not stored in the repository.
