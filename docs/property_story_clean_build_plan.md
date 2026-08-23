# Property Story — clean build plan

This pass implements the original intent of issue #89 without retaining the
visual inconsistencies of the first rollout.

## Visual thesis

The property page is an editorial asset page with two cinematic moments:

1. a compact property filmstrip for identity;
2. the same filmstrip language for the route to the gate.

Maps, reviews, official records, and comparisons remain calm decision surfaces.
Motion establishes place; facts remain stationary.

## Source-bound direction

ThreeUI is a mechanics reference, not a component dependency.

- Character Carousel informs the centered filmstrip, adjacent-frame peeks,
  focus falloff, delayed idle movement, and mobile axis change.
- Gallery and carousel hosts inform visibility pausing, reduced motion,
  bounded loading, and teardown.
- OpenEstates issue #88 owns the official-record visual language: warm paper,
  thin rules, almost-flat elevation, real facts, and direct source access.

No Three.js gallery, iframe renderer, demo chrome, or MengTo visual identity is
copied into the product.

## Fixed contract

- One React filmstrip engine serves hero and arrival.
- Hero and arrival use the same bounded stage height.
- Current imagery and proposed renders are explicitly labelled.
- Property name, price, configuration, size, and status appear once in the
  identity chapter.
- Existing Around This Home filters, map interactions, evidence tray, notes,
  zoom, and expanded mode remain intact.
- Existing `GoogleReviewsSection` remains intact.
- RERA links to the full report and never invents a status.
- Compare renders exactly three interactive home cards and preserves the full
  Compare handoff.
- Save and Note stay in the top bar. The duplicate final decision deck is
  removed.
- The no-op Story / Full dossier switch is removed until a real alternate mode
  exists.

## Adaptive contract

- Rich galleries show one focused frame plus adjacent peeks.
- A single image becomes a still stage with no timer.
- Missing images collapse to compact identity, not an empty cinematic slab.
- Motion theme remains deterministic but only changes image treatment.
- Mobile uses a vertical/stacked media composition with 44 px controls.
- Missing map, arrival, reviews, RERA, or compare evidence omits that chapter.

## Build sequence

### 1. Story Lab and shared media contract

- Add a production-grade `PropertyFilmstrip` primitive.
- Render it in Story Lab with rich, partial, sparse, current, proposed,
  reduced-motion, hidden, and offscreen controls.
- Keep layout driven by production projection fixtures.

### 2. Hero and arrival

- Rebuild `PropertySceneCard` around the shared filmstrip.
- Rebuild `PropertyArrivalFilm` around the same engine.
- Keep first-image priority, next-image preload, image failure handling,
  visibility pause, keyboard controls, and gallery access.

### 3. Chapter rhythm

- Give Around This Home an editorial rail and full map canvas without changing
  map logic.
- Use one heading scale and spacing rhythm across reviews, RERA, and compare.
- Keep media chapters visually strong and fact chapters quiet.

### 4. Official record and comparison

- Project only supported registration/document facts.
- Use one flat interactive surface language for RERA and compare.
- Align the same comparable dimensions across all three homes.

### 5. Proof loops

- Review rich, partial, and sparse fixtures at desktop and mobile sizes.
- Run the UI-critic duplicate-copy pass after every visual loop.
- Verify reduced motion, keyboard use, map reachability, build, lint, and
  property-story tests.

