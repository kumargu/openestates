# Day 62: Search Leverages RERA Facts (answers_preferences-Driven Ranking)

## Goal

Make search ranking use `project_status` SourcedFacts via `answers_preferences` so queries like "ready to move in Whitefield" boost matching societies through the self-describing graph, not legacy hardcoded logic.

## Product Reason

Trust badges are visible (Day 61) but search is blind to them. A user typing "ready to move in Whitefield" gets results ranked by text matching on the old `possession_status` string field, not by the RERA-verified `project_status` fact with confidence 1.0. This is the Sprint 3 promise gap: the data is there, the badges are there, but the core ranking loop does not use them. Fixing this makes RERA data actually power discovery, not just decoration.

## Sprint Context

Day 4 of 14 in Sprint 3 (Days 59-72). Theme: "Root the graph in government truth. Make trust visible."

## Feedback Addressed

1. **Day 59 verifier: fill RERA dates for remaining societies** — Day 60 addressed this (54/65 matched). Today's work makes those dates matter in search.
2. **Vision.md Sprint 3: "minimal filters, maximum intelligence"** — search should use `answers_preferences` from SourcedFacts, not hardcoded preference maps. Today implements this for project status.
3. **TextMatch scoring bug** — `score_fact_with_hint` only recognizes sentiment words ("good", "high"), so `project_status` facts with value `"ready_to_move"` score 0.0 even when matched via `answers_preferences`.

## Deliverables

### 1. Fix TextMatch scoring for category values (critical bug)

**File:** `backend/src/routes/search.rs` — function `score_fact_with_hint`

**Problem:** The `TextMatch` scoring direction only recognizes sentiment words ("good", "high", "positive"). A `project_status` fact with value `"ready_to_move"` falls through to score 0.0, even though it was correctly matched via `answers_preferences`.

**Fix:** When `TextMatch` direction and the fact was matched via `answers_preferences`, return `weight` immediately. The `answers_preferences` match already proves relevance. Add `matched_via_answers_preferences: bool` parameter to `score_fact_with_hint`, or simpler: for `TextMatch` direction, if the text value is not empty and not negative ("poor", "bad", "low"), score at `weight`.

### 2. Expand intent preference extraction for project status queries

**File:** `backend/src/search/intent.rs` — `PREFERENCE_PATTERNS`

Add/fix patterns:
- "ready to move", "ready possession", "immediate possession", "delivered", "completed" → "ready to move"
- "under construction", "ongoing", "in progress" → "under construction"
- "new launch", "newly launched", "just launched" → "new launch"
- "delayed", "behind schedule" → "delayed"
- "upcoming", "pre-launch", "future project" → "upcoming"

Remove "under construction" and "upcoming" from the "new construction" pattern.

### 3. Add answers_preferences fuzzy matching

**File:** `backend/src/routes/search.rs` — function `graph_preference_score_detailed`

Change matching from exact equality to contains-based:
```rust
let answers = fact.answers_preferences.iter().any(|ap| {
    let ap_lower = ap.to_lowercase();
    ap_lower == pref_lower
        || ap_lower.contains(&pref_lower)
        || pref_lower.contains(&ap_lower)
});
```

### 4. Add integration tests

Prove:
- "ready to move in Whitefield" extracts preference "ready to move"
- Society with `project_status: ready_to_move` gets graph-driven score boost (not 0.0)
- Society with `project_status: under_construction` does NOT get boosted for "ready to move"
- "under construction Sarjapur" extracts "under construction" (not "new construction")

### 5. Verify legacy fallback still works

Properties without KG facts should still score via `legacy_preference_score`. No changes needed, verify in testing.

## Technical Guidance

- Fix must be generic to all `TextMatch` facts with `answers_preferences`, not special-cased for `project_status`
- Keep `PREFERENCE_PATTERNS` aligned with `answers_preferences` values from `classify_project_status.py`
- `cargo test` must pass after changes
- Reference: `backend/src/knowledge/fact.rs` for `ScoringHint`, `ScoringDirection`, `SourcedFact` structs

## Constraints

- No Python pipeline changes (purely Rust backend)
- No new API endpoints
- No frontend changes
- TextMatch fix must be generic
- Do not break legacy fallback path

## Success Criteria

1. `cargo check` and `cargo test` pass
2. "ready to move in Whitefield" ranks RERA-verified `ready_to_move` societies higher
3. `match_explanation` shows `scoring_method: "graph"`, `fact_key: "project_status"`, non-zero score
4. "under construction Sarjapur" extracts "under construction" (not "new construction")
5. "new launch" extracts and matches `project_status: new_launch`
6. `project_status` fact with `TextMatch` direction scores at `weight` (3.0), not 0.0
7. Properties without KG nodes still score via legacy fallback
