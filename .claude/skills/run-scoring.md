# Skill: Run the Scoring/Ranking Engine

## When to use
When you need to score and rank societies for an area, or when you need to modify the scoring logic.

## Quick start

```bash
# Score all societies in an area
python3 pipeline/society_scorer.py whitefield

# Output: data/intelligence/whitefield/_ranked_results.json
```

## How the scorer works

The scorer (`pipeline/society_scorer.py`) is fully deterministic -- no AI calls. It reads pre-computed intelligence and produces ranked results.

### Input data sources

| Source | Path | Required? |
|--------|------|-----------|
| Discovered societies | `data/intelligence/{area}/_discovered_societies.json` | Yes |
| Reddit synthesis | `data/intelligence/{area}/{society}/reddit.json` | No (defaults to 50) |
| Seed societies | `data/seed/societies.json` | No (enhances scores) |
| Area profiles | `data/seed/area_profiles.json` | No (used for value scoring) |
| Photos | `data/intelligence/{area}/{society}/photos.json` | No (passed through) |

### Scoring dimensions

| Dimension | Weight | Score Range | Primary Signal |
|-----------|--------|-------------|----------------|
| `family_friendly` | 30% | 0-100 | Reddit family_suitability.score |
| `maintenance_quality` | 20% | 0-100 | Reddit maintenance_signal + seed sentiment |
| `school_access` | 15% | 0-100 | Seed positives + area infrastructure_tags |
| `calm_environment` | 15% | 0-100 | Reddit complaints (noise/traffic inverse) |
| `builder_trust` | 10% | 0-100 | Builder name lookup (hardcoded tiers) |
| `value` | 10% | 0-100 | Price range vs area median |

Overall score = weighted sum of dimension scores.

### Output structure

The scorer produces `_ranked_results.json` which the backend serves directly at `GET /api/societies/search`:

```json
{
  "query_interpreted": {
    "original": "family-friendly society in Whitefield",
    "area": "Whitefield",
    "city": "Bengaluru",
    "intent": "family-friendly",
    "weights_applied": { "family_friendly": 0.30, ... }
  },
  "results": [
    {
      "slug": "prestige_lakeside_habitat",
      "name": "Prestige Lakeside Habitat",
      "overall_score": 72.5,
      "rank": 1,
      "dimension_scores": { "family_friendly": 80, "maintenance_quality": 75, ... },
      "best_for_label": "Best for families",
      "life_fit_reason": "...",
      "signals": ["Family-friendly", "Well-maintained", ...],
      "cautions": ["Traffic congestion", ...],
      "confidence": "high",
      "evidence": { "reddit_threads": 15, "has_seed_data": true, ... },
      ...
    }
  ],
  "result_count": 15,
  "area_context": { ... },
  "enrichment_status": { ... }
}
```

## Modifying the scoring logic

### Change dimension weights

Edit the `FAMILY_WEIGHTS` dict at the top of `pipeline/society_scorer.py`:

```python
FAMILY_WEIGHTS = {
    "family_friendly": 0.30,
    "maintenance_quality": 0.20,
    "school_access": 0.15,
    "calm_environment": 0.15,
    "builder_trust": 0.10,
    "value": 0.10,
}
```

Weights must sum to 1.0.

### Add a new scoring dimension

1. Write a `score_my_dimension(synthesis, seed, discovered)` function
2. Add it to the `scores` dict in `score_society()`
3. Add its weight to `FAMILY_WEIGHTS`
4. Adjust other weights so they sum to 1.0
5. Add the dimension to `pick_best_for_label()` labels dict
6. Add signal/caution chips in `build_signals()` / `build_cautions()`

### Modify builder trust tiers

Edit the sets at the top of the file:

```python
HIGH_TRUST_BUILDERS = {"prestige group", "brigade group", "sobha limited", ...}
MEDIUM_TRUST_BUILDERS = {"assetz property group", "vaswani group", ...}
```

### Add a new query intent (beyond "family-friendly")

Currently the scorer only supports family-friendly weighting. To support different intents:

1. Define a new weights dict (e.g., `VALUE_WEIGHTS`, `COMMUTE_WEIGHTS`)
2. Accept an `--intent` CLI argument
3. Select the appropriate weights dict based on intent
4. The rest of the scoring logic stays the same

## Running for a new area

Before scoring, you need intelligence data:

```bash
# 1. Discover societies
python3 pipeline/society_discovery.py --area sarjapur

# 2. Fetch photos
python3 pipeline/fetch_society_photos.py

# 3. Enrich with Reddit
python3 pipeline/reddit_enrichment.py sarjapur

# 4. Score
python3 pipeline/society_scorer.py sarjapur
```

## Verifying scores

```bash
# Quick ranking check
python3 -c "
import json
with open('data/intelligence/whitefield/_ranked_results.json') as f:
    data = json.load(f)
for r in data['results'][:10]:
    print(f\"#{r['rank']} {r['name']}: {r['overall_score']} ({r['confidence']})\")
    for dim, score in r['dimension_scores'].items():
        print(f\"    {dim}: {score}\")
"

# Check enrichment coverage
python3 -c "
import json
with open('data/intelligence/whitefield/_ranked_results.json') as f:
    data = json.load(f)
status = data['enrichment_status']
print(f\"Discovered: {status['societies_discovered']}\")
print(f\"Scored: {status['societies_scored']}\")
print(f\"Reddit enriched: {status['reddit_enriched']}\")
print(f\"Seed matched: {status['seed_matched']}\")
print(f\"Photos available: {status['photos_available']}\")
"
```

## After scoring

The backend reads `_ranked_results.json` from disk on each request to `/api/societies/search`, so no restart is needed. Just verify:

```bash
curl http://localhost:4000/api/societies/search | python3 -m json.tool | head -30
```
