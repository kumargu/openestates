# Context-Based Search — Implementation Spec (Claude's Take)

**Author:** Claude, based on codebase audit of OpenEstates as of 2026-03-10
**Status:** Proposal for review

---

## What exists today (honest assessment)

Before proposing anything, here's what the system already does well and where it falls short.

### Already working

1. **Intent parsing** — Gemini extracts area, BHK, budget, preferences from natural language. Returns structured `SearchIntent`. This works.

2. **Knowledge graph with self-describing facts** — 55 society nodes, ~5000+ SourcedFacts, each carrying `display_template`, `answers_preferences`, `scoring_hint`. This is genuinely well-designed. Adding a new dimension requires zero Rust code changes.

3. **Graph-driven preference scoring** — When a user says "quiet neighborhood," the system looks up facts whose `answers_preferences` includes that phrase, applies the fact's own `scoring_hint`, and returns a scored match with provenance. The legacy hardcoded fallback shrinks as skills enrich more entities.

4. **Semantic embedding boost** — 768-dim embeddings on society nodes, cosine similarity search, adds up to 15% boost to matching societies. Brute-force, fine at current scale.

5. **Live discovery** — When search has no good matches, Gemini + Google Search discovers new properties/societies, ingests them into KG + seed data, and re-runs search. The flywheel works.

6. **Match explanations** — Structured reasons with fact_key, confidence, source_type. Preference coverage (matched/partial/no_data). Graph-driven percentage. This is real transparency.

7. **Enrichment queue** — Search gaps trigger enrichment tasks, prioritized by query frequency. The loop exists conceptually.

### What's actually broken or missing

1. **Search scores properties, not societies.** The user's real question is "which society should I live in?" but the system returns individual property listings scored against preferences. Society-level intelligence exists in the graph but only appears as a secondary boost, not the primary retrieval unit.

2. **No negative signal scoring.** The spec you wrote emphasizes this heavily, and it's right. Today, if a society has `waterlogging_risk: "high"` as a fact, and the user says "avoid water issues," there's no penalty mechanism. The `scoring_hint` system supports `LowerIsBetter` direction, but the search route doesn't use it for negative preference matching.

3. **Preference expansion is absent.** "Family friendly" today matches facts with `answers_preferences: ["family friendly"]`. But it doesn't expand to also check "child safety," "playground," "school nearby," "low density." The system matches literally, not semantically.

4. **No issue-level evidence retrieval.** Society facts include positives and negatives, but there's no structured issue taxonomy. A society with 3 separate Reddit threads mentioning water problems and 1 mentioning maintenance issues — that signal structure is lost. Everything is flattened into summary text.

5. **Area intelligence is disconnected from ranking.** Area nodes have waterlogging, traffic, metro summaries. But these don't flow into property/society scoring. A property in a waterlogging-prone area gets no penalty from area-level signals.

6. **Embedding coverage is incomplete.** Only societies with `summary_embedding` get semantic boost. Many discovered societies lack embeddings. Properties and areas have no embeddings at all.

7. **The Python scoring engine (`engine/`) is completely disconnected.** It has 7 well-designed scoring dimensions (value, commute, society_quality, risk, greenery, resale, market_activity) but none of them are called during live search. The Rust backend does its own simpler scoring.

8. **No buyer archetype influence on ranking.** Intent parsing extracts preferences but doesn't model buyer type. A family buyer and an investor asking about the same area get identical ranking weights.

---

## What I'd actually build (and what I wouldn't)

### The core insight

You don't need 8 separate vector indexes. You don't need 5 LLM skill calls per query. You don't need a BM25 engine.

What you need is:

> **Make the knowledge graph the search engine.**

The graph already has the richest structured intelligence in the system. Society nodes have scored dimensions, issue signals, resident sentiment, builder trust, area context — all as typed, sourced facts. The gap isn't data. The gap is that search doesn't fully use this data for ranking.

The architecture should be:

```
Query → Intent parse (existing, works)
     → Graph-powered scoring (the big upgrade)
     → Embedding recall for broadening (existing, extend)
     → Explanation synthesis (existing, improve)
```

Not:

```
Query → Intent parse → Query expansion LLM call → Source routing LLM call
     → 8 vector indexes → Retrieve → Assemble → Ranking weights LLM call
     → Re-rank → Explanation LLM call
```

The second architecture is 4-5 LLM calls per search, $0.02-0.10 per query, 3-8 seconds latency, and hard to debug. The first is mostly deterministic graph traversal with one LLM call for intent parsing (already exists) and optional semantic broadening.

---

## The spec

### Layer 1: Query Understanding (upgrade existing)

**What exists:** Gemini extracts area, BHK, budget, preferences as string list.

**What to add:**

```rust
struct SearchIntent {
    raw_query: String,
    areas: Vec<String>,
    bhk: Option<Vec<u8>>,
    budget_min: Option<f64>,
    budget_max: Option<f64>,
    possession_status: Option<Vec<String>>,
    builders: Option<Vec<String>>,

    // NEW: structured preference model
    positive_preferences: Vec<PreferenceSignal>,
    negative_preferences: Vec<PreferenceSignal>,
    buyer_archetype: Option<BuyerArchetype>,
}

struct PreferenceSignal {
    raw_text: String,           // "avoid water issues"
    canonical_key: String,      // "water_supply"
    expanded_keys: Vec<String>, // ["waterlogging_risk", "tanker_dependency", "borewell"]
    weight: f32,                // 1.0 default, higher if user emphasized
    polarity: Polarity,         // Positive or Negative
}

enum BuyerArchetype {
    Family,
    Investor,
    EndUser,
    RiskAverse,
    ValueBuyer,
    LuxuryBuyer,
}
```

**Key change: preference expansion happens at intent parse time, not as a separate skill.**

The Gemini prompt should be updated to:
1. Extract the raw preference text
2. Map it to canonical fact keys from a known taxonomy
3. Expand it to related fact keys
4. Detect polarity (positive seek vs. negative avoidance)
5. Infer buyer archetype from the overall query pattern

This is one LLM call. The expansion taxonomy can be provided in the prompt as a reference table (not a separate LLM call for "query expansion").

**Preference expansion taxonomy** (provided to Gemini as context):

```
family_friendly → child_safety, playground, school_nearby, low_density, calm_environment, community_vibe
open_space → density, greenery_score, open_space_score, breathing_room
water_issues → water_supply, waterlogging_risk, tanker_dependency, borewell_status
quiet → noise_score, traffic_score, calm_environment
good_maintenance → maintenance_quality, maintenance_cost, society_management
builder_trust → builder_reputation, delivery_track_record, quality_perception
commute → metro_distance, traffic_score, road_quality
investment → resale_strength, market_activity, area_trend, rental_yield
legal_safety → rera_status, document_completeness, litigation_risk
```

This table is finite and deterministic. It doesn't need an LLM to route. It needs an LLM to map the user's fuzzy language onto these canonical keys. That mapping is what Gemini already does for basic preferences — we just need to make it richer.

**Why not a separate "query expansion skill"?**

Because the expansion is a closed vocabulary problem. The fact keys in the knowledge graph are known. The mapping from human language to these keys is the LLM's job, but it's a single classification task, not an open-ended reasoning task. Putting it in the same intent-parse call saves a round trip and keeps the system simpler.

---

### Layer 2: Society-First Scoring (the big change)

**The fundamental shift:** Search should score and rank **societies first**, then return properties within top societies.

Why? Because:
- The knowledge graph's richest intelligence is at society level
- User preferences ("family friendly," "good maintenance," "avoid water issues") are society-level questions, not property-level
- Properties within the same society share 80% of their livability characteristics
- The current system scores each property independently, which means 10 listings in the same society get 10 slightly different scores for what is essentially the same livability question

**Scoring flow:**

```
1. Filter properties by hard constraints (BHK, budget, area, possession)
2. Get unique society_ids from filtered properties
3. For each society, compute society_score from KG facts
4. For each society, compute area_score from area KG node
5. Combine: entity_score = society_score * 0.6 + area_score * 0.2 + property_fit * 0.2
6. Rank societies
7. Within each society, rank properties by property-specific fit (price, floor, area sqft)
8. Return flattened list but grouped by society context
```

**Society scoring from graph facts:**

```rust
fn score_society_for_intent(
    society_node: &Node,
    area_node: Option<&Node>,
    intent: &SearchIntent,
) -> SocietyScore {
    let mut score = 0.0;
    let mut max_possible = 0.0;
    let mut matched_reasons: Vec<MatchReason> = vec![];
    let mut concerns: Vec<Concern> = vec![];
    let mut unmatched_preferences: Vec<String> = vec![];

    // Score positive preferences
    for pref in &intent.positive_preferences {
        let fact_hits = find_facts_matching_keys(
            society_node, area_node, &pref.expanded_keys
        );

        if fact_hits.is_empty() {
            unmatched_preferences.push(pref.raw_text.clone());
            max_possible += pref.weight;
            continue;
        }

        let (pref_score, reason) = score_facts_for_preference(
            &fact_hits, &pref, Polarity::Positive
        );
        score += pref_score * pref.weight;
        max_possible += pref.weight;
        matched_reasons.push(reason);
    }

    // Score negative preferences (penalty)
    for pref in &intent.negative_preferences {
        let fact_hits = find_facts_matching_keys(
            society_node, area_node, &pref.expanded_keys
        );

        if fact_hits.is_empty() {
            // No data on this risk — note it but don't penalize
            concerns.push(Concern::no_data(pref.raw_text.clone()));
            continue;
        }

        let (penalty, concern) = score_facts_for_preference(
            &fact_hits, &pref, Polarity::Negative
        );
        score -= penalty * pref.weight;
        if penalty > 0.3 {
            concerns.push(concern);
        }
    }

    // Buyer archetype modifier
    if let Some(archetype) = &intent.buyer_archetype {
        let archetype_weights = get_archetype_weights(archetype);
        // Boost/dampen score components based on archetype
        // e.g., Family → boost family_friendly, calm_environment facts
        // e.g., Investor → boost resale_strength, market_activity facts
    }

    // Evidence confidence adjustment
    let confidence = compute_evidence_confidence(society_node);
    // More facts, higher confidence sources → higher confidence
    // Fewer facts, more LLM-only sources → lower confidence

    SocietyScore {
        society_id: society_node.id.clone(),
        raw_score: score,
        normalized_score: score / max_possible.max(1.0),
        confidence,
        matched_reasons,
        concerns,
        unmatched_preferences,
    }
}
```

**The key function — `score_facts_for_preference`:**

This is where the self-describing fact system pays off. Each fact already carries its `scoring_hint`, so the scoring function is generic:

```rust
fn score_facts_for_preference(
    facts: &[&SourcedFact],
    pref: &PreferenceSignal,
    polarity: Polarity,
) -> (f32, MatchReason) {
    // Use the best (highest confidence) fact for scoring
    let best_fact = facts.iter()
        .max_by(|a, b| a.confidence.partial_cmp(&b.confidence).unwrap())
        .unwrap();

    let raw_score = match &best_fact.scoring_hint {
        Some(hint) => apply_scoring_hint(&best_fact.value, hint),
        None => {
            // No scoring hint — use semantic similarity as fallback
            // (embedding distance between preference text and fact value text)
            0.5 // neutral
        }
    };

    let effective_score = match polarity {
        Polarity::Positive => raw_score,
        Polarity::Negative => 1.0 - raw_score, // invert for penalties
    };

    let reason = MatchReason {
        preference: pref.raw_text.clone(),
        fact_key: best_fact.key.clone(),
        display: best_fact.display_template
            .replace("{value}", &best_fact.value.to_display_string()),
        score: effective_score,
        confidence: best_fact.confidence,
        source: best_fact.source.source_type.clone(),
        supporting_facts_count: facts.len(),
    };

    (effective_score, reason)
}
```

This function doesn't know what "family_friendly" or "water_supply" means. It doesn't need to. The fact's own `scoring_hint` tells it how to score. **This is the whole point of the self-describing architecture — and right now the search doesn't fully use it.**

---

### Layer 3: Area Signal Inheritance (new)

**Problem:** A society in a flood-prone area gets no penalty today, even though the area node has `waterlogging_risk: "high"`.

**Solution:** When scoring a society, also consult the parent area node's facts. Area-level signals should flow into society scoring with reduced weight (they're contextual, not specific).

```rust
fn find_facts_matching_keys(
    society_node: &Node,
    area_node: Option<&Node>,
    keys: &[String],
) -> Vec<FactHit> {
    let mut hits = vec![];

    // Society facts — full weight
    for fact in &society_node.facts {
        if keys.contains(&fact.key) {
            hits.push(FactHit { fact, source_level: EntityLevel::Society, weight: 1.0 });
        }
    }

    // Area facts — reduced weight, only if society lacks direct evidence
    if let Some(area) = area_node {
        for fact in &area.facts {
            if keys.contains(&fact.key) {
                let society_has_direct = hits.iter().any(|h| h.fact.key == fact.key);
                let weight = if society_has_direct { 0.3 } else { 0.7 };
                hits.push(FactHit { fact, source_level: EntityLevel::Area, weight });
            }
        }
    }

    hits
}
```

**Why this matters:** This is where negative signal handling comes from without needing a separate "issues index." The area node already has waterlogging, traffic, and infrastructure facts. The society node has maintenance, safety, density facts. Querying both with the right weight hierarchy gives you issue detection for free.

---

### Layer 4: Semantic Broadening (extend existing)

**What exists:** Embedding similarity boost adds up to 15% for societies with matching embeddings.

**What to change:**

The current embedding boost is additive and small. It should play a different role: **candidate recall expansion**, not score adjustment.

```
Phase 1: Filter by hard constraints → candidate properties
Phase 2: Score candidates' societies via graph facts (Layer 2)
Phase 3: If top_k results all score < 0.4, OR if user didn't specify an area:
         → Run embedding similarity across ALL society nodes
         → Add top-N similar societies as "also consider" results
         → Score these through the same graph scoring
Phase 4: Merge, deduplicate, final rank
```

This separates two concerns:
- **Graph scoring** answers "how well does this society match what you asked for?"
- **Embedding similarity** answers "what other societies feel similar that you didn't think to ask about?"

The embedding layer becomes a discovery tool, not a scoring modifier. It surfaces societies the user wouldn't have found through explicit preferences alone.

**Embedding coverage gap:** Currently only ~40% of societies have embeddings. Fix this by running `embed_entity.py` on all 55 society nodes. This is a batch job, not a search-time concern.

---

### Layer 5: Explanation Generation (improve existing)

**What exists:** `MatchReason` with fact_key, display text, confidence. `PreferenceCoverage` showing matched/partial/no_data.

**What to improve:**

The current explanations are structured but feel mechanical. Example of what gets returned today:

```json
{
  "fact_key": "maintenance_quality",
  "display": "Maintenance quality is good",
  "confidence": 0.6,
  "source_type": "Llm"
}
```

This is transparency, but it's not *helpful* transparency. Compare:

```json
{
  "category": "positive_match",
  "preference": "good maintenance",
  "summary": "Residents generally report reliable maintenance, though some note slow response times for non-urgent requests",
  "evidence_strength": "medium",
  "evidence_sources": ["Reddit resident discussions", "Google reviews (4.3/5, 248 reviews)"],
  "nuance": "Based on online discussions — individual experience may vary"
}
```

**The change:** After graph scoring produces `SocietyScore` with matched_reasons and concerns, run a lightweight template-based explanation synthesis:

```rust
fn synthesize_explanation(
    society_score: &SocietyScore,
    society_node: &Node,
) -> ExplanationCard {
    let why_matches: Vec<String> = society_score.matched_reasons.iter()
        .filter(|r| r.score > 0.5)
        .map(|r| {
            // Use the fact's display_template as the base
            // Qualify with confidence and source
            let qualifier = match r.confidence {
                c if c >= 0.8 => "",
                c if c >= 0.5 => "Signals suggest ",
                _ => "Limited evidence suggests ",
            };
            format!("{}{}", qualifier, r.display)
        })
        .collect();

    let concerns: Vec<String> = society_score.concerns.iter()
        .map(|c| match c {
            Concern::Detected { display, confidence, .. } => {
                if *confidence > 0.7 {
                    format!("{}", display)
                } else {
                    format!("{} (based on area-level signals — verify at society level)", display)
                }
            },
            Concern::NoData { preference } => {
                format!("No data available for '{}' — this should be verified independently", preference)
            },
        })
        .collect();

    let confidence_label = match society_score.confidence {
        c if c >= 0.7 => "high",
        c if c >= 0.4 => "medium",
        _ => "low",
    };

    ExplanationCard {
        why_matches,
        concerns,
        unmatched: society_score.unmatched_preferences.clone(),
        confidence_label: confidence_label.to_string(),
        facts_consulted: society_score.matched_reasons.len() + society_score.concerns.len(),
        graph_driven_pct: compute_graph_pct(&society_score),
    }
}
```

**No LLM call needed for explanations.** The facts are self-describing. The display_templates are human-readable. Confidence qualifiers are deterministic. This keeps explanation generation at ~0ms cost and fully deterministic.

If we later want more natural prose, we can add an optional LLM polish step — but the structured explanation should always be the source of truth.

---

### Layer 6: Buyer Archetype Profiles (new, simple)

**Not an LLM skill. A config file.**

```json
{
  "family": {
    "boost_keys": ["family_friendly", "child_safety", "school_nearby", "calm_environment", "playground", "community_vibe"],
    "boost_weight": 1.5,
    "penalize_keys": ["noise_score", "traffic_score", "density"],
    "penalize_weight": 1.3,
    "description": "Prioritizes safety, community, and calm over luxury or investment returns"
  },
  "investor": {
    "boost_keys": ["resale_strength", "market_activity", "rental_yield", "metro_distance", "area_trend"],
    "boost_weight": 1.5,
    "penalize_keys": ["litigation_risk", "rera_status"],
    "penalize_weight": 1.5,
    "description": "Prioritizes returns, location demand, and legal safety"
  },
  "risk_averse": {
    "boost_keys": ["rera_status", "document_completeness", "builder_reputation", "delivery_track_record"],
    "boost_weight": 1.3,
    "penalize_keys": ["litigation_risk", "waterlogging_risk", "possession_delay"],
    "penalize_weight": 2.0,
    "description": "Strongly penalizes any risk signals, rewards legal and builder safety"
  },
  "value_buyer": {
    "boost_keys": ["value_for_money", "maintenance_cost"],
    "boost_weight": 1.5,
    "penalize_keys": ["premium_pricing"],
    "penalize_weight": 1.3,
    "description": "Optimizes for cost-effectiveness, not luxury"
  }
}
```

When intent parsing detects `buyer_archetype: "family"`, the scoring loop applies these modifiers to fact weights. This is a lookup, not an LLM call.

**Why not an LLM-based "ranking weights skill"?**

Because ranking weight selection is a closed-set classification problem. There are ~6 buyer archetypes. Each maps to a known set of weight adjustments. Using an LLM to decide "for a family buyer, should I weight family_friendly higher?" is burning tokens on a question with an obvious answer. The LLM's job is to detect the archetype from fuzzy human language (part of intent parsing). Once detected, the weight profile is deterministic.

---

### What I'm explicitly NOT building

1. **8 separate vector indexes.** One embedding space for society nodes is enough at this scale. Properties don't need embeddings — they're found through hard constraints. Areas don't need embeddings — they're found through society→area edges. Reviews and issues don't need separate indexes — their signals are already aggregated into society/area facts by the skills.

2. **BM25/lexical search engine.** At 55 societies and 155 properties, substring matching on names handles the "exact term" case. Adding Tantivy or Elasticsearch is infrastructure for a problem that doesn't exist yet.

3. **Separate "source routing skill."** Which facts to consult is determined by the preference's `expanded_keys`. If the user cares about water, we check water-related fact keys. This is a key lookup, not a routing decision that needs LLM judgment.

4. **Separate "query expansion skill."** The expansion taxonomy is finite and provided to Gemini as context during intent parsing. One call, not two.

5. **Separate "explanation synthesis skill."** The facts are self-describing. `display_template` + confidence qualifier = explanation. No LLM needed.

6. **Entity-level evidence bundles assembled from multiple retrieval passes.** The knowledge graph IS the evidence bundle. A society node already aggregates all known signals. We don't need to "assemble" evidence from chunks — the skills already did that during enrichment.

**The insight:** Your spec describes building a retrieval-augmented generation (RAG) system. But you already have something better — a **knowledge graph with self-describing, pre-scored facts**. RAG retrieves raw evidence at query time and asks an LLM to reason over it. Your system has ALREADY done the reasoning (via skills at enrichment time) and stored the conclusions as typed, scored facts. Search should read those conclusions, not re-derive them.

---

## What to actually build, in order

### Phase 1: Upgrade intent parsing (1-2 days)

**File:** `backend/src/search/text.rs` (modify `parse_intent`)

Changes:
- Update Gemini prompt to return `positive_preferences` and `negative_preferences` as structured objects with `canonical_key` and `expanded_keys`
- Provide the preference expansion taxonomy in the prompt
- Add `buyer_archetype` detection
- No new LLM calls — just a richer prompt for the existing one

**Validation:** Run 10 test queries, verify intent objects have correct expanded keys and polarity.

### Phase 2: Society-first scoring (2-3 days)

**File:** `backend/src/search/text.rs` (new function `score_society_for_intent`)

Changes:
- After hard-constraint filtering, group candidate properties by society_id
- For each unique society, look up the KG node
- Score using `score_society_for_intent` (graph facts + area inheritance)
- Apply buyer archetype weight modifiers
- Rank societies, then rank properties within each society
- Return results with society context prominent

**Validation:** Compare ranking of "family friendly Whitefield" vs "investment opportunity Whitefield" — should produce visibly different orderings.

### Phase 3: Negative signal scoring (1 day)

**File:** `backend/src/search/text.rs`

Changes:
- Implement `Polarity::Negative` handling in `score_facts_for_preference`
- Area signal inheritance for risk facts (waterlogging, traffic, etc.)
- Concern generation in explanations

**Validation:** Query "avoid water issues in Whitefield" — societies with waterlogging facts should rank lower. Explanations should surface the concern.

### Phase 4: Improved explanations (1 day)

**File:** `backend/src/search/mod.rs` (update `MatchReason`, add `ExplanationCard`)

Changes:
- Confidence-qualified display text
- Concern cards with source attribution
- "No data" signals for unmatched preferences
- Evidence strength indicator

**Validation:** Every result in a 10-query test set has at least 1 specific reason and 0-2 concerns. No generic explanations.

### Phase 5: Embedding coverage + semantic broadening (1 day)

**Task:** Run `embed_entity.py` on all 55 society nodes to get full embedding coverage.

**File:** `backend/src/search/text.rs`

Changes:
- When filtered results are sparse (< 5 or all scores < 0.4), run embedding similarity across all societies
- Add "You might also consider" section with semantically similar societies outside the explicit area filter
- Score these through the same graph scoring pipeline

**Validation:** Query with no area specified returns relevant societies from embedding recall.

### Phase 6: Fuzzy testing checkpoint (1-2 days)

**No code. Just evaluation.**

Run these queries and manually inspect results:

```
1. "something calmer for my parents, less chaos, more breathing room"
2. "good family project but not fake luxury and not too dense"
3. "okay to stretch budget if daily life is better"
4. "don't want to get trapped in a shiny project with maintenance headaches"
5. "society that feels easier to live in, not just impressive on paper"
6. "near Whitefield but avoid places that feel too packed and water-stressed"
7. "3 BHK for a young couple, modern, good resale, don't care about schools"
8. "safe investment, builder should have good track record, legal papers clean"
9. "anything affordable in East Bangalore with decent commute"
10. "luxury doesn't matter, livability matters, tell me what's actually good"
```

For each, evaluate:
- Are the top 3 results intuitively reasonable?
- Are concerns being surfaced for the right queries?
- Is the ranking different for different buyer intents?
- Do explanations feel specific or generic?
- Are negative preferences actually penalizing results?

Document observations. Fix the most impactful issues found.

---

## Architecture diagram

```
                    ┌─────────────────────┐
                    │   User query        │
                    └─────────┬───────────┘
                              │
                    ┌─────────▼───────────┐
                    │  Intent Parser       │
                    │  (Gemini Flash)      │
                    │  - hard constraints  │
                    │  - positive prefs    │
                    │  - negative prefs    │
                    │  - buyer archetype   │
                    │  - expanded keys     │
                    └─────────┬───────────┘
                              │
                 ┌────────────▼────────────┐
                 │  Hard Constraint Filter  │
                 │  (BHK, budget, area,    │
                 │   possession, builder)   │
                 └────────────┬────────────┘
                              │
                    candidate properties
                              │
                 ┌────────────▼────────────┐
                 │  Group by Society       │
                 └────────────┬────────────┘
                              │
           ┌──────────────────▼──────────────────┐
           │      Society Scoring (per society)   │
           │                                      │
           │  ┌─────────────┐  ┌──────────────┐  │
           │  │ Society Node │  │  Area Node   │  │
           │  │   KG facts   │  │  KG facts    │  │
           │  └──────┬──────┘  └──────┬───────┘  │
           │         │                │           │
           │         └───────┬────────┘           │
           │                 │                    │
           │    ┌────────────▼────────────┐       │
           │    │  Preference Matching    │       │
           │    │  + Negative Penalties   │       │
           │    │  + Archetype Weights    │       │
           │    │  + Evidence Confidence  │       │
           │    └────────────┬────────────┘       │
           │                 │                    │
           │    ┌────────────▼────────────┐       │
           │    │  SocietyScore           │       │
           │    │  - score, confidence    │       │
           │    │  - reasons, concerns    │       │
           │    │  - unmatched prefs      │       │
           │    └─────────────────────────┘       │
           └──────────────────┬──────────────────┘
                              │
              ┌───────────────▼───────────────┐
              │  If sparse results:           │
              │  Embedding Similarity Recall   │
              │  → Score new societies too     │
              └───────────────┬───────────────┘
                              │
              ┌───────────────▼───────────────┐
              │  If still sparse:             │
              │  Live Discovery (Gemini)      │
              │  → Ingest → Score → Return    │
              └───────────────┬───────────────┘
                              │
              ┌───────────────▼───────────────┐
              │  Explanation Synthesis         │
              │  (template-based, no LLM)     │
              │  - why_matches                │
              │  - concerns                   │
              │  - confidence_label           │
              │  - evidence_sources           │
              └───────────────┬───────────────┘
                              │
              ┌───────────────▼───────────────┐
              │  SearchResponse               │
              │  - intent                     │
              │  - ranked results             │
              │  - area context               │
              │  - discovery status           │
              └───────────────────────────────┘
```

---

## What this gets you vs. the original spec

| Dimension | Original spec | This spec |
|-----------|--------------|-----------|
| LLM calls per search | 3-5 (intent, expansion, routing, ranking weights, explanation) | 1 (intent parsing, enriched prompt) |
| Vector indexes | 8 | 1 (society embeddings) |
| New infrastructure | BM25 engine, vector DB, metadata store | None — uses existing KG |
| Latency | 3-8s (multiple LLM round trips) | 200-500ms (graph lookup + optional Gemini intent parse) |
| Cost per query | $0.02-0.10 | $0.001-0.005 |
| Negative signal handling | Yes (dedicated issues index) | Yes (fact-level, area inheritance) |
| Buyer archetype support | Yes (LLM skill) | Yes (config lookup) |
| Explanation quality | High (LLM-generated) | Medium-high (template + confidence qualifier) |
| Debugging | Hard (5 LLM calls to trace) | Easy (deterministic graph scoring) |
| Time to build | 3-6 months | 2-3 weeks |

The original spec is the right destination for a mature system at scale. This spec is the right thing to build now, with 55 societies and 155 properties, to prove the product thesis before investing in infrastructure.

---

## When to upgrade toward the original spec

**Add separate review/issue indexes when:**
- You have > 500 societies and raw review text that doesn't fit in node facts
- Society-level fact summaries lose too much nuance from the underlying reviews
- Users want to see "what residents actually said" (verbatim quotes), not just summaries

**Add BM25/lexical search when:**
- You have > 1000 entities and substring matching is too slow
- Users search by exact society names or builder names frequently and need fuzzy name matching

**Add LLM-based explanation synthesis when:**
- Template-based explanations feel too mechanical in user testing
- You want to generate comparative explanations ("Society A vs B because...")

**Add source routing as a separate concern when:**
- You have 5+ distinct data source types with different freshness/reliability profiles
- Some queries should skip certain sources entirely for cost/relevance reasons

**Add query expansion as a separate step when:**
- The preference taxonomy grows beyond what fits in a single Gemini prompt (~50+ canonical keys)
- You need domain-specific expansion that varies by city/market

---

## Data enrichment priorities to make this work

The scoring quality is bounded by fact coverage. Here's what to prioritize:

### Critical (blocks Phase 2)
- [ ] Run `embed_entity.py` on all 55 society nodes (some lack embeddings)
- [ ] Ensure every society node has at least: maintenance_quality, family_friendly, builder_trust, value_for_money, calm_environment (these are the top-5 queried preferences)
- [ ] Ensure area nodes have: waterlogging_risk, traffic_score, metro_access, livability_score

### Important (blocks Phase 3)
- [ ] Add `scoring_hint` to facts that lack it (some skills produce facts without scoring hints)
- [ ] Normalize fact keys across skills — ensure `learn_society.py` and `score_society.py` use the same canonical keys
- [ ] Add `answers_preferences` to facts that lack it — this is how preference matching works

### Nice to have (improves Phase 6 quality)
- [ ] Run `learn_society.py` on societies that only have Gemini-discovered data (thin facts)
- [ ] Add builder-level facts (currently no builder nodes in KG)
- [ ] Add school/metro distance facts to society nodes

---

## Response contract (updated)

```json
{
  "query": "family friendly 3BHK in Whitefield, avoid water issues",
  "intent": {
    "areas": ["whitefield"],
    "bhk": [3],
    "buyer_archetype": "family",
    "positive_preferences": [
      {
        "raw_text": "family friendly",
        "canonical_key": "family_friendly",
        "expanded_keys": ["family_friendly", "child_safety", "calm_environment", "community_vibe"],
        "polarity": "positive"
      }
    ],
    "negative_preferences": [
      {
        "raw_text": "avoid water issues",
        "canonical_key": "water_supply",
        "expanded_keys": ["water_supply", "waterlogging_risk", "tanker_dependency"],
        "polarity": "negative"
      }
    ]
  },
  "results": [
    {
      "society": {
        "id": "prestige-lakeside-habitat",
        "name": "Prestige Lakeside Habitat",
        "area": "Whitefield",
        "builder": "Prestige Group"
      },
      "properties": [
        {
          "id": "prop-w-012",
          "title": "3 BHK in Prestige Lakeside Habitat",
          "price": 24500000,
          "bhk": 3
        }
      ],
      "score": 0.82,
      "confidence": "high",
      "why_matches": [
        "Residents report a family-oriented community with active children's play areas",
        "Society rated 78/100 on calm environment based on resident discussions",
        "Builder (Prestige Group) has strong delivery track record in this area"
      ],
      "concerns": [
        "Area-level waterlogging risk is moderate — society-specific drainage status should be verified on-site"
      ],
      "unmatched": [],
      "evidence_summary": {
        "facts_consulted": 12,
        "sources": ["Reddit discussions", "Google reviews (4.3★, 248 reviews)", "RERA registry"],
        "graph_driven_pct": 85
      }
    }
  ],
  "also_consider": [],
  "discovery_status": null
}
```

---

## Final word

The original spec you wrote is thoughtful and architecturally correct. It describes a mature property intelligence platform. But it's designed for a system with thousands of entities, dozens of data sources, and millions of queries.

What OpenEstates needs right now is to prove that **context-aware search feels meaningfully better than filter search** for the 55 societies and 155 properties that exist today. The fastest path to that proof is making the knowledge graph — which is already the richest data asset in the system — the primary scoring engine, not building a parallel RAG infrastructure alongside it.

Build the simple version. Run the fuzzy checkpoint. If the graph-powered scoring produces results that feel right, you have product validation. Then scale the architecture.

If the graph-powered scoring feels thin, you'll know exactly where — which preferences lack facts, which areas lack coverage, which explanations feel generic. That tells you what to enrich, not what infrastructure to build.
