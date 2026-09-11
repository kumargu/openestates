# Home Atlas → property page integration

## Baseline and scope

Branch: `feat/home-atlas-property-integration`. Prepared from `origin/main` at
`b4c2ccc` (PR #128), with PR #126 (`b36ccae`) merged at `7797111`.
The archived experiment remains intact. This integration changes the real
property page, not the Sites prototype. When an `arrival_story` scene carries a
valid anchor and experience policy, the map owns the property canvas and the
existing workspace sidebar remains the sole navigation shell. The older stacked
hero/map/reviews composition remains the fail-closed path for properties without
that immersive scene. The Rust serving contract is unchanged.

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
| UI switches and selected place | Existing `PropertyArrivalMap` |
| Google elements, overlays, focus camera | Existing `PropertyArrivalGoogle3DMap` |
| Road speed/camera tuning and overlay styling | `app/config/ui/home-atlas.json` |
| Property identity and save/note actions | Existing property detail projection and controls |

The renderer consumes scene-provided coordinates. It has no Waterford-name branch.
Society camera framing continues to fit the mapped footprint through
`societyCameraComposition`; large boundaries therefore widen the view. The
portable estate/township classification and Brigade-specific reference experience
are preserved in PR #126, but a production split-context locator is **not** wired
by this change. Do not describe that separate layout as integrated.

## Buyer experience

- One uninterrupted, viewport-height 3D world beside the existing sidebar. The
  property name and essential facts sit directly on the scene; the duplicate
  listing hero, old map, and arrival tile are not stacked above it.
- Direct, scene-derived Society, Schools, Hospitals, Roads, Metro, Lakes, and
  other nearby categories. Categories disappear when the backend returns no
  evidence rather than rendering empty controls.
- Society reveal, an optional top perspective, sourced OSM boundary, and quiet
  surroundings. Quiet mode retains holes for the home and the places currently
  being inspected, so focus remains readable instead of dimming the evidence.
- Metro points and the actual API-provided track segments. Segments stay separate;
  no straight line is invented between stations. Numbered nearby list, individual
  focus, Show together, source-backed detail card, and a guided sequence through
  available places live in one right-side drawer.
- Nearby categories are derived from the existing payload. Existing 2D evidence
  remains available. Polygon overlays preserve supplied holes and multipolygon
  parts; missing polygons are not manufactured.
- Approach road starts with context, descends to the accepted forward aerial
  camera, holds briefly for orientation, then moves continuously. One speed
  slider controls 0.5–2× progression. This is an aerial inspection of mapped road
  alignment, not a claim about walking/driving permissions, widths, or travel time.
- Street View is opt-in. It starts near the current aerial route position using
  the existing panorama sequence. **Back to aerial** is available even while
  Street View loads or has no coverage. Escape also exits. The returned panorama
  position is projected onto the same aerial route, preserving heading; replay
  is explicit after returning.
- Pointer interaction, view changes, and document hiding cancel automatic
  movement. Pause/resume uses the existing controller. Reduced-motion users do
  not receive the automatic aerial flight.

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

Inspected the preserved Human Atlas `app/page.tsx` selection/visibility/view
state and its focus scene, plus its slider component. Borrowed separate camera
and selection state, cancellable cinematic movement, and a small continuous road
speed lever. The accepted Atlas Site supplied the full-stage composition, direct
nearby categories, numbered evidence drawer, OSM/quiet analysis controls, and
calm bottom journey dock.
Searched for a directly applicable ThreeUI map interaction; no specific additional
map implementation was established. No unverified ThreeUI source is claimed.
Did not borrow anatomical geometry, explosion/lift, or side-by-side map
comparisons.

Rest: the 3D property canvas is the primary surface. Hover/focus: restrained
glass controls and visible focus rings. Touch: a horizontally scrollable category
rail, full-width bottom dock, and a bounded drawer. Reduced motion: settle the
camera instead of automatically flying. Property identity is projected once;
save/note reuse the existing controls and do not create a second state store.

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
captures society, metro focus, road, and mobile road screenshots in
`frontend/test-results/atlas/`. It requires the Google key and working real 3D;
it does not silently pass using a mocked renderer.

Observed in this environment: lint, TypeScript, the production bundle, and all
265 frontend tests pass. The Atlas browser specification now asserts the
full-stage shell, direct category drawer, focused nearby item, aerial road speed,
pause/resume, Street View exit, and mobile state. The host runs Node 24, while the
repository targets Node 22; CI/local Node 22 is an additional parity gate.

**Local real-3D browser validation is blocked here.** Playwright is installed but
its Chromium binary is absent, and the managed cloud browser cannot reach the
localhost server. No certificate checks or browser safeguards were disabled. No
photographic smoothness approval is claimed from this environment.
Before merging, run the browser gate above on a machine with Google access and
review the road descent, pause/resume, station focus, quiet boundary, mobile
controls, and Street View exit.

The real promoted Parquet bundle/Rust API was not available for end-to-end
validation in this pass. Sparse production scenes will remain sparse: they do
not silently borrow fixture evidence. Validate one real Waterford ID against your
local backend before treating this as production-ready.
