# Livability Brief & Community Pulse — Implementation Plan

**Status:** Draft (approved direction, not yet implemented)  
**Last updated:** 2026-07-19  
**Owners:** OpenEstates discovery / evidence surfaces

---

## 1. Problem statement

Buyers evaluating a society need more than listing copy or a Google rating. They need a **receipt-backed livability read** that connects:

- how you actually reach the gate (approach road)
- what daily life costs in friction (maintenance, water, STP, lifts, parking)
- what environmental and infrastructure risks brochures omit (waterlogging, rajakaluve, HT wires, construction dust)
- what positive signals justify the price (greenery, clubhouse upkeep, school access, mature resident community)
- whether the asset is best understood as **end-use livability** vs **price-per-sqft speculation** (lifecycle, resale inventory, rental demand)

Today we have pieces of this scattered across evidence folds, community pulse, and mined Google/Reddit facts — but no unified **diligence brief** and the community paragraph still tends to paraphrase reviews instead of chaining verified signals.

---

## 2. Product promise

> Show homes with receipts. Help buyers judge **verified livability**, not only brand name or quoted price.

### Two complementary surfaces

| Surface | Job | Length | Sources |
|---------|-----|--------|---------|
| **Community pulse** | What residents say — qualitative signal + themes + quote receipts | Short (badge + optional 1–2 sentence living read) | Google reviews, Reddit |
| **Livability brief** | What a diligent buyer should verify before shortlisting | 4 blocks (~250–400 words total) | Structured DAG facts + review-mined signals + Reddit themes |

**Hard rules (both surfaces):**

- No numeric scores in UI (`3.9/5`, `78/100`, `% confidence`, risk fill bars).
- No review quotes inside summary paragraphs — quotes live only in receipt cards below.
- Every sentence must trace to a fact key, mined theme hit, or explicit “verify on visit” gap marker.
- No LLM calls on `/api/search` or property detail request path. Compose offline or at bundle materialization; serve from Rust memory.

---

## 3. Target UX copy (reference)

### Livability brief — four blocks

**Block 1 — Operating quality**  
This society is best understood as a livability-first gated community rather than just a price-per-sqft asset. Buyers should look closely at daily operating quality: maintenance charges, lift uptime, water source, tanker dependence, STP handling, waste management, parking pressure, and how well the association responds to complaints.

**Block 2 — Risk signals**  
The biggest risk signals to verify are waterlogging around approach roads, rajakaluve or stormwater-drain proximity, high-tension wire buffers, basement seepage, sewage smell near STP areas, and whether nearby construction creates dust/noise. These issues may not show up in brochure material but can materially affect resale value, tenant demand, and day-to-day comfort.

**Block 3 — Positive signals**  
The positive signals to look for are stable water supply, transparent association finances, well-maintained common areas, functional amenities, good security, clean internal roads, low noise, reliable power backup, and easy access to schools, offices, groceries, hospitals, and main roads. Societies with mature resident communities and predictable monthly expenses tend to feel safer for both end-use and rental investors.

**Block 4 — Judgment frame**  
Overall, this society should be judged on verified livability, not only brand name or quoted price. Before shortlisting, confirm recent resident feedback, maintenance cost trend, water source, flooding history, legal approvals, OC/CC/RERA status, and any visible environmental or infrastructure risks around the project.

> **Note:** The above is the *shape* of the output. Actual text is **composed deterministically** from evidenced themes and facts — not pasted as static copy.

### Community pulse (unchanged direction)

```
Community pulse
Google review · Mixed-positive          ← qualitative badge only

[Optional 1–2 sentence living read — graph/theme chained, no quotes]

From reviews
Residents like   greenery · clubhouse · connectivity
Worth checking   traffic · STP smell

[Quote cards — raw crawled text]
[Source links]
```

---

## 4. What already exists

| Asset | Location | Role |
|-------|----------|------|
| Buyer context sections | `data/product/buyer_context_sections.json` | Evidence fold taxonomy: lifecycle, approach road, waterlogging, surroundings |
| Google review crawl | `pipeline/skills/fetch_google_review_links.py` | `google_review_snippets`, rating, review URL |
| Review signal extraction | `backend/src/assets/google.rs` | Mines `approach_road_condition`, `stp_concern`, `high_tension_wire_concern` from snippets |
| Community summarizer | `backend/src/community.rs` | Theme ranker + paragraph composer + `CommunityPulse` types |
| Community facts asset | `backend/src/assets/community.rs` | Emits `community_review_summary`, theme tags, highlights |
| Reddit search skill | `pipeline/skills/search_reddit.py` | Public JSON API; handles `RedditSourceBlocked` |
| Reddit DAG assets | `backend/src/assets/reddit.rs` | `reddit_threads_daily`, resident facts materialization |
| Reddit theme mining (offline) | `pipeline/skills/mine_reddit_intent_themes.py` | Schema discovery helper |
| Property evidence builder | `backend/src/routes/properties.rs` | `collect_community_evidence_records`, source panels |
| Community pulse UI | `frontend/src/components/evidence/CommunityPulseCard.tsx` | Badge, paragraph, themes, quotes |

### Gaps

1. Community paragraph still appends `"One recurring note: \"...\""` from review text — must remove.
2. Evidence collection is society-scoped only — area and approach-road entities not merged for theme mining.
3. Theme registry has 8 society themes — needs expansion to full livability diligence lens.
4. No `LivabilityBrief` type, composer, API field, or UI surface.
5. Reddit fetch blocked from Arca egress IP — needs isolated fetcher worker.
6. No dedicated Google crawl for **approach road place** reviews (gate / last-mile road).

---

## 5. Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│ OFFLINE (Python pipeline + Rust asset materialization)                  │
├─────────────────────────────────────────────────────────────────────────┤
│ Crawl                                                                   │
│   society Google reviews ──┐                                            │
│   approach-road Google  ───┼──► CommunityEvidenceRecord (scoped)        │
│   area / road Reddit    ───┘                                            │
│                                                                         │
│ Mine                                                                    │
│   LivabilityThemeRanker ──► ScopedThemeHit[] per entity                 │
│   Structured facts     ──► lifecycle, schools, waterbody distance     │
│                                                                         │
│ Compose (Rust, deterministic)                                           │
│   LivabilityBriefComposer ──► 4 blocks + receipt map                    │
│   CommunityPulseComposer  ──► short living read (no quotes)             │
│                                                                         │
│ Materialize                                                             │
│   serving bundle facts: livability_brief, community_pulse_*             │
└─────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────┐
│ RUNTIME (Rust API — read only)                                          │
│   GET /api/properties/{id}  →  property.livability_brief                │
│                            →  evidence.community_pulse                  │
└─────────────────────────────────────────────────────────────────────────┘
```

**Boundary discipline:**

- Python: crawl, normalize, optional offline enrichment.
- Rust: deterministic composition, ranking, API serving.
- No network I/O on property page request path.

---

## 6. Data model

### 6.1 Livability theme registry

New file: `data/product/livability_theme_registry.json`

Each theme entry:

```json
{
  "key": "tanker_dependence",
  "label": "tanker dependence",
  "lens": "operating",
  "polarity": "concern",
  "scopes": ["society", "area"],
  "terms": ["tanker", "water tanker", "tanker dependency"],
  "evidence_queries": ["summer water shortage tanker dependency borewell"],
  "min_hits": 2,
  "source_types": ["Google", "Reddit"]
}
```

#### Lenses

| Lens | `lens` key | Example themes |
|------|------------|----------------|
| Operating quality | `operating` | maintenance charges, lift reliability, water source, summer shortage, tanker dependence, STP smell/functioning, waste management, parking pressure, association responsiveness |
| Risk signals | `risk` | approach-road waterlogging, rajakaluve/storm drain, HT wire buffers, basement seepage, sewage smell, construction dust/noise, empty unsold towers, flooding history |
| Positive signals | `positive` | stable water supply, transparent association finances, maintained common areas, functional amenities, security, clean internal roads, low noise, power backup, school/office/grocery access |
| Lifecycle / investment frame | `lifecycle` | delivered vs under construction, built year, resale inventory, rental demand, “understand vs ready to move” |

#### Structured facts (not theme-mined — DAG-backed)

| Fact key | Lens | Scope |
|----------|------|-------|
| `home_state` | lifecycle | society |
| `home_age_bucket` | lifecycle | society |
| `home_timeline_state` | lifecycle | society |
| `road_width` | risk / positive | road_segment |
| `access_road_quality` | risk | road_segment |
| `waterlogging_detail` | risk | area |
| `waterlogging_risk` | risk | area |
| `nearest_lake_distance_m` | risk | waterbody |
| `lake_waterlogging_context` | risk | waterbody |
| `nearby_schools` | positive | poi |
| `stp_concern` | risk | society (review signal) |
| `high_tension_wire_concern` | risk | society (review signal) |
| `approach_road_condition` | risk / positive | society / road (review signal) |

### 6.2 Scoped theme hit

```rust
pub struct ScopedThemeHit {
    pub theme_key: String,
    pub label: String,
    pub lens: LivabilityLens,
    pub polarity: ThemePolarity,
    pub scope: String,           // "society" | "area" | "road_segment"
    pub scope_label: String,     // society name, area name, road name
    pub hit_count: u32,
    pub source_types: Vec<String>,
    pub fact_keys: Vec<String>,  // receipt lineage
}
```

### 6.3 LivabilityBrief (API)

```rust
pub struct LivabilityBriefBlock {
    pub lens: String,            // "operating" | "risk" | "positive" | "judgment"
    pub title: String,
    pub paragraph: String,
    pub themes: Vec<String>,     // chips for drill-down
    pub fact_keys: Vec<String>,  // internal lineage (optional in API)
}

pub struct LivabilityBrief {
    pub blocks: Vec<LivabilityBriefBlock>,
    pub lifecycle_flag: Option<String>,  // e.g. "livability-first" | "speculative-asset"
    pub source_urls: Vec<String>,
    pub confidence_label: String,        // "Strong proof" | "Directional" — qualitative only
}
```

### 6.4 CommunityPulse (refined)

```rust
pub struct CommunityPulse {
    pub review_signal: String,   // "Google review · Mixed-positive"
    pub living_read: String,     // short theme-chained read, NO quotes
    pub positives: Vec<String>,
    pub concerns: Vec<String>,
    pub quotes: Vec<CommunityPulseQuote>,
    pub source_urls: Vec<String>,
}
```

Remove from buyer UI: `confidence_pct`, duplicate `source_label` + `sentiment_band` if `review_signal` subsumes them.

---

## 7. Composer logic

### 7.1 LivabilityBriefComposer (new: `backend/src/livability_brief.rs`)

**Input:** `LivabilityBriefInput` assembled in `properties.rs` from serving bundle + graph projection.

**Steps:**

1. **Collect evidence** across scopes:
   - society: Google snippets, community themes, STP/HT/approach signals
   - area: waterlogging facts, area Reddit, locality review snippets (future)
   - road_segment: approach road place reviews (future), `road_width`, `access_road_quality`

2. **Mine themes** using registry (reuse `CommunityThemeRanker` pattern, generalized).

3. **Merge structured facts** into lens buckets (lifecycle facts always populate judgment block).

4. **Rank** top N themes per lens by `hit_count × source_confidence`, deprioritize duplicates across scopes.

5. **Compose** each block:
   - Opening sentence from lifecycle context when available.
   - Body lists evidenced themes as natural language (join with commas / “and”).
   - Missing lens → omit block or use thin-evidence copy: “No recurring resident signal on X yet — verify on visit.”

6. **Emit receipt map** for each sentence (fact_keys + theme_keys) for admin/debug.

**Constraints:**

- Max ~100 words per block (~400 total).
- Never embed review quote text.
- Never emit numeric scores.

### 7.2 CommunityPulseComposer (refactor `compose_community_paragraph`)

- Input: scoped theme hits from society (+ area when available).
- Output: 1–2 sentences chaining scopes: “Inside {society}… Around {road/area}…”
- **Delete** the `"One recurring note:"` quote append path entirely.

### 7.3 Lifecycle flag: “understand vs ready to move”

Derived deterministically:

| Signals | Flag |
|---------|------|
| `home_state` = delivered, age > 5y, resale listings present | `livability-first` |
| `home_state` = under construction, builder brand strong, thin review text | `understand-before-you-buy` |
| High rental theme hits + mature age | `rental-viable` |

Used in judgment block opening sentence only when evidenced.

---

## 8. Crawl & enrichment plan

### 8.1 Society Google reviews (exists)

- Skill: `fetch_google_review_links`
- Facts: `google_review_snippets`, `google_rating`, `google_reviews_url`
- Signals mined in `google.rs`: approach road, STP, HT wires

### 8.2 Approach road Google reviews (new — Phase 2)

**Goal:** Separate last-mile context: waterlogging, traffic, busy hours, distance from main road.

**Approach:**

1. Resolve approach road place query from society geo + road name (or manual seed).
2. Crawl Google place reviews for that road segment entity (not society place).
3. Emit facts on `road_segment:{id}`:
   - `approach_road_review_snippets`
   - `approach_road_traffic_signal`
   - `approach_road_waterlogging_signal`
   - `approach_road_distance_from_main_road` (when extractable from text)

**Skill candidate:** `fetch_approach_road_reviews` in `pipeline/skills/`.

### 8.3 Reddit (expand — Phase 3)

**Subreddits:** `bangalore`, `BangaloreRealEstates`, `IndianRealEstate` (configurable).

**Queries per society:** `{society_name} {area_name}`, `{builder} {project}`.

**Facts emitted:**

- `reddit_threads`, `reddit_thread_count`
- `reddit_resident_discussion` (text)
- `reddit_concern_themes`, `reddit_positive_themes` (after theme miner)

Reddit is especially valuable for: maintenance burden, tanker dependence, association disputes, STP smell, lift issues, empty towers.

### 8.4 Schools (structured + thematic)

- Structured: existing POI / `nearby_schools` facts.
- Thematic: mine “school access”, “good schools nearby” from reviews.
- Brief positive block mentions schools only when **both** POI fact or repeated theme evidence exists.

---

## 9. Reddit fetcher container (isolated egress)

### Problem

Reddit public JSON API returns HTTP 403 from Arca/datacenter egress (`RedditSourceBlocked` in `search_reddit.py`). Main pipeline must not depend on Reddit calls from that IP.

### Solution

Isolated **reddit-fetcher** worker with different egress IP; writes lake artifacts only.

```
┌──────────────────┐      parquet/json       ┌─────────────────┐
│ reddit-fetcher   │ ──────────────────────► │ data/lake/      │
│ (Fargate/EC2)    │                         │ reddit_threads  │
└──────────────────┘                         └────────┬────────┘
                                                      │
                                                      ▼
                                            Arca pipeline (consumer only)
                                            reddit_resident_facts asset
```

### Deliverables

```
pipeline/docker/reddit-fetcher/
  Dockerfile
  fetch_batch.py          # reads society queue, calls search_reddit
  README.md               # deploy + cron instructions
```

**Runtime config:**

- `REDDIT_OUTPUT_URI` → lake path (local FS or S3-compatible)
- `SOCIETY_QUEUE_PATH` → manifest of entities to fetch
- `REDDIT_RATE_LIMIT_SECS=2`
- `REDDIT_SUBREDDITS=bangalore,BangaloreRealEstates`

**Deploy options (preference order):**

1. AWS Fargate scheduled task (daily) with NAT gateway egress
2. Small EC2/Lightsail cron (~$5/mo)
3. GitHub Actions scheduled workflow (hackathon fallback — less reliable)

**Compliance:**

- Public JSON endpoint only; proper User-Agent
- No logged-in scraping
- Cache results; treat blocks as infra alerts, not zero-data

---

## 10. UI plan

### 10.1 Property page

New section: **Livability brief** (above or below evidence stack)

```
┌─────────────────────────────────────────────┐
│ Livability brief          Strong proof      │
├─────────────────────────────────────────────┤
│ [Block 1 paragraph]                         │
│ Operating themes: maintenance · water · STP │
├─────────────────────────────────────────────┤
│ [Block 2 paragraph]                         │
│ Risks to verify: waterlogging · HT wires    │
├─────────────────────────────────────────────┤
│ [Block 3 paragraph]                         │
│ Positives: greenery · school access         │
├─────────────────────────────────────────────┤
│ [Block 4 paragraph]                         │
│ Lifecycle: Delivered · 7+ years             │
└─────────────────────────────────────────────┘
```

Clicking a theme chip scrolls to matching evidence fold (`approach_road`, `waterlogging_context`, `surroundings`, `community`).

### 10.2 Community pulse card fixes (Phase 1)

- Remove duplicate badges (fold header vs card header).
- Section label **“From reviews”** for theme chips + quotes.
- CSS: theme row `grid-template-columns: 7.5rem 1fr; align-items: start`.

### 10.3 Hero compressed read (optional Phase 4)

- First sentence of livability brief or dedicated `living_read` on hero.
- Max 2 lines.

---

## 11. Implementation phases

### Phase 1 — Foundation (3–4 days)

**Goal:** Fix community pulse; lay composer + registry groundwork.

| Task | Files |
|------|-------|
| Add `livability_theme_registry.json` | `data/product/` |
| Generalize theme ranker to read registry | `backend/src/community.rs` or new `livability_themes.rs` |
| Remove quote excerpt from community paragraph | `backend/src/community.rs` |
| Widen evidence collection to area scope | `backend/src/routes/properties.rs` |
| Refine `CommunityPulse` API + UI | `community.rs`, `CommunityPulseCard.tsx`, `evidence.css` |
| Tests for no-quote paragraph + scoped themes | `backend/src/community.rs` tests |

**Exit criteria:**

- Community pulse shows qualitative badge, theme-chained `living_read`, quotes only below.
- Theme chips layout fixed.
- `cargo test --lib` green; `tsc --noEmit` green.

### Phase 2 — Livability brief composer (4–5 days)

| Task | Files |
|------|-------|
| `LivabilityBrief` types + composer | `backend/src/livability_brief.rs` |
| Wire into property detail response | `backend/src/routes/properties.rs`, `models/` |
| Materialize `livability_brief` fact in serving bundle (optional v1: compute at request from cached facts) | `backend/src/assets/community.rs` or new asset |
| `LivabilityBriefCard` component | `frontend/src/components/evidence/` |
| Property page integration | `PropertyPage.tsx` |
| Contract tests | `backend/tests/livability_brief_contract.rs` |

**Exit criteria:**

- Property page shows 4-block brief for societies with Google review evidence.
- Each block omits themes without evidence (no hallucination).
- Receipt lineage available in admin/debug endpoint.

### Phase 3 — Approach road crawl (3–4 days)

| Task | Files |
|------|-------|
| `fetch_approach_road_reviews` skill | `pipeline/skills/` |
| Road segment entity + facts in DAG | pipeline materialization |
| Mine traffic / waterlogging / main-road distance signals | `backend/src/assets/google.rs` |
| Feed into livability brief risk + operating blocks | `livability_brief.rs` |

**Exit criteria:**

- At least one pilot society shows approach-road-specific risk language in brief.
- Facts appear in `approach_road` evidence fold.

### Phase 4 — Reddit fetcher container (3–4 days)

| Task | Files |
|------|-------|
| Docker image + batch fetch script | `pipeline/docker/reddit-fetcher/` |
| Deploy scheduled worker (Fargate or EC2) | infra README |
| Arca pipeline consumes lake artifacts only | `collect_asset_sources.py`, reddit assets |
| Merge Reddit themes into brief | `livability_brief.rs` |

**Exit criteria:**

- Daily Reddit snapshot lands in lake without Arca egress.
- Societies with Reddit threads show Reddit-sourced themes in brief (labeled).
- `RedditSourceBlocked` no longer blocks local dev on Arca (fetcher is remote).

### Phase 5 — Polish & measurement (2–3 days)

| Task | Notes |
|------|-------|
| Hero compressed living read | 1–2 lines from brief |
| Theme chip → evidence fold scroll | UX linking |
| Search quality fixtures | `data/search/` — expected brief themes per query |
| Admin data-health | count societies with brief coverage |

---

## 12. Testing strategy

### Unit tests (Rust)

- Theme miner: given snippet corpus → expected lens/polarity hits
- Composer: given `LivabilityBriefInput` → stable paragraph output (snapshot tests)
- No quote leakage into `living_read` or brief blocks
- Word limits enforced per block

### Contract tests

- `livability_brief_contract.rs`: property API returns brief when society has review facts
- `community_summary_asset_contract.rs`: theme facts align with registry keys
- `reddit_asset_contract.rs`: thread snapshot → resident facts

### Manual QA checklist

- [ ] Society with rich Google reviews → 4 brief blocks populated
- [ ] Society with rating only → thin copy, no invented themes
- [ ] STP mention in reviews → `stp_concern` in risk block
- [ ] HT wire mention → surroundings + risk block
- [ ] Community pulse quotes ≠ paragraph text
- [ ] No numeric scores anywhere on property page evidence surfaces

---

## 13. Non-goals (this plan)

- LLM prose generation on request path
- Full runtime graph edge traversal (serving facts + scoped entity IDs sufficient for v1)
- Legal document validation workflows
- Payment / negotiation features
- Replacing evidence folds — brief **points to** folds, does not duplicate all rows

---

## 14. Open questions

1. **Materialization vs runtime compose:** v1 can compose brief at request from in-memory facts (simpler). Promote to serving bundle fact when latency or consistency matters.
2. **Approach road place resolution:** auto from geo vs manual seed table for pilot societies?
3. **Reddit deploy target:** Fargate vs EC2 — depends on existing AWS access from hackathon environment.
4. **Brief on search results:** show compressed 1-line living read on result cards, or property page only for v1?

---

## 15. Success metrics

| Metric | Target |
|--------|--------|
| Societies with non-empty livability brief | >60% of pilot bundle |
| Brief sentences with receipt lineage | 100% |
| Community paragraph containing quote text | 0% |
| Reddit fetch success rate (fetcher container) | >90% daily runs |
| Buyer comprehension (qualitative) | Brief reads like diligence checklist, not marketing |

---

## 16. File checklist (expected touch points)

```
data/product/livability_theme_registry.json     NEW
docs/livability_brief_plan.md                   THIS DOC

backend/src/livability_brief.rs                 NEW
backend/src/livability_themes.rs                NEW (optional split)
backend/src/community.rs                        REFACTOR
backend/src/routes/properties.rs                WIRE
backend/src/assets/google.rs                    EXTEND
backend/src/assets/community.rs                 EXTEND
backend/tests/livability_brief_contract.rs      NEW

pipeline/skills/fetch_approach_road_reviews.py  NEW (Phase 3)
pipeline/docker/reddit-fetcher/                   NEW (Phase 4)

frontend/src/components/evidence/LivabilityBriefCard.tsx  NEW
frontend/src/components/evidence/CommunityPulseCard.tsx   FIX
frontend/src/styles/evidence.css                          FIX
frontend/src/pages/PropertyPage.tsx                       WIRE
frontend/src/lib/types.ts                                 EXTEND
```

---

## 17. Related references

- Product principles: `AGENTS.md` — receipts beat claims; one signal one surface
- Buyer context folds: `data/product/buyer_context_sections.json`
- Community summarizer: `backend/src/community.rs`
- Reddit skill: `pipeline/skills/search_reddit.py`
- Reddit taxonomy (offline): `data/reddit/taxonomy.json`
