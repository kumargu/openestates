# Day 32: Structured Intent Parsing — Preference Expansion & Polarity

## 1. Goal

Upgrade intent parsing to return structured preferences with canonical keys, expanded keys, polarity (positive/negative), and buyer archetype detection. One LLM call, much richer output.

## 2. Product Reason

Today, intent parsing returns `preferences: ["quiet", "family friendly"]` as flat strings. The search then literal-matches these against `answers_preferences` on facts. "Family friendly" only matches facts tagged with exactly "family friendly" — it misses "child safety," "playground," "school nearby," "calm environment."

Also, there's no distinction between "I want open space" (positive) and "avoid water issues" (negative). Both are treated the same way during scoring.

This is the intent-parsing upgrade from the context search spec.

## 3. Deliverables

### D1: Define `PreferenceSignal` struct in Rust

In `backend/src/search/mod.rs`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreferenceSignal {
    pub raw_text: String,
    pub canonical_key: String,
    pub expanded_keys: Vec<String>,
    pub polarity: Polarity,
    pub weight: f32,  // 1.0 default, higher if user emphasized
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Polarity {
    Positive,
    Negative,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BuyerArchetype {
    Family,
    Investor,
    EndUser,
    RiskAverse,
    ValueBuyer,
    LuxuryBuyer,
}
```

### D2: Update `SearchIntent` struct

Add new fields alongside existing ones (backward compatible):

```rust
pub struct SearchIntent {
    // existing fields...
    pub area: Option<String>,
    pub bhk: Option<u8>,
    pub budget_max: Option<f64>,
    pub preferences: Vec<String>,  // keep for backward compat

    // NEW
    pub positive_preferences: Vec<PreferenceSignal>,
    pub negative_preferences: Vec<PreferenceSignal>,
    pub buyer_archetype: Option<BuyerArchetype>,
}
```

### D3: Update Gemini intent-parse prompt

Provide the preference expansion taxonomy in the prompt so Gemini maps user language to canonical keys:

```
PREFERENCE TAXONOMY (map user language to these canonical keys):
family_friendly → child_safety, playground, school_nearby, low_density, calm_environment, community_vibe
open_space → density, greenery_score, open_space_score, breathing_room
water_issues → water_supply, waterlogging_risk, tanker_dependency, borewell_status
quiet → noise_score, traffic_score, calm_environment
good_maintenance → maintenance_quality, maintenance_cost, society_management
builder_trust → builder_reputation, delivery_track_record, quality_perception
commute → metro_distance, traffic_score, road_quality
investment → resale_strength, market_activity, area_trend, rental_yield
legal_safety → rera_status, document_completeness, litigation_risk
greenery → greenery_score, open_space_score, park_nearby
low_density → density, overcrowding, open_space_score
premium → builder_reputation, amenity_quality, finish_quality
value_for_money → value_for_money, price_per_sqft, maintenance_cost
```

Update the Gemini response JSON schema to include:
```json
{
  "area": "whitefield",
  "bhk": 3,
  "budget_max": 25000000,
  "positive_preferences": [
    {"raw_text": "family friendly", "canonical_key": "family_friendly", "expanded_keys": ["child_safety", "playground", "calm_environment", "community_vibe"]}
  ],
  "negative_preferences": [
    {"raw_text": "avoid water issues", "canonical_key": "water_issues", "expanded_keys": ["water_supply", "waterlogging_risk", "tanker_dependency"]}
  ],
  "buyer_archetype": "family"
}
```

### D4: Parse the new response format in Rust

Update `parse_intent_from_gemini_response()` (or equivalent) to extract `positive_preferences`, `negative_preferences`, and `buyer_archetype` from the Gemini response. Fall back to existing `preferences` parsing if the new fields are absent.

### D5: Backward compatibility

The existing `preferences: Vec<String>` field should still be populated (union of positive and negative raw_text) so that downstream code that hasn't been updated yet still works.

## 4. Technical Guidance

**Files to modify:**
- `backend/src/search/mod.rs` — new types: PreferenceSignal, Polarity, BuyerArchetype; update SearchIntent
- `backend/src/search/text.rs` — update Gemini prompt and response parsing in the intent extraction function
- `frontend/src/lib/types.ts` — add TypeScript types for new fields (for future frontend use)

**The Gemini prompt update is the core change.** The prompt already asks Gemini to extract preferences. We're adding:
1. The taxonomy reference table
2. A request for positive/negative separation
3. Expanded keys per preference
4. Buyer archetype detection

**Key detail:** The taxonomy should be provided as literal text in the prompt, not as a separate API call. This is a ~300 word addition to the existing prompt.

## 5. Constraints

- Do NOT add a second LLM call for query expansion — it must happen in the same intent-parse call
- Do NOT change the search scoring yet (that's Day 33) — just parse the richer intent
- Keep backward compatibility: if Gemini returns the old format, fall back gracefully
- The taxonomy is a CLOSED vocabulary — don't let Gemini invent new canonical keys

## 6. Success Criteria

- [ ] `PreferenceSignal`, `Polarity`, `BuyerArchetype` types defined in Rust
- [ ] `SearchIntent` has `positive_preferences`, `negative_preferences`, `buyer_archetype` fields
- [ ] Gemini prompt includes the preference taxonomy
- [ ] Query "family friendly 3BHK Whitefield" returns `buyer_archetype: Family` and `positive_preferences` with expanded keys
- [ ] Query "avoid water issues and overcrowding" returns `negative_preferences` with `Polarity::Negative`
- [ ] Old `preferences` field still populated for backward compatibility
- [ ] TypeScript types updated in `frontend/src/lib/types.ts`
- [ ] `cargo check` passes
- [ ] `npm run build` passes
