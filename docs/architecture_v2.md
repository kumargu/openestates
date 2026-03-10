# OpenEstates v2 Architecture

Last updated: 2026-03-09

This document defines the target architecture for OpenEstates v2. It covers storage layout, vector embeddings, crawler/agent abstractions, caching, and end-to-end data flow.

The guiding principle: every layer should increase transparency, inspectability, and explainability. No hidden magic.

---

## 1. Storage Layer Design (S3-Ready Local Filesystem)

The storage layer uses a local directory tree that maps 1:1 to an S3 bucket prefix scheme. When we move to S3, every path below becomes `s3://openestates-data/{path}`.

### 1.1 Key/Prefix Scheme

```
data/
  seed/                                          # Hand-curated baseline data
    properties.json                              # All seed properties (flat list)
    societies.json                               # All seed societies
    area_profiles.json                           # All seed area profiles
    upcoming_launches.json                       # Sponsored/upcoming launches

  entities/                                      # Normalized entity store (future)
    properties/{city}/{area}/{property_id}/
      manifest.json                              # Property metadata + pointer to latest version
      v1.json                                    # Versioned snapshot
      v2.json
    societies/{city}/{area}/{society_slug}/
      manifest.json
      v1.json
    areas/{city}/{area_id}/
      manifest.json
      v1.json

  intelligence/                                  # AI-enriched + crawled data
    {area_slug}/                                 # Per-area intelligence folder
      _discovered_societies.json                 # Discovery output for this area
      _curated_societies.json                    # Human-reviewed subset
      _ranked_results.json                       # Scorer output (what the API serves)
      _photo_summary.json                        # Photo fetch status
      {society_slug}/
        photos.json                              # Photo URLs and metadata
        reddit.json                              # Reddit threads + AI synthesis
        reviews_google.json                      # Google reviews (future)
        enrichment.json                          # AI-generated summaries, tags (future)
    societies/                                   # Legacy per-society photos (migrate to area-scoped)
      {society_slug}/
        photos.json

  embeddings/                                    # Vector embeddings (future)
    properties/
      index.faiss                                # FAISS index file
      id_map.json                                # Maps FAISS index positions to entity IDs
      metadata.json                              # Embedding model, dimension, count, last_updated
    societies/
      index.faiss
      id_map.json
      metadata.json
    areas/
      index.faiss
      id_map.json
      metadata.json

  cache/                                         # Pipeline caches (gitignored)
    reddit/                                      # Raw Reddit API responses
      {query_hash}.json
    crawl/                                       # Raw HTTP responses from crawlers
      {url_hash}.json
    ai/                                          # AI enrichment responses
      {input_hash}.json
```

### 1.2 Manifest Files

Each entity directory contains a `manifest.json` that acts as a lightweight index:

```json
{
  "entity_type": "property",
  "entity_id": "prop_w_001",
  "city": "bengaluru",
  "area": "whitefield",
  "latest_version": 2,
  "versions": [
    {"version": 1, "created_at": "2026-02-15T10:00:00Z", "source": "seed"},
    {"version": 2, "created_at": "2026-03-01T14:00:00Z", "source": "crawler_magicbricks"}
  ],
  "enrichments": {
    "reddit": {"status": "complete", "last_run": "2026-03-08T15:00:00Z"},
    "embeddings": {"status": "complete", "model": "text-embedding-3-small", "last_run": "2026-03-08T15:30:00Z"},
    "ai_summary": {"status": "pending"}
  }
}
```

### 1.3 Versioning Strategy

- Entities that change over time (price updates, new reviews) get versioned snapshots: `v1.json`, `v2.json`, etc.
- The manifest always points to the latest version.
- Intelligence data (reddit.json, enrichment.json) is append/overwrite: each run produces a new file, the old one is replaced. If we need history, the manifest tracks `last_run` timestamps.
- Seed data (`data/seed/`) is the hand-curated baseline. It does not version -- it gets replaced when the curator updates it.

### 1.4 Access Tiers

| Tier | Data | Access Pattern | Storage |
|------|------|----------------|---------|
| Hot | Ranked results, seed properties, area profiles | Every API request | In-memory (Rust AppState) |
| Warm | Intelligence (reddit, photos, enrichments) | API detail pages, on-demand | Filesystem read, future: S3 with local cache |
| Cold | Raw crawl responses, old versions, embeddings index | Pipeline runs, batch jobs | Filesystem, future: S3 Standard-IA |

### 1.5 Migration Path

Current state uses flat files in `data/seed/` and `data/intelligence/`. The migration is incremental:

1. **Now**: Keep flat seed files. Pipeline writes to `data/intelligence/{area}/{society}/`.
2. **Next**: Introduce `data/entities/` for normalized storage. Backend reads from both seed and entities.
3. **Later**: Move to S3. Backend loads hot data at startup, fetches warm data on demand. Swap filesystem reads for S3 SDK calls.

---

## 2. Vector Embedding Strategy

### 2.1 What to Embed

| Entity | Text to Embed | Purpose |
|--------|--------------|---------|
| Property | `description_summary` + `transparency_tags` joined + area name + society name | Semantic property search |
| Society | `summary` + `review_summary` + `common_positives` joined + `common_complaints` joined | Society similarity + search |
| Area | `livability_summary` + `trend_summary` + `community_notes` + `externality_tags` joined | Area matching |
| Search Query | Raw user query text | Query-to-entity matching |

### 2.2 Embedding Model

- **Primary**: OpenAI `text-embedding-3-small` (1536 dimensions, cheap, good quality)
- **Fallback**: `text-embedding-3-large` if quality is insufficient
- Cost is negligible at current scale (dozens to hundreds of entities)

### 2.3 Local Storage

Use **FAISS** (Facebook AI Similarity Search) for local vector index:

```python
# pipeline/embeddings.py (future)
import faiss
import numpy as np
import json

class EmbeddingIndex:
    def __init__(self, dimension=1536):
        self.index = faiss.IndexFlatIP(dimension)  # Inner product (cosine after normalization)
        self.id_map = []  # Position -> entity_id

    def add(self, entity_id: str, vector: list[float]):
        vec = np.array([vector], dtype='float32')
        faiss.normalize_L2(vec)
        self.index.add(vec)
        self.id_map.append(entity_id)

    def search(self, query_vector: list[float], k=10) -> list[tuple[str, float]]:
        vec = np.array([query_vector], dtype='float32')
        faiss.normalize_L2(vec)
        scores, indices = self.index.search(vec, k)
        results = []
        for score, idx in zip(scores[0], indices[0]):
            if idx < len(self.id_map):
                results.append((self.id_map[idx], float(score)))
        return results

    def save(self, directory: str):
        faiss.write_index(self.index, f"{directory}/index.faiss")
        with open(f"{directory}/id_map.json", "w") as f:
            json.dump(self.id_map, f)

    def load(self, directory: str):
        self.index = faiss.read_index(f"{directory}/index.faiss")
        with open(f"{directory}/id_map.json") as f:
            self.id_map = json.load(f)
```

### 2.4 Indexing Strategy

- **Full rebuild**: Run after major data changes (new crawl, new seed data). Takes seconds at current scale.
- **Incremental**: When a single entity is added or updated, embed it and append to the index. Rebuild periodically to compact.
- **Re-embed trigger**: When the embedding model changes, or when the text template changes, do a full rebuild.

### 2.5 Search Flow

```
User query ("calm 3BHK near metro in Whitefield under 1.2Cr")
  |
  v
1. Intent extraction (Claude API)
   -> structured intent: {area: "whitefield", bhk: 3, budget_max_cr: 1.2, soft_prefs: ["calm", "metro_access"]}
  |
  v
2. Embed the query text
   -> 1536-dim vector
  |
  v
3. ANN search against property + society indexes
   -> top 50 candidates by semantic similarity
  |
  v
4. Structured filter (bhk, price range, area)
   -> filter down to matching properties
  |
  v
5. Re-rank with scoring engine
   -> apply dimension weights (value, calm, metro, family, etc.)
   -> compute overall_score per property
  |
  v
6. Explain
   -> for each result, generate "why this property" from dimension scores
  |
  v
7. Serve API response with ranked results + explanations
```

### 2.6 Migration to Managed Vector DB

When scale demands it (thousands of properties, sub-second latency requirements):
- **Qdrant** (self-hosted, Rust-native, good fit for our stack)
- or **Pinecone** (managed, zero-ops)
- The `EmbeddingIndex` abstraction above makes swapping straightforward: implement a new backend that calls the managed API instead of local FAISS.

---

## 3. Crawler/Agent Abstraction

### 3.1 Base Crawler Interface

All data collection scripts should follow this pattern:

```python
# pipeline/base_crawler.py (target interface)
from abc import ABC, abstractmethod
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path
from typing import Any, Optional

@dataclass
class CrawlResult:
    source: str              # e.g., "reddit", "magicbricks", "google_places"
    entity_type: str         # e.g., "society", "property", "area"
    entity_id: str           # e.g., "prestige_lakeside_habitat"
    raw_data: Any            # Raw response (for caching)
    normalized_data: dict    # Cleaned, schema-conforming output
    fetched_at: datetime
    cache_key: str           # Hash for dedup/cache lookup

class BaseCrawler(ABC):
    """Base class for all data collection crawlers."""

    def __init__(self, cache_dir: Path, rate_limit_seconds: float = 2.0):
        self.cache_dir = cache_dir
        self.rate_limit_seconds = rate_limit_seconds

    @abstractmethod
    def fetch(self, query: str, **kwargs) -> list[CrawlResult]:
        """Fetch raw data from the source."""
        ...

    @abstractmethod
    def normalize(self, raw: Any) -> dict:
        """Convert raw response into our schema."""
        ...

    def fetch_with_cache(self, query: str, **kwargs) -> list[CrawlResult]:
        """Check cache first, fetch if miss, store result."""
        cache_key = self._cache_key(query, **kwargs)
        cached = self._read_cache(cache_key)
        if cached is not None:
            return cached
        results = self.fetch(query, **kwargs)
        self._write_cache(cache_key, results)
        return results

    def _cache_key(self, query: str, **kwargs) -> str:
        import hashlib
        raw = f"{self.__class__.__name__}:{query}:{sorted(kwargs.items())}"
        return hashlib.sha256(raw.encode()).hexdigest()[:16]

    def _read_cache(self, key: str) -> Optional[list]:
        path = self.cache_dir / f"{key}.json"
        if path.exists():
            import json
            with open(path) as f:
                return json.load(f)
        return None

    def _write_cache(self, key: str, data: list):
        import json
        self.cache_dir.mkdir(parents=True, exist_ok=True)
        path = self.cache_dir / f"{key}.json"
        with open(path, "w") as f:
            json.dump([self._serialize_result(r) for r in data], f, indent=2, default=str)

    def _serialize_result(self, r: CrawlResult) -> dict:
        return {
            "source": r.source,
            "entity_type": r.entity_type,
            "entity_id": r.entity_id,
            "normalized_data": r.normalized_data,
            "fetched_at": r.fetched_at.isoformat(),
            "cache_key": r.cache_key,
        }
```

### 3.2 Concrete Crawler Examples

Each existing pipeline script maps to a crawler:

| Current Script | Target Crawler Class | Source |
|---------------|---------------------|--------|
| `society_discovery.py` | `SocietyDiscoveryCrawler` | Claude API + web search |
| `reddit_enrichment.py` | `RedditCrawler` | Reddit JSON API |
| `fetch_society_photos.py` | `PhotoCrawler` | DuckDuckGo image search |
| (future) | `MagicBricksCrawler` | MagicBricks listings |
| (future) | `GoogleReviewsCrawler` | Google Places API |

### 3.3 Rate Limiting and Retry

```python
# Built into BaseCrawler
import time

class RateLimiter:
    def __init__(self, min_interval: float = 2.0, max_retries: int = 3):
        self.min_interval = min_interval
        self.max_retries = max_retries
        self.last_request_time = 0.0

    def wait(self):
        elapsed = time.time() - self.last_request_time
        if elapsed < self.min_interval:
            time.sleep(self.min_interval - elapsed)
        self.last_request_time = time.time()

    def execute_with_retry(self, fn, *args, **kwargs):
        for attempt in range(self.max_retries):
            try:
                self.wait()
                return fn(*args, **kwargs)
            except Exception as e:
                if attempt == self.max_retries - 1:
                    raise
                wait_time = 2 ** attempt * self.min_interval
                print(f"  Retry {attempt+1}/{self.max_retries} after {wait_time}s: {e}")
                time.sleep(wait_time)
```

### 3.4 AI Enrichment Agent

The AI enrichment layer wraps Claude API calls with structured input/output contracts:

```python
# pipeline/ai_enrichment.py (target interface)
from abc import ABC, abstractmethod
from dataclasses import dataclass

@dataclass
class EnrichmentRequest:
    entity_type: str
    entity_id: str
    input_data: dict          # The data to enrich
    enrichment_type: str      # e.g., "summarize_reviews", "extract_signals", "score_dimensions"

@dataclass
class EnrichmentResult:
    entity_type: str
    entity_id: str
    enrichment_type: str
    output: dict              # Structured enrichment output
    model_used: str
    tokens_used: int
    cached: bool

class BaseEnricher(ABC):
    """Base class for AI enrichment tasks."""

    @abstractmethod
    def enrich(self, request: EnrichmentRequest) -> EnrichmentResult:
        ...

    def enrich_with_cache(self, request: EnrichmentRequest, cache_dir: Path) -> EnrichmentResult:
        cache_key = self._cache_key(request)
        cached = self._read_cache(cache_dir, cache_key)
        if cached:
            return EnrichmentResult(**cached, cached=True)
        result = self.enrich(request)
        self._write_cache(cache_dir, cache_key, result)
        return result
```

How Claude API fits in:
- `SocietyDiscoveryCrawler` uses Claude (via OpenAI-compatible SDK) to discover real societies given a query.
- `RedditSynthesizer` (part of `RedditCrawler`) uses Claude to synthesize Reddit threads into structured intelligence (sentiment, scores, quotes).
- `SocietyScorer` uses deterministic scoring (no AI), but the dimension weights could be tuned via AI in the future.
- Future: `IntentExtractor` uses Claude to parse natural language search queries into structured intents.

---

## 4. Caching Strategy

### 4.1 Cache Layers

```
Layer 1: HTTP Response Cache (pipeline)
  - Raw Reddit API responses, crawled HTML, image search results
  - Key: SHA256 of (source + query + params)
  - Location: data/cache/crawl/ and data/cache/reddit/
  - TTL: 24 hours for Reddit, 7 days for property listings, 30 days for images
  - Format: JSON files on disk

Layer 2: AI Enrichment Cache (pipeline)
  - Claude API responses for summarization, extraction, scoring
  - Key: SHA256 of (enrichment_type + input_data hash)
  - Location: data/cache/ai/
  - TTL: indefinite (invalidate manually when prompt/model changes)
  - Format: JSON files on disk

Layer 3: Backend Data Cache (Rust)
  - All seed data loaded into AppState at startup (current approach)
  - Intelligence data loaded on-demand for detail pages
  - Future: in-memory LRU cache for warm data
  - Invalidation: restart server after pipeline runs new data

Layer 4: Frontend Cache (browser)
  - React Query or SWR for API response caching (future)
  - staleTime: 5 minutes for list pages, 15 minutes for detail pages
  - Shortlist state: local storage (already using zustand persist)
```

### 4.2 Cache Invalidation Patterns

| Event | Invalidation |
|-------|-------------|
| New pipeline run produces updated `_ranked_results.json` | Restart backend (picks up new data) |
| Seed data updated | Restart backend |
| Embedding model changed | Full re-embed (pipeline), restart backend |
| AI prompt changed | Clear `data/cache/ai/`, re-run enrichment |
| New crawler added | No invalidation needed (additive) |

### 4.3 Cache-First Pipeline Pattern

Every pipeline step should follow:

```
1. Compute cache key from inputs
2. Check cache -> if hit, return cached result
3. Rate-limit wait
4. Fetch/compute fresh result
5. Write to cache
6. Return result
```

This is already partially implemented in `reddit_enrichment.py` and `society_discovery.py` but should be standardized through `BaseCrawler.fetch_with_cache()`.

---

## 5. End-to-End Data Flow

### 5.1 Pipeline Flow (Python)

```
                    ┌─────────────────┐
                    │  User triggers   │
                    │  pipeline run    │
                    └────────┬────────┘
                             │
              ┌──────────────┼──────────────┐
              v              v              v
     ┌──────────────┐ ┌───────────┐ ┌───────────────┐
     │  Discovery   │ │  Reddit   │ │  Photo Fetch  │
     │  (Claude)    │ │  Crawl    │ │  (DuckDuckGo) │
     └──────┬───────┘ └─────┬─────┘ └──────┬────────┘
            │               │              │
            v               v              v
     ┌──────────────────────────────────────────┐
     │         data/intelligence/{area}/         │
     │  _discovered_societies.json               │
     │  {society}/reddit.json                    │
     │  {society}/photos.json                    │
     └────────────────────┬─────────────────────┘
                          │
                          v
                 ┌────────────────┐
                 │  Scorer        │
                 │  (deterministic│
                 │   Python)      │
                 └────────┬───────┘
                          │
                          v
              ┌───────────────────────┐
              │ _ranked_results.json  │
              │ (API-ready output)    │
              └───────────────────────┘
```

Pipeline execution order:

```bash
# Full pipeline for an area:
python3 pipeline/society_discovery.py --area whitefield     # Step 1: Discover
python3 pipeline/fetch_society_photos.py                     # Step 2: Photos
python3 pipeline/reddit_enrichment.py whitefield             # Step 3: Reddit
python3 pipeline/society_scorer.py whitefield                # Step 4: Score + Rank
```

### 5.2 Backend Flow (Rust)

```
Server startup:
  1. Load data/seed/properties.json -> Vec<Property>
  2. Load data/seed/societies.json -> Vec<Society>
  3. Load data/seed/area_profiles.json -> Vec<AreaProfile>
  4. Store all in Arc<AppState>

Request: GET /api/properties
  -> Iterate AppState.properties
  -> Map to PropertyCard (join society name)
  -> Return JSON array

Request: GET /api/properties/{id}
  -> Find in AppState.properties
  -> Join society + area data
  -> Return PropertyDetail

Request: GET /api/societies/search?q=...
  -> Read data/intelligence/{area}/_ranked_results.json from disk
  -> Return pre-computed ranked results

Future:
  -> Parse query with Claude (intent extraction)
  -> Embed query vector
  -> ANN search + structured filter + re-rank
  -> Return ranked results with explanations
```

### 5.3 Frontend Flow (React)

```
Homepage:
  -> GET /api/properties -> PropertyCard[] -> render grid
  -> GET /api/areas -> AreaListItem[] -> render area cards

Property Detail:
  -> GET /api/properties/{id} -> PropertyDetailResponse
  -> Compute themes (compare.ts), market activity (market.ts)
  -> Render conviction widgets, transparency tags

Society Search:
  -> GET /api/societies/search?q=... -> SocietySearchResponse
  -> Render ranked society cards with scores, signals, cautions

Shortlist:
  -> Local zustand store (shortlist-store.ts)
  -> Compare: fetch detail for each shortlisted property
  -> Render side-by-side comparison
```

---

## 6. Future Architecture Evolution

### 6.1 Database (when product shape stabilizes)

- **PostgreSQL** for entities (properties, societies, areas, users, shortlists)
- **pgvector** extension for embeddings (keeps everything in one DB)
- Migrate from flat files: write a one-time import script that reads seed + intelligence JSON and inserts into Postgres

### 6.2 Search Service

- Move from pre-computed ranked results to live query processing
- Rust backend calls Claude API for intent extraction
- Query pgvector for semantic matches
- Apply scoring engine (port from Python to Rust)
- Return ranked results with per-property explanations

### 6.3 Real-time Data

- Webhook from pipeline to backend: "new data available for area X"
- Backend reloads affected data without full restart
- Eventually: change data capture from Postgres triggers

---

## 7. Key Design Decisions Log

| Decision | Rationale | Revisit When |
|----------|-----------|-------------|
| Flat JSON files for storage | Product shape still evolving; DB schema would be premature | >100 properties or need for relational queries |
| Pre-computed rankings | Avoids real-time AI costs; ensures consistent results | Need for personalized or live-query rankings |
| Python for pipeline, Rust for API | Python is fast to iterate on scraping; Rust gives type safety and performance for API | Never (firm split) |
| FAISS for local embeddings | Zero infrastructure; good enough for <10K entities | >10K entities or need for distributed search |
| Claude for AI enrichment | Best quality for summarization and extraction | Cost becomes a concern at scale |
| No database yet | Premature optimization; flat files are sufficient | Product surfaces stabilize, need user state |
