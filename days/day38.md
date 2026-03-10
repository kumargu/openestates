# Day 38: Multi-Aspect Embeddings — Future-Proof for Scale

## 1. Goal

Implement multi-aspect embeddings per entity: one embedding per scoring dimension. This replaces the single summary embedding with dimension-specific vectors that enable precise semantic matching at scale.

## 2. Product Reason

A single 768-dim embedding collapses a multi-dimensional entity into one point. "Prestige Lakeside" has aspects: family-friendly (strong), water supply (weak), maintenance (good), builder trust (high), density (high). A query about "family friendly" and a query about "avoid water issues" both compare against the SAME vector. The embedding can't distinguish which dimension matters.

At 48 societies this is tolerable — graph-fact scoring handles the precision. But at 500+ societies, the embedding recall layer needs to surface the right candidates for diverse query types. Multi-aspect embeddings solve this.

This is the long-term foundation. Build it now so the system doesn't need re-architecture at scale.

## 3. Deliverables

### D1: Define aspect taxonomy

6 aspects (matching the scoring dimensions):

```
livability    — maintenance, society management, community vibe, daily convenience
family        — child safety, schools, calm environment, low density, playgrounds
risk          — water supply, waterlogging, legal, builder trust, possession delay
investment    — resale strength, rental yield, market activity, area trend, metro access
environment   — greenery, noise, air quality, open space, density
infrastructure — metro distance, road quality, commute, traffic, connectivity
```

### D2: Aspect-specific text builder

Update `pipeline/skills/embed_entity.py` to generate text per aspect:

```python
def build_aspect_texts(node_data: dict) -> dict[str, str]:
    """Build embedding text per aspect from KG facts."""
    aspects = {
        "livability": [],
        "family": [],
        "risk": [],
        "investment": [],
        "environment": [],
        "infrastructure": [],
    }

    ASPECT_KEY_MAP = {
        "livability": ["maintenance_quality", "society_management", "community_vibe", "livability_score", "daily_convenience"],
        "family": ["family_friendly", "child_safety", "school_nearby", "calm_environment", "playground", "low_density"],
        "risk": ["water_supply", "waterlogging_risk", "tanker_dependency", "litigation_risk", "builder_reputation", "rera_status", "possession_delay"],
        "investment": ["resale_strength", "market_activity", "rental_yield", "area_trend", "metro_distance", "price_per_sqft"],
        "environment": ["greenery_score", "noise_score", "air_quality", "open_space_score", "density"],
        "infrastructure": ["metro_distance", "road_quality", "traffic_score", "commute_score", "connectivity"],
    }

    for fact in node_data.get("facts", []):
        for aspect, keys in ASPECT_KEY_MAP.items():
            if fact["key"] in keys and fact.get("display_template"):
                display = fact["display_template"].replace("{value}", str(fact["value"].get("data", "")))
                aspects[aspect].append(display)

    # Build text per aspect
    name = node_data.get("name", "")
    result = {}
    for aspect, texts in aspects.items():
        if texts:
            result[aspect] = f"{name}. {'. '.join(texts)}"
        # Skip aspects with no facts — no embedding for unknown dimensions

    return result
```

### D3: Store multi-aspect embeddings on nodes

Update `backend/src/knowledge/node.rs`:

```rust
pub struct Node {
    // ... existing fields
    pub summary_embedding: Option<Vec<f32>>,  // KEEP for backward compat
    pub aspect_embeddings: Option<HashMap<String, Vec<f32>>>,  // NEW
}
```

Node JSON storage:
```json
{
  "id": "society:prestige-lakeside",
  "summary_embedding": [0.1, 0.2, ...],
  "aspect_embeddings": {
    "livability": [0.1, 0.2, ...],
    "family": [0.1, 0.2, ...],
    "risk": [0.1, 0.2, ...],
    "investment": [0.1, 0.2, ...]
  }
}
```

### D4: Aspect-aware similarity search

Update `backend/src/knowledge/embeddings.rs`:

```rust
impl KnowledgeGraph {
    /// Find similar nodes using aspect-specific embeddings.
    /// Maps query aspects to the right embedding dimension per node.
    pub fn similar_to_vector_by_aspect(
        &self,
        query_embedding: &[f32],
        aspects: &[String],  // which aspects matter for this query
        top_n: usize,
        node_type_filter: Option<NodeType>,
    ) -> Vec<SimilarEntity> {
        // For each node:
        //   1. Average cosine similarity across the specified aspects
        //   2. Fall back to summary_embedding if aspect embeddings missing
    }
}
```

### D5: Map query preferences to aspects

In the search route, map the parsed intent's preferences to aspects:

```rust
fn intent_to_aspects(intent: &SearchIntent) -> Vec<String> {
    let mut aspects = HashSet::new();

    let pref_to_aspect: HashMap<&str, &str> = [
        ("family_friendly", "family"), ("child_safety", "family"),
        ("water_supply", "risk"), ("waterlogging_risk", "risk"), ("builder_reputation", "risk"),
        ("maintenance_quality", "livability"), ("community_vibe", "livability"),
        ("resale_strength", "investment"), ("market_activity", "investment"),
        ("greenery_score", "environment"), ("noise_score", "environment"),
        ("metro_distance", "infrastructure"), ("traffic_score", "infrastructure"),
    ].into();

    for pref in intent.positive_preferences.iter().chain(&intent.negative_preferences) {
        for key in &pref.expanded_keys {
            if let Some(aspect) = pref_to_aspect.get(key.as_str()) {
                aspects.insert(aspect.to_string());
            }
        }
    }

    aspects.into_iter().collect()
}
```

### D6: Batch re-embed all nodes with aspects

Update `pipeline/scripts/reembed_all.py` to generate aspect embeddings for all society and area nodes. This is a batch job that replaces Day 31's summary-only embeddings.

Rate limit: 1 embedding call per second. 6 aspects × 55 societies = 330 API calls = ~6 minutes.

## 4. Technical Guidance

**Files to modify:**
- `pipeline/skills/embed_entity.py` — add `build_aspect_texts()`, update skill to produce aspect embeddings
- `backend/src/knowledge/node.rs` — add `aspect_embeddings` field
- `backend/src/knowledge/embeddings.rs` — add `similar_to_vector_by_aspect()`
- `backend/src/routes/search.rs` — use aspect-aware similarity for "also consider"
- `pipeline/scripts/reembed_all.py` — batch re-embed with aspects

**Backward compatibility:** Keep `summary_embedding` as fallback. If a node has `aspect_embeddings`, use them. If not, fall back to `summary_embedding`. This means the system degrades gracefully.

**Cost:** 330 embedding API calls × $0.0001 = $0.03 total. Negligible.

**Performance:** Aspect-aware similarity is slightly more compute than single-vector similarity (6 cosine sims per node instead of 1). At 55 nodes, this is still <1ms. At 5000 nodes, it's still <10ms. Not a concern.

## 5. Constraints

- Do NOT change the embedding model — stay on gemini-embedding-001
- Do NOT add a vector database — brute-force is fine even with 6× more vectors
- Keep `summary_embedding` for backward compat (nodes without aspects still work)
- Skip aspects with no facts — don't embed empty text
- Max 6 aspects — don't over-fragment

## 6. Success Criteria

- [ ] `build_aspect_texts()` generates per-aspect text from KG facts
- [ ] All 55 society nodes have aspect embeddings stored
- [ ] All 16 area nodes have aspect embeddings stored
- [ ] `similar_to_vector_by_aspect()` works and returns relevant results
- [ ] "Family friendly" query uses family+livability aspects for recall
- [ ] "Investment opportunity" query uses investment+infrastructure aspects
- [ ] Nodes without aspect embeddings fall back to summary_embedding
- [ ] Batch re-embed script runs in <10 minutes
- [ ] `cargo check` passes
- [ ] `npm run build` passes
