# Day 29: Explainable Search — Structured Match Reasons + Preference Coverage

## Dependency

Day 28 complete. We have:
- 55 societies in KG with self-describing facts (`answers_preferences`, `scoring_hint`, `display_template`)
- Graph-driven scoring via `graph_preference_score()` in `routes/search.rs`
- Legacy fallback scoring in `text.rs` for properties without graph data
- Semantic boosting layer (embedding similarity on society nodes)
- Live discovery fallback (Gemini Flash)
- Frontend showing `match_label` ("Strong match") and `match_reason` (flat text string)

---

## 1. The Problem

The search pipeline scores correctly — graph facts influence ranking, semantic similarity boosts results, preferences filter and weight. But **the user sees none of this reasoning**. They get:

- A colored badge: "Strong match" / "Good match"
- A flat text paragraph: "Matches: 3 BHK in Whitefield, metro access, quiet neighborhood"

This is barely better than what 99acres shows. The entire point of OpenEstates is transparency — the user should see *which specific facts* drove each result's ranking, *how confident* the system is in each signal, and *which preferences it couldn't evaluate*.

**Today's goal: every search result explains itself.**

---

## 2. What We're Building

### Phase A: Structured Match Reasons in Rust (1.5 hours)

#### A.1 New Types

Add to `backend/src/search/mod.rs`:

```rust
#[derive(Serialize, Clone)]
pub struct MatchReason {
    pub preference: String,           // "quiet neighborhood"
    pub fact_key: String,             // "noise_level"
    pub display: String,              // "Noise level is low — residential pocket"
    pub score: f64,                   // 0.0-1.0 contribution
    pub confidence: f32,              // fact confidence (1.0 for RERA, 0.6 for Reddit)
    pub source_type: String,          // "Reddit", "Rera", "Llm"
    pub scoring_method: String,       // "graph" or "legacy"
}

#[derive(Serialize, Clone)]
pub struct PreferenceCoverage {
    pub preference: String,           // "metro access"
    pub status: String,               // "matched", "partial", "no_data"
    pub fact_key: Option<String>,     // null if no_data
}

#[derive(Serialize, Clone)]
pub struct MatchExplanation {
    pub reasons: Vec<MatchReason>,            // why this result scored well
    pub preference_coverage: Vec<PreferenceCoverage>,  // how each user pref was handled
    pub graph_driven_pct: f32,                // % of score from graph vs legacy
    pub total_facts_consulted: usize,         // how many facts the scorer looked at
}
```

#### A.2 Extend SearchResultCard

```rust
pub struct SearchResultCard {
    #[serde(flatten)]
    pub card: PropertyCard,
    pub match_score: f64,
    pub match_label: String,
    pub match_reason: String,          // keep for backward compat
    pub match_explanation: Option<MatchExplanation>,  // NEW
    pub semantic_score: Option<f64>,
}
```

#### A.3 Build MatchExplanation During Scoring

The scoring logic in `search.rs` and `text.rs` already calls `graph_preference_score()` and `legacy_preference_score()`. Today we collect the *why* alongside the *score*:

In `routes/search.rs`, modify the preference scoring loop:

```rust
// Current: just accumulates a number
let mut pref_score = 0.0;
for pref in &intent.preferences {
    if let Some(gs) = graph_preference_score(&graph, society_id, pref) {
        pref_score += gs;
    } else {
        pref_score += legacy_preference_score(property, pref);
    }
}

// New: accumulates structured reasons
let mut reasons: Vec<MatchReason> = Vec::new();
let mut coverage: Vec<PreferenceCoverage> = Vec::new();
let mut graph_count = 0;
let mut legacy_count = 0;

for pref in &intent.preferences {
    if let Some((score, fact)) = graph_preference_score_detailed(&graph, society_id, pref) {
        reasons.push(MatchReason {
            preference: pref.clone(),
            fact_key: fact.key.clone(),
            display: render_display_template(&fact),
            score,
            confidence: fact.confidence,
            source_type: fact.source.source_type_str(),
            scoring_method: "graph".into(),
        });
        coverage.push(PreferenceCoverage {
            preference: pref.clone(),
            status: if score > 0.5 { "matched" } else { "partial" }.into(),
            fact_key: Some(fact.key.clone()),
        });
        graph_count += 1;
    } else if let Some((score, key)) = legacy_preference_score_detailed(property, pref) {
        reasons.push(MatchReason {
            preference: pref.clone(),
            fact_key: key.clone(),
            display: format_legacy_reason(pref, property),
            score,
            confidence: 0.5,  // legacy data = moderate confidence
            source_type: "Seed".into(),
            scoring_method: "legacy".into(),
        });
        coverage.push(PreferenceCoverage {
            preference: pref.clone(),
            status: if score > 0.5 { "matched" } else { "partial" }.into(),
            fact_key: Some(key),
        });
        legacy_count += 1;
    } else {
        coverage.push(PreferenceCoverage {
            preference: pref.clone(),
            status: "no_data".into(),
            fact_key: None,
        });
    }
}
```

This is a refactor of existing logic, not new scoring. The numbers stay the same — we just capture the reasoning.

#### A.4 graph_preference_score_detailed()

Modify `graph_preference_score()` to return the matching fact alongside the score:

```rust
pub fn graph_preference_score_detailed(
    graph: &KnowledgeGraph,
    society_id: &str,
    preference: &str,
) -> Option<(f64, &SourcedFact)>
```

Same logic as today, but returns the fact reference so we can extract `display_template`, `confidence`, `source`.

#### A.5 render_display_template()

```rust
fn render_display_template(fact: &SourcedFact) -> String {
    match &fact.display_template {
        Some(tmpl) => tmpl.replace("{value}", &fact.value.display_string()),
        None => format!("{}: {}", fact.key, fact.value.display_string()),
    }
}
```

This already exists conceptually in `build_knowledge_context()` — extract it as a shared helper.

### Phase B: Frontend Types + API Contract (30 min)

#### B.1 TypeScript Types

Add to `frontend/src/lib/types.ts`:

```typescript
export type MatchReason = {
  preference: string;
  fact_key: string;
  display: string;
  score: number;
  confidence: number;
  source_type: string;
  scoring_method: "graph" | "legacy";
};

export type PreferenceCoverage = {
  preference: string;
  status: "matched" | "partial" | "no_data";
  fact_key: string | null;
};

export type MatchExplanation = {
  reasons: MatchReason[];
  preference_coverage: PreferenceCoverage[];
  graph_driven_pct: number;
  total_facts_consulted: number;
};

export type SearchResultItem = PropertyCard & {
  match_score: number;
  match_label: string;
  match_reason: string;
  match_explanation?: MatchExplanation;  // NEW
  semantic_score?: number;
};
```

### Phase C: Frontend — "Why This Match" on Result Cards (1.5 hours)

#### C.1 MatchReasonBadge Component

`frontend/src/components/MatchReasonBadge.tsx` (~60 lines)

Small pill-shaped badge showing one match reason:

```
┌─────────────────────────────────┐
│ 🔇 Noise level is low          │  ← from display_template
│ Reddit · 70% confident         │  ← source + confidence
└─────────────────────────────────┘
```

Design:
- Background color by source: green (RERA, confidence=1.0), blue (Reddit), purple (Llm), gray (Seed/legacy)
- Compact: one line of text, source + confidence as subtitle
- Hover: shows full fact key + scoring method (graph vs legacy)

#### C.2 PreferenceCoveragePills

Row of pills showing coverage of each user preference:

```
metro access ✓   quiet ✓   greenery ◐   pet friendly ?
```

- ✓ green = matched (score > 0.5)
- ◐ yellow = partial (score > 0, ≤ 0.5)
- ? gray = no_data (system couldn't evaluate)

This immediately tells the user: "the system understood 3 of your 4 preferences, and couldn't evaluate pet-friendliness because that data isn't available yet."

#### C.3 Integration into ResultsPageA.tsx

Below the existing match_label badge on each card, add:

```tsx
{result.match_explanation && (
  <div className="mt-2">
    {/* Preference coverage pills */}
    <div className="flex flex-wrap gap-1.5 mb-2">
      {result.match_explanation.preference_coverage.map(pc => (
        <PreferencePill key={pc.preference} coverage={pc} />
      ))}
    </div>

    {/* Top 3 match reasons (expandable to all) */}
    <div className="space-y-1">
      {result.match_explanation.reasons.slice(0, 3).map(r => (
        <MatchReasonBadge key={r.fact_key} reason={r} />
      ))}
      {result.match_explanation.reasons.length > 3 && (
        <button className="text-xs text-blue-500">
          +{result.match_explanation.reasons.length - 3} more reasons
        </button>
      )}
    </div>

    {/* Graph vs legacy indicator */}
    {result.match_explanation.graph_driven_pct > 0 && (
      <p className="text-xs text-gray-400 mt-1">
        {Math.round(result.match_explanation.graph_driven_pct)}% scored from verified data
      </p>
    )}
  </div>
)}
```

#### C.4 Empty State

When there are no preferences in the query (e.g., "3 BHK Whitefield" with no soft prefs), don't show the explanation block at all. The match_label and match_reason string are sufficient.

When there are preferences but all have `no_data`, show:

```
We don't have enough data to evaluate your preferences for this property yet.
This property matched on location and specs.
```

### Phase D: Commit Days 26-28 (30 min)

Before building on top, commit the 60+ modified files and 17 new files from Days 26-28 as a clean checkpoint.

Commit message: `Days 26-28: Entity resolution, RERA scraper, area intelligence, enrichment engine, frontend tiles`

This ensures Day 29 work is cleanly separated.

### Phase E: Verify (30 min)

1. `cargo check` — types compile
2. Search with preferences: `curl "http://localhost:4000/api/search?q=quiet+family+apartment+whitefield"` — verify `match_explanation` in response
3. Search without preferences: `curl "http://localhost:4000/api/search?q=3bhk+whitefield"` — verify `match_explanation` is null or empty
4. Frontend: search "quiet apartment near metro whitefield" — verify preference pills and match reason badges render
5. Verify legacy fallback: search for a property with no graph data — should show reasons with `scoring_method: "legacy"`
6. Verify no_data: search with a preference the system can't evaluate (e.g., "pet friendly") — should show gray `?` pill
7. `npm run build` succeeds

---

## 3. Scope

| Phase | What | Time |
|-------|------|------|
| **A** | Structured match reasons in Rust | 1.5 hours |
| **B** | Frontend types + API contract | 30 min |
| **C** | Frontend "Why This Match" display | 1.5 hours |
| **D** | Commit Days 26-28 checkpoint | 30 min |
| **E** | Verify end-to-end | 30 min |
| **Total** | | ~4.5 hours |

---

## 4. Files

### New
- `frontend/src/components/MatchReasonBadge.tsx` — match reason pill component (~60 lines)
- `frontend/src/components/PreferencePill.tsx` — preference coverage pill (~40 lines)

### Modified (Backend)
- `backend/src/search/mod.rs` — add MatchReason, PreferenceCoverage, MatchExplanation types
- `backend/src/routes/search.rs` — collect structured reasons during scoring, attach to results
- `backend/src/search/text.rs` — extract `legacy_preference_score_detailed()` variant

### Modified (Frontend)
- `frontend/src/lib/types.ts` — add MatchReason, PreferenceCoverage, MatchExplanation types
- `frontend/src/pages/ResultsPageA.tsx` — integrate explanation display into result cards

---

## 5. Design Principles (Day 29 Specific)

### Show your work
The system already scores correctly. Today we make it *show* why each score was assigned. This is the difference between "trust me, it's a strong match" and "here's exactly what we checked and what we found."

### Structured, not prose
Match explanations are structured objects (preference + fact + score + source), not generated text. This means they're:
- **Testable** — you can assert that a "quiet" preference surfaces a "noise_level" fact
- **Filterable** — frontend can sort/group reasons by source or confidence
- **Composable** — same MatchReason struct works for graph facts and legacy scoring
- **Honest** — shows "legacy" vs "graph" so the user (and us) can track how much of the scoring is powered by real data

### No new scoring logic
Today is purely about observability, not about changing how ranking works. The scores stay identical. We just capture the reasoning that was already being computed and discarded.

### Graceful degradation
- No preferences in query → no explanation block
- All preferences have no_data → honest empty state
- Mix of graph + legacy → show both, label the difference
- The `match_reason` string field stays for backward compat (existing frontend, API consumers)

---

## 6. What NOT to Build Today

- New scoring dimensions or preference types — today is observability, not new intelligence
- Preference learning / personalization — future, requires user sessions
- Explanation generation via LLM — structured templates are better (cheaper, testable, instant)
- Comparison of match reasons across results — that's a compare workspace feature
- Embedding-based preference matching — current keyword matching on `answers_preferences` is sufficient
- Re-ranking based on explanation quality — the ranking is correct, we're just surfacing its reasoning

---

## 7. Success Criteria

- [ ] `SearchResultCard` includes optional `match_explanation` field
- [ ] `MatchExplanation` contains `reasons` (per-fact) and `preference_coverage` (per-preference)
- [ ] Graph-scored facts show `scoring_method: "graph"` with real confidence and source
- [ ] Legacy-scored facts show `scoring_method: "legacy"` with 0.5 confidence
- [ ] Unresolvable preferences show `status: "no_data"` in coverage
- [ ] `graph_driven_pct` accurately reflects ratio of graph vs legacy scoring
- [ ] Frontend renders preference coverage pills (green/yellow/gray)
- [ ] Frontend renders top 3 match reasons with expandable "more"
- [ ] Frontend shows "X% scored from verified data" when graph facts are present
- [ ] No change in search result ordering — this is pure observability
- [ ] `cargo check` + `npm run build` pass
- [ ] Days 26-28 committed as clean checkpoint

---

## 8. The Principle

Transparency isn't a feature you bolt on after building the scoring engine. It's the scoring engine *explaining itself*. Today we close the loop:

```
Days 22-25: Skills produce self-describing facts
  → facts carry display_template, answers_preferences, scoring_hint

Day 26-28: Enrichment engine fills the graph
  → 55 societies with real facts from RERA, Reddit, Claude

Day 29: Search shows its reasoning
  → every result explains which facts drove its score
  → every preference shows whether the system could evaluate it
  → the user sees exactly what the system knows and doesn't know
```

This is the transparency promise made concrete. Not "we're transparent" as marketing. But literally: "here are the 4 facts we used to rank this result, here's where each came from, and here's the 1 thing we couldn't check."

No other property platform does this.

---

## 9. After Day 29

```
The Search Experience:

  User: "quiet family apartment near metro whitefield under 1.5cr"

  Intent: { area: "Whitefield", bhk: null, budget_max: 15000000,
            preferences: ["quiet neighborhood", "metro access", "value for money"] }

  Result 1: Prestige Lakeside Habitat — 3 BHK
    Match: 0.87 (Strong match)

    Preferences:  quiet ✓   metro ✓   value ◐
                  ───────   ──────   ────────
    Why:
    ┌ Noise level is low — residential area away from main road
    │ Source: Reddit (47 threads) · 70% confident · Graph-scored
    ├ Metro: 8 min walk to Whitefield Metro
    │ Source: Google · 85% confident · Graph-scored
    └ Price ₹9,200/sqft — slightly above area median
      Source: Seed data · 50% confident · Legacy-scored

    87% scored from verified data

  Result 2: Sobha Insignia — 3 BHK
    Match: 0.72 (Good match)

    Preferences:  quiet ?   metro ✓   value ✓
    ...

  The user instantly sees:
  - WHY Prestige ranked higher (more preferences matched, higher confidence)
  - WHAT the system couldn't check for Sobha (quiet — no noise data yet)
  - WHERE the data came from (Reddit vs RERA vs seed)
  - HOW much of the ranking is backed by real data (87%)

  This is explainable search. This is the product.
```

---

## 10. Day 30+ Preview

With structured match explanations in place, the next moves become clear:

- **Day 30: Preference Learning** — track which match reasons users click/save → weight future scoring
- **Day 31: Compare Workspace V2** — side-by-side match explanation comparison ("Result A has verified quiet data, Result B doesn't")
- **Day 32: Search Quality Dashboard** — aggregate `graph_driven_pct` and `no_data` rates to prioritize enrichment
- **Day 33: Embedding Refinement** — use match explanation quality as a signal for embedding model tuning
