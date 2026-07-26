# Phase 5 Handoff — UI Truth Consolidation

> **For:** implementation agent  
> **Prerequisite:** Phase 4 in progress or complete (config-driven evidence sections in `properties.rs`; serving bundle with real facts)  
> **Parent plan:** [`dag_execution_plan.md`](./dag_execution_plan.md) § Phase 5  
> **Parallel work:** Phase 3/4 backend — this phase is mostly **frontend + API response shape**.

---

## Mission

**One signal, one surface.** The property page should not show the same buyer insight in three places (livability brief themes, community pulse chips, evidence risk folds). Users see **receipts** — facts, sources, links, freshness — not scores or aggregated proof labels.

**Do not show users any scoring.** Numeric confidence, proof tiers (Verified / Supported / Early signal), transparency scores, and heat labels derived from confidence are **not accurate enough** for buyer-facing UI. Keep confidence internal for ranking and resolver only.

Phase 5 is done when:

1. Each buyer signal has a **clear primary surface** (brief vs pulse vs evidence fold).
2. Frontend **does not hardcode** section constellations, display titles, or presentation rules.
3. **`confidence_pct` and proof labels are never buyer-facing** — strip from API responses and UI; show source + fact instead.
4. Adding a section or leaf in config appears on the property page **without a React edit**.

**Not in scope:** EntityContext graph summary API (Phase 10), search tile chip registry (Phase 8), Reddit pipeline (Phase 6).

---

## Read first (mandatory)

1. `.claude/skills/coding-practices.md` — calm, premium UI bar
2. `AGENTS.md` — § "One signal, one primary surface" + "Confidence internal only"
3. `docs/dag_execution_plan.md` — §3 Confidence policy
4. `app/config/product/evidence_sections.json` — section layout source of truth
5. `app/config/dag/ui_surfaces.json` — surface → leaf_keys (for dedup rules)

---

## Product hierarchy (enforce this)

| Surface | Owns | Must NOT duplicate |
|---------|------|-------------------|
| **Livability brief** (`LivabilityBriefCard`) | Risk / operating / positive **prose** + lens-level themes | Raw review quotes, per-fact receipts |
| **Community pulse** (`CommunityPulseCard`) | Review **receipts** — quotes, source URLs, "residents like / worth checking" from reviews | Long risk paragraphs already in brief |
| **Evidence stack** (`EvidenceStack`) | **Fact receipts** per section — RERA fields, market trail, nearby POIs, media | Theme chips that repeat brief wording |
| **Action rail** | Decision verdict + market pulse + **one-line hook** from brief (not a second risk essay) | Full evidence duplication |

**Rule:** Before adding a chip, card, or fold — check if the same `fact_key` or theme already appears upstream. Merge, replace, or drill down.

**What users see on each fact:** label, value, **source name**, optional source URL — not a confidence % or proof tier.

---

## What's already done (do not redo)

| Item | Status |
|------|--------|
| `riskSignalsFor` / `RiskBar` removed from `PropertyPage.tsx` | ✅ |
| `EvidenceSectionCard.tsx` orphan deleted | ✅ |
| `buildDecision()` uses livability brief + RERA, not seed scores | ✅ |
| Backend loads `evidence_sections.json` | ✅ (partial — see gaps) |

**Remove in Phase 5:** `evidenceProofLabel()`, `confidenceTone()`, `summarizeEvidence().heat` if they surface scoring to users.

---

## Phase 5A — Signal deduplication

### 5A.1 Brief vs pulse vs evidence

**Problem:** Risk/operating themes can appear in:
- `LivabilityBriefCard` theme chips (`brief.blocks[].themes`)
- `CommunityPulseCard` positive/concern chips
- Evidence folds (`waterlogging_context`, `surroundings`, `approach_road`)

**Work:**

1. Audit overlap for 2–3 fixture societies (Prestige Waterford, Brigade Woods, etc.)
2. **Brief** keeps synthesized lens prose + max 2–3 theme chips per block (high-level only)
3. **Pulse** keeps quotes + review-derived chips only — remove chips that mirror brief themes verbatim
4. **Evidence** keeps fact rows — no theme chips in evidence folds
5. Backend: when building `community_pulse`, filter `positives`/`concerns` that duplicate `livability_brief` block themes (case-insensitive)

**Files:**
- `backend/src/community.rs` — pulse composer
- `backend/src/livability_brief.rs` — brief composer
- `frontend/src/components/evidence/CommunityPulseCard.tsx`
- `frontend/src/components/evidence/LivabilityBriefCard.tsx`

### 5A.2 Action rail — single hook

**Work:**

1. Action rail shows **one line** from brief (top theme or decision summary) — no score, no proof tier
2. Do not add a second risk list or score bar in the rail
3. Full risk detail stays in `LivabilityBriefCard` only
4. Remove `brief.confidence_label` from buyer UI if it reads like a score ("Well supported" is OK only if it describes evidence breadth in plain language — prefer dropping it)

**Files:**
- `frontend/src/pages/PropertyPage.tsx` — `property-action-rail`, `buildDecision()`

### 5A.3 Approach road — one primary surface

**Current:** `ApproachRoadTrail` rendered above brief; `approach_road` section excluded from `EvidenceStack` when trail shows.

**Keep:** Trail owns visual proof; brief owns risk prose; evidence section excluded when trail present. Document this in a code comment referencing `ui_surfaces.json` `approach_road` surface.

---

## Phase 5B — Config-driven evidence UI (delete frontend maps)

### 5B.1 Backend: enrich `EvidenceSection` API

Add fields the frontend currently derives from const maps:

```json
{
  "kind": "rera",
  "title": "RERA file",
  "constellation": "trust",
  "priority": 10,
  "presentation": { "variant": "timeline", "density": "compact", "max_preview_items": 4 },
  "source_types": ["rera"],
  "items": [
    { "label": "Registration", "value": "...", "source_type": "rera", "source_url": "..." }
  ]
}
```

**Work:**

1. Extend `app/config/product/evidence_sections.json` with optional `constellation` per section (or derive from `ui_surfaces.json` at load time)
2. `EvidenceSection` struct — add `constellation` from config
3. **Do not add `proof_label` to buyer API** — confidence stays in lake/bundle for internal use only
4. **Stop serializing `confidence_pct`** on `EvidenceSection`, `SourceItem`, and related buyer-facing types (or omit with `#[serde(skip_serializing)]`)
5. Section header meta: **fact count + source names** (e.g. `4 facts · RERA, Google`) — not confidence or proof tier

### 5B.2 Frontend: delete hardcoded maps

**Delete from `frontend/src/lib/evidence.ts`:**

| Const / fn | Replace with |
|------------|----------------|
| `SECTION_CONSTELLATION` | `section.constellation` from API |
| `SECTION_DISPLAY_TITLES` | `section.title` from API |
| `constellationForSection()` | `section.constellation` with layout-only fallback |
| `evidenceProofLabel()` | **Delete** — no user-facing proof tiers |
| `summarizeEvidence().heat` / `confidencePct` in tile UI | fact count + source types only |

**Update:**
- `frontend/src/lib/types.ts` — add `constellation`; remove or internalize `confidence_pct` on buyer types
- `frontend/src/components/evidence/EvidenceStack.tsx` — use `section.constellation`; **delete `confidenceTone()`**
- Fold styling: constellation + variant only — **not** confidence-derived CSS classes

```tsx
// BAD — delete
className={`ev-fold ${confidenceTone(section.confidence_pct)}`}

// GOOD
className={`ev-fold ev-fold--${section.constellation} ev-fold--variant-${presentation.variant}`}
```

### 5B.3 Generic presentation variants

**Goal:** `presentation.variant` from config drives renderer — no `if (kind === 'rera')` in React.

| `variant` | Component |
|-----------|-----------|
| `timeline` | timeline layout (RERA, lifecycle) |
| `story` | `CommunityPulseCard` wrapper |
| `fact_grid` | compact grid |
| `fact_list` | default fold facts |
| `media_grid` | `EvidenceMediaStripView` |
| `risk_grid` | fact grid with risk styling |

**Work:**

1. `EvidenceStack.tsx` — `variantComponents[section.presentation.variant]` map (layout primitives only)
2. Icons may stay kind-driven (cosmetic); layout must be variant-driven

---

## Phase 5C — Strip scoring from buyer UI

### 5C.1 Property page + tiles

- Evidence fold header: **"4 facts · RERA"** — not confidence %, not proof label
- `LivingEvidenceTile`: remove `evidenceProofLabel(heat)`; show fact count + top source type if useful
- `LivabilityBriefCard`: drop `confidence_label` badge if it implies scored proof — keep prose only

### 5C.2 Backend

- Remove `confidence_pct` from buyer-facing serializers (`EvidenceSection`, `SourceItem`, `CommunityPulse`, property detail)
- Keep confidence in Parquet / admin / data-health only
- `livability_brief.confidence_label` — remove or replace with non-scored copy (e.g. "Based on N sources") only if honest; otherwise omit

### 5C.3 Types cleanup

- `decisionReadLabel()` — remove `transparency_score` / numeric trust paths; use factual decision copy only
- `trustPercent()` — delete or restrict to internal use; `buildDecision()` should use RERA + brief themes, not a number

---

## Phase 5D — SEO & copy

| Location | Current | Target |
|----------|---------|--------|
| `PropertyPage.tsx` meta | "Proof-backed livability brief…" | ✅ keep direction |
| `ResultsPageA.tsx` | "full transparency reports" | "homes with receipts" / "proof-backed results" |
| `main.tsx` root meta (if stale) | "transparency scores" | receipts / livability copy |
| `HomePlanPage.tsx` | OK | no change unless stale |

**Voice:** Short, premium — align with AGENTS.md product quotes. Never promise scores.

---

## Phase 5E — Legacy fallback removal

| Item | Action |
|------|--------|
| `panelsToSections()` in `evidence.ts` | Remove when `GET /api/properties/:id` always returns `evidence.sections` |
| `fallbackSections` prop on `EvidenceStack` | Remove from `PropertyPage` once API guaranteed |
| `source_panels` on detail response | Deprecate; evidence endpoint is canonical |

**Gate:** Confirm in tests that property detail always includes `evidence` for bundled societies.

---

## Suggested commit sequence

```text
Commit 1  Backend: constellation on EvidenceSection; strip confidence_pct from buyer API
Commit 2  Frontend: delete SECTION_CONSTELLATION / confidenceTone / evidenceProofLabel
Commit 3  Dedup: brief vs pulse theme filter (backend) + visual pass on PropertyPage
Commit 4  Generic variant renderer in EvidenceStack; action rail single hook
Commit 5  SEO copy + remove panelsToSections fallback + types cleanup
```

---

## Acceptance checklist

### One signal, one surface

- [ ] Risk theme appears in **brief prose** OR **pulse chips** OR **evidence facts** — not all three with same wording
- [ ] Action rail has one-line hook, not duplicate risk essay
- [ ] `approach_road` documented: trail > brief > evidence (no triple surface)

### Config-driven UI

- [ ] No `SECTION_CONSTELLATION` or `SECTION_DISPLAY_TITLES` in frontend
- [ ] New section in `evidence_sections.json` renders after backend reload — **no React edit**
- [ ] `presentation.variant` drives layout component selection

### No user-facing scoring

- [ ] No `confidence_pct`, score bars, proof tiers, or transparency scores visible to users
- [ ] No `evidenceProofLabel`, `confidenceTone`, or heat derived from confidence
- [ ] Each fact shows source name (+ URL where available); sections show fact count + sources

### Quality

- [ ] `npx tsc --noEmit` clean
- [ ] Manual pass: PropertyPage for 3 societies — no duplicate chips, calm layout
- [ ] `app/config/coverage.json` updated if frontend hardcoding items removed

---

## Explicitly out of scope

| Item | Phase |
|------|-------|
| EntityContext graph summary paragraph | 10 |
| `GET /api/config/fact-registry` debug endpoint | 8 |
| Search result tile chips from `ui.tile_eligible` | 8 |
| Compare page evidence model parity | 8 |
| `approach_road_visuals.json` → lake migration | 4/9 |
| Graph traversal / `GraphIndex` | 4 / 10 |
| Buyer-facing proof label tiers | never — use receipts |

---

## Key files

```text
# Config
app/config/product/evidence_sections.json
app/config/dag/ui_surfaces.json

# Backend
backend/src/routes/properties.rs           # EvidenceSection, build_property_evidence_response
backend/src/livability_brief.rs
backend/src/community.rs                   # CommunityPulse composer

# Frontend
frontend/src/pages/PropertyPage.tsx
frontend/src/lib/evidence.ts               # DELETE const maps + scoring helpers
frontend/src/lib/types.ts
frontend/src/components/evidence/EvidenceStack.tsx
frontend/src/components/evidence/CommunityPulseCard.tsx
frontend/src/components/evidence/LivabilityBriefCard.tsx
frontend/src/components/evidence/LivingEvidenceTile.tsx
frontend/src/pages/ResultsPageA.tsx        # SEO copy
```

---

## Testing

```bash
# Types
cd frontend && npx tsc --noEmit

# Backend evidence shape
cd backend && cargo test property_evidence

# Manual
# 1. Open /property/{id} for a society with reviews + risk facts
# 2. Confirm: brief has prose, pulse has quotes, evidence has fact rows with sources
# 3. Confirm: NO confidence %, proof tiers, or score bars anywhere on page
# 4. Add a dummy section to evidence_sections.json → restart backend → section appears
```

---

## Definition of done

The property page feels **layered, not repetitive**: brief explains tradeoffs, pulse shows review receipts, evidence shows source-backed facts. Frontend const maps for product vocabulary are gone. **Users never see scores** — only facts, sources, and links.

**Add-section test:** Add a new entry to `evidence_sections.json` with a new `kind`, `facts[]`, and `presentation.variant` → property page shows the section without any frontend code change.
