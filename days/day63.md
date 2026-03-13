# Day 63: Builder Delivery Track Record & Cross-Node Trust Signals

## Goal

Compute `builder_delivery_rate` as a self-describing SourcedFact on builder nodes, wire it into search via cross-node scoring (society → BuiltBy → builder), and add "reliable builder" preference patterns so queries like "reliable builder Whitefield" rank by RERA-proven delivery performance.

## Product Reason

Users asking "reliable builder in Whitefield" or "trusted builder near Sarjapur" currently get zero preference boost — the system has no builder-level facts and no way to traverse from a property's society to its builder's track record. The RERA data to compute this already exists (completion dates, delay months, project counts). This is the sprint's promise: "make trust visible."

## Sprint Context

Day 5 of 14 in Sprint 3 (Days 59-72). Theme: "Root the graph in government truth. Make trust visible."

## Feedback Addressed

1. **Day 62 builder concern: TextMatch blacklist approach** — Acceptable, generic. No changes needed.
2. **Day 62 verifier: Add test for legacy fallback** — Include in test suite today.
3. **Vision.md: builder_delivery_rate SourcedFact** — This is the explicit Sprint 3 deliverable being implemented today.

## Deliverables

### 1. Python Skill: `compute_builder_delivery_rate.py`

**New file:** `pipeline/skills/compute_builder_delivery_rate.py`

Pure computation skill (zero cost, no LLM). Follows `classify_project_status.py` pattern.

**Algorithm:**
1. For each builder node in `data/knowledge/nodes/builder/`:
   - Find all societies linked via `BuiltBy` edges in `edges.json`
   - For each linked society, read its `project_status` fact
   - Count: `total_projects` = societies with a project_status fact
   - Count: `on_time_projects` = societies with project_status NOT "delayed"
   - Compute: `delivery_rate = on_time_projects / total_projects` (0.0-1.0)
   - Skip builders with fewer than 2 projects (insufficient signal)
2. Produce `builder_delivery_rate` SourcedFact (Numeric, HigherIsBetter, weight 2.5)
3. Produce `builder_project_count` SourcedFact (Numeric, HigherIsBetter, weight 1.0)
4. Produce `builder_zero_revocations` for builders with 0 revocations (TextMatch, weight 2.0)
5. Write facts to builder node JSON files using atomic write

### 2. Rust Backend: Cross-Node Preference Scoring

**File:** `backend/src/routes/search.rs`

Modify `graph_preference_score_detailed` to traverse `BuiltBy` edges from society to builder node and check builder facts when society facts don't match.

### 3. Intent Parser: Builder Preference Patterns

**File:** `backend/src/search/intent.rs`

Add patterns:
- "reliable builder", "dependable builder" → "reliable builder"
- "trusted builder", "good builder", "reputed builder" → "trusted builder"
- "on time delivery", "no delays", "timely delivery" → "on time delivery"

### 4. Cross-Node Claims in `build_knowledge_context`

**File:** `backend/src/routes/search.rs`

Extend `build_knowledge_context` to also extract claims from builder nodes via `BuiltBy` edges.

### 5. Tests

- Cross-node builder scoring (society → BuiltBy → builder → fact match)
- No match without BuiltBy edge
- Builder preference patterns in intent parser
- Legacy fallback still works for properties without KG nodes

## Not in Scope

- Frontend builder badge/display — deferred to Day 64
- Builder deduplication — deferred to Sprint 4
- New PropertyCard fields — facts flow through MatchReason.display automatically

## Success Criteria

1. `python3 -m pipeline.skills.compute_builder_delivery_rate` populates builder nodes with delivery facts
2. `cargo test` passes with all new tests
3. "reliable builder whitefield" returns results boosted by builder delivery rate
4. `MatchReason` shows "Builder delivers on time: X% of projects"
5. Cross-node scoring pattern works generically via BuiltBy edge traversal
6. Legacy fallback unbroken for properties without KG nodes
