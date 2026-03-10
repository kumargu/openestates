# Day 31: Enriched Embeddings — Embed Intelligence, Not Marketing

## 1. Goal

Re-embed all society nodes using KG fact intelligence instead of thin marketing summaries. Fix the foundation that all semantic search depends on.

## 2. Product Reason

Current embeddings are built from ~20 words of marketing copy ("Premium gated community in Whitefield. Best for: families. Signals: good-maintenance"). The KG has 10-15 rich SourcedFacts per society (maintenance scores, water supply status, family friendliness ratings, builder trust assessments) — none of which gets embedded. This means semantic search compares user queries against listing blurbs, not actual intelligence. A user searching "avoid water issues" gets similarity scores against marketing text that never mentions water problems.

This is the single highest-leverage fix for search quality.

## 3. Deliverables

### D1: Upgrade `build_summary_text()` in `pipeline/skills/embed_entity.py`

For societies, construct embedding text from KG facts:

```python
def build_summary_text(input_data: dict) -> str:
    parts = [name]
    if summary:
        parts.append(summary)

    # Include KG fact display texts
    facts = input_data.get("facts", [])
    for fact in facts:
        tmpl = fact.get("display_template", "")
        value = fact.get("value", {})
        if tmpl and value:
            data = value.get("data", "")
            display = tmpl.replace("{value}", str(data))
            parts.append(display)

    # Include known issues/complaints
    negatives = input_data.get("common_complaints", [])
    if negatives:
        parts.append(f"Known concerns: {', '.join(negatives)}")

    positives = input_data.get("common_positives", [])
    if positives:
        parts.append(f"Strengths: {', '.join(positives)}")

    return ". ".join(parts)
```

### D2: Create `pipeline/scripts/reembed_all.py` batch script

Script that:
1. Reads all society nodes from `data/knowledge/nodes/society/`
2. Also reads matching society from `data/seed/societies.json` for common_complaints/common_positives
3. Builds enriched input_data with facts included
4. Calls `EmbedEntitySkill.run()` for each
5. Pushes updated embeddings to KG via graph_client or direct file write
6. Reports: old embedding dims, new embedding text length, entities processed

### D3: Also embed area nodes

Area nodes have rich facts (waterlogging, traffic, metro access, livability) but currently have NO embeddings. Embed all 16 area nodes so they can participate in semantic search later.

### D4: Verify embedding quality with test queries

After re-embedding, run 5 test queries through the embedding similarity function and log results:
- "family friendly quiet society" → should rank family-oriented societies higher
- "avoid water issues" → should rank water-stressed societies LOWER
- "good maintenance reliable builder" → should rank well-maintained societies higher
- "investment opportunity good resale" → should rank market-active societies higher
- "peaceful less crowded greenery" → should rank low-density societies higher

Compare before/after similarity rankings to validate improvement.

## 4. Technical Guidance

**Files to modify:**
- `pipeline/skills/embed_entity.py` — upgrade `build_summary_text()`
- `pipeline/scripts/reembed_all.py` — new batch script

**Key constraint:** The embedding API is `gemini-embedding-001` (768 dims). Max input is ~2048 tokens. With 15 facts, each fact's display text is ~10-20 words. Total text should be ~200-400 words — well within limits.

**Data flow:**
1. Read society node JSON from `data/knowledge/nodes/society/{slug}.json`
2. Read society seed data from `data/seed/societies.json` for common_complaints/positives
3. Merge into input_data dict
4. Call `build_summary_text()` → enriched text
5. Call `call_embedding_api(text)` → 768-dim vector
6. Write updated `summary_embedding` back to node JSON file

**Important:** Use atomic file writes (write to .tmp, rename) to avoid corrupting node files.

## 5. Constraints

- Do NOT change the embedding model or dimensions (stay on gemini-embedding-001, 768-dim)
- Do NOT embed properties — they're found through hard constraints, not semantic search
- Do NOT add new vector indexes or a vector DB — brute-force is fine at this scale
- Keep `embed_entity.py` backward compatible — it should work for any entity type
- Rate limit embedding API calls: max 1 per second (Google rate limits)

## 6. Success Criteria

- [ ] `build_summary_text()` for societies includes KG fact display texts and complaints/positives
- [ ] All 48+ society nodes re-embedded with enriched text
- [ ] All 16 area nodes embedded (they had no embeddings before)
- [ ] Test query "avoid water issues" ranks water-stressed societies lower by cosine similarity
- [ ] Test query "family friendly" ranks family-oriented societies higher by cosine similarity
- [ ] Embedding text length is 100-500 words per entity (not 20 words like before)
- [ ] `cargo check` passes
- [ ] `npm run build` passes
