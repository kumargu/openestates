# Day 20: Knowledge Graph Foundation — The Brain That Learns

## 1. The Big Idea

Every search builds the graph. The graph makes every future search better. This is the flywheel.

Today we lay the foundation: **typed knowledge graph in Rust**, a **robot query generator** that stress-tests and seeds the graph using real Bangalore Reddit data, and the **skill abstraction** that lets LLMs (Claude, Gemini, or future agents) contribute structured knowledge.

The robot generator serves three purposes:
1. **Testing harness** — generates realistic queries so we can validate search quality as we build
2. **Seeding tool** — runs daily to slowly index all of Bangalore's micro-markets
3. **Future agent** — designed as a standalone agent that can be handed off to OpenClaw or any orchestration layer

## 2. Why Knowledge Graphs, Not Just a Database

A database stores facts. A knowledge graph stores **relationships and provenance**.

```
"Prestige Lakeside Habitat has good maintenance"
    → WHO says this? (Reddit user, 3 threads)
    → HOW confident? (moderate — 5 threads, no Google reviews yet)
    → WHEN was this learned? (2 days ago)
    → WHAT search triggered learning this? ("family-friendly Whitefield")
    → WHAT else connects? (Society → Area → Metro line → School)
```

This is what makes OpenEstates fundamentally different from 99acres or MagicBricks. Every claim has a chain of evidence. Every ranking can be explained. Every search makes the system smarter.

## 3. Architecture

### 3.1 Graph Schema (Rust types)

```
backend/src/knowledge/
  mod.rs              # KnowledgeGraph struct, public API
  node.rs             # Node enum (Property, Society, Area, Builder, Metro, School, Signal)
  edge.rs             # Edge types (BelongsTo, LocatedIn, NearTo, HasSignal, SourcedFrom)
  fact.rs             # SourcedFact — the atomic unit, every fact has provenance
  store.rs            # Persistence layer (JSON files now, SQLite/Postgres later)
  query.rs            # Graph traversal queries (neighbors, path, subgraph)
  embeddings.rs       # Vector index over entity summaries + search queries
  skills/
    mod.rs            # Skill trait definition + SkillRegistry
    learn_society.rs  # Skill: enrich a society node from Reddit/Google/AI
    learn_area.rs     # Skill: enrich an area node
    extract_intent.rs # Skill: parse NL query into structured intent
```

### 3.2 Core Types

```rust
// The atomic unit — every piece of knowledge has provenance
struct SourcedFact {
    key: String,                // "maintenance_quality", "family_friendly", "metro_distance"
    value: FactValue,           // Numeric(0.82) | Text("good") | Bool(true) | Tags(vec![...])
    confidence: f32,            // 0.0 - 1.0
    source: FactSource,         // Where this fact came from
    learned_at: DateTime<Utc>,  // When the system learned this
    version: u32,               // Facts can be updated (newer version wins)
}

enum FactValue {
    Numeric(f64),
    Text(String),
    Bool(bool),
    Tags(Vec<String>),
    Score { value: f64, explanation: String },
}

struct FactSource {
    source_type: SourceType,    // Reddit, RERA, Google, Computed, Manual, LLM
    url: Option<String>,        // Link to original source
    model: Option<String>,      // "claude-sonnet-4-5" or "gemini-2.0-flash"
    skill_id: Option<String>,   // Which skill produced this fact
    triggered_by: Option<String>, // Which search query triggered learning this
}

enum SourceType {
    Reddit,         // r/bangalore thread           — dynamic, refresh weekly
    Google,         // Google Reviews / Places       — dynamic, refresh weekly
    RERA,           // Karnataka RERA registry       — static, fetch once (confidence: 1.0)
    BBMP,           // BBMP property tax / records   — static, refresh yearly
    News,           // News article                  — dynamic, on-demand
    Computed,       // Derived from other facts      — recompute when inputs change
    Manual,         // Seed data, hand-curated       — static until manually updated
    LLM,            // AI-generated synthesis        — dynamic, refresh with new data
}

impl SourceType {
    /// Whether this source type needs periodic refresh
    fn is_static(&self) -> bool {
        matches!(self, Self::RERA | Self::BBMP | Self::Manual)
    }

    /// Default confidence level for this source type
    fn default_confidence(&self) -> f32 {
        match self {
            Self::RERA => 1.0,     // Government authority
            Self::BBMP => 0.9,     // Government records
            Self::Manual => 0.8,   // Hand-curated seed data
            Self::Google => 0.8,   // Crowd-sourced reviews
            Self::Reddit => 0.7,   // Community sentiment
            Self::News => 0.7,     // Journalism
            Self::Computed => 0.6, // Derived (depends on input quality)
            Self::LLM => 0.5,     // AI synthesis (lowest default)
        }
    }
}
```

### 3.3 Node Types

```rust
enum NodeType {
    Property,   // Individual listing
    Society,    // Apartment complex / gated community
    Area,       // Micro-market (Whitefield, Sarjapur, etc.)
    Builder,    // Developer (Prestige, Brigade, Sobha, etc.)
    Metro,      // Metro station
    School,     // School / educational institution
    Signal,     // Reusable signal ("waterlogging_risk", "metro_access")
}

struct Node {
    id: NodeId,
    node_type: NodeType,
    name: String,
    facts: Vec<SourcedFact>,
    summary_embedding: Option<Vec<f32>>,  // For semantic search
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}
```

### 3.4 Edge Types

```rust
enum Relation {
    PropertyInSociety,    // prop → society
    SocietyInArea,        // society → area
    BuiltBy,              // society → builder
    NearMetro,            // property/society → metro station
    NearSchool,           // property/society → school
    HasSignal,            // any entity → signal
    SimilarTo,            // society ↔ society (computed from embeddings)
    CompetesWithPrice,    // property ↔ property (same area, similar BHK)
    SourcedFrom,          // fact → source URL/thread
}

struct Edge {
    from: NodeId,
    to: NodeId,
    relation: Relation,
    weight: f32,          // Strength of relationship
    metadata: HashMap<String, String>,  // Extra context
    source: FactSource,
}
```

### 3.5 The Graph

```rust
struct KnowledgeGraph {
    nodes: HashMap<NodeId, Node>,
    edges: Vec<Edge>,

    // Indexes for fast lookup
    edges_by_from: HashMap<NodeId, Vec<usize>>,
    edges_by_to: HashMap<NodeId, Vec<usize>>,
    nodes_by_type: HashMap<NodeType, Vec<NodeId>>,

    // Search event log
    search_log: Vec<SearchEvent>,

    // Enrichment queue
    enrichment_queue: Vec<EnrichmentTask>,
}

struct SearchEvent {
    query: String,
    intent: SearchIntent,
    query_embedding: Option<Vec<f32>>,
    results_returned: usize,
    graph_nodes_hit: Vec<NodeId>,
    enrichment_gaps: Vec<String>,  // What we didn't know
    timestamp: DateTime<Utc>,
}

struct EnrichmentTask {
    entity_id: NodeId,
    skill_needed: String,
    priority: f32,         // Higher = more users asked about this
    triggered_by: Vec<String>,  // Which queries triggered this
    status: TaskStatus,
}
```

## 4. The Skill Abstraction

Skills are the **structured extraction functions** that turn unstructured world knowledge into typed graph nodes.

### 4.1 Skill Trait (Rust)

```rust
#[async_trait]
trait Skill: Send + Sync {
    fn id(&self) -> &str;
    fn description(&self) -> &str;

    /// What this skill needs as input
    fn input_schema(&self) -> SkillInput;

    /// What this skill produces
    fn output_schema(&self) -> SkillOutput;

    /// Execute the skill, returning new facts and edges
    async fn execute(&self, input: SkillInput, graph: &KnowledgeGraph)
        -> Result<SkillResult>;

    /// Estimated cost (for budgeting)
    fn estimated_cost(&self) -> SkillCost;
}

struct SkillResult {
    facts: Vec<SourcedFact>,
    edges: Vec<Edge>,
    new_nodes: Vec<Node>,
    confidence: f32,
    cost_actual: SkillCost,
}

struct SkillCost {
    llm_tokens: u32,
    api_calls: u32,
    estimated_usd: f32,
}
```

### 4.2 Initial Skills

| Skill ID | Input | Output | LLM | Cost |
|----------|-------|--------|-----|------|
| `learn_society` | society name + area | facts: maintenance, sentiment, family-fit, signals, cautions | Claude Sonnet | ~$0.02 |
| `learn_area` | area name + city | facts: metro, traffic, waterlogging, schools, vibe | Gemini Flash | ~$0.005 |
| `search_reddit` | query string | raw threads: titles, comments, URLs, dates | None (API) | Free |
| `synthesize_threads` | list of threads | structured sentiment + quotes + signals | Claude Haiku | ~$0.005 |
| `extract_intent` | NL search query | structured intent: area, bhk, budget, preferences | Claude Haiku | ~$0.002 |
| `embed_entity` | entity summary text | 768-dim embedding vector | Google text-embedding-004 | Free tier |
| `compare_entities` | entity A + entity B | comparative analysis with tradeoffs | Claude Sonnet | ~$0.03 |
| `fetch_google_reviews` | society name + area | rating, review count, top reviews, sentiment | Gemini Flash (grounded) | Free tier |
| `verify_rera` | project name or RERA number | registration status, promoter, dates, verification score | Python scraper | Free |

### 4.3 Why Skills Are Powerful

**Skills ARE the code.** They're not configuration or prompts stored in a database. They live in the codebase, version-controlled, reviewed, tested. Each skill is a Python module (or Rust module for hot-path) that does one thing well.

```
pipeline/skills/
  __init__.py                 # SkillRegistry
  base.py                     # BaseSkill class
  learn_society.py            # One file = one skill
  learn_area.py
  search_reddit.py
  synthesize_threads.py
  extract_intent.py
  embed_entity.py
  compare_entities.py
  fetch_google_reviews.py     # Gemini-powered Google review extraction
  verify_rera.py              # Karnataka RERA verification (static truth)
```

1. **Composable** — `learn_society` calls `search_reddit` → `synthesize_threads` → `embed_entity`
2. **Cacheable** — same skill + same input + same version = skip. Facts have timestamps.
3. **Auditable** — every fact traces to which skill produced it, from what source, for which query
4. **LLM-agnostic** — swap Claude for Gemini or local LLMs without changing the graph schema
5. **Cost-controllable** — set daily budget, prioritize by demand
6. **Agent-ready** — each skill is a self-contained unit of work. Hand it to OpenClaw, CrewAI, or any agent framework.
7. **Code-reviewable** — skills are just Python. You can read, test, and improve them like any other code.

## 5. The Robot Query Generator

### 5.1 What It Is

A Python agent that generates realistic search queries for all Bangalore micro-markets, using Reddit threads as inspiration. It runs queries through the search system, observes what the graph knows vs doesn't know, and triggers enrichment.

### 5.2 Design

```python
# pipeline/robot_generator.py

class RobotQueryGenerator:
    """
    Generates realistic property search queries from Reddit data.
    Used for:
    1. Testing search quality during development
    2. Seeding the knowledge graph with real-world patterns
    3. Identifying enrichment gaps across areas
    """

    BANGALORE_AREAS = [
        "Whitefield", "Sarjapur Road", "Bellandur", "HSR Layout",
        "Electronic City", "Marathahalli", "Koramangala", "Indiranagar",
        "Hebbal", "Yelahanka", "Bannerghatta Road", "Kanakapura Road",
        "Thanisandra", "Devanahalli", "Hennur", "Old Airport Road",
        "JP Nagar", "Jayanagar", "Banashankari", "Rajajinagar",
        "Malleshwaram", "Basavanagudi", "KR Puram", "Mahadevapura",
    ]

    async def generate_queries(self, area: str) -> list[GeneratedQuery]:
        """
        Step 1: Fetch top Reddit threads mentioning this area
        Step 2: Ask LLM to generate realistic search queries
                that a person reading those threads would type
        Step 3: Return structured queries with expected intent
        """

    async def run_query(self, query: str) -> QueryResult:
        """
        Run a query against the search API.
        Returns: results, intent parsed, graph coverage, gaps found
        """

    async def evaluate_quality(self, query: str, results: list) -> QualityReport:
        """
        Ask LLM: are these results good for this query?
        Score: relevance, completeness, explanation quality
        """

    async def run_area_sweep(self, area: str):
        """
        Full cycle for one area:
        1. Generate 10-15 realistic queries
        2. Run each through search
        3. Log graph coverage + gaps
        4. Trigger enrichment for gaps
        5. Re-run queries, measure improvement
        """

    async def run_full_sweep(self):
        """
        Sweep all areas. Daily cron job in production.
        Budget-aware: stops when daily token budget is exhausted.
        """
```

### 5.3 Query Generation Strategy

The robot doesn't just generate random queries. It generates queries **from real Reddit context**:

```
Reddit thread: "Moving to Bangalore, looking for 3BHK near metro"
    → Generated queries:
      "3BHK near metro under 1.5Cr"
      "spacious flat with metro access Whitefield"
      "family apartment near Whitefield metro station"

Reddit thread: "Prestige vs Brigade for families?"
    → Generated queries:
      "best society for families in Whitefield"
      "Prestige Lakeside vs Brigade Lakefront"
      "safe gated community Whitefield with kids"

Reddit thread: "waterlogging in Bellandur area"
    → Generated queries:
      "apartments in Bellandur without waterlogging"
      "safe from flooding near Bellandur"
      "elevated societies Bellandur no water issues"
```

This grounds the queries in **real concerns real people have**, not synthetic gibberish.

But the robot also **generates its own original queries** based on what it's learned. As it builds knowledge about an area, it starts asking questions the graph doesn't yet answer — probing for gaps, testing edge cases, exploring adjacent concerns. The robot has a personality: it's a curious, thorough home-buyer who keeps digging. This self-directed curiosity is what turns it from a test tool into a knowledge-building agent.

### 5.4 Agent Design (Future-Ready)

```python
class RobotGeneratorAgent:
    """
    Designed as a standalone agent that can be:
    - Run manually: python3 pipeline/robot_generator.py --area Whitefield
    - Run as cron: python3 pipeline/robot_generator.py --sweep --budget 5.00
    - Handed to OpenClaw: agent receives area, returns enrichment report
    - Called from Rust: backend triggers via subprocess or HTTP
    """

    def __init__(self, config: AgentConfig):
        self.llm = config.llm  # Claude, Gemini, or any
        self.budget = config.daily_budget_usd
        self.graph_api = config.graph_endpoint  # Talk to Rust backend
        self.output_dir = config.output_dir  # data/intelligence/{area}/
```

## 6. Storage & Persistence

**Design principle: S3 is the base layer from day one.**

Every storage decision must answer: "Can I push this to S3 with zero restructuring?" If the answer is no, redesign. S3 is the durable store. Everything else (local FS, SQLite) is a fast cache layer on top.

### 6.1 S3-First Key Design

```
s3://openestates-knowledge/
  # Knowledge graph entities — one file per entity, sharded by area
  graph/
    entities/
      bengaluru/whitefield/societies/prestige_lakeside_habitat.json
      bengaluru/whitefield/societies/brigade_lakefront.json
      bengaluru/whitefield/areas/whitefield.json
      bengaluru/sarjapur/societies/sobha_dream_acres.json
    edges/
      bengaluru/whitefield/edges.json       # All edges for this area
      bengaluru/sarjapur/edges.json

  # Embeddings — sharded by entity type
  embeddings/
    societies/index.npy           # Embedding matrix (all societies)
    societies/ids.json            # ID → row mapping
    areas/index.npy
    areas/ids.json
    queries/index.npy             # Search query embeddings
    queries/log.json

  # Search events
  search_log/
    2026/03/09.jsonl              # Daily append-only log (JSONL for streaming)
    2026/03/10.jsonl

  # Enrichment queue
  enrichment/
    queue.json                    # Pending tasks
    completed/2026-03-09.jsonl    # Completed task log
```

**Why this sharding works:**
- **Per-area sharding** — S3 GET/PUT limits are per-prefix. Sharding by area means concurrent reads across areas never contend.
- **One file per entity** — small files = fast GET, easy update, no read-modify-write of large blobs.
- **JSONL for logs** — append-only, no read-modify-write. Can be compacted later.
- **Embeddings as numpy arrays** — single GET loads the full index. Rebuild on entity change. Small enough (<1MB for 1000 entities) to cache in memory.

**S3 latency design:**
- Hot path (search): Rust loads entity files + embeddings into memory at startup. S3 is cold store.
- Warm path (enrichment): Python writes new facts to S3. Rust reloads on signal or TTL.
- Cold path (analytics): Query JSONL search logs for patterns.

**Compaction strategy:**
- Daily: compact search logs (JSONL → aggregated daily summary)
- Weekly: compact entity files (merge fact versions, prune old versions)
- On-demand: rebuild embedding indexes after batch enrichment

### 6.2 Phase 1 (Day 20): Local FS mirroring S3 layout

```
data/knowledge/                   # Exact mirror of S3 prefix structure
  graph/entities/bengaluru/whitefield/societies/prestige_lakeside_habitat.json
  graph/edges/bengaluru/whitefield/edges.json
  embeddings/societies/index.npy
  search_log/2026/03/09.jsonl
  enrichment/queue.json
```

The `StorageBackend` trait (already in `backend/src/storage/`) handles local FS now, S3 later. Same key paths. Zero code change when migrating.

### 6.3 SQLite as fast cache layer

SQLite sits between Rust memory and S3. Not a replacement — a cache.

- On startup: Rust loads from S3 (or local FS) into SQLite
- On query: Rust reads from SQLite (fast, indexed)
- On enrichment: Python writes to S3, signals Rust to reload
- SQLite is ephemeral — can be rebuilt from S3 at any time

```
data/cache/knowledge.db           # Ephemeral, rebuilt from S3
  tables: nodes, facts, edges, search_events

data/cache/vectors.db             # sqlite-vss for vector queries
  virtual tables for embedding search
```

### 6.4 S3 Tables (future)

When we need SQL over S3 data without SQLite intermediary:
- S3 Tables (Iceberg format) for analytics queries over search logs
- No data movement — query S3 directly
- Same prefix structure, just add Iceberg metadata

## 7. How the Graph Learns

### 7.1 The Learning Loop

```
User/Robot searches "3BHK family-friendly Whitefield"
    │
    ├── 1. INTENT EXTRACTION (Skill: extract_intent)
    │   → { area: "Whitefield", bhk: 3, preferences: ["family-friendly"] }
    │
    ├── 2. GRAPH LOOKUP
    │   → Find all Society nodes in Area "Whitefield"
    │   → Check: do they have fact "family_friendly_score"?
    │   → 3/12 have it, 9 don't
    │
    ├── 3. SERVE WHAT WE HAVE
    │   → Rank the 3 scored societies
    │   → Show: "Building intelligence for 9 more societies"
    │
    ├── 4. LOG SEARCH EVENT
    │   → Store query, intent, results, gaps
    │   → Embed query for future similarity matching
    │
    └── 5. QUEUE ENRICHMENT
        → EnrichmentTask { entity: each of 9 societies, skill: "learn_society" }
        → Priority = number of queries that hit this gap

--- Background enrichment runs ---

    6. EXECUTE SKILLS (async, budget-controlled)
       → learn_society("Brigade Metropolis", focus="family_friendly")
       → search_reddit → synthesize_threads → score
       → New SourcedFacts added to graph

    7. NEXT SIMILAR QUERY
       → Graph now has 12/12 scored → full ranked results
       → "Here are all 12 societies ranked for family-friendliness"
```

### 7.2 Embedding Strategy

**Embed immediately:**
- Search queries (OpenAI ada, ~$0.0001 each) — enables query clustering
- Society summaries — enables "similar societies" and semantic search
- Area descriptions — enables "areas like this"

**Don't embed:**
- Structured numeric facts (price, sqft, scores) — use exact filters
- Individual tags — use keyword matching

**When to embed:**
- On node creation (new society discovered)
- On significant fact update (new Reddit data changes the summary)
- On search (embed the query, find similar past queries)

### 7.3 Cross-Query Intelligence

Once embeddings exist:
- "3BHK Whitefield family" ≈ "apartment for kids Whitefield" → same cached graph traversal
- "Compare Whitefield vs Sarjapur" → graph has both area nodes, traverse and diff
- "Quiet society" → embedding similarity to societies with fact "noise_level: low"

## 8. Embeddings: When and How

### 8.1 Model Choice

**Google `text-embedding-004`** (768-dim)
- Free tier: 1500 requests/min via AI Studio
- Same API key as Gemini Flash (one key, two purposes)
- Quality comparable to OpenAI ada for entity/query similarity
- No separate billing — free tier covers months of dev + daily sweeps
- Fallback: `sentence-transformers` locally (zero API, slower)

### 8.2 Index

**Phase 1:** numpy brute-force (already in `engine/vector_search.py`) — 768-dim vectors
**Phase 2:** FAISS IVF index when >5000 vectors
**Phase 3:** Rust-native vector index (or pgvector in Postgres)

### 8.3 What Gets Embedded

| Entity | Text Embedded | When |
|--------|--------------|------|
| Society | `"{name}. {summary}. Best for: {best_for}. Signals: {signals}"` | On creation + on enrichment |
| Area | `"{name}. {livability_summary}. {metro_access}. {vibe}"` | On creation + on enrichment |
| Property | `"{title}. {description_summary}. {area}. {tags}"` | On creation |
| Search query | Raw query text | On every search |

## 9. Implementation Plan — Phased, Not Rushed

### Phase A: Types & Storage (2-3 hours)

**Goal:** The Rust types compile, the graph can be loaded/saved.

1. Create `backend/src/knowledge/` module structure
2. Define `SourcedFact`, `FactValue`, `FactSource`, `SourceType`
3. Define `Node`, `NodeType`, `NodeId`
4. Define `Edge`, `Relation`
5. Define `KnowledgeGraph` with basic CRUD
6. Implement JSON serialization (serde)
7. Write `store.rs` — load/save from `data/knowledge/`
8. **PAUSE. Compile. Test with a hardcoded node. Verify round-trip.**

### Phase B: Seed Graph from Existing Data (1-2 hours)

**Goal:** Bootstrap the graph from what we already have.

1. Write `bootstrap.rs` — reads `data/seed/properties.json`, `societies.json`, `area_profiles.json`
2. Creates nodes for all 20 properties, 12 societies, 5 areas
3. Creates edges: PropertyInSociety, SocietyInArea, BuiltBy
4. Converts existing scores/tags into `SourcedFact` with `source_type: Manual`
5. Saves to `data/knowledge/graph.json`
6. **PAUSE. Load the graph. Query it. "Give me all societies in Whitefield." Does it work?**

### Phase C: Search Event Logging (1 hour)

**Goal:** Every search is recorded, gaps are identified.

1. Add `SearchEvent` struct
2. Wire into existing `/api/search` handler — log query, intent, results
3. Save search log to `data/knowledge/search_log.json`
4. **PAUSE. Run a few searches. Check the log. Does it capture what we need?**

### Phase D: Robot Query Generator (2-3 hours)

**Goal:** Generate realistic queries, run them, measure graph coverage.

1. Create `pipeline/robot_generator.py`
2. Implement Reddit-sourced query generation for one area (Whitefield)
3. Implement query runner (calls backend search API)
4. Implement gap detection (which entities lack facts for the query's preferences)
5. Output: `data/knowledge/robot_report_{area}.json` with coverage + gaps
6. **PAUSE. Run for Whitefield. Read the report. Are the queries realistic? Are the gaps real?**

### Phase E: First Skill — learn_society (2-3 hours)

**Goal:** One skill that enriches a society node using LLM + Reddit.

1. Define `Skill` trait in Rust
2. Implement `LearnSociety` skill (calls Claude API via HTTP)
3. Test: enrich one society that has gaps
4. Verify: new SourcedFacts appear in graph with correct provenance
5. **PAUSE. Look at the enriched node. Is the provenance chain clear? Would you trust this?**

### Phase F: RERA Verification + Google Reviews Skills (2-3 hours)

**Goal:** Two more skills that prove the static vs dynamic pattern.

1. Build `pipeline/skills/verify_rera/` module (search, parser, verifier, models)
2. Test: `verify_project(project_name="Prestige Lakeside Habitat")` → RERA result
3. Build `pipeline/skills/fetch_google_reviews.py` using Gemini grounded search
4. Test: fetch reviews for one Whitefield society
5. Both skills write `SourcedFact` entries to graph with correct source types
6. **PAUSE. Look at one society node. It now has RERA (static, confidence 1.0) + Reddit (dynamic, confidence 0.7) + Google Reviews (dynamic, confidence 0.8). Does the provenance make sense?**

### Phase G: Embeddings (1-2 hours)

**Goal:** Entities and queries have embeddings, semantic search works.

1. Embed all society summaries (Google text-embedding-004)
2. Embed all area descriptions
3. Store in `data/knowledge/embeddings/`
4. Implement similarity query: "find societies similar to X"
5. Embed search queries, find similar past queries
6. **PAUSE. "Societies similar to Prestige Lakeside" — does it return sensible results?**

### Phase H: Integration (2 hours)

**Goal:** The knowledge graph powers the existing search API.

1. Load graph into `AppState` alongside existing data
2. Search handler checks graph for enrichment before ranking
3. API responses include source provenance on claims
4. Frontend renders sources inline (Perplexity-style)
5. **PAUSE. End-to-end: search → graph lookup → ranked results with sources. Does it feel real?**

## 10. Robot Generator as Future Agent

The robot generator is designed to be **agent-compatible from day one:**

```python
class RobotGeneratorAgent:
    """
    Interface contract — any orchestration layer can use this.
    """

    # Standard agent interface
    def describe(self) -> str:
        return "Generates search queries for a Bangalore area, runs them, identifies knowledge gaps"

    def inputs(self) -> dict:
        return {"area": "str", "budget_usd": "float", "query_count": "int"}

    def outputs(self) -> dict:
        return {"queries_generated": "int", "gaps_found": "list", "enrichment_triggered": "int"}

    async def run(self, area: str, budget_usd: float = 1.0, query_count: int = 10):
        # 1. Generate queries from Reddit
        # 2. Run through search API
        # 3. Identify gaps
        # 4. Trigger enrichment skills
        # 5. Return report
```

This agent can be:
- **Manual:** `python3 pipeline/robot_generator.py --area Whitefield`
- **Cron:** `python3 pipeline/robot_generator.py --sweep --budget 5.00` (daily)
- **OpenClaw task:** hand off area + budget, get back enrichment report
- **Claude Code skill:** `.claude/skills/run-robot-generator.md`

## 11. Gemini as a Knowledge Extraction Engine

Gemini Flash with Google Search grounding is uniquely powerful for knowledge graph construction. One API key gives us:

### 11.1 Google Reviews via Gemini (no Places API needed)

Instead of paying for Google Places API, use Gemini with grounding to extract review intelligence:

```python
# pipeline/skills/fetch_google_reviews.py

PROMPT = """
Search for Google reviews of "{society_name}" apartment complex in {area}, {city}.

Return a structured JSON with:
- google_rating: float (1-5)
- review_count: int (approximate)
- top_positive_reviews: list of 3 real review excerpts
- top_negative_reviews: list of 2 real review excerpts
- common_themes: list of recurring topics (maintenance, security, parking, etc.)
- sentiment_summary: 1-2 sentence synthesis

IMPORTANT: Only include information you find from actual Google reviews.
If you cannot find reviews for this specific society, return {"found": false}.
Include the Google Maps URL if found.
"""
```

**Why this works:** Gemini Flash with grounding can search Google in real-time and return structured results. We get review data without a separate API key or billing. The free tier (15 RPM) is enough for batch enrichment.

### 11.2 Prompt Design Principles for Gemini

Effective Gemini prompts for knowledge extraction:

1. **Be specific about the entity** — include area, city, builder name. Gemini searches better with context.
2. **Request structured JSON** — Gemini respects JSON schemas well. Define the exact shape.
3. **Include validation constraints** — "Only include information you find" prevents hallucination.
4. **Ask for source URLs** — grounded search returns URLs, capture them for provenance.
5. **Use system instructions for role** — "You are a real estate researcher extracting factual information."
6. **Keep prompts focused** — one skill = one focused prompt. Don't ask for everything in one call.

### 11.3 Gemini Skill Examples

```python
# Area learning — Gemini is ideal because it can search for latest info
AREA_PROMPT = """
Research {area_name}, {city} as a residential area. Search for recent information.

Return JSON:
{
  "metro_status": "operational | under_construction | planned | none",
  "metro_details": "nearest station name and distance",
  "traffic_reality": "1-2 sentences about daily commute",
  "waterlogging_risk": "low | moderate | high",
  "waterlogging_detail": "specific incidents or areas if any",
  "school_quality": "list of top 3 schools within 5km",
  "upcoming_infra": "any major infrastructure projects",
  "price_trend": "appreciating | stable | declining",
  "sources": ["url1", "url2"]
}
"""

# Society-specific — grounded search for latest resident feedback
SOCIETY_PROMPT = """
Research "{society_name}" by {builder_name} in {area}, {city}.

Find: Google reviews, Reddit mentions, news articles, resident forums.

Return JSON:
{
  "year_built": int,
  "total_units": int (approximate),
  "google_rating": float,
  "review_count": int,
  "resident_sentiment": "positive | mixed | negative",
  "top_positives": ["...", "..."],
  "top_complaints": ["...", "..."],
  "best_resident_quote": "verbatim or close paraphrase",
  "maintenance_quality": "good | average | poor",
  "family_suitability": "high | moderate | low",
  "sources": [{"type": "google|reddit|news", "url": "..."}]
}
"""
```

## 12. Source Freshness Model — Static vs Dynamic Facts

Not all facts age the same way. This is critical for storage design and refresh scheduling.

### 12.1 Static Sources (fetch once, trust forever)

| Source | Example Facts | Refresh | Storage |
|--------|--------------|---------|---------|
| **RERA** | Registration number, promoter name, approval date, project status | Never (until status change) | `graph/entities/.../rera.json` |
| **Builder identity** | Builder name, founding year, portfolio | Yearly | Part of entity node |
| **Location** | GPS coordinates, area boundaries, metro station positions | Never | Part of entity node |
| **Building specs** | Total floors, year built, total units | Never | Part of entity node |

### 12.2 Dynamic Sources (refresh regularly)

| Source | Example Facts | Refresh Frequency | Storage |
|--------|--------------|-------------------|---------|
| **Google Reviews** | Rating, review count, recent reviews, sentiment | Weekly | `graph/entities/.../google_reviews.json` |
| **Reddit threads** | New threads, sentiment shifts, emerging complaints | Weekly | `graph/entities/.../reddit.json` |
| **Price trends** | Area median, appreciation rate | Monthly | Part of area node |
| **Market activity** | Saves, offers, days on market | Daily (when live) | Part of property node |
| **News/events** | New metro line, waterlogging incident | On-demand | Append to entity facts |

### 12.3 How This Affects Storage

```
graph/entities/bengaluru/whitefield/societies/prestige_lakeside_habitat/
  node.json                    # Core entity (static fields)
  rera.json                    # RERA verification (static, fetch once)
  google_reviews.json          # Reviews (dynamic, refresh weekly)
  reddit.json                  # Reddit intelligence (dynamic, refresh weekly)
  enrichment_log.json          # When each source was last refreshed
```

The `enrichment_log.json` tracks freshness:
```json
{
  "rera": { "last_fetched": "2026-03-09", "next_refresh": "never", "status": "complete" },
  "google_reviews": { "last_fetched": "2026-03-09", "next_refresh": "2026-03-16", "status": "complete" },
  "reddit": { "last_fetched": "2026-03-08", "next_refresh": "2026-03-15", "status": "complete" }
}
```

The robot generator uses this to decide what to refresh — it skips RERA (static) but re-runs Google Reviews if stale.

### 12.4 The One-Stop Discovery Promise

The product goal: **a user should never need to open another tab.** Everything they'd check across multiple sites — RERA status, Google reviews, Reddit sentiment, area infrastructure, price trends — is aggregated, sourced, and explained in one place.

Each skill is one external source eliminated:
- `verify_rera` → no need to visit rera.karnataka.gov.in
- `fetch_google_reviews` → no need to search Google Maps
- `search_reddit` + `synthesize_threads` → no need to browse r/bangalore
- `learn_area` → no need to research infrastructure projects

The knowledge graph is the **aggregation layer** that makes this possible while keeping every fact traceable to its original source.

## 13. RERA Verification Skill

### 13.1 Why RERA Is Special

RERA is the **highest-authority source** for property verification in India. It's:
- Government-issued (Karnataka RERA at rera.karnataka.gov.in)
- Legally binding
- Static once registered (status changes are rare events)
- Publicly available

A RERA verification score immediately elevates trust. "RERA verified" with a certificate link is the strongest transparency signal we can show.

### 13.2 Module Structure

```
pipeline/skills/verify_rera/
  __init__.py
  search.py        # Search Karnataka RERA by project name or registration number
  parser.py        # Parse HTML tables from RERA certificate pages
  verifier.py      # Main entry: verify_project() → ReraResult
  models.py        # Pydantic models for normalized output
```

### 13.3 Verification Score Logic

```python
def compute_verification_score(result: ReraResult) -> int:
    score = 0
    if result.registration_found:     score += 40
    if result.promoter_matches:       score += 20  # Matches known builder name
    if result.certificate_available:  score += 20  # PDF/URL exists
    if result.status == "Registered": score += 10  # Active registration
    if result.completion_date:        score += 10  # Timeline provided
    return score
```

### 13.4 Output Schema

```python
class ReraResult(BaseModel):
    verified: bool
    verification_score: int          # 0-100
    project_name: str
    rera_registration_number: str | None
    promoter_name: str | None
    project_address: str | None
    approval_date: str | None        # ISO format
    completion_date: str | None
    status: str                      # "Registered" | "Not Found" | "Expired"
    extension_granted: bool | None
    documents: list[dict]            # [{"type": "RERA Certificate", "url": "..."}]
    source: str = "Karnataka RERA"
    source_url: str | None           # Direct link to certificate page
```

### 13.5 Integration with Knowledge Graph

RERA facts are stored as `SourcedFact` with `source_type: RERA`:

```rust
SourcedFact {
    key: "rera_verified",
    value: Bool(true),
    confidence: 1.0,              // Government source = maximum confidence
    source: FactSource {
        source_type: RERA,
        url: Some("https://rera.karnataka.gov.in/certificate?CER_NO=..."),
        skill_id: Some("verify_rera"),
    },
}
```

RERA facts have `confidence: 1.0` — the only source that gets unconditional trust.

### 13.6 Future Verification Sources (same pattern)

The RERA verifier establishes a pattern for future verification skills:

| Skill | Source | Trust Level | Refresh |
|-------|--------|-------------|---------|
| `verify_rera` | Karnataka RERA | 1.0 (government) | Static |
| `verify_bbmp_tax` | BBMP property tax records | 0.9 | Yearly |
| `check_court_cases` | eCourts | 0.9 | Monthly |
| `check_flood_zone` | BBMP/KSNDMC flood maps | 0.8 | Static |
| `check_satellite` | Satellite imagery (construction progress) | 0.7 | Monthly |

Each is a skill. Each produces `SourcedFact` entries. The graph doesn't care where facts come from — it just tracks source type, confidence, and freshness.

## 14. What NOT to Build Today

- Multi-user (single developer mode)
- Real-time updates (batch is fine)
- Perfect graph schema (iterate — the types will evolve)
- Production cron jobs (manual runs first)
- Full Bangalore coverage (Whitefield first, prove the loop)
- S3 deployment (design for it, but local FS mirrors the layout)

## 15. Success Criteria

- [ ] `backend/src/knowledge/` compiles with all core types
- [ ] Graph bootstrapped from seed data (20 properties, 12 societies, 5 areas as nodes)
- [ ] Graph persisted to S3-mirrored local layout and loads correctly
- [ ] Search events logged with intent + results + gaps (JSONL)
- [ ] Robot generator produces 10+ realistic Whitefield queries (Reddit-sourced + self-generated)
- [ ] Robot report shows graph coverage + gaps per query
- [ ] At least 1 skill (`learn_society`) enriches a node with sourced facts
- [ ] `verify_rera` skill scrapes Karnataka RERA and returns verification score
- [ ] `fetch_google_reviews` skill works via Gemini grounded search
- [ ] Entity embeddings exist for all societies and areas (Google text-embedding-004)
- [ ] Query embedding + similarity search returns sensible matches
- [ ] End-to-end: search → graph → ranked results with provenance

## 16. API Keys & Credits Needed

| Service | Key | Purpose | Estimated Cost (Day 20) | Ongoing Daily |
|---------|-----|---------|------------------------|---------------|
| **Anthropic** | `ANTHROPIC_API_KEY` | Claude Sonnet/Haiku for synthesis, intent extraction, scoring | ~$1-3 | ~$0.50-2.00 |
| **Google AI** | `GOOGLE_AI_API_KEY` | Gemini Flash for search skills + `text-embedding-004` for all embeddings | Free tier | Free tier |
| **Reddit** | None needed | Public JSON API (`reddit.com/search.json`) | Free | Free |

**Total Day 20 estimated cost: $1-4**
**Ongoing daily robot sweep cost: ~$1-3** (budget-capped)

### What you already have:
- `ANTHROPIC_API_KEY` — already in `.env` ✓

### What you need to add:
- `GOOGLE_AI_API_KEY` — for Gemini Flash (search skills) AND `text-embedding-004` (all embeddings). One key, two purposes. Get from [aistudio.google.com](https://aistudio.google.com). Free tier: 1500 embedding requests/min, 15 Gemini requests/min. More than enough.

### Cost control:
- Robot generator has a `--budget` flag (default $1/run)
- Each skill reports estimated + actual cost
- Daily sweep stops when budget is exhausted
- All LLM calls are cached (same input = no re-call)

## 17. The Killer Insight

The knowledge graph isn't just a backend feature. It IS the product.

Every property portal has listings. Nobody has a **living knowledge graph that gets smarter with every search, explains every claim with sources, and honestly says "we don't know this yet but we're learning."**

That honesty — "we have moderate confidence on this society based on 5 Reddit threads" — is the transparency promise made real.

The robot generator isn't just testing infrastructure. It's the system **teaching itself about Bangalore** one area at a time. Today it's a test tool. Tomorrow it's the indexer. Next month it's an autonomous agent that ensures every micro-market has fresh intelligence.

Skills are the right abstraction because they make knowledge acquisition **modular, auditable, and improvable.** Swap in a better Reddit parser. Add a Google Reviews skill. Add a RERA scraper skill. The graph doesn't care where the facts come from — it just tracks provenance.

## 18. Decisions Made

1. **LLM calls: Python.** Rich library ecosystem. Rust delegates to Python skills via subprocess. Migrate hot-path skills to Rust later if needed.

2. **Embeddings: Google `text-embedding-004`** via free tier. Python computes batch embeddings. Rust loads and queries them at runtime (ndarray crate for 768-dim vectors).

3. **Enrichment: Lazy with budget caps.** Queue enrichment tasks, run in background batch. Daily robot sweep processes the queue budget-controlled.
