# Skill: Rank Societies for User Intent

## When to use
When you need to rank societies based on a specific user search query. This replaces hardcoded ranking logic — the LLM reads all available knowledge graph data and produces context-sensitive rankings with explanations.

## How to run

```bash
# Rank for a query (auto-detects area from query, or specify --area)
python3 -m pipeline.skills.rank_for_intent "quiet family apartment Whitefield"
python3 -m pipeline.skills.rank_for_intent "best value 3bhk Sarjapur Road" --area Sarjapur
python3 -m pipeline.skills.rank_for_intent "premium society with good maintenance Bellandur"

# Run effectiveness test
python3 -m pipeline.skills.rank_for_intent --test
```

## How it works

1. Parses the user query for area/intent
2. Loads all candidate societies for that area (up to 15)
3. For each candidate, loads ALL knowledge graph facts + seed data
4. Sends everything to Gemini Flash with ranking instructions
5. Returns ranked list with per-society explanations

## What makes this different from traditional ranking

- **Query-specific**: "quiet" query penalizes noisy societies, "family" query penalizes safety issues
- **Evidence-based**: cites specific facts (Google rating, Reddit incidents, RERA status)
- **Explains tradeoffs**: every ranked society has a "why" and a "tradeoff"
- **No hardcoded weights**: the LLM decides what matters based on the query

## Output format

```json
{
  "query_understood": "User wants a quiet family apartment in Whitefield",
  "ranking": [
    {
      "rank": 1,
      "slug": "prestige-somerville",
      "score": 85,
      "why": "No noise complaints, praised for open spaces and security",
      "tradeoff": "Higher pricing and limited availability",
      "match_tags": ["quiet", "family-friendly", "spacious"]
    }
  ]
}
```

## Cost
~$0.005-0.01 per query (Gemini 2.5 Flash). Depends on number of candidates.

## Key behaviors proven in testing

- Ranked Prestige Lakeside Habitat LAST for "family" query because Reddit data shows child safety incidents
- Ranked Abhee Celestial City LAST for "value" query because Google reviews cite construction quality issues
- The skill reads evidence and makes judgment calls, not pattern matching
