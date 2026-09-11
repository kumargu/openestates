# Brigade Orchards aerial-tour experiment

## Intent
Help a buyer understand a large township by moving through it, retaining geographic context while focusing on individual precincts. Waterford remains the neighborhood experiment; `/brigade/` explores the inside of a township.

Human Atlas references reviewed: `app/page.tsx` selection and contextual detail state; `app/scene.tsx` camera damping, focus, and reduced surrounding emphasis. Applied as selected-precinct focus, a stable locator map, quiet surroundings, and reversible camera controls. No anatomy geometry or arbitrary building models were copied.

## Portable pieces
- `web/atlas-core.js`: shared Waterford geometry and camera math, unchanged.
- `web/brigade/tour-core.js`: dependency-free route distance sampling, tangent heading, camera blending, scene timeline. Only depends on shared `geoDistance`.
- `web/brigade/inventory.json`: OSM research snapshot; derived from `research/brigade-orchards/osm-inventory.json`.
- `web/brigade/app.js`: Google renderer adapter, scene recipes, playback cancellation, UI bindings. A future integration should split the adapter from the DOM controller after we validate the experience.
- `web/brigade/style.css`: full-screen visual treatment and responsive controls.
- `web/atlas-policy.js`: shared scale, backdrop, camera, label, and route policy resolver.
- `web/atlas-sites.js`: small per-site declarations for Waterford and Brigade.

## Journey
Overview → descend to the southern spinal-road segment → Pavilion Villas → follow road → Deodar → Cedar → Aspen → Kino → Neem Grove → The Arcade → township overview. Each focus scene blends from the road to a precinct orbit, then back to the same road pose, avoiding hard cuts. The tour can pause, resume, scrub, restart, or yield to a map gesture. Direct precinct selection uses the same camera primitives.

The route uses one continuous mapped OSM way, the longest named spinal-road way. It does not connect disconnected roads, infer entry gates, promise a walking route, or obey turn-by-turn driving semantics. The geographic point of interest follows the mapped road; the camera remains elevated behind it. This is an aerial exploration, not eye-level imagery.

## Generic rule seam

`resolveAtlasPolicy({config, site, features})` turns source data into a presentation policy:

| Input signal | Resulting decision |
|---|---|
| mapped area, width, and height | society, estate, or township profile |
| named precinct count | nested focus and label density |
| named roads | corridor mode and route eligibility |
| building count | no labels, clustered labels, or texture-only footprints |
| per-site overrides | deliberate exceptions without forking the renderer |

The current thresholds are intentionally small and inspectable. A future OpenEstates API adapter can feed the same resolver from its stored atlas document. The renderer then reads `policy.backdrop.layout`, `policy.mainViews`, `policy.camera`, and `policy.labels`; it does not infer product behavior from raw OSM tags.

This is the right place to decide whether a main view is one full-bleed backdrop or a split-context composition. The underlying map remains the same; the surrounding context, locator, route spine, and label budget change with the resolved profile.

## Data and confidence
- OSM boundaries are community-mapped context, not legal parcel limits.
- The inventory's selection rule is bounding-box-center containment, not polygon clipping. Ways retain complete geometries, including vertices that may extend beyond the main boundary. It is not an exhaustive amenity inventory: unnamed leisure/land-use features and relation-member geometries are not fully normalized.
- OSM footprints do not establish accurate facade appearance, heights, or current construction status. The scene uses Google's imagery rather than extruding invented towers.
- Google ElevationService samples terrain along the road before enabling cameras. If unavailable, the tour reports a loading error rather than claiming a correct low-altitude view.
- Real photorealistic detail depends on Google's coverage. This experiment does not claim internal Street View coverage.
- Quiet surroundings masks the exterior of the selected precinct or township. It never moves real buildings.

## Later integration into OpenEstates
Move pure math and scene contracts into a renderer-independent atlas module. Adapt validated stored OSM geometry into a site/precinct/feature hierarchy. Keep geometry provenance separate from Google imagery and terrain provenance. Add a connected road graph only when expanding beyond the one verified continuous way. Do not convert this experiment into a generic framework before the visual behavior is accepted.

## Review gates
Validate finite camera output, path endpoint preservation, monotonic timeline, scene continuity, cancellation on gestures and visibility changes, boundary toggles, direct focus, reduced motion, and availability/error behavior. Inspect actual imagery before promising a ground-level experience. Keep API keys in runtime config and ignored local environment files.

## Browser review
The cloud browser does not provide WebGL2, so photorealistic 3D playback could not be visually verified here. A labelled raster satellite fallback was exercised with actual Google imagery: township overview, Cedar selection, quiet-surroundings toggle, and map sizing. The production 3D path remains separate. Renderer-free checks cover all adjacent scene camera endpoints, finite samples, heading wrap, mapped route endpoints, and timeline bounds.
