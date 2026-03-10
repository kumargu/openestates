# Skill: Debug Pipeline Failures

## When to use
When a pipeline script fails, produces bad data, or the scored results look wrong.

## Common failure modes

### 1. Script crashes on import

**Symptom:** `ModuleNotFoundError` or `ImportError`

**Fix:**
```bash
# Activate venv
source .venv/bin/activate

# Check if dependency is installed
pip list | grep openai

# Install missing deps
pip install openai httpx
```

**Note:** Pipeline scripts use minimal dependencies: `openai`, `httpx` (optional). Most use only stdlib (`urllib`, `json`, `pathlib`).

### 2. API key missing

**Symptom:** `openai.AuthenticationError` or empty responses

**Check:**
```bash
# Verify .env exists and has keys
cat .env | grep -E "OPENAI_API_KEY|ANTHROPIC_API_KEY"
```

Pipeline scripts load `.env` manually (not via `dotenv`). Check the `_load_dotenv()` function in the script.

### 3. Reddit rate limiting

**Symptom:** HTTP 429 errors, empty reddit.json files

**Fix:**
- Wait 60+ seconds and re-run
- Already-cached responses are skipped automatically
- Increase `time.sleep()` between requests if persistent

**Verify cache:**
```bash
ls data/cache/reddit/ 2>/dev/null | wc -l
# If cache dir doesn't exist, the script doesn't use caching yet
```

### 4. Bad AI synthesis

**Symptom:** Scores look wrong, weird labels, nonsensical summaries

**Debug steps:**

```bash
# 1. Check the raw reddit data
cat data/intelligence/whitefield/prestige_lakeside_habitat/reddit.json | python3 -m json.tool | head -100

# 2. Look at the synthesis section specifically
python3 -c "
import json
with open('data/intelligence/whitefield/prestige_lakeside_habitat/reddit.json') as f:
    data = json.load(f)
print(json.dumps(data.get('synthesis', {}), indent=2))
"

# 3. Check thread count (low thread count = low confidence)
python3 -c "
import json
with open('data/intelligence/whitefield/prestige_lakeside_habitat/reddit.json') as f:
    data = json.load(f)
print('Thread count:', data.get('thread_count', {}))
"
```

If synthesis is bad:
- Delete the reddit.json for that society
- Re-run reddit enrichment: `python3 pipeline/reddit_enrichment.py whitefield`
- The cache-first pattern means you may need to also delete cached raw responses

### 5. Scorer produces unexpected rankings

**Debug steps:**

```bash
# 1. Run scorer with verbose output
python3 pipeline/society_scorer.py whitefield

# 2. Check individual society scores
python3 -c "
import json
with open('data/intelligence/whitefield/_ranked_results.json') as f:
    data = json.load(f)
for r in data['results']:
    print(f\"#{r['rank']} {r['name']}: {r['overall_score']}\")
    print(f\"  Scores: {r['dimension_scores']}\")
    print(f\"  Confidence: {r['confidence']}\")
    print()
"

# 3. Check which societies have Reddit data
python3 -c "
import json
with open('data/intelligence/whitefield/_ranked_results.json') as f:
    data = json.load(f)
for r in data['results']:
    evidence = r['evidence']
    print(f\"{r['name']}: reddit={evidence['reddit_threads']}, seed={evidence['has_seed_data']}, conf={r['confidence']}\")
"
```

### 6. Discovery finds wrong/fake societies

**Symptom:** `_discovered_societies.json` contains societies that don't exist

**Debug:**
```bash
# Check discovered societies
python3 -c "
import json
with open('data/intelligence/whitefield/_discovered_societies.json') as f:
    data = json.load(f)
for s in data['societies']:
    print(f\"{s['name']} ({s['builder']}) - {s.get('total_units', '?')} units\")
"
```

**Fix:** Edit `_discovered_societies.json` manually to remove fake entries, or run `_curated_societies.json` to maintain a human-verified list.

### 7. Backend doesn't serve new data

**Symptom:** API still returns old data after pipeline run

**Cause:** Rust backend loads data at startup into memory.

**Fix:**
```bash
# Restart the backend
cd backend && cargo run

# For seed data changes, verify the file:
python3 -m json.tool data/seed/properties.json | head -5

# For intelligence data (societies search), the backend reads from disk on each request
# so it should pick up changes automatically. If not, check the path:
ls -la data/intelligence/whitefield/_ranked_results.json
```

### 8. Frontend shows stale data

**Fix:**
- Hard refresh: Cmd+Shift+R
- Check browser dev tools Network tab for cached responses
- Verify backend is serving correct data: `curl http://localhost:4000/api/properties | python3 -m json.tool | head -20`

## Data integrity checks

```bash
# Validate all JSON files are parseable
for f in data/seed/*.json; do
    python3 -m json.tool "$f" > /dev/null 2>&1 || echo "INVALID: $f"
done

for f in data/intelligence/whitefield/*/*.json; do
    python3 -m json.tool "$f" > /dev/null 2>&1 || echo "INVALID: $f"
done

# Check property IDs are unique
python3 -c "
import json
with open('data/seed/properties.json') as f:
    props = json.load(f)
ids = [p['id'] for p in props]
dupes = [x for x in ids if ids.count(x) > 1]
if dupes:
    print(f'DUPLICATE IDs: {set(dupes)}')
else:
    print(f'All {len(ids)} property IDs are unique')
"

# Check society IDs referenced by properties exist
python3 -c "
import json
with open('data/seed/properties.json') as f:
    props = json.load(f)
with open('data/seed/societies.json') as f:
    socs = json.load(f)
soc_ids = {s['id'] for s in socs}
missing = {p['society_id'] for p in props if p['society_id'] not in soc_ids}
if missing:
    print(f'Properties reference missing societies: {missing}')
else:
    print('All society references valid')
"
```

## Full pipeline reset for an area

If everything is broken and you want to start fresh:

```bash
# Delete all intelligence for the area
rm -rf data/intelligence/whitefield/

# Re-run full pipeline
python3 pipeline/society_discovery.py --area whitefield
python3 pipeline/fetch_society_photos.py
python3 pipeline/reddit_enrichment.py whitefield
python3 pipeline/society_scorer.py whitefield

# Restart backend
cd backend && cargo run
```
