# Day 27: RERA Transparency Tile + Async Cache Architecture

## Dependency

Day 26 must be complete. We need:
- `fetch_rera.py` producing structured RERA facts in the knowledge graph
- Entity resolver linking RERA projects to our society/property nodes
- Area intelligence facts from r/bangalore in the graph

---

## 1. The Problem

Day 26 built the pipeline. Now we need to:
1. **Serve RERA data to the frontend** — fast, from cache, never blocking on scrape
2. **Display it beautifully** — a transparency tile on property detail pages
3. **Think through the async cache model** — cold starts, background refresh, staleness

The key constraint: **RERA scraping is slow (3-5s per project detail page).** It must NEVER be in the request path. The Rust backend serves from its in-memory knowledge graph. The Python pipeline enriches asynchronously. The frontend shows what's available, gracefully handles missing data.

---

## 2. Async Cache Architecture

### 2.1 The Three Layers

```
Layer 1: Disk (source of truth)
  data/knowledge/nodes/society/sobha-insignia.json
  ├─ fact: rera_registered = true (confidence 1.0)
  ├─ fact: rera_completion_date = "2026-03-31"
  ├─ fact: rera_complaints_count = 1
  └─ ... 15+ RERA facts

Layer 2: In-memory KnowledgeGraph (hot cache)
  Loaded at startup from Layer 1
  Behind RwLock — concurrent reads, exclusive writes
  Serves all API requests
  TTL: until restart (or hot-reload, future)

Layer 3: API response cache (per-request)
  InMemoryCache with TTL per key
  Property detail responses cached 5 min
  Search responses cached 1 min
  Prevents redundant graph traversal
```

### 2.2 Cold Start Flow

```
Startup
  ├─ Load KG from disk (Layer 1 → Layer 2)
  │   All RERA facts available immediately if previously fetched
  │
  ├─ If no RERA facts for a society:
  │   Backend serves property detail WITHOUT RERA tile
  │   Frontend shows: "RERA data not yet available"
  │
  └─ Background: pipeline cron enriches entities
      Python fetches RERA → writes to Layer 1
      Next restart (or hot-reload) picks up new data
```

### 2.3 Background Refresh Model

```
Option A: Manual pipeline runs (current, keep for now)
  python3 -m pipeline.orchestrate --type society --stale-only
  Runs nightly or on-demand. Updates Layer 1.
  Backend restart picks up changes.

Option B: Hot-reload endpoint (build today)
  POST /api/admin/reload-knowledge
  Backend re-reads Layer 1 → updates Layer 2 in-place
  No restart needed. Pipeline pushes facts → calls reload.
  Protected by admin token.

Option C: Webhook from pipeline (future)
  Pipeline finishes enrichment → POST /api/knowledge/nodes/{id}/facts
  Backend updates Layer 2 in-memory immediately
  Already partially works via graph_client.py
```

**Today we build Option B** — a hot-reload endpoint. This closes the loop:
Pipeline writes to disk → calls reload → backend serves fresh data → no restart.

### 2.4 Staleness Signals

The frontend needs to know how fresh the data is:

```json
{
  "rera_facts": {
    "registered": true,
    "completion_date": "2026-03-31",
    "complaints_count": 1
  },
  "rera_freshness": {
    "last_fetched": "2026-03-10T14:30:00Z",
    "source": "rera.karnataka.gov.in",
    "confidence": 1.0
  }
}
```

If `last_fetched` is > 30 days ago, frontend can show "Data may be outdated — last verified March 2026".

---

## 3. What We're Building

### Phase A: Backend — RERA Data Serving (1.5 hours)

#### A.1 Property Detail Response Enhancement

Extend the property detail API to include RERA data from the knowledge graph.

In `backend/src/routes/properties.rs`:

```rust
pub struct PropertyDetailResponse {
    // ... existing fields ...
    pub rera: Option<ReraInfo>,  // NEW
    pub area_intelligence: Option<AreaIntelligence>,  // NEW (Day 26 reddit data)
}

#[derive(Serialize)]
pub struct ReraInfo {
    pub registered: bool,
    pub registration_number: Option<String>,
    pub status: Option<String>,           // "Approved", "Expired"
    pub completion_date: Option<String>,   // ISO date
    pub original_completion_date: Option<String>,
    pub delay_months: Option<i32>,
    pub total_units: Option<i32>,
    pub total_project_cost_inr: Option<f64>,
    pub land_cost_inr: Option<f64>,
    pub construction_cost_inr: Option<f64>,
    pub complaints_count: Option<i32>,
    pub complaints_resolved_pct: Option<f64>,
    pub builder_total_projects: Option<i32>,
    pub builder_revocations: Option<i32>,
    pub land_litigation: Option<bool>,
    pub escrow_bank: Option<String>,
    pub carpet_area_sqm: Option<f64>,
    pub lat_lng: Option<String>,
    pub rera_portal_url: Option<String>,   // direct link to RERA page
    pub last_verified: Option<String>,     // ISO datetime
}
```

Implementation:
1. When building PropertyDetailResponse, look up the society's KG node
2. Filter facts where `key.starts_with("rera_")`
3. Map to `ReraInfo` struct
4. If no RERA facts exist → `rera: None`

#### A.2 Area Intelligence in Response

```rust
#[derive(Serialize)]
pub struct AreaIntelligence {
    pub safety: Option<String>,
    pub commute_reality: Option<String>,
    pub water_supply: Option<String>,
    pub noise_level: Option<String>,
    pub green_cover: Option<String>,
    pub community_vibe: Option<String>,
    pub recurring_complaints: Vec<String>,
    pub hidden_gems: Vec<String>,
    pub grocery_shopping: Option<String>,
    pub healthcare_access: Option<String>,
    pub school_quality: Option<String>,
    pub last_updated: Option<String>,
    pub source_count: i32,  // number of Reddit threads analyzed
}
```

Same pattern: read area node facts, map to struct. If no facts → `None`.

#### A.3 Hot-Reload Endpoint

```rust
// POST /api/admin/reload-knowledge
// Header: X-Admin-Token: {ADMIN_TOKEN env var}
async fn reload_knowledge(state: ...) -> impl IntoResponse {
    let new_graph = knowledge::store::load_graph(&state.project_root);
    let mut graph = state.knowledge.write().await;
    *graph = new_graph;
    // Clear response cache
    state.cache.clear().await;
    Json(json!({"status": "reloaded", "nodes": graph.nodes.len()}))
}
```

This is simple but powerful. Pipeline writes to disk → POST reload → backend is fresh.

### Phase B: Frontend — RERA Transparency Tile (2 hours)

#### B.1 TypeScript Types

In `frontend/src/lib/types.ts`:

```typescript
interface ReraInfo {
  registered: boolean;
  registration_number?: string;
  status?: string;
  completion_date?: string;
  original_completion_date?: string;
  delay_months?: number;
  total_units?: number;
  total_project_cost_inr?: number;
  land_cost_inr?: number;
  construction_cost_inr?: number;
  complaints_count?: number;
  complaints_resolved_pct?: number;
  builder_total_projects?: number;
  builder_revocations?: number;
  land_litigation?: boolean;
  escrow_bank?: string;
  rera_portal_url?: string;
  last_verified?: string;
}

interface AreaIntelligence {
  safety?: string;
  commute_reality?: string;
  water_supply?: string;
  noise_level?: string;
  green_cover?: string;
  community_vibe?: string;
  recurring_complaints: string[];
  hidden_gems: string[];
  school_quality?: string;
  last_updated?: string;
  source_count: number;
}

interface PropertyDetailResponse {
  // ... existing ...
  rera?: ReraInfo;
  area_intelligence?: AreaIntelligence;
}
```

#### B.2 RERA Transparency Tile Component

`frontend/src/components/ReraTile.tsx`

Design:

```
┌──────────────────────────────────────────────────┐
│  RERA Verification                     ✓ Verified │
│                                                   │
│  ┌──────────┐  ┌──────────┐  ┌──────────────────┐│
│  │ Reg No   │  │ Status   │  │ Completion       ││
│  │ PRM/...  │  │ Approved │  │ Mar 2026         ││
│  │          │  │          │  │ (on track)        ││
│  └──────────┘  └──────────┘  └──────────────────┘│
│                                                   │
│  ┌──────────┐  ┌──────────┐  ┌──────────────────┐│
│  │ Units    │  │ Cost     │  │ Complaints       ││
│  │ 33       │  │ ₹76.2 Cr │  │ 1 filed          ││
│  │          │  │          │  │ (0 against this)  ││
│  └──────────┘  └──────────┘  └──────────────────┘│
│                                                   │
│  Builder Track Record                             │
│  ○ 42 RERA projects across 7 states               │
│  ○ 0 revocations                                  │
│  ○ No land litigation                             │
│                                                   │
│  ┌─────────────────────────────────────────────┐  │
│  │ ⚠ Cost Transparency                        │  │
│  │ Land: ₹14.9 Cr (20%) | Build: ₹61.4 Cr   │  │
│  │ Total: ₹76.2 Cr for 33 units               │  │
│  │ Per-unit avg: ₹2.31 Cr (RERA filing)       │  │
│  └─────────────────────────────────────────────┘  │
│                                                   │
│  Last verified: 10 Mar 2026                       │
│  [View on RERA Portal →]                          │
└───────────────────────────────────────────────────┘
```

Key design decisions:
- **Green header** when verified, yellow when "verification pending", red when "not found"
- **Delay indicator**: if delay_months > 0, show warning badge with "X months behind original schedule"
- **Cost transparency block**: shows land vs construction cost split — this is data NO other platform shows
- **Per-unit average cost**: total_project_cost / total_units — compare with listing price to understand builder margin
- **Graceful empty state**: if `rera` is null, show muted tile: "RERA verification pending — check back soon"

#### B.3 Area Intelligence Tile Component

`frontend/src/components/AreaIntelligenceTile.tsx`

Design:

```
┌──────────────────────────────────────────────────┐
│  Area Intelligence: Whitefield                    │
│  Based on 47 Reddit discussions                   │
│                                                   │
│  Safety          ████████░░  Good — well patrolled│
│  Commute         ██████░░░░  30-45 min to CBD     │
│  Water Supply    ███████░░░  Reliable, Cauvery    │
│  Green Cover     ████████░░  Parks, lake nearby   │
│  Walkability     ████░░░░░░  Car-dependent        │
│                                                   │
│  What residents love:                             │
│  • Great schools (Inventure, TISB)                │
│  • Food scene improving rapidly                   │
│  • Metro finally operational                      │
│                                                   │
│  What to watch out for:                           │
│  • Waterlogging near lake during monsoon           │
│  • Construction noise in many pockets             │
│  • Traffic on main road peak hours                │
│                                                   │
│  Source: r/bangalore · Updated 10 Mar 2026        │
└───────────────────────────────────────────────────┘
```

#### B.4 Integration into PropertyPage

In `PropertyPage.tsx`, add both tiles after the existing content:

```tsx
{/* After existing property details, before similar properties */}

{detail.rera && <ReraTile rera={detail.rera} />}
{!detail.rera && <ReraPendingTile />}

{detail.area_intelligence && (
  <AreaIntelligenceTile
    area={detail.area}
    intelligence={detail.area_intelligence}
  />
)}
```

### Phase C: Async Pipeline Integration (30 min)

#### C.1 Pipeline → Backend Hot-Reload

After `orchestrate.py` enriches an entity, it calls the reload endpoint:

```python
# In pipeline/orchestrate.py, after writing facts to disk:
def notify_backend():
    """Tell the Rust backend to reload the knowledge graph."""
    try:
        req = Request(
            "http://localhost:4000/api/admin/reload-knowledge",
            method="POST",
            headers={"X-Admin-Token": os.environ.get("ADMIN_TOKEN", "dev")}
        )
        urlopen(req, timeout=5)
        logger.info("Backend reloaded knowledge graph")
    except Exception as e:
        logger.warning("Backend reload failed (may not be running): %s", e)
```

#### C.2 End-to-End Test

```bash
# 1. Run pipeline for one entity
python3 -m pipeline.orchestrate --entity society:sobha-insignia

# 2. Verify facts on disk
cat data/knowledge/nodes/society/sobha-insignia.json | jq '.facts[] | select(.key | startswith("rera_"))'

# 3. Check backend serves RERA data
curl http://localhost:4000/api/properties/discovered-sobha-insignia-3bhk | jq '.rera'

# 4. Open in browser
open http://localhost:5173/property/discovered-sobha-insignia-3bhk
# → RERA tile should be visible with real data
```

---

## 4. Scope

| Phase | What | Time |
|-------|------|------|
| **A** | Backend: RERA + area data serving, hot-reload endpoint | 1.5 hours |
| **B** | Frontend: RERA tile, area intelligence tile, integration | 2 hours |
| **C** | Pipeline → backend integration, end-to-end test | 30 min |

---

## 5. Files

### New (Frontend)
- `frontend/src/components/ReraTile.tsx` — RERA transparency tile
- `frontend/src/components/AreaIntelligenceTile.tsx` — area intelligence tile

### Modified (Backend)
- `backend/src/routes/properties.rs` — add `rera` and `area_intelligence` to detail response
- `backend/src/routes/mod.rs` — add admin reload route
- `backend/src/main.rs` — register reload endpoint

### Modified (Frontend)
- `frontend/src/lib/types.ts` — add ReraInfo, AreaIntelligence types
- `frontend/src/pages/PropertyPage.tsx` — integrate both tiles

### Modified (Pipeline)
- `pipeline/orchestrate.py` — add backend reload notification

---

## 6. What NOT to Build Today

- Full RERA listing import — that's a batch job, not a product surface
- RERA score computation — feeds into existing scores, no standalone number
- Database — files work, don't prematurely optimize storage
- Cron scheduler — manual pipeline runs are fine
- RERA certificate PDF download — just link to the portal
- Builder profile page — future feature, not today

---

## 7. Success Criteria

- [ ] Property detail API returns `rera` object with 15+ fields when RERA data exists
- [ ] Property detail API returns `rera: null` gracefully when no RERA data
- [ ] Area intelligence served from KG facts for enriched areas
- [ ] Hot-reload endpoint works: POST → backend re-reads KG from disk
- [ ] RERA tile renders with real Sobha Insignia data
- [ ] RERA tile shows cost breakdown (land vs construction) — unique to OpenEstates
- [ ] Area intelligence tile shows Reddit-sourced insights
- [ ] Empty states for both tiles are clean and informative
- [ ] `cargo check` + `cargo test` pass
- [ ] `npm run build` succeeds
- [ ] End-to-end: pipeline enriches → hot-reload → frontend shows fresh data

---

## 8. The Principle

The async model is simple:
- **Pipeline is the slow brain** — fetches, analyzes, enriches. Minutes, not milliseconds.
- **Backend is the fast mouth** — serves what the graph knows. Milliseconds.
- **Frontend is the clear window** — shows what's available, honest about what's missing.

No request ever waits for a scrape. No tile lies about its freshness. No data appears without provenance.

This is what "transparency-first" looks like at the infrastructure level: the system is honest about what it knows, when it learned it, and how confident it is.

---

## 9. Architecture After Day 27

```
Python Pipeline (slow, async, batch)
  ├─ RERA scraper → structured facts (confidence 1.0)
  ├─ Reddit fetcher → raw threads
  ├─ Claude synthesis → area/society intelligence
  ├─ Entity resolver → canonical linking
  └─ orchestrate.py → writes to disk → calls hot-reload

  ↓ (disk: data/knowledge/nodes/)

Rust Backend (fast, sync, in-memory)
  ├─ KnowledgeGraph loaded from disk at startup
  ├─ Hot-reload endpoint for live updates
  ├─ Property detail API: base data + RERA + area intelligence
  ├─ Search API: text + semantic + graph scoring
  └─ Response cache (TTL 5 min)

  ↓ (HTTP JSON)

React Frontend
  ├─ PropertyPage: existing details + RERA tile + area tile
  ├─ Graceful empty states for missing data
  └─ Freshness indicators (last verified date)
```

The system is eventually consistent, explicitly so, and transparent about it.
