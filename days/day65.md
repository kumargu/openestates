# Day 65: Data Freshness Indicators & Confidence Meter

## Goal

Add data freshness indicators and confidence meter to search results and property pages — the two remaining trust-visibility features from Sprint 3's vision.

## Product Reason

Sprint 3's theme is "make trust visible." Days 59-64 built the trust infrastructure (RERA seeding, trust badges, builder trust, project status). But users still can't see *when* data was last enriched or *how confident* the system is in a result. These two features complete the transparency promise.

## Sprint Context

Day 7 of 14 in Sprint 3 (Days 59-72). Theme: "Root the graph in government truth. Make trust visible."

## Feedback Resolution

1. **Day 62: TextMatch scores ANY non-empty non-negative text at full weight** — Accept. When a fact is matched via `answers_preferences`, the relevance is proven by the match. Scoring at full weight for matched, non-negative text is the intended design.

2. **Day 62: Add test for property WITHOUT KG node falling through to legacy_preference_score** — Include as test case today.

3. **Day 63: Used confidence 0.9 for computed builder facts** — Accept. Computed facts traversing BuiltBy edges add indirection; 0.9 is correct.

4. **Day 63: check_builder_facts returns first matching builder fact if multiple BuiltBy edges** — Accept for Sprint 3. Builder deduplication is Sprint 4 scope.

## Deliverables

### 1. Backend: `DataFreshness` struct + extraction

**File:** `backend/src/routes/enrichment.rs`

```rust
#[derive(Serialize, Clone, Debug)]
pub struct DataFreshness {
    pub last_enriched: String,      // ISO 8601 timestamp (node.updated_at)
    pub days_ago: u32,              // computed: (now - updated_at).num_days()
    pub freshness_label: String,    // "Fresh" (<7d), "Recent" (<30d), "Stale" (>30d)
    pub fact_count: u32,            // number of facts on the society node
    pub source_breakdown: HashMap<String, u32>, // e.g. {"Rera": 15, "Reddit": 3}
}
```

Compute from the society KG node's `updated_at` field and facts. Wire into `enrich_property_card_with_sellers()`.

### 2. Backend: Add `data_freshness` to PropertyCard

**File:** `backend/src/models/property.rs`

Add `pub data_freshness: Option<DataFreshness>` to PropertyCard struct.

### 3. Backend: `ConfidenceScore` on SearchResultCard

**File:** `backend/src/search/mod.rs`

```rust
pub struct ConfidenceScore {
    pub overall: f32,           // 0.0-1.0 composite
    pub label: String,          // "High", "Moderate", "Low"
    pub components: Vec<ConfidenceComponent>,
}

pub struct ConfidenceComponent {
    pub name: String,           // "Data Source", "Fact Coverage", "Data Freshness"
    pub score: f32,             // 0.0-1.0
}
```

**Confidence formula:**
- Source quality (0-1): RERA root = 1.0, discovered = 0.5, legacy = 0.3, no node = 0.1
- Fact coverage (0-1): min(fact_count / 15, 1.0)
- Data freshness (0-1): <7d = 1.0, <30d = 0.8, <90d = 0.5, else 0.3
- Match quality (0-1): graph_driven_pct / 100.0
- Overall: weighted average (source 0.4, coverage 0.2, freshness 0.2, match quality 0.2)

Label: >=0.7 "High", >=0.4 "Moderate", else "Low"

Compute in `backend/src/search/text.rs` during scoring.

### 4. Frontend: TypeScript types

**File:** `frontend/src/lib/types.ts`

Add `data_freshness` to PropertyCard type, `confidence_score` to SearchResultItem.

### 5. Frontend: `DataFreshnessBadge` component

**New file:** `frontend/src/components/DataFreshnessBadge.tsx`

Compact pill: "Updated X days ago" with green/blue/amber based on freshness. Renders nothing when undefined.

### 6. Frontend: `ConfidenceMeter` component

**New file:** `frontend/src/components/ConfidenceMeter.tsx`

Small horizontal bar showing High/Moderate/Low confidence with color coding. Expand on hover to show component breakdown.

### 7. Wire into ResultsPageA.tsx and PropertyPage.tsx

- ResultsPageA: Add DataFreshnessBadge + ConfidenceMeter to card signals row
- PropertyPage: Add DataFreshnessBadge near trust badge area

### 8. Backend test: Legacy fallback for properties without KG nodes

**File:** `backend/src/routes/search.rs`

Test that properties without KG nodes fall through to legacy scoring gracefully.

## Files to Modify

| File | Change |
|------|--------|
| `backend/src/models/property.rs` | Add `data_freshness` field |
| `backend/src/routes/enrichment.rs` | Add DataFreshness struct + extraction |
| `backend/src/search/mod.rs` | Add ConfidenceScore structs + field on SearchResultCard |
| `backend/src/search/text.rs` | Compute confidence_score during search |
| `backend/src/routes/search.rs` | Add legacy fallback test |
| `frontend/src/lib/types.ts` | Add freshness + confidence types |
| `frontend/src/components/DataFreshnessBadge.tsx` | New component |
| `frontend/src/components/ConfidenceMeter.tsx` | New component |
| `frontend/src/pages/ResultsPageA.tsx` | Wire in new components |
| `frontend/src/pages/PropertyPage.tsx` | Wire in DataFreshnessBadge |

## Constraints

- No new API endpoints — extend existing PropertyCard and SearchResultCard responses
- No pipeline/Python changes
- No changes to scoring logic — confidence is observational, not a ranking input
- Graceful degradation when data is absent

## Success Criteria

1. `cargo check` and `cargo test` pass
2. `npm run build` succeeds
3. Search results show freshness badge for enriched societies
4. Search results show confidence meter (High/Moderate/Low)
5. RERA-rooted, well-enriched societies show "High confidence"
6. Properties without KG nodes show no badges (graceful absence)
7. Legacy fallback test passes
8. PropertyPage shows DataFreshnessBadge
