# Day 59: Sprint 3 Foundation — root_source, project_status Classification, Bug Fixes

## Goal

Lay the infrastructure foundation for Sprint 3 (RERA Data Foundation & Trust Model). Add `root_source` to all KG nodes, create the `classify_project_status` skill that computes project lifecycle status from existing RERA dates, and fix the carried-over bugs from Days 56-57.

## Product Reason

Everything in Sprint 3 depends on two primitives that do not exist yet: (1) every node knowing where it came from (`root_source`), and (2) every society with RERA dates having a computed `project_status` fact. Without `root_source`, trust badges have nothing to switch on. Without `project_status`, searches like "ready to move in Whitefield" cannot match. Day 59 establishes both primitives so the remaining 13 days of Sprint 3 can build on them.

## Sprint Context

Day 1 of 14 in Sprint 3 (Days 59-72). Theme: "Root the graph in government truth. Make trust visible."

---

## Deliverables

### 1. Fix carried bugs from Days 56-57

**1a. Sparkline div-by-zero guard (frontend)**

**File:** `frontend/src/pages/SellerDashboardPage.tsx`

Add a guard: if `data.length === 0`, render nothing (return `null`). If `data.length === 1`, render a single dot instead of a polyline.

**1b. build_interest_timeline chronological ordering**

Verify the existing code in `backend/src/routes/sellers.rs` already handles chronological ordering correctly (string comparison works for RFC3339 timestamps). Close this item if confirmed.

**1c. Concurrent read locks in get_property (Day 57 question)**

**Decision:** Acceptable. Only 2 concurrent read locks (not 3 as stated — `societies` and `areas` are plain `Vec`, not `RwLock`). Tokio `RwLock` read guards are non-exclusive and cheap. Minor optimization: scope the properties lock to clone-and-find, then drop before acquiring the graph lock. Not urgent.

**1d. Sparkline for zero-interest properties (Day 56 question)**

**Decision:** Yes, render the sparkline for zero-interest properties. Flat line at zero is honest signal.

### 2. Add `root_source` field to Rust Node model

**File:** `backend/src/knowledge/node.rs`

Add an enum and field:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RootSource {
    Rera,       // Government-verified, confidence floor = 1.0
    Seller,     // Self-reported, enrichable but no legal proof
    Discovered, // Live discovery (Gemini), verification pending
    Legacy,     // Pre-Sprint 3 seed data, unclassified
}
```

Add to `Node`:
```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub root_source: Option<RootSource>,
```

The `default` serde attribute means existing JSON files without `root_source` deserialize cleanly as `None`. No migration breaks.

### 3. Python backfill script: set `root_source` on all existing nodes

**File:** `pipeline/skills/backfill_root_source.py` (new)

One-shot script that reads every node file in `data/knowledge/nodes/` and sets `root_source`:

- If node has any fact with `source.source_type == "Rera"` or `skill_id == "fetch_rera"` → `root_source: "rera"`
- If node has any fact with `skill_id == "discover_properties"` and source is `"Google"` → `root_source: "discovered"`
- Else → `root_source: "legacy"`

Expected classification:
- **55 societies** with RERA facts → `"rera"`
- **32 builders** (only `name` + `embedding_computed`) → `"legacy"` (upgraded to `"rera"` when builder enrichment runs)
- **126 properties** → `"legacy"` (seed data)
- **16 areas** → `"legacy"`

Atomic writes: write to `.tmp`, rename. No Rust backend dependency.

Run: `python3 -m pipeline.skills.backfill_root_source`

### 4. Create `classify_project_status` skill

**File:** `pipeline/skills/classify_project_status.py` (new)

Pure-computation skill (no LLM, no network calls, zero cost). Reads existing RERA date facts from KG node files and produces a `project_status` SourcedFact.

**Classification logic** (from vision.md):

```python
today = date.today()

if completion_date and completion_date <= today:
    status = "ready_to_move"
elif start_date and (today - start_date).days <= 365:
    status = "new_launch"
elif completion_date and completion_date > today:
    if original_completion_date and completion_date > original_completion_date:
        status = "delayed"
    else:
        status = "under_construction"
elif start_date and start_date > today:
    status = "upcoming"
else:
    status = None  # insufficient data, skip
```

**Output:** One `project_status` SourcedFact per society, with full self-describing metadata:

```python
SourcedFact(
    key="project_status",
    value={"type": "Text", "data": "ready_to_move"},
    confidence=1.0,
    source=FactSource(source_type="Computed", skill_id="classify_project_status"),
    display_template="Ready to Move — delivered {completion_date}",
    answers_preferences=["ready to move", "ready possession", "completed project",
                         "move in now", "immediate possession", "ready flat"],
    scoring_hint={"direction": "TextMatch", "weight": 3.0},
)
```

Each of the 5 status values has its own `answers_preferences` and `display_template`.

**Date parsing:** Reuse `_parse_rera_date()` from `fetch_rera.py`. Dates are DD/MM/YYYY or DD-MM-YYYY format (inconsistent from RERA portal).

Run: `python3 -m pipeline.skills.classify_project_status`

### 5. Rust backend: auto-exposure via serde

No search or API code changes needed. The `root_source` field serializes automatically in API responses. The `project_status` fact matches via existing `answers_preferences` system — zero search code changes.

### 6. Verify and test

- `cargo check` — Node struct compiles with new `RootSource` enum
- `cargo test` — all existing tests pass (backward compat: missing `root_source` = `None`)
- Run `backfill_root_source.py` — all 229 nodes get `root_source` field
- Run `classify_project_status.py` — societies with RERA dates get `project_status` fact
- Start backend, hit `GET /api/knowledge/nodes/society:prestige-lakeside-habitat` — verify `root_source: "rera"` and `project_status` fact appear
- Spot-check: Prestige Lakeside Habitat has `rera_completion_date: "31/01/2020"` (past) → should be `"ready_to_move"`

---

## File Changes Summary

| File | Action | Reason |
|------|--------|--------|
| `backend/src/knowledge/node.rs` | Modify | Add `RootSource` enum and `root_source` field to `Node` |
| `pipeline/skills/backfill_root_source.py` | Create | One-shot backfill of `root_source` on all 229 nodes |
| `pipeline/skills/classify_project_status.py` | Create | Compute `project_status` from RERA dates, zero-cost skill |
| `frontend/src/pages/SellerDashboardPage.tsx` | Modify | Guard sparkline for empty/single-entry data |

## What This Unblocks (Days 60-72)

- **Day 60-61:** Trust badges UI — can switch on `root_source` (rera/seller/discovered/legacy)
- **Day 62-63:** `seed_from_rera` skill expansion — new societies enter with `root_source: "rera"` + `project_status` from day one
- **Day 64-65:** Builder delivery rate — iterate builder's projects, check `project_status`, compute on-time rate
- **Day 66+:** Builder enrichment, edges, search improvements — all depend on `root_source` and `project_status` existing

## Decisions Made

| Question | Decision | Rationale |
|----------|----------|-----------|
| Sparkline for zero-interest properties? | Yes, show flat line | Honest signal; cannot distinguish "new listing" from "no interest" in seed data |
| 3 concurrent read locks in get_property? | Only 2 locks (not 3). Acceptable. | `societies` and `areas` are plain `Vec`, not `RwLock`. Minor optimization possible but not urgent. |
| `root_source` as field vs fact? | Field on Node struct | Metadata about the node itself, not knowledge. Should not have confidence/version/source — it IS the source. |
| `project_status` as fact vs field? | SourcedFact | Computed knowledge with confidence, display_template, answers_preferences. Follows self-describing pattern. |

## Success Criteria

1. `cargo check` and `cargo test` pass with new `RootSource` enum
2. All 229 node files have `root_source` field after backfill
3. All societies with `rera_completion_date` have a `project_status` fact
4. Prestige Lakeside Habitat: `root_source: "rera"`, `project_status: "ready_to_move"`
5. Frontend sparkline handles empty data without rendering errors
6. No regressions in existing search or property detail flows
