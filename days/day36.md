# Day 36: Fuzzy Testing Checkpoint — Does Context Search Actually Work?

## 1. Goal

Pause building. Test whether the context-based search system (Days 31-35) actually produces results that feel right for messy, real-world buyer queries. Document what works and what doesn't.

## 2. Product Reason

We've built: enriched embeddings, structured preference parsing, society-first scoring with graph facts, negative signal penalties, area inheritance, explanation cards, and semantic recall. But we haven't tested whether it all comes together to produce search results that a real buyer would find useful.

This is the fuzzy testing checkpoint from the spec. It's explicitly not optional. The difference between a technically functional search and a product-quality search will only be visible through manual evaluation of messy queries.

## 3. Deliverables

### D1: Evaluation script `pipeline/eval_search.py`

Script that:
1. Runs a curated set of test queries against the live backend
2. Captures full response (intent, results, also_consider, explanation cards)
3. Outputs a structured evaluation report

### D2: Curated test query set (20 queries)

**Intent clarity tests:**
```
1. "3 BHK Whitefield around 2.5 cr family friendly"  (clear, structured)
2. "quiet apartment near metro whitefield"  (mix of hard + soft)
3. "good society for family in east bangalore"  (no price, area-level)
```

**Soft preference tests:**
```
4. "something calmer for my parents, less chaos, more breathing room"
5. "good family project but not fake luxury and not too dense"
6. "society that feels easier to live in, not just impressive on paper"
7. "peaceful less crowded with greenery, okay to be slightly far from city"
```

**Negative preference tests:**
```
8. "avoid water issues, no tanker dependency"
9. "don't want maintenance headaches or shady builder"
10. "not too packed, avoid highway noise and construction dust"
```

**Archetype tests:**
```
11. "best investment opportunity in whitefield under 1.5 cr"  (investor)
12. "safe legal paperwork, reliable builder, no risk"  (risk-averse)
13. "affordable 2BHK for young couple, good commute"  (value buyer)
14. "premium 4BHK, builder reputation matters, willing to pay more"  (luxury)
```

**Contradiction/ambiguity tests:**
```
15. "cheap but also premium quality"  (contradictory)
16. "near whitefield but avoid traffic"  (tension)
17. "good for family AND good investment"  (dual-intent)
```

**Edge cases:**
```
18. "apartments"  (vague, no context)
19. "tell me what's actually good in bangalore"  (fully exploratory)
20. "something like prestige lakeside but cheaper"  (comparative)
```

### D3: Evaluation rubric

For each query result set, evaluate (1-5 scale):

| Dimension | What it measures |
|-----------|-----------------|
| **Intent understanding** | Did it correctly parse what the user wants? |
| **Relevance** | Are the top 3 results reasonable for this query? |
| **Differentiation** | Do different queries produce different rankings? |
| **Negative handling** | Are negative preferences actually penalizing results? |
| **Concern surfacing** | Are concerns being raised for the right queries? |
| **Explanation quality** | Do explanations feel specific, not generic? |
| **Confidence honesty** | Does "low confidence" appear when evidence is thin? |
| **Also-consider quality** | Are semantic suggestions useful (when shown)? |

### D4: Automated checks

Some things can be checked programmatically:
- `buyer_archetype` detected correctly for archetype tests
- `negative_preferences` populated for negative preference tests
- `concerns` array non-empty when query mentions avoidance
- `also_consider` populated when primary results are sparse
- Different rankings for "family friendly" vs "investment opportunity" in same area
- `graph_driven_pct` > 50% for most queries (indicating graph is doing the work)

### D5: Evaluation report

Output: `docs/eval_search_v1.md`

Structure:
1. Summary: overall quality score, biggest wins, biggest gaps
2. Per-query results: intent parsed, top 3 results, explanation quality, issues found
3. Pattern analysis: what types of queries work well? what fails?
4. Priority fixes: ranked list of issues to address in Days 37-38

### D6: Comparison against baselines

For 5 representative queries, show results from:
- **Baseline A:** structured filter only (no graph scoring, no preferences)
- **Baseline B:** current production search (before Days 31-35 changes)
- **New system:** society-first graph scoring with explanation cards

This makes it concrete whether the new system is actually better.

## 4. Technical Guidance

**Files to create:**
- `pipeline/eval_search.py` — evaluation runner (~150 lines)
- `docs/eval_search_v1.md` — evaluation report (generated)

**How the eval script works:**
```python
import json, urllib.request

QUERIES = [...]  # 20 test queries
BASE_URL = "http://localhost:4000"

def run_eval():
    results = []
    for query in QUERIES:
        url = f"{BASE_URL}/api/search?q={urllib.parse.quote(query)}"
        resp = json.loads(urllib.request.urlopen(url).read())
        results.append({
            "query": query,
            "intent": resp.get("intent"),
            "num_results": len(resp.get("results", [])),
            "top_3": [summarize(r) for r in resp["results"][:3]],
            "also_consider_count": len(resp.get("also_consider", [])),
            "has_concerns": any(r.get("concerns") for r in resp["results"][:3]),
            "graph_driven_pct": avg_graph_pct(resp["results"][:3]),
            "buyer_archetype": resp.get("intent", {}).get("buyer_archetype"),
        })
    return results
```

**The manual evaluation is the important part.** The script collects data. A human (you) reads the results and judges whether they feel right.

**Backend must be running** for this day. Start with `cargo run` in backend/.

## 5. Constraints

- Do NOT write code fixes during this day — only evaluate and document
- Do NOT skip the manual evaluation — automated checks alone don't test "feel"
- Run against the LIVE system with real KG data, not mocked responses
- Be honest about failures — the report should surface real issues, not just celebrate wins

## 6. Success Criteria

- [ ] 20 test queries executed against live backend
- [ ] Each query's intent, top 3 results, and explanation quality documented
- [ ] Automated checks run: archetype detection, negative handling, concern surfacing
- [ ] Baseline comparison for 5 queries (filter-only vs new system)
- [ ] Evaluation report written at `docs/eval_search_v1.md`
- [ ] Priority fix list generated for Days 37-38
- [ ] `graph_driven_pct` > 50% for majority of queries
- [ ] Different buyer archetypes produce different rankings for same area
