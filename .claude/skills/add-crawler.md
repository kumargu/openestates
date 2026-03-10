# Skill: Add a New Data Crawler

## When to use
When you need to add a new data source to the OpenEstates pipeline (e.g., MagicBricks listings, Google Reviews, 99acres, NoBroker, Housing.com).

## Prerequisites
- Understand the target data source's API or web structure
- Know what entity type(s) the crawler produces (property, society, area, review)
- Read `docs/architecture_v2.md` section 3 for the crawler interface

## Steps

### 1. Create the crawler file

Create `pipeline/{source_name}_crawler.py`. Follow the naming convention of existing crawlers.

Look at `pipeline/reddit_enrichment.py` and `pipeline/society_discovery.py` as reference implementations.

### 2. Required structure

Every crawler script must have:

```python
"""
{Source} Crawler for OpenEstates

Usage:
  python3 pipeline/{source_name}_crawler.py {area_slug}
  python3 pipeline/{source_name}_crawler.py --all
"""

from pathlib import Path
import json, time, hashlib

PROJECT_ROOT = Path(__file__).resolve().parent.parent
INTELLIGENCE_DIR = PROJECT_ROOT / "data" / "intelligence"
CACHE_DIR = PROJECT_ROOT / "data" / "cache" / "{source_name}"
```

### 3. Implement the core functions

```python
def fetch(query: str, **kwargs) -> list[dict]:
    """Fetch raw data from the source. Always cache-first."""
    cache_key = _cache_key(query, **kwargs)
    cached = _read_cache(cache_key)
    if cached is not None:
        print(f"  Cache hit: {cache_key}")
        return cached

    # Actual fetch logic here
    # ALWAYS rate limit: time.sleep(2.0) between requests
    raw_data = _do_fetch(query, **kwargs)

    _write_cache(cache_key, raw_data)
    return raw_data

def normalize(raw: dict) -> dict:
    """Convert raw API response into OpenEstates schema."""
    # Map source fields to our entity fields
    return {
        "source": "{source_name}",
        "entity_type": "...",
        "entity_id": "...",
        "data": { ... },
        "fetched_at": datetime.now().isoformat(),
    }

def store(area_slug: str, entity_slug: str, normalized: dict):
    """Write normalized data to the intelligence directory."""
    output_dir = INTELLIGENCE_DIR / area_slug / entity_slug
    output_dir.mkdir(parents=True, exist_ok=True)
    output_path = output_dir / "{source_name}.json"
    with open(output_path, "w") as f:
        json.dump(normalized, f, indent=2, ensure_ascii=False)
```

### 4. Cache helper functions

```python
def _cache_key(query: str, **kwargs) -> str:
    raw = f"{query}:{sorted(kwargs.items())}"
    return hashlib.sha256(raw.encode()).hexdigest()[:16]

def _read_cache(key: str):
    path = CACHE_DIR / f"{key}.json"
    if path.exists():
        with open(path) as f:
            return json.load(f)
    return None

def _write_cache(key: str, data):
    CACHE_DIR.mkdir(parents=True, exist_ok=True)
    with open(CACHE_DIR / f"{key}.json", "w") as f:
        json.dump(data, f, indent=2, default=str)
```

### 5. Add rate limiting

- Minimum 2 seconds between HTTP requests
- Exponential backoff on 429/5xx responses (2s, 4s, 8s)
- Maximum 3 retries per request
- Print progress: `print(f"  Fetching {url}...")` so operator can monitor

### 6. Add a CLI entrypoint

```python
if __name__ == "__main__":
    import sys
    if len(sys.argv) < 2:
        print("Usage: python3 pipeline/{source_name}_crawler.py <area_slug>")
        sys.exit(1)
    area_slug = sys.argv[1].lower().strip()
    run(area_slug)
```

### 7. Wire into the scorer (if applicable)

If the crawler produces data that affects scoring:
1. Open `pipeline/society_scorer.py`
2. In `score_society()`, load the new data file
3. Add a new scoring dimension or enhance an existing one
4. Update the weights dictionary if needed

### 8. Test

```bash
# Run for a single area
python3 pipeline/{source_name}_crawler.py whitefield

# Verify output
cat data/intelligence/whitefield/{some_society}/{source_name}.json | python3 -m json.tool

# Run scorer to verify integration
python3 pipeline/society_scorer.py whitefield
```

## Output locations

- Raw cached responses: `data/cache/{source_name}/`
- Normalized data: `data/intelligence/{area_slug}/{society_slug}/{source_name}.json`
- Summary/index files: `data/intelligence/{area_slug}/_{source_name}_summary.json`

## Checklist

- [ ] File created at `pipeline/{source_name}_crawler.py`
- [ ] Has docstring with usage instructions
- [ ] Implements fetch, normalize, store pattern
- [ ] Uses cache-first approach
- [ ] Rate limits all HTTP requests (minimum 2s between requests)
- [ ] Has retry logic with exponential backoff
- [ ] Writes output to `data/intelligence/{area}/{society}/`
- [ ] Has `__main__` CLI entrypoint
- [ ] Prints progress so operator can monitor
- [ ] Does NOT import from `agents/` or `engine/` (those are v1 code)
