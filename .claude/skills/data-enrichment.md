# Skill: Data Enrichment Pipeline

## When to use
When you need to enrich entities (societies, properties, areas) with data from external sources and AI-generated intelligence.

## Architecture: Skills Think, Graph Remembers

```
Scripts fetch raw data → Skills apply judgment → Knowledge Graph stores facts → Backend serves
```

Skills are the intelligence layer. They replace traditional ML by reading raw data and producing scored, explained SourcedFacts.

## Available Skills

| Skill | File | What it does | Cost |
|-------|------|-------------|------|
| **identify_gaps** | `pipeline/skills/identify_gaps.py` | Find what data is missing, prioritize enrichment | Free |
| **score_society** | `pipeline/skills/score_society.py` | Score 6 dimensions with explanations | ~$0.015/society |
| **rank_for_intent** | `pipeline/skills/rank_for_intent.py` | Rank societies for a user query | ~$0.007/query |
| **search_reddit** | `pipeline/skills/search_reddit.py` | Fetch Reddit threads for a society | Free |
| **learn_society** | `pipeline/skills/learn_society.py` | Synthesize Reddit into structured facts | ~$0.02/society |
| **fetch_google_reviews** | `pipeline/skills/fetch_google_reviews.py` | Fetch Google Maps reviews | Free |
| **verify_rera** | `pipeline/skills/verify_rera.py` | Check RERA compliance | Free |
| **embed_entity** | `pipeline/skills/embed_entity.py` | Compute vector embedding | ~$0.001/entity |
| **fetch_images** | `pipeline/skills/fetch_images.py` | Download society photos | Free |

## Recommended enrichment order

Start with `identify_gaps --summary` to see what's needed, then:

```bash
# 1. See what's missing
python3 -m pipeline.skills.identify_gaps --summary

# 2. Fill the biggest gaps first (usually Google reviews)
# Then Reddit, then LLM synthesis, then scoring
python3 -m pipeline.skills.run_skill fetch_google_reviews --entity soc-xyz
python3 -m pipeline.skills.run_skill search_reddit --entity soc-xyz
python3 -m pipeline.skills.run_skill learn_society --entity soc-xyz
python3 -m pipeline.skills.score_society --id soc-xyz

# 3. Verify coverage improved
python3 -m pipeline.skills.identify_gaps --id soc-xyz
```

## Adding a new enrichment skill

1. Create `pipeline/skills/your_skill.py` extending `BaseSkill`
2. Produce `SourcedFact` entries with:
   - `display_template` — how to render for users
   - `answers_preferences` — which search preferences it satisfies
   - `scoring_hint` — how it affects ranking
3. Add to `DESIRED_FACTS` in `identify_gaps.py` so gaps are tracked
4. Add playbook at `.claude/skills/your-skill.md`

No Rust code changes needed — the knowledge graph accepts any self-describing fact.

## Environment

```bash
# Required for LLM skills:
GOOGLE_AI_API_KEY=...   # Gemini Flash (primary)
ANTHROPIC_API_KEY=...    # Claude (fallback)
```
