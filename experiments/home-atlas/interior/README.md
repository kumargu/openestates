# Home Atlas · Interiors

An isolated, empty-home tour experiment for OpenEstates. The experience enters through an explicit front door, walks through connected spaces, pauses on arrival, and shows room measurements. It also supports manual exploration, a dollhouse and a selectable floor plan.

Nothing imports this package from the production frontend. No backend, search, DAG, OSM collector or production API is changed.

## Run

Requires Node 22.13+.

```bash
cd experiments/home-atlas/interior
npm ci
npm run dev
```

Open the local URL printed by Vite. The default fixture is a manually traced Waterford B1 apartment. Add `?plan=studio`, `?plan=1br`, `?plan=2br` or `?plan=4br` for synthetic topology fixtures. The 2-bedroom fixture is rotated. These fixtures exercise reuse; the synthetic bedroom chain is not a proposed residential design.

```bash
npm test
npm run check
npm run lint
npm run build
```

## Three small boundaries

| Folder      | Owns                                                                                 | Must not own                                 |
| ----------- | ------------------------------------------------------------------------------------ | -------------------------------------------- |
| `core/`     | Metre coordinates, polygons, clearance, doorway routes, measurements, playback clock | React, Three.js, fetching, apartment names   |
| `renderer/` | Meshes, lighting, camera application, input events, resource disposal                | Source parsing, room order, tour timers      |
| `react/`    | Accessible controls, room list, source disclosure, responsive layout                 | Geometry edits, independent animation clocks |
| `fixtures/` | Source-specific conversion into the contract                                         | Shared navigation or camera code             |

The source adapter returns `HomePlan`. `prepareHome(plan, policy)` computes stops and continuous routes. `HomeTour.tick(seconds)` advances one state machine. `HomeViewer` applies the resulting pose to Three.js. `HomeExplorer` is a thin React consumer.

```tsx
import { HomeExplorer } from "./experiments/home-atlas/interior/react/HomeExplorer";
import "./experiments/home-atlas/interior/react/home-explorer.css";

// This adapter belongs with the existing evidence/serving boundary.
const plan = toInteriorPlan(verifiedFloorPlanFacts);
return <HomeExplorer plan={plan} />;
```

`toInteriorPlan` above is illustrative integration code, not an implemented API. React and Three.js are peer dependencies. The core has no runtime dependencies and can be tested in Node or moved into a worker without importing the viewer.

## The floor-plan contract

- Coordinates are local X/Z metres, with Y reserved for height. Apply a single calibrated scale to the whole plan in its adapter. Never scale rooms independently to make incompatible measurements fit.
- Rooms have stable IDs, a semantic kind and simple boundary polygons. Supply walls independently so shared walls appear once.
- Each wall has endpoints and optionally one opening, defined by start/end fractions. Split a wall into consecutive segments to represent multiple openings. Doors and sliders identify the two connected rooms; `null` explicitly means exterior. Windows block walking.
- `entranceWallId` identifies the front door. The entrance's inward direction is derived from the room boundary. It is a local travel heading, not a geographic bearing.
- Ceiling and opening heights are metre values. Keep `heightsVerified` false for defaults. Source status distinguishes traced, validated and synthetic inputs.
- `sourceDimensions` is a display-only transcription. It never warps the model. Optional source links and images are supplied by the consuming app, which owns asset access.

The current contract supports a single level of simple polygons, including concave and rotated rooms. It does **not** parse arbitrary PDFs/images, model stairs or polygon holes, or infer doors from adjoining rectangles. Those require evidence normalization before this boundary. Room polygons and walls must agree; the current validator is not a survey or complete planar-topology auditor. Reject or review ambiguous extraction upstream.

## Tour behavior

An explicit doorway graph determines the visiting order. Configurable room-kind priorities favor the living room and its balcony before service and bedroom spaces. Graph traversal brings attached bathrooms and balconies after their connected room without any bedroom-count branches.

A clearance-aware navigation grid finds a continuous path through wall openings. Line-of-sight simplification removes unnecessary corners only where the full segment remains walkable. A bounded grid fails explicitly if the floor plan is too large or a doorway cannot be traversed. There is no fallback teleport on routing errors.

The clock progresses through entrance, walking, settling and inspecting. Each room receives a ten-second inspection **after arrival and settling**, with a gentle quarter turn to reveal breadth. Pause stops translation, rotation and inspection time together. Manual movement collision-checks every step; resuming replans from the actual position. Room selection also walks from the actual position.

Default settings live in `DEFAULT_POLICY`: 0.8 m/s walking, 1.6 m eye height, 0.18 m clearance plus half the wall thickness, 0.13 m grid, 45°/s turning, and a 120,000-node budget. They are named policy values, not apartment-specific camera poses. Long frames are clamped so returning to a hidden tab cannot skip rooms. Hidden tabs pause.

Reduced-motion preference disables animated travel and room panning while retaining explicit stops and inspection time. This intentional nonanimated transition is separate from routing failure; all routes must still validate.

Measurements are polygon-clipped orthogonal spans through the viewpoint, aligned to a room edge. For concave rooms they describe the connected span, not a bounding box or certified architectural length. Both dimensions remain readable in the room caption even when a floor line is outside the camera view.

## Evidence and visual treatment

Waterford B1 is manually digitized from the supplied Prestige Waterford brochure, page 14 (`PWF_05.pdf` in the source workflow). Its metre conversion uses an approximate 63 pixels/metre calibration against printed bedroom spans. The trace is **not validated**. The original living/dining footprint is irregular, so its printed 23′ × 12′ dimensions are not treated as a rectangular bounding box.

The fixture preserves printed dimensions separately from computed model spans. No north arrow, apartment orientation, exterior scenery, furniture or amenity claim is invented. Height, finish and lighting defaults remain explicitly illustrative. Source assets are optional and are not duplicated into this experiment; a consumer can supply its authorized brochure URL and image in `plan.source`.

This is a spatial preview. A trustworthy substitute for an in-person inspection would need verified unit-specific geometry, heights, openings and captured views, with evidence provenance. Better shading cannot establish those facts.

## Relationship to PR #126

[PR #126](https://github.com/kumargu/openestates/pull/126), inspected at `b36ccae0fdff82f372d02d0112f4415f46812496`, separates portable geometry/camera/journey planning from the rendering prototype. This package follows that separation and adds only local interior geometry under `experiments/home-atlas/interior/`, without editing its pending files.

The production path remains collectors → DAG facts → serving bundle → Rust `SurfaceSceneResponse` → presentation planners → renderer. `HomePlan` is an experiment input, not a second persistence schema or API. At integration time, add a small adapter from approved floor-plan evidence and use the existing product selection/playback owner. Do not mount two independent playback controllers over one camera.

Outdoor Google/OSM coordinates remain owned by the existing geo pipeline. The interior engine uses a local metre frame. A surveyed placement transform can eventually connect these frames; no arbitrary latitude-to-room conversion belongs here.

## Interaction research and review

- [Human Atlas](https://github.com/ashemag/human-atlas): inspired the compact controls, persistent spatial context, and selection-focused exploration. Anatomy assets and code were not copied.
- [OpenAI developer showcase](https://developers.openai.com/showcase/websites): its Courtyard House listing supplied a relevant architectural-tour reference. The implementation here is independently authored; no claim is made that its scene source was inspected.
- [ThreeUI Community](https://github.com/MengTo/threeui): reviewed its catalog/package structure and documented responsive controls. No floor-plan navigation primitive was established from that review; no unrelated decorative shader/template was copied.

At rest the viewport dominates, with a compact floor map and one primary entry action. Hover uses a restrained brightness change. Focus uses visible outlines and semantic buttons. Touch supports drag-to-look, manual direction buttons and a collapsible room list. Reduced motion retains room stops without camera travel/panning. The source disclosure is modal; measurement labels do not intercept pointer input. No search or property-detail route is changed.

## Validation and remaining integration gates

The deterministic tests cover all 13 Waterford spaces, attached-room route traversal, multiple bedroom counts, rotated and concave geometry, blocked/disconnected plans, grid limits, pause and manual resume, collision-safe playback at 30/60 fps, and reduced motion.

Type checking and the standalone production build are required alongside those tests. Formatting is checked with Prettier. The Site host is built separately from the portable package.

Before promoting to buyer-facing UI: validate the brochure trace, attach resting/interior/mobile screenshots, exercise WebGL on target devices, profile navigation preparation on larger plans, and adapt the shell to OpenEstates' shared controls. Browser/device visual QA has not been performed in this implementation session. Keep the PR draft until that review is complete. Core preparation is currently synchronous; large inputs may warrant worker execution. Three.js is lazy-loaded, but its bundle still merits a production budget decision.
