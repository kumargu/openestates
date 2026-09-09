# Calm Atlas shell and ECC Road journey

The Waterford experiment now presents a compact property identity, a single exploration dock, and mutually exclusive Nearby / View / More drawers. The large chapter timeline and marketing paragraphs are no longer exposed. Existing nearby selection, comparison, saved views, camera controls and the neighborhood film remain reachable.

Human Atlas reference: `/workspace/scratch/human-atlas-reference/app/page.tsx` informed mutually exclusive panels and separation of selected object, visibility and camera state; `app/scene.tsx` informed smooth focus transitions. No anatomy data or UI component source was copied.

## Integration boundaries

- `road-path.js`: renderer-independent arc-length sampling, look-ahead bearing, and nearest route position. Selects the longest continuous supplied OSM segment; does not connect disconnected segments or infer a gate. Direction is an explicit host option (`as-mapped` or `reverse`) rather than a place-specific coordinate heuristic. ECC Road currently has one segment.
- `arrival-walk.js`: bounded road controller using the existing host camera adapter (`move`, `apply`, `stop`, `ready`, `prepare`, `restore`). Flight proceeds at 12 m/s with damped heading, manual pause, reduced-motion behavior, and cancellation on exit/backgrounding.
- `atlas-shell.js`: presentation adapter around existing app actions. Moves controls into stable containers; does not duplicate selection data or create another aerial camera owner.
- `atlas-shell.css`: scoped consolidated layout; `arrival-walk.css`: compact full-screen road mode.

The old app still updates hidden chapter DOM nodes. This is an explicit compatibility bridge for this experiment, not the target OpenEstates component architecture. During integration replace these nodes and mutation observers with typed state/events supplied by the real app. Property identity and road labels are still Waterford-specific. Keep the geometry module generic, and provide society identity and host actions through the eventual OpenEstates adapter.

## User flow

ECC Road establishes an aerial road view, holds, descends, then follows the mapped road continuously. A compact 0.5–2× speed lever changes progression without changing the camera geometry. Street is opt-in at the current route position. It uses native Google road navigation, not timed panorama swaps. A persistent Back above action and the Above control both project the current Street View position onto the route and preserve the viewing direction; playback resumes only on request. Coverage failure stays above. The journey is a camera flight, not a simulated pedestrian video.

OpenEstates integration should retain the product's existing Street View component, which is currently clearer than this prototype's native panorama shell. Import the aerial road path, descent, speed and camera handoff; connect its Street action to the existing OpenEstates surface.

## Verification and remaining limits

Existing Atlas checks plus `check-road-journey.mjs` verify OSM alignment, endpoint bounds, disconnected segments, pause during the introduction, loop cancellation and exit restoration. Browser checks cover drawer exclusivity and the visible shell. The cloud browser cannot render Google 3D (WebGL2 unavailable), so the final photographic movement and Street View handoff still require review on a supported device. No claim of full visual 3D validation is made.
