# Skill: Identify Knowledge Gaps

## When to use
When you need to understand what data is missing for a society and prioritize what to enrich next. This drives the learning flywheel — every gap found becomes an enrichment task.

## How to run

```bash
# Coverage matrix across all societies (sorted worst → best)
python3 -m pipeline.skills.identify_gaps --summary

# Single society gap analysis
python3 -m pipeline.skills.identify_gaps --id soc-prestige-lakeside-habitat
```

## What it checks

Compares each society's knowledge graph facts against the "fully enriched" template:

| Priority | Facts | Enrichment Action |
|----------|-------|-------------------|
| P1 | name, area, builder_name | seed_data |
| P2 | google_rating, google_review_count, google_sentiment, rera_verified | fetch_google_reviews, verify_rera |
| P3 | reddit_threads, reddit_thread_count, year_built, total_units | search_reddit, seed_data |
| P4 | maintenance_quality, family_suitability, noise_level, security_quality | learn_society |
| P5 | score_maintenance_quality, score_family_friendly, overall_score | score_society |

## Output

- **Coverage**: 0-100% of desired facts present
- **Readiness**: "ready" (scored + reviewed) / "partial" (some data) / "minimal" (identity only) / "empty"
- **Next action**: the single most impactful enrichment skill to run
- **Enrichment queue**: aggregated across all societies — which skills to run for maximum coverage gain

## Cost
Zero — no API calls. Pure logic comparing what exists vs what's desired.

## Using the output

The enrichment queue tells you exactly what to do next:
```
fetch_google_reviews: 37 societies
learn_society: 4 societies
search_reddit: 1 society
```

Run the recommended skills in order of queue size for maximum impact.
