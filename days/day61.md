# Day 61: Trust Badges UI — Make Trust Visible

## Goal

Make trust visible in the UI. Add trust badges and project status tags to property cards, search results, and detail pages so users see at a glance which data is government-verified versus discovered/unverified.

## Product Reason

The data foundation is built (54/65 societies RERA-rooted with project_status). But none of this is visible to users. A property card showing "RERA Verified" next to the builder name, or "Ready to Move — delivered Jan 2020" with a government checkmark, instantly communicates trust. This is the core Sprint 3 promise: "make trust visible."

## Sprint Context

Day 3 of 14 in Sprint 3 (Days 59-72). Theme: "Root the graph in government truth. Make trust visible."

## Feedback Addressed

1. **11 unmatched societies** → Defer alias support to Sprint 4. 54/65 coverage is sufficient.
2. **Sumadhura Epitome 1 fetch failure** → Defer retry to Sprint 4 data cleanup.
3. **False positives in matching** → Accept as known limitation, fix in Sprint 4.
4. **Seed mode area filtering crude** → Accept, improve in Sprint 4.

## Deliverables

### 1. Backend: Add `root_source` and `project_status` to PropertyCard

**File:** `backend/src/models/property.rs`

Add three optional fields to `PropertyCard`:
```rust
pub root_source: Option<String>,          // "rera", "seller", "discovered", "legacy"
pub project_status: Option<String>,       // "ready_to_move", "under_construction", etc.
pub project_status_display: Option<String> // "Ready to Move — delivered 31/01/2020"
```

**File:** `backend/src/routes/enrichment.rs`

In `enrich_property_card_with_sellers()`, after the existing society node lookup:
- Extract `root_source` from `node.root_source`
- Extract `project_status` from `get_text_fact(facts, "project_status")`
- Extract `project_status_display` from the fact's `display_template` field

### 2. Backend: Add fields to PropertyDetail response

**File:** `backend/src/routes/properties.rs`

Add `root_source` and `project_status_display` to `PropertyDetail`. Populate from society KG node.

### 3. Frontend: TypeScript types

**File:** `frontend/src/lib/types.ts`

Add `root_source`, `project_status`, `project_status_display` to PropertyCard type.

### 4. Frontend: TrustBadge component

**File:** `frontend/src/components/TrustBadge.tsx` (new)

| root_source | Badge | Style |
|-------------|-------|-------|
| `"rera"` | Shield checkmark + "RERA Verified" | Green pill |
| `"discovered"` | Clock + "Verification Pending" | Amber pill |
| `"seller"` | User + "Seller Listed" | Yellow pill |
| `"legacy"` / undefined | Nothing | — |

Props: `{ rootSource?: string; compact?: boolean }`

### 5. Frontend: ProjectStatusTag component

**File:** `frontend/src/components/ProjectStatusTag.tsx` (new)

Renders `project_status_display` as a colored tag. Falls back to `possession_status` if unavailable.

| Status | Color |
|--------|-------|
| `ready_to_move` | Green |
| `under_construction` | Blue |
| `new_launch` | Purple |
| `delayed` | Amber |
| `upcoming` | Gray |

### 6. Frontend: Wire into search results (ResultsPageA.tsx)

- Add `<TrustBadge compact />` in the signals row of CardA
- Replace possession_status span with `<ProjectStatusTag>`

### 7. Frontend: Wire into PropertyCard.tsx

Same pattern as CardA — add compact TrustBadge and ProjectStatusTag.

### 8. Frontend: Wire into PropertyPage.tsx (detail page)

- Full (non-compact) TrustBadge near property title
- ProjectStatusTag in specs row

## Technical Guidance

- `display_template` is `Option<String>` on `SourcedFact` — read it in enrichment.rs
- `SearchResultCard` uses `#[serde(flatten)]` on PropertyCard, so new fields flow to search results automatically
- No new API endpoints needed — extend existing response shapes
- Components are pure presentational — no API calls inside them
- Reference skill files: `.claude/skills/add-api-endpoint.md`

## Constraints

- No new API endpoints. Extend existing responses only.
- No pipeline or KG changes. Read-only from existing data.
- Badges must not break existing layout — fit naturally in signals rows.
- Display text from skill's `display_template`, not frontend hardcoded strings.
- Graceful degradation when data is absent — show nothing, not errors.

## Success Criteria

1. `cargo check` passes with new fields on PropertyCard and PropertyDetail
2. `npm run build` passes with new TrustBadge and ProjectStatusTag components
3. Search results show green "RERA Verified" badge for RERA-rooted societies
4. Search results show amber "Verification Pending" badge for discovered societies
5. Property cards show project status from `display_template` (e.g., "Ready to Move — delivered 31/01/2020")
6. Property detail page shows trust badge prominently near title
7. Properties without root_source or project_status show existing UI (graceful degradation)
