# Day 39: Search Observability — Debug Why Results Rank

## 1. Goal

Add comprehensive search debug logging and a debug API mode so we can trace exactly why any result ranked where it did. Essential for ongoing quality tuning.

## 2. Product Reason

Search quality work is impossible without observability. When a result ranks unexpectedly (too high, too low, or missing), we need to trace: What intent was parsed? Which facts matched? What scores did each preference get? What penalties applied? What confidence was computed?

Currently, search events are logged to JSONL but without score breakdowns. The evaluation script (Day 36) can only check outputs, not internals. This makes it hard to diagnose ranking issues.

## 3. Deliverables

### D1: Search debug trace struct

```rust
pub struct SearchDebugTrace {
    pub timestamp: String,
    pub query: String,
    pub parsed_intent: SearchIntent,
    pub candidate_count: usize,
    pub societies_scored: Vec<SocietyDebugScore>,
    pub also_consider_triggered: bool,
    pub also_consider_reason: Option<String>,
    pub discovery_triggered: bool,
    pub total_latency_ms: u64,
    pub intent_parse_ms: u64,
    pub scoring_ms: u64,
    pub embedding_ms: u64,
}

pub struct SocietyDebugScore {
    pub society_id: String,
    pub society_name: String,
    pub final_score: f32,
    pub confidence: f32,
    pub preference_scores: Vec<PreferenceDebugScore>,
    pub archetype_modifier: f32,
    pub area_signals_used: Vec<String>,
    pub facts_consulted: usize,
}

pub struct PreferenceDebugScore {
    pub preference: String,
    pub polarity: String,
    pub matched_fact_key: Option<String>,
    pub matched_fact_value: Option<String>,
    pub raw_score: f32,
    pub weighted_score: f32,
    pub source: Option<String>,
    pub confidence: f32,
}
```

### D2: Debug mode API parameter

`GET /api/search?q=...&debug=true`

When `debug=true`:
- Response includes a `debug` field with full `SearchDebugTrace`
- Every scored society shows per-preference score breakdown
- Timing breakdown included

When `debug=false` (default):
- `debug` field is null/absent (no extra compute or payload)

### D3: Enhanced search event logging

Update the search event JSONL log to include:
- Parsed intent (not just raw query)
- Buyer archetype detected
- Number of graph-scored vs legacy-scored results
- Top 3 society IDs and their scores
- Whether "also consider" was triggered
- Whether live discovery was triggered
- Total latency

This makes the daily JSONL logs useful for aggregate quality analysis.

### D4: Score comparison endpoint

`GET /api/debug/score?q=...&society=society:prestige-lakeside`

Scores a specific society against a specific query and returns the full debug breakdown. Useful for "why did X rank low for this query?"

### D5: Latency instrumentation

Add `Instant::now()` timing to each search phase:
1. Intent parsing (Gemini call)
2. Hard-constraint filtering
3. Society grouping + graph scoring
4. Embedding similarity (parallel)
5. Explanation synthesis
6. Total

Log these in the debug trace and as metrics in search event logs.

### D6: Frontend debug panel (dev only)

When `?debug=true` is in the URL, show a collapsible debug panel below search results:

- Parsed intent JSON
- Per-result score breakdown (expandable)
- Timing breakdown
- "Why this ranked here" for each result

Only visible in development. Hidden in production via env flag.

## 4. Technical Guidance

**Files to modify:**
- `backend/src/search/mod.rs` — SearchDebugTrace, SocietyDebugScore types
- `backend/src/routes/search.rs` — collect timing, build debug trace, conditional include
- `backend/src/knowledge/graph.rs` — enhanced search event logging
- `frontend/src/pages/ResultsPageA.tsx` — debug panel (dev mode)
- `frontend/src/lib/types.ts` — debug types

**New endpoint:**
- `GET /api/debug/score` — score one society against one query

**Performance:** Debug trace collection adds minimal overhead (a few HashMap inserts and timing calls). The debug payload is only serialized when requested. No impact on non-debug queries.

**Security:** The debug endpoint should be gated by an env flag (`ENABLE_DEBUG_API=true`) or admin token. Don't expose score internals in production.

**Follow .claude/skills/add-api-endpoint.md** for the new debug endpoint.

## 5. Constraints

- Debug mode must NOT affect ranking — same results with or without debug=true
- Debug payload only included when explicitly requested
- Debug endpoint gated by env flag (not exposed in production)
- Latency instrumentation must have <1ms overhead
- Do NOT log full embedding vectors (too large) — just similarity scores

## 6. Success Criteria

- [ ] `?debug=true` returns full SearchDebugTrace in response
- [ ] Per-society score breakdown shows each preference's contribution
- [ ] Latency timing broken down by phase
- [ ] Search event JSONL includes intent, archetype, top scores, latency
- [ ] `/api/debug/score` endpoint returns single-society debug breakdown
- [ ] Frontend debug panel shows score breakdown (dev mode only)
- [ ] Debug mode adds <5ms overhead
- [ ] Non-debug queries unaffected
- [ ] `cargo check` passes
- [ ] `npm run build` passes
