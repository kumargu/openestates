# Property Detail UI Consistency — Implementation Plan

**Status:** P0–P3 implemented; P4–P7 planned
**Last updated:** 2026-07-26
**Surface:** `/property/:id` (verified on `discovered-prestige-waterford-1bhk`)
**Owners:** OpenEstates discovery / detail UI

---

## 1. Problem statement

The property detail page already has a calm, premium direction (hero → snapshot → around-this-home → receipts → continue exploring). Headless Chrome review found **reliability gaps** and **mobile consistency gaps** that undercut that feel:

1. Map WebGL failure takes down the **entire** page via the root `ErrorBoundary`.
2. Desktop map often renders with a **blank half** (layout/resize timing).
3. Mobile **title and chips clip**; bottom workspace nav can overlap early content.
4. Recommendation cards and gallery labels truncate in ways that look unfinished.
5. Secondary surfaces (tags, cards, headings) use mixed visual languages.

This plan fixes those issues in passes. No backend, ranking, or DAG work.

---

## 2. Product bar for this change

> Detail should feel like an asset page: readable on phone, resilient when map fails, and consistent in chip/card language.

**Hard rules:**

- Buyer-facing copy stays calm — no map/debug/pipeline jargon in fallbacks.
- Prefer wrap / progressive disclosure over hard ellipsis on primary title and decision notes.
- Isolate map failures to the map plate; never white-screen the property.
- Prefer CSS/layout fixes over markup rewrites unless structure is the blocker.
- No new instructional captions (“click to…”, “still enriching…”).

---

## 3. Findings → work items

| ID | Severity | Finding | Intended fix |
|----|----------|---------|--------------|
| P0.1 | Critical | `AroundThisHomeMap` WebGL throw → full-page error | Local map boundary + quiet fallback |
| P0.2 | High | Desktop map half-blank | `map.resize()` + `ResizeObserver` after layout |
| P0.3 | High | Mobile `h1` truncates | Allow wrap (≤2 lines); fix overflow ancestors |
| P0.4 | High | Mobile dock overlaps content | Safe bottom padding / safe-area on detail page |
| P1.1 | Medium | Tags clip; unclear `approved` chip | Wrap tags; normalize/suppress weak status labels |
| P1.2 | Medium | Gallery labels clip (`NEIGHBOURHOOD`) | Shorter labels or 2-line/smaller type |
| P1.3 | Medium | “May interest you” notes clip on mobile | Wider snap cards; wrap note to 2 lines |
| P2.1 | Low | Mixed card treatments | Align radius/border/padding tokens lightly |
| P2.2 | Low | Narrow desktop content column | Slightly widen decision main / map plate |
| P2.3 | Low | Low-contrast muted meta | Bump contrast on `/sqft`, empty builder, notes |
| P3.1 | Medium | Desktop recommendations occupy ghost grid columns | Collapse to content-sized tracks |
| P3.2 | Medium | Same-society recommendations repeat one image | Select a stable scene per property |
| P4 | Medium | Sources and builder use different heading hierarchy | Align section heading structure |
| P5 | Medium | Map fallback has no recovery action | Add accessible local retry |
| P6 | Medium | Detail-state rules lack regression tests | Add focused unit coverage |
| P7 | Low | Superseded detail CSS remains | Remove verified unreferenced selectors |

**Implemented scope:** **P0–P3**.

---

## 4. Target UX (after)

### Map plate
- Map loads → full container, pins fitted, filters work.
- WebGL/unavailable → plate stays; show quiet static fallback (home context + nearby list still usable). No page-level “Something went wrong”.

### Hero (mobile)
- Title wraps to two lines when needed; location may truncate.
- Proof chips stay primary; fact tags wrap; no orphan clipped chip at the edge.
- Bottom nav never covers gallery or first content block.

### Continue exploring
- Mobile: horizontal snap with readable titles + 2-line contrast notes.
- Desktop: existing grid preserved.

---

## 5. What already exists

| Asset | Location | Role |
|-------|----------|------|
| Detail page | `frontend/src/pages/PropertyPage.tsx` | Hero, tags, sticky facts, plates, recommendations |
| Scene gallery | `frontend/src/components/property/PropertySceneCard.tsx` + `property-scene.css` | Layered gallery + scene labels |
| Nearby map | `AroundThisHomeMap.tsx` / `AroundThisHomePlate.tsx` | MapLibre plate + POI list |
| Status chip | `ProjectStatusTag.tsx` | Project/possession status label |
| Recommendations | `AlternativePaths.tsx` + `.alt-paths__*` in `evidence.css` | “May interest you” |
| Detail CSS | `frontend/src/index.css` (`.property-*`) | Layout, tags, mobile rules |
| Root error boundary | `ErrorBoundary.tsx` via `main.tsx` | Page-level catch — too coarse for map |

### Gaps

1. No map-local error boundary / fallback.
2. Map create does not defensively handle WebGL failure for the plate.
3. Resize/layout settle for MapLibre is incomplete.
4. Mobile title/tag overflow rules are partial (`overflow-wrap` present; ancestors/`nowrap` still win in places).
5. Workspace dock safe-area not consistently applied to detail scroll content.
6. `ProjectStatusTag` can surface raw status strings (e.g. bare `approved`).
7. Alt-paths mobile min card width still too tight for contrast notes.

---

## 6. Implementation plan

### Pass P0 — Reliability & mobile read

#### P0.1 — Isolate map failures
- Add a small local error boundary around the map canvas only (inside `AroundThisHomePlate`, or wrap `AroundThisHomeMap`).
- Catch MapLibre/WebGL init failure; render fallback UI that reuses plate chrome + nearby list.
- Log to console for debugging; buyer copy stays minimal (e.g. omit map, keep list) — no “WebGL failed” wording.

#### P0.2 — Fix blank map half
- After `load` / style ready: `map.resize()`, then re-fit bounds.
- Attach `ResizeObserver` on the map container; debounce resize + fit.
- Confirm container has explicit height before map construct.

#### P0.3 — Title wrap
- Audit `.property-brief-copy h1` and parents for `nowrap` / fixed height / overflow clip.
- Prefer wrap up to ~2 lines; ellipsis only if absolutely required after wrap.
- Re-check at 390px width in headless Chrome.

#### P0.4 — Dock clearance
- Add bottom padding / `env(safe-area-inset-bottom)` on `.property-decision-page` (or workspace main content) matching dock height.
- Verify gallery and sticky facts clear the dock.

### Pass P1 — Density & clipping

#### P1.1 — Tags / status chip
- Finish `.property-brief-tags` wrap behavior on mobile.
- Cap or de-dupe tags that repeat sticky-facts (BHK already shown).
- In `ProjectStatusTag`, map known statuses to buyer labels; suppress unknown/raw values that read like internal state (`approved` unless mapped to a clear buyer phrase). Prefer silence over unclear chip.

#### P1.2 — Gallery labels
- Shorten long scene labels and/or allow smaller / 2-line type in scene thumbs.
- Prefer readable title case over ALL-CAPS truncation when space is tight.

#### P1.3 — Recommendation cards
- Keep horizontal snap carousel on mobile.
- Increase min card width; ensure caption/note use `min-width: 0` and wrap to 2 lines.
- Preserve desktop grid.

### Pass P2 — Consistency polish (optional same PR)

#### P2.1 — Card language
- Light-touch shared radius/border/padding for Approach, Market, Sources, Builder cards — no redesign.

#### P2.2 — Desktop width
- Slightly widen decision column / map plate so unused right void feels intentional.

#### P2.3 — Contrast
- Raise muted meta contrast (`/sqft`, builder empty line, alt-path notes) without changing hierarchy (title + price stay largest).

### Pass P3 — Recommendation composition
- Collapse the desktop recommendation grid to real cards instead of preserving empty `auto-fill` tracks.
- Select a deterministic scene per property so adjacent configurations from one society remain distinguishable.
- Preserve the mobile snap carousel.

### Pass P4 — Evidence hierarchy
- Use one section-heading pattern for Sources and Builder.
- Keep the builder record as the receipt card, not a second competing heading.

### Pass P5 — Map recovery and accessibility
- Add a buyer-readable local retry action when WebGL initialization fails.
- Give the map region an accessible name while preserving marker controls.

### Pass P6 — Regression coverage
- Add focused tests for detail visibility, status normalization, and deterministic scene selection.

### Pass P7 — Scoped cleanup
- Remove only verified-unreferenced property-detail selectors and helpers.
- Run final desktop/mobile/WebGL-failure checks after deletion.

---

## 7. File touch list (expected)

| File | Pass |
|------|------|
| `frontend/src/components/evidence/AroundThisHomeMap.tsx` | P0.1, P0.2 |
| `frontend/src/components/evidence/AroundThisHomePlate.tsx` | P0.1 |
| `frontend/src/styles/evidence.css` | P0.2, P1.3, P2.x |
| `frontend/src/index.css` | P0.3, P0.4, P1.1, P2.x |
| `frontend/src/pages/PropertyPage.tsx` | P1.1 (if tag list logic changes) |
| `frontend/src/components/ProjectStatusTag.tsx` | P1.1 |
| `frontend/src/components/property/PropertySceneCard.tsx` (+ scene CSS) | P1.2 |
| `frontend/src/components/recommendations/AlternativePaths.tsx` | P1.3 (only if markup needed) |
| Workspace frame / dock CSS (if padding lives there) | P0.4 |

No Rust, pipeline, or config changes expected.

---

## 8. Non-goals

- Backend / DAG / scoring / enrichment changes
- Landing-page hero redesign rules applied to detail
- New map tutorial or debug chrome for buyers
- Broad design-system rewrite or new component library
- Changing recommendation ranking logic

---

## 9. Verification checklist

- [x] Headless Chrome desktop + mobile screenshots of Prestige Waterford detail
- [x] Forced WebGL failure → page usable; map plate fallback; no root error screen
- [x] Desktop map fills container (no half-blank tiles); pins remain sensible after resize
- [x] Mobile: title wraps; tags wrap; dock does not cover content
- [x] Gallery labels readable; alt-path notes readable on ~390px
- [x] P2 card borders/radii align and secondary copy remains readable
- [x] P2 desktop + mobile headless screenshots reviewed
- [x] P3 desktop recommendation tracks and image variety verified
- [ ] P4 builder/source hierarchy verified
- [ ] P5 retry and accessible map region verified
- [ ] P6 focused detail tests pass
- [ ] P7 stale-code deletion verified
- [x] No new buyer-facing internal jargon
- [x] Frontend TypeScript build clean

---

## 10. Suggested PR shape

1. **PR title:** Improve property detail mobile layout and map resilience
2. **Order of commits / work:** P0.1 → P0.2 → P0.3 → P0.4 → P1.1 → P1.2 → P1.3 → (P2 if time)
3. **Review focus:** map isolation, mobile overflow, buyer copy on status chips

---

## 11. Implementation result

P0 + P1 + P2 shipped as separate commits. P2 standardizes detail-card geometry and
section rhythm, slightly widens the large-screen canvas, and improves secondary-copy
contrast without changing the information architecture.
