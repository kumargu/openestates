# Day 33: Society-First Scoring — Graph Facts Power the Ranking

## 1. Goal

Replace the current property-centric scoring with society-first scoring. Rank societies using KG facts (positive preference matching + negative penalty + area signal inheritance), then return properties within top societies.

## 2. Product Reason

Current search scores each property independently using a mix of text-field matching and legacy preference→fact lookups. But user preferences ("family friendly," "good maintenance," "avoid water issues") are society-level questions. 10 listings in the same society get 10 slightly different scores for what is essentially the same livability answer.

The KG has rich self-describing facts with `scoring_hint` (direction, weight, thresholds) and `answers_preferences`. This system was designed exactly for this use case — but search doesn't fully use it for ranking.

This is the biggest single improvement to search quality.

## 3. Deliverables

### D1: `score_society_for_intent()` function

New function in `backend/src/search/text.rs` (or a new `backend/src/search/scoring.rs` module):

```rust
pub struct SocietyScore {
    pub society_id: String,
    pub score: f32,
    pub confidence: f32,  // based on evidence richness
    pub matched_reasons: Vec<MatchReason>,
    pub concerns: Vec<Concern>,
    pub unmatched_preferences: Vec<String>,
}

pub struct Concern {
    pub preference: String,
    pub display: String,
    pub confidence: f32,
    pub source_level: String,  // "society" or "area"
}

fn score_society_for_intent(
    society_node: &Node,
    area_node: Option<&Node>,
    intent: &SearchIntent,
) -> SocietyScore
```

Logic:
1. For each `positive_preference` in intent:
   - Search society node facts for matching `expanded_keys`
   - If no society match, search area node facts (reduced weight: 0.7 if no society fact, 0.3 if society has direct evidence)
   - Use fact's `scoring_hint` to compute 0-1 score
   - Record as MatchReason with fact provenance
2. For each `negative_preference` in intent:
   - Same fact search, but invert the score (high value = high penalty)
   - If negative signal found with score > 0.3, record as Concern
   - If no data, record as Concern::NoData
3. Apply buyer archetype weight modifiers (from config)
4. Compute evidence confidence (more facts, higher-confidence sources → higher confidence)

### D2: Area signal inheritance

When scoring a society, also consult the area node's facts:

```rust
fn find_facts_matching_keys(
    society_node: &Node,
    area_node: Option<&Node>,
    keys: &[String],
) -> Vec<FactHit>
```

Rules:
- Society facts get weight 1.0
- Area facts get weight 0.7 if society has no direct evidence for that key
- Area facts get weight 0.3 if society already has direct evidence (supplementary)
- This allows area-level waterlogging risk to penalize societies that lack their own water data

### D3: Buyer archetype weight profiles

Create `backend/src/search/archetypes.json` (or a Rust const):

```json
{
  "family": {
    "boost_keys": ["family_friendly", "child_safety", "calm_environment", "community_vibe", "school_nearby"],
    "boost_weight": 1.5,
    "penalize_keys": ["noise_score", "traffic_score", "density"],
    "penalize_weight": 1.3
  },
  "investor": {
    "boost_keys": ["resale_strength", "market_activity", "rental_yield", "metro_distance"],
    "boost_weight": 1.5,
    "penalize_keys": ["litigation_risk"],
    "penalize_weight": 1.5
  },
  "risk_averse": {
    "boost_keys": ["rera_status", "document_completeness", "builder_reputation"],
    "boost_weight": 1.3,
    "penalize_keys": ["litigation_risk", "waterlogging_risk", "possession_delay"],
    "penalize_weight": 2.0
  },
  "value_buyer": {
    "boost_keys": ["value_for_money", "maintenance_cost"],
    "boost_weight": 1.5,
    "penalize_keys": [],
    "penalize_weight": 1.0
  }
}
```

### D4: Wire into search route

Update `backend/src/routes/search.rs`:

1. After hard-constraint filtering, group candidate properties by `society_id`
2. For each unique society_id, look up the KG society node and area node
3. Call `score_society_for_intent()`
4. Rank societies by score
5. Within each society, rank properties by property-specific fit (price closeness to budget, BHK match, etc.)
6. Flatten into final result list, but preserve society context in response

### D5: Updated SearchResponse

Add society scoring data to the response:

```rust
pub struct SearchResult {
    // existing property fields...
    pub society_score: Option<f32>,
    pub society_confidence: Option<String>,  // "high", "medium", "low"
    pub concerns: Vec<Concern>,
    pub unmatched_preferences: Vec<String>,
}
```

### D6: Fallback for properties without societies

Some properties (especially discovered ones) may not have a linked society node. For these, fall back to the existing property-level scoring. Don't break existing behavior.

## 4. Technical Guidance

**Files to modify:**
- `backend/src/search/mod.rs` — add SocietyScore, Concern types; update SearchResult
- `backend/src/search/text.rs` — add `score_society_for_intent()`, `find_facts_matching_keys()`
- `backend/src/routes/search.rs` — wire society-first scoring into search handler

**New file (optional):**
- `backend/src/search/scoring.rs` — if the scoring logic gets too large for text.rs

**Key architectural point:** The `scoring_hint` on each SourcedFact already tells us how to score. The scoring function is GENERIC — it doesn't know what "family_friendly" means. It reads the fact's scoring_hint direction (HigherIsBetter/LowerIsBetter/TextMatch) and weight. This is the whole point of the self-describing architecture.

**Performance:** At 55 society nodes with ~15 facts each, scoring all societies takes <1ms. No optimization needed.

## 5. Constraints

- Do NOT remove the existing scoring path — keep it as fallback for properties without society nodes
- Do NOT add LLM calls during scoring — this must be deterministic and fast (<5ms)
- Do NOT change the frontend yet — just add new fields to the API response
- Keep the existing `match_explanation` field populated for backward compatibility

## 6. Success Criteria

- [ ] `score_society_for_intent()` function implemented and called during search
- [ ] Positive preferences boost societies with matching facts
- [ ] Negative preferences penalize societies with matching negative signals
- [ ] Area facts flow into society scoring with appropriate weight reduction
- [ ] Buyer archetype "family" produces different rankings than "investor" for the same area
- [ ] Properties without society nodes still get scored via fallback
- [ ] `concerns` array populated in search response for negative signals
- [ ] Society scoring adds <5ms to search latency
- [ ] `cargo check` passes
- [ ] `npm run build` passes
