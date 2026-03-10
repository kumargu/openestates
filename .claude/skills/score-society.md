# Skill: Score a Society

## When to use
When you need to produce dimension scores for a society using all available knowledge graph data. This is the first "Claude Skills as ML" implementation — Claude reads raw data and produces structured scores with explanations.

## Architecture

```
Knowledge Graph (facts) → score_society.py → Gemini/Claude API → SourcedFacts → push back to graph
```

The skill reads ALL facts for a society (Google reviews, Reddit threads, RERA status, seed metadata) and produces 6 dimension scores with explanations. No hardcoded scoring logic — the LLM applies judgment.

## How to run

```bash
# Score a single society
python3 -m pipeline.skills.score_society --id soc-prestige-lakeside-habitat

# Score all societies (uses cache — won't re-score already scored)
python3 -m pipeline.skills.score_society

# Force re-score (ignores cache)
python3 -m pipeline.skills.score_society --force

# Run effectiveness test (3 societies with ground truth expectations)
python3 -m pipeline.skills.score_society --test

# Score first N societies
python3 -m pipeline.skills.score_society --limit 5
```

## Dimensions scored

| Dimension | What it measures | User preferences answered |
|-----------|-----------------|--------------------------|
| maintenance_quality | Common area upkeep, staff responsiveness | "well maintained", "good society" |
| family_friendly | Child safety, amenities, schools nearby | "family friendly", "kids" |
| builder_trust | Builder reputation, RERA, delivery track record | "trusted builder", "reliable" |
| value_for_money | Price fairness for what you get | "good value", "affordable" |
| calm_environment | Noise, traffic, green spaces | "quiet", "peaceful", "green" |
| community_vibe | Resident satisfaction, social life | "good community", "friendly" |

## Data sources consumed

The skill reads from:
1. **Knowledge graph node** (`data/knowledge/nodes/society/{slug}.json`) — all facts with confidence + source
2. **Seed data** (`data/seed/societies.json`) — builder, year, units, area

Fact types the LLM sees:
- `google_rating`, `google_review_count`, `google_sentiment`, `google_top_positives`, `google_top_negatives`
- `reddit_thread_count`, `reddit_total_score`, `reddit_threads` (thread titles)
- `rera_verified`, `rera_status`, `rera_verification_score`
- `maintenance_quality`, `family_suitability`, `noise_level` (from learn_society)
- Any other facts that exist — the skill is additive

## Output

Produces these SourcedFacts (pushed to knowledge graph):
- `score_maintenance_quality` (Numeric, 0-100)
- `score_maintenance_quality_reason` (Text — explanation)
- `score_family_friendly` + reason
- `score_builder_trust` + reason
- `score_value_for_money` + reason
- `score_calm_environment` + reason
- `score_community_vibe` + reason
- `overall_score` (Numeric, 0-100)
- `best_for_label` (Text — e.g., "Best for families")
- `one_line_verdict` (Text)
- `top_signals` (Tags)
- `top_cautions` (Tags)

## Effectiveness test

The `--test` flag runs 3 known societies with ground truth expectations:

| Society | Key test | Why |
|---------|----------|-----|
| Sobha Marvella | family_friendly >= 55 | Good amenities + security |
| Prestige Lakeside Habitat | family_friendly <= 60 | Child safety incidents on Reddit |
| Abhee Celestial City | maintenance <= 55 | Google reviews cite real issues |

Expected pass rate: 80%+. Current: 100%.

## Cost

~$0.015 per society (Gemini 2.5 Flash). Full 48-society run ≈ $0.72.

## Troubleshooting

- **No facts produced**: Check if knowledge graph node exists for that society. Run enrichment skills first.
- **JSON parse error**: Gemini sometimes truncates output. The skill has JSON repair logic. If it still fails, the response was too long — check if the society has an unusually large number of facts.
- **Scores seem wrong**: Run `--test` to validate calibration. Check the knowledge graph data — bad input = bad scores.
- **API error**: Check `GOOGLE_AI_API_KEY` in `.env`. Falls back to `ANTHROPIC_API_KEY` if available.
