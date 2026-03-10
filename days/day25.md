# Day 25: Wire Embeddings Into Search — Semantic Matching Meets Transparency

## 1. The Problem

Embeddings exist across the system but are completely disconnected from search:

- **Python skill** (`pipeline/skills/embed_entity.py`) generates 768-dim vectors via Google `gemini-embedding-001` and stores them on KG nodes
- **Rust backend** (`backend/src/knowledge/embeddings.rs`) has `similar_to()` and `similar_to_vector()` with brute-force cosine similarity
- **API endpoint** `/api/knowledge/nodes/{id}/similar` exists and works
- **Python engine** (`engine/vector_search.py`) has a standalone numpy-based vector search

But the actual search route (`/api/search`) uses **zero embeddings**. It's pure text matching + structured intent + graph preference scoring. The `similar_to_vector()` function has literally never been called by any search flow.

This means a query like _"peaceful green campus living in Whitefield"_ fails to match a society described as _"serene, landscaped, garden-facing community"_ — because none of those words overlap. Embedding similarity would catch this instantly.

Meanwhile, `engine/vector_search.py` loads `.npy` files that don't exist — the pipeline writes embeddings as JSON in KG nodes, not as numpy binaries. The Python engine is dead code in practice.

---

## 2. What We're Building

A **hybrid search** system where:
1. Text matching + structured intent remains the primary scoring path (transparent, explainable)
2. Embedding similarity acts as a **semantic boost** — rescues fuzzy matches that text matching misses
3. A **"Similar Properties"** feature on property detail pages uses the existing similarity endpoint
4. Dead code and disconnects are cleaned up

The key constraint: **embeddings enhance explainability, they don't replace it**. Every embedding-boosted result gets a reason like _"semantically similar to your search"_ — not a mystery score.

---

## 3. Architecture After Day 24

```
User Query: "peaceful green campus living Whitefield"
  ↓
parse_intent() → { area: Whitefield, preferences: ["peaceful", "green", "campus"] }
  ↓
TextSearch::search_with_intent()
  ├─ Hard filters: BHK, budget, area
  ├─ Text score: keyword matching across fields
  ├─ Graph preference score: KG facts + legacy fallback
  └─ Returns scored results
  ↓
NEW: Semantic boost
  ├─ Embed query via Gemini API (768-dim)
  ├─ KnowledgeGraph::similar_to_vector(query_emb, top_20, society)
  ├─ For each text result: if its society has high embedding similarity → boost score
  ├─ For societies NOT in text results but high similarity → inject as "semantic match"
  └─ All boosted/injected results tagged with reason: "semantically similar"
  ↓
Merged + re-ranked results → SearchResponse
```

---

## 4. Scope

| Phase | What | Time |
|-------|------|------|
| **A** | Embed queries in Rust (Gemini API call) | 45 min |
| **B** | Wire semantic boost into search route | 1 hour |
| **C** | "Similar Properties" on detail page | 45 min |
| **D** | Clean up dead code + fix disconnects | 30 min |
| **E** | Verify end-to-end | 30 min |

---

## 5. Implementation Plan

### Phase A: Query Embedding in Rust (45 min)

**Goal:** The Rust backend can embed a search query string into a 768-dim vector at search time.

#### A.1 Add embedding client to backend

Create `backend/src/knowledge/embed_client.rs`:

```rust
/// Lightweight client to embed text via Google gemini-embedding-001 API.
/// Used at search time to embed the user's query for semantic matching.
pub struct EmbedClient {
    api_key: String,
    http: reqwest::Client,
}

impl EmbedClient {
    pub fn new(api_key: String) -> Self { ... }

    /// Embed a text string into a 768-dim vector.
    /// Returns None if the API call fails (search continues without semantic boost).
    pub async fn embed(&self, text: &str) -> Option<Vec<f32>> { ... }
}
```

Key design decisions:
- Uses the same `GOOGLE_AI_API_KEY` env var as the Python skill
- Uses `reqwest` (already a dependency for Gemini discovery client)
- Returns `Option` — embedding failure is non-fatal, search degrades gracefully
- Timeout: 5 seconds max. If Gemini is slow, skip the boost.
- No caching yet (queries are unique). Can add LRU cache later if needed.

#### A.2 Add EmbedClient to AppState

In `backend/src/state.rs`:
```rust
pub struct AppState {
    // ... existing fields ...
    pub embed_client: Option<EmbedClient>,  // None if GOOGLE_AI_API_KEY not set
}
```

Initialize in `main.rs` alongside GeminiClient — both use the same API key.

**PAUSE. Verify: `cargo check` passes. EmbedClient compiles. Can test with a simple endpoint if needed.**

---

### Phase B: Wire Semantic Boost Into Search (1 hour)

**Goal:** Search results include a semantic similarity component. Fuzzy matches that text matching misses are rescued.

#### B.1 Add semantic scoring function

In `backend/src/search/semantic.rs` (new file):

```rust
/// Semantic search boost: finds societies with high embedding similarity to the query.
/// Returns a map of society_node_id → similarity_score for boosting text search results.
pub async fn semantic_society_scores(
    embed_client: &EmbedClient,
    graph: &KnowledgeGraph,
    query: &str,
    top_n: usize,
) -> HashMap<String, f64> {
    // 1. Embed the query
    let query_emb = match embed_client.embed(query).await {
        Some(emb) => emb,
        None => return HashMap::new(),  // Graceful degradation
    };

    // 2. Find similar society nodes
    let similar = graph.similar_to_vector(
        &query_emb,
        top_n,
        Some(NodeType::Society),
    );

    // 3. Build score map (only include scores above a minimum threshold)
    similar.into_iter()
        .filter(|s| s.similarity > 0.3)  // Below 0.3 cosine = noise
        .map(|s| (s.node_id, s.similarity))
        .collect()
}
```

#### B.2 Integrate into search route

In `backend/src/routes/search.rs`, after text search runs:

```rust
// --- Semantic boost (non-blocking, best-effort) ---
let semantic_scores = if let Some(ref embed_client) = state.embed_client {
    let graph = state.knowledge.read().await;
    semantic_society_scores(embed_client, &graph, &query, 20).await
} else {
    HashMap::new()
};

// Apply semantic boost to existing results
for result in &mut results {
    if let Some(society_id) = property_to_society(&result.card.id, &properties) {
        let node_id = society_node_id(&society_id);
        if let Some(&sim) = semantic_scores.get(&node_id) {
            // Boost: add up to 0.15 to normalized score (max 15% boost)
            let boost = (sim - 0.3) * 0.2;  // Maps 0.3-1.0 similarity → 0.0-0.14
            result.match_score = (result.match_score + boost).min(1.0);
            result.match_reason = format!("{} + semantically relevant", result.match_reason);
        }
    }
}

// Inject semantic-only matches (high similarity but zero text score)
for (node_id, sim) in &semantic_scores {
    if sim > 0.5 && !results.iter().any(|r| /* property's society matches node_id */) {
        // Find properties in this society and inject as "Semantic match" results
        // ... create SearchResultCard with match_label = "Semantic match" ...
    }
}

// Re-sort by updated scores
results.sort_by(|a, b| b.match_score.partial_cmp(&a.match_score).unwrap_or(Ordering::Equal));
```

#### B.3 Add `semantic_boost` field to SearchResultCard

Extend the response so the frontend knows which results were semantically boosted:

```rust
pub struct SearchResultCard {
    // ... existing fields ...
    pub semantic_score: Option<f64>,  // NEW: cosine similarity if boosted, None otherwise
}
```

This lets the frontend optionally show a badge like "Semantic match" on boosted results.

#### B.4 Handle timing

The embedding API call adds latency (~200-500ms). Mitigate:
- Fire the embed call **in parallel** with text search (both are read-only)
- Use `tokio::join!` to run both concurrently
- Text search results are available immediately; semantic boost merges after

```rust
let (text_results, semantic_scores) = tokio::join!(
    async {
        let graph = state.knowledge.read().await;
        let properties = state.properties.read().await;
        TextSearch::search_with_intent(&properties, &society_names, &state.societies, &query, &parsed_intent, Some(&graph))
    },
    async {
        if let Some(ref ec) = state.embed_client {
            let graph = state.knowledge.read().await;
            semantic_society_scores(ec, &graph, &query, 20).await
        } else {
            HashMap::new()
        }
    }
);
```

**PAUSE. Verify: `cargo check` passes. Search query "peaceful green campus" returns results that include semantically similar societies even without keyword overlap. Results that were only boosted have `semantic_score` set. Text-only results still work unchanged.**

---

### Phase C: "Similar Properties" Feature (45 min)

**Goal:** Property detail page shows "You might also like" section using embedding similarity.

#### C.1 Frontend: Add similar properties to detail page

The endpoint already exists: `GET /api/knowledge/nodes/{id}/similar?top_n=5&type=society`

But we need a **property-level** similarity, not just society-level. Two options:

**Option 1 (simpler, do this):** Backend returns similar societies → frontend shows properties from those societies.

Add to `PropertyDetailResponse`:
```rust
pub struct PropertyDetailResponse {
    // ... existing fields ...
    pub similar_properties: Vec<PropertyCard>,  // NEW: top 4-6 similar by embedding
}
```

Backend logic:
1. Find the property's society node
2. Call `graph.similar_to(society_node_id, 5, Some(NodeType::Society))`
3. For each similar society, pick one representative property
4. Return as `similar_properties`

#### C.2 Frontend: Render similar properties section

In `PropertyPage.tsx`, after the main content:

```tsx
{detail.similar_properties.length > 0 && (
  <section className="similar-properties">
    <h2>You might also like</h2>
    <div className="grid grid-cols-2 gap-4">
      {detail.similar_properties.map(p => (
        <PropertyCard key={p.id} property={p} />
      ))}
    </div>
  </section>
)}
```

#### C.3 Add API type

In `frontend/src/lib/types.ts`, add `similar_properties` to the detail response type.

**PAUSE. Verify: Property detail page shows "You might also like" with 3-5 properties from similar societies. Clicking them navigates to correct detail pages. Properties without embeddings gracefully show no section.**

---

### Phase D: Clean Up Dead Code + Fix Disconnects (30 min)

#### D.1 Fix `engine/vector_search.py` disconnect

The Python `VectorSearch` class loads `.npy` files from `embeddings/{entity_type}/` — but no pipeline step writes `.npy` files. The skill writes JSON embeddings to KG nodes.

**Decision:** Remove `engine/vector_search.py`. The Rust backend now owns all vector search. The Python engine's `VectorSearch` is unused dead code.

If we later need Python-side vector search (e.g., for batch scoring), we can add a function that reads KG node JSON files directly instead of expecting `.npy` files.

#### D.2 Fix test dimension mismatch

In `test_enrichment.py` (or wherever the 3072-dim expectation is), update to 768 to match `gemini-embedding-001` output.

#### D.3 Update `engine/__init__.py` or imports

Remove `VectorSearch` from any public exports in the engine module.

#### D.4 Update architecture docs

In `docs/architecture_v2.md`, update the embeddings section to reflect:
- Embeddings are now integrated into search (hybrid text + semantic)
- Query embedding happens in Rust at search time
- `engine/vector_search.py` removed — Rust is the single vector search runtime
- "Similar properties" feature live on detail pages

**PAUSE. Verify: No broken imports. `cargo test` and `npm run build` pass. No references to deleted files.**

---

### Phase E: End-to-End Verification (30 min)

#### E.1 Semantic boost test

1. Search: "peaceful green campus living Whitefield"
2. Verify: results include societies described with synonyms (serene, landscaped, garden-facing) even if no keyword overlap
3. Verify: boosted results have `semantic_score` field set
4. Verify: `match_reason` includes "semantically relevant" for boosted results

#### E.2 Graceful degradation test

1. Unset `GOOGLE_AI_API_KEY`
2. Search: same query
3. Verify: results are pure text-match (same as before Day 24), no errors
4. Verify: `semantic_score` is `null` for all results

#### E.3 Similar properties test

1. Open a property detail page for a property whose society has embeddings
2. Verify: "You might also like" section shows 3-5 related properties
3. Verify: similar properties come from different but semantically related societies
4. Open a property whose society has NO embedding → section gracefully absent

#### E.4 Latency check

1. Search with embeddings enabled
2. Verify: total response time < 1 second (text + embed in parallel)
3. If >1s, check if embed API is the bottleneck → consider adding LRU cache for common queries

#### E.5 Consistency check (Day 23 regression)

1. Same property appears in `/api/properties` and `/api/search` → card data identical
2. No frontend fallback data anywhere
3. All error states still clean

**PAUSE. All tests pass. Embeddings are wired into search. The system is smarter without being less transparent.**

---

## 6. Files Changed

### New
- `backend/src/knowledge/embed_client.rs` — Query embedding client (Gemini API)
- `backend/src/search/semantic.rs` — Semantic scoring logic for hybrid search

### Modified (Backend)
- `backend/src/knowledge/mod.rs` — export `embed_client`
- `backend/src/state.rs` — add `embed_client: Option<EmbedClient>` to AppState
- `backend/src/main.rs` — initialize EmbedClient from env var
- `backend/src/routes/search.rs` — integrate semantic boost, parallel query embed + text search
- `backend/src/routes/properties.rs` — add `similar_properties` to detail response
- `backend/src/search/mod.rs` — export semantic module, add `semantic_score` to SearchResultCard
- `backend/src/search/text.rs` — minor: expose property→society lookup if not already public

### Modified (Frontend)
- `frontend/src/lib/types.ts` — add `semantic_score`, `similar_properties` to types
- `frontend/src/pages/PropertyPage.tsx` — render "You might also like" section

### Deleted
- `engine/vector_search.py` — dead code, replaced by Rust-side similarity search

### Updated (Docs)
- `docs/architecture_v2.md` — embeddings section updated

---

## 7. What NOT to Build Today

- Query embedding cache (LRU) — only if latency is a problem, which we'll check in Phase E
- Property-level embeddings in search — we embed at society level, which is the right granularity for now
- FAISS or HNSW — brute-force cosine over <500 vectors is fast enough
- Re-running embed_entity skill on all entities — use existing embeddings, fill gaps in a future day
- Frontend "embedding coverage" dashboard — internal-only concern
- Rewriting the Python scoring engine — Rust owns search now

---

## 8. Success Criteria

- [ ] Query embedding works in Rust via Gemini API (< 500ms)
- [ ] Search results include semantic boost for fuzzy matches
- [ ] `semantic_score` field populated on boosted results
- [ ] `match_reason` explains semantic boost ("semantically relevant")
- [ ] Text search + embed run in parallel (no added latency for text-only matches)
- [ ] Graceful degradation: no API key → pure text search, zero errors
- [ ] "You might also like" section on property detail pages
- [ ] Similar properties come from semantically related societies
- [ ] `engine/vector_search.py` deleted
- [ ] Test dimension expectation fixed (768, not 3072)
- [ ] `cargo check` + `cargo test` pass
- [ ] `npm run build` succeeds
- [ ] End-to-end: fuzzy query → semantic matches rescued → transparent match reasons

---

## 9. The Principle

Embeddings are the bridge between what the user *says* and what the data *means*. Text matching catches literal overlap. Embeddings catch conceptual overlap. Together, they make search feel like the system *understands* — without sacrificing the transparency promise.

The key is that semantic matching is **additive, not replacement**. Every boosted result still has a reason. Every injected result is labeled "Semantic match". The user never sees a mystery ranking.

This is what "AI is supportive, not the center" looks like in search: structured intent is the skeleton, text matching is the muscle, and embeddings are the intuition.
