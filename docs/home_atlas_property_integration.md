# Home Atlas → property page integration

## Baseline and scope

Branch: `feat/home-atlas-property-integration`. Prepared from `origin/main` at
`b4c2ccc` (PR #128), with PR #126 (`b36ccae`) merged at `7797111`.
The archived experiment remains intact. This integration changes the real
property page, not the Sites prototype. It does not alter the workspace sidebar,
listing header, carried search, theme tokens, or the Rust serving contract.

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

The renderer consumes scene-provided coordinates. It has no Waterford-name branch.
Society camera framing continues to fit the mapped footprint through
`societyCameraComposition`; large boundaries therefore widen the view. The
portable estate/township classification and Brigade-specific reference experience
are preserved in PR #126, but a production split-context locator is **not** wired
by this change. Do not describe that separate layout as integrated.

## Buyer experience

- Society reveal, an optional top perspective, sourced boundary, and quiet
  surroundings, with settings collected under **View**.
- Metro points and the actual API-provided track segments. Segments stay separate;
  no straight line is invented between stations. Numbered nearby list, individual
  focus, Show all, and a guided sequence through available places.
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
and selection state, one settings disclosure, and a small continuous lever.
Searched for a directly applicable ThreeUI map interaction; no specific additional
map implementation was established. No unverified ThreeUI source is claimed.
Did not borrow anatomical geometry, explosion/lift, a full-screen demo shell,
or side-by-side map comparisons.

Rest: property content and map remain the primary surface. Hover/focus: existing
button treatment and visible focus rings. Touch: native range/select controls,
wrapping road controls, horizontal nearby list. Reduced motion: settle the camera
instead of automatically flying. The repository UI Critic and React review
informed these choices; visual sign-off is still pending below.

## Verification and remaining gate

Commands:

```bash
cd frontend
npm run lint
npm test
npm run build -- --mode development
npx playwright install chromium
npm run test:atlas-browser
```

The browser check runs the actual property route with only fixture API data and
captures society, metro focus, road, and mobile road screenshots in
`frontend/test-results/atlas/`. It requires the Google key and working real 3D;
it does not silently pass using a mocked renderer.

Observed in this environment: the frontend unit suite passed, the new scene
integration contracts passed, and all 16 preserved Atlas tests passed. Build and
lint were also exercised. The host runs Node 24, while the repository targets
Node 22; CI/local Node 22 is an additional parity gate.

**Browser/visual validation is blocked here.** The browser automation daemon
could not start. Its Chrome installer failed certificate validation, and the
standard Playwright browser download timed out. No certificate checks were
disabled. No real screenshots or photographic smoothness approval are claimed.
Before merging, run the browser gate above on a machine with Google access and
review the road descent, pause/resume, station focus, quiet boundary, mobile
controls, and Street View exit.

The real promoted Parquet bundle/Rust API was not available for end-to-end
validation in this pass. Sparse production scenes will remain sparse: they do
not silently borrow fixture evidence. Validate one real Waterford ID against your
local backend before treating this as production-ready.
