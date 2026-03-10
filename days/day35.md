# Day 35: Semantic Recall Rework — Embeddings for Discovery, Not Scoring

## 1. Goal

Change the role of embeddings from "score boost" to "candidate recall." Embeddings find societies the user didn't explicitly ask about. Graph-fact scoring ranks them.

## 2. Product Reason

Currently, embedding similarity adds up to +15% boost to text search scores. This conflates two concerns:
- **Ranking** (how well does this match your preferences?) — should be graph-fact scoring
- **Discovery** (what else might interest you?) — should be embedding similarity

After Days 31-34, graph-fact scoring is the primary ranker. Embeddings should serve exactly one role: surfacing "you might also consider" societies that wouldn't appear through hard-constraint filtering alone.

This is especially important for:
- Queries with no specific area ("good family societies in East Bangalore")
- Area exploration ("what's similar to Whitefield but less crowded?")
- Sparse results (user's filters are too narrow)

## 3. Deliverables

### D1: Remove embedding score boost from main ranking

In `backend/src/routes/search.rs`, remove the current semantic boost logic that adds up to 0.15 to scores. The embedding similarity should NOT influence the primary ranking anymore.

### D2: Add "also consider" semantic recall

New function:

```rust
async fn find_similar_societies(
    graph: &KnowledgeGraph,
    embed_client: &EmbedClient,
    query: &str,
    already_shown: &[String],  // society IDs already in results
    top_n: usize,
) -> Vec<SimilarSociety>
```

Logic:
1. Embed the user's query
2. Run `similar_to_vector()` across all society + area nodes
3. Filter out societies already in the primary results
4. Filter by threshold (cosine > 0.4)
5. For each candidate, run `score_society_for_intent()` to get a real score
6. Return top_n with both similarity score and graph-fact score

### D3: Two-tier response structure

Update `SearchResponse`:

```rust
pub struct SearchResponse {
    pub query: String,
    pub intent: SearchIntent,
    pub results: Vec<SearchResult>,            // primary: hard-constraint filtered + graph-scored
    pub also_consider: Vec<SearchResult>,       // semantic: embedding-discovered + graph-scored
    pub area_context: Option<AreaContext>,
    pub discovery_status: Option<DiscoveryStatus>,
}
```

### D4: Trigger conditions for semantic recall

Semantic recall runs when:
- Primary results < 5 (sparse results)
- All primary results score < 0.4 (weak matches)
- No area specified in query (exploratory search)
- User explicitly asks for alternatives ("what else is similar")

It does NOT run when:
- Primary results are strong (>= 5 results, top score > 0.6)
- The query is purely filter-based (just BHK + budget + area)

### D5: Frontend "Also Consider" section

Add a new section below primary results in `ResultsPageA.tsx`:

```
─── Also Consider ───
These societies are semantically similar to your search but outside your explicit filters.
[PropertyCard] [PropertyCard] [PropertyCard]
```

Styled differently from primary results — lighter background, smaller cards, with a brief note explaining why they appeared ("Similar profile to your search preferences").

### D6: Embed queries with the full expanded intent

Instead of embedding just the raw query text, embed a richer representation:

```rust
let embed_text = format!(
    "{}. Looking for: {}. Avoiding: {}",
    query,
    intent.positive_preferences.iter().map(|p| &p.raw_text).join(", "),
    intent.negative_preferences.iter().map(|p| &p.raw_text).join(", "),
);
```

This gives the embedding model more signal about what the user actually wants.

## 4. Technical Guidance

**Files to modify:**
- `backend/src/routes/search.rs` — remove boost, add also_consider logic
- `backend/src/search/mod.rs` — update SearchResponse, add AlsoConsider types
- `backend/src/search/semantic.rs` — update to return candidates for scoring (not direct boosts)
- `frontend/src/pages/ResultsPageA.tsx` — render also_consider section
- `frontend/src/lib/types.ts` — update SearchResponse type

**Performance:** Embedding the query is the expensive part (~200ms API call). This already happens in parallel with text search. The brute-force similarity scan over ~70 nodes is <1ms. Scoring the similar societies via graph facts is <1ms. Total additional latency: ~0ms (already parallelized).

**Important:** The `also_consider` results go through the same `score_society_for_intent()` pipeline as primary results. They get the same explanation cards, the same concerns, the same confidence labels. The only difference is how they were found (embedding recall vs hard-constraint filter).

## 5. Constraints

- Do NOT use embedding similarity as a ranking signal — it's purely for recall
- Do NOT show also_consider if primary results are strong
- Do NOT add more than 5 also_consider results
- Keep the embed API call parallelized with text search (no additional latency)

## 6. Success Criteria

- [ ] Embedding boost removed from primary ranking
- [ ] Primary results ranked purely by graph-fact society scoring
- [ ] "Also consider" section appears when primary results are sparse
- [ ] Also_consider societies are scored through graph-fact scoring (not raw cosine)
- [ ] Query with no area returns semantically relevant societies from across areas
- [ ] Also_consider section rendered in frontend with appropriate styling
- [ ] No regression in search latency (embedding query still parallelized)
- [ ] `cargo check` passes
- [ ] `npm run build` passes
