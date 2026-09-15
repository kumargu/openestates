# Home Atlas → property page integration

## Baseline and scope

Branch: `feat/home-atlas-property-integration`. Prepared from `origin/main` at
`b4c2ccc` (PR #128), with PR #126 (`b36ccae`) merged at `7797111`.
The archived experiment remains intact. This integration changes the real
property page, not the Sites prototype. When an `arrival_story` scene carries a
valid anchor and experience policy, it joins the normal property journey: a
three-frame cinematic hero, Home + Nearby Atlas, a separate approach-road
chapter, and reviews. The workspace sidebar remains the sole navigation shell.
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

- The cinematic hero owns the property name, facts, Save, and Note. Atlas does
  not repeat them.
- Atlas opens on the home. Nearby layers live in one compact on-map panel;
  direct row toggles replace the old Home / Arrival / Nearby tabs and dropdown.
- A segmented 2 km / 5 km / All control scopes every Nearby point layer.
  Changing it updates the map, row counts, list, and tour. A selected or
  search-matched place remains visible outside the chosen radius.
- Scene-derived Schools, Hospitals, Tech parks, Parks, Lakes, Breweries, and
  Metro remain payload-driven. One layer is shown at a time so eligibility and
  proof stay legible.
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

[#137](https://github.com/kumargu/openestates/issues/137) borrows Human Atlas's
spatial separation: layers on the left, camera actions on the right, and the
subject in the available center. It applies that pattern to sourced property
geometry without borrowing the anatomical dashboard, multi-layer combinations,
explode slider, auto-rotation, or decorative orbit.

At rest, all Nearby switches are off and the home remains the subject. Hover,
keyboard focus, and pressed states are explicit. Selecting a row expands only
that layer's scoped places; selecting a place adds Directions locally and fits
the home-to-place pair between both control rails. Reduced motion skips
automatic flights. The browser layout is the completed target in this pass;
touch/mobile adaptation remains a separate follow-up.

Nearby motion reuses the existing sourced chapter sequence through one rail
control that becomes Pause and Resume. No timeline, speed control, or decorative
orbit was added.

The page uses the same warm background as the rest of the property experience.
The hero is limited to three cinematic frames while the full gallery remains
available from its existing action.

The selected place remains selected while the inspector is visible. A selected
home-to-destination relationship rotates onto the map's wide axis before fitting,
so bearing alone cannot force a much wider camera range. Camera fitting reserves
the layer panel and camera rail before solving the scene. A direct gesture
interrupts a tour without moving the camera.
Searched for a directly applicable ThreeUI map interaction; no specific additional
map implementation was established. No unverified ThreeUI source is claimed.
Did not borrow anatomical geometry, explosion/lift, or side-by-side map
comparisons.

Rest: hero followed by the home-focused Atlas. Browse: one active layer, direct
radius choices, and its nearby rows. Selected: the same row adds rating and
Directions. Tour: one play/pause action appears on the camera rail. Save and Note
stay on the hero.

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

The localhost browser pass used real Google 3D. It confirmed the three-frame
hero, default home framing, 2 km scope, layer toggles, selected-pair clearance,
Approach Road playback controls, and restored Google reviews. Mobile was not
treated as an acceptance target for this iteration.

Observed in this environment: lint, TypeScript, the production build, all 280
frontend tests, and `git diff --check` pass.

The real promoted Parquet bundle/Rust API was not available for end-to-end
validation in this pass. Sparse production scenes will remain sparse: they do
not silently borrow fixture evidence. Validate one real Waterford ID against your
local backend before treating this as production-ready.
