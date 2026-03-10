# OpenEstates Cleanup Plan

Last updated: 2026-03-09

This document identifies dead code, misplaced logic, and cleanup opportunities in the codebase. It is a plan, not a mandate. Each section explains what to do, why, and what (if anything) to preserve.

---

## 1. Dead Code to Remove

### 1.1 `agents/` directory — DONE (deleted Day 21)

**Status: DELETED**

### 1.2 `simulation/` directory — DONE (deleted Day 21)

**Status: DELETED**

### 1.3 `engine/` directory — ACTIVE CODE, NOT DEAD

**Status: KEEP**

**Correction (Day 21):** The original assessment was written when `engine/` only had stubs. It now contains real, active scoring modules: `dimensions.py`, `ranker.py`, `scorer.py`, `vector_search.py`, `types.py`. These are used by the pipeline for multi-dimensional scoring and vector search. This is NOT dead code.

### 1.4 `src/` directory (if it exists)

**Status:** Does not exist in current tree. Was the old click CLI package. Already removed.

---

## 2. Pipeline Scripts to Refactor

### 2.1 `pipeline/agent.py` (41KB)

**Verdict: KEEP but isolate**

This is the day-planning agent that orchestrates ChatGPT (for plans) and Claude (for coding). It is a meta-tool, not part of the product pipeline. It should stay in `pipeline/` but be understood as a development tool, not a product component.

**No action needed** unless the file grows further. Consider moving to `tools/agent.py` in the future.

### 2.2 `pipeline/chatgpt_client.py`

**Verdict: KEEP (development tool)**

Browser automation for ChatGPT. Used by `agent.py`. Not a product component.

### 2.3 `pipeline/brainstorm_day19.py` and `pipeline/brainstorm_search.py` — DONE (deleted Day 21)

**Status: DELETED** (along with `pipeline/migrate_to_lake.py`)

### 2.4 `pipeline/smoke_test.py` and `pipeline/smoke_test_api.py`

**Verdict: KEEP**

These test the backend API and frontend rendering. Useful for CI/CD.

### 2.5 `pipeline/journey_property_to_shortlist.py`, `pipeline/review_capture.py`, `pipeline/review_journey.py`, `pipeline/run_review_gate.py`, `pipeline/check_render_truth.py`, `pipeline/capture_deployed_pages.py`

**Verdict: KEEP but consolidate**

These are review/testing tools that verify the frontend renders correctly. They are useful but scattered. Consider consolidating into a `pipeline/review/` subdirectory:

```
pipeline/review/
  capture_pages.py
  check_render.py
  review_journey.py
  review_gate.py
```

### 2.6 Core pipeline scripts (KEEP as-is)

These are the production pipeline:
- `pipeline/society_discovery.py` -- discovers real societies via Claude
- `pipeline/reddit_enrichment.py` -- fetches and synthesizes Reddit threads
- `pipeline/fetch_society_photos.py` -- fetches society photos
- `pipeline/society_scorer.py` -- scores and ranks societies

These should eventually conform to the `BaseCrawler` interface defined in `docs/architecture_v2.md`, but refactoring them is not urgent.

---

## 3. Backend Code Needing Storage Abstraction

### 3.1 `backend/src/data_loader.rs`

**Current state:** Hardcoded to load from `data/seed/` directory at startup. Uses `load_json()` which panics on missing files.

**Needed:**
- Graceful handling of missing files (return empty vec, log warning)
- Support loading from `data/intelligence/` for warm data
- Eventually: support loading from S3 or database

**Priority: LOW** -- the current approach works fine for the seed data size.

### 3.2 `backend/src/routes/societies.rs`

**Current state:** Reads `_ranked_results.json` from disk on every request. The file path is hardcoded to `data/intelligence/whitefield/`.

**Needed:**
- Accept area parameter: `GET /api/societies/search?q=...&area=whitefield`
- Look up the correct `_ranked_results.json` based on area
- Cache the parsed JSON in memory (re-read only when file mtime changes)

**Priority: MEDIUM** -- this blocks supporting multiple areas.

### 3.3 `backend/src/routes/shortlist.rs`

**Current state:** Returns a hardcoded stub response.

**Needed:**
- Accept POST to add/remove from shortlist (in-memory for now)
- Or: keep shortlist entirely client-side (current frontend approach via zustand)
- Decide whether shortlist is server-side or client-side

**Priority: LOW** -- frontend already handles shortlist locally.

---

## 4. Frontend Files to Clean Up

### 4.1 `frontend/src/pages/ResultsPageA.tsx` and `ResultsPageB.tsx`

**Verdict: Investigate**

These appear to be A/B test variants of the results page alongside the main `ResultsPage.tsx`. If the experiment is concluded, remove the unused variant(s).

### 4.2 `frontend/src/lib/sample-data.ts`

**Verdict: REMOVE once API is reliable**

Contains hardcoded sample data for offline development. Once the backend API is the sole source of truth, this file should be removed to avoid confusion about where data comes from.

**Priority: LOW** -- useful during development.

---

## 5. Data Files to Clean Up

### 5.1 `data/intelligence/societies/` (legacy path)

**Status:** Contains per-society photo data at the old path (`data/intelligence/societies/{slug}/photos.json`).

**Needed:** Verify whether any code reads from this path. If all code now reads from `data/intelligence/{area}/{slug}/photos.json`, delete the legacy directory.

```bash
# Check if anything references the old path
grep -r "intelligence/societies" pipeline/ backend/ frontend/ --include="*.py" --include="*.rs" --include="*.ts"
```

### 5.2 `data/seed/upcoming_launches.json`

**Status:** Untracked file. Likely created by a day 17 pipeline for sponsored launches feature.

**Action:** Either add to git or add to .gitignore, depending on whether it is curated seed data or generated.

---

## 6. Cleanup Priority Order

1. ~~**Now (5 minutes):** Delete `agents/`, `simulation/`, brainstorm scripts~~ **DONE (Day 21)**
2. **Soon:** Fix hardcoded Whitefield path in `societies.rs`
3. **When convenient:** Consolidate review/test scripts into `pipeline/review/`
4. **Eventually:** Refactor pipeline scripts to use `BaseCrawler` interface
5. **When stable:** Remove `sample-data.ts`, decide on shortlist architecture

## 7. What NOT to delete

- `pipeline/agent.py` and `pipeline/chatgpt_client.py` -- development tools, still actively used
- `pipeline/smoke_test*.py` -- testing tools
- `data/seed/*` -- curated baseline data
- Any `days/*.md` or `docs/*.md` -- learning artifacts
- `LEARNING.md` -- project learnings
