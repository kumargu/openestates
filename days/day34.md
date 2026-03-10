# Day 34: Explanation Cards — Trust Through Transparency

## 1. Goal

Upgrade search result explanations from mechanical fact dumps to confidence-qualified, concern-aware explanation cards. Make every result answer "why this matched" and "what should I worry about."

## 2. Product Reason

Current match explanations show fact_key and a display_template string. They're structured but don't feel helpful. "Maintenance quality is good (confidence: 0.6, source: Llm)" is transparency, but it's not the kind that builds trust.

Users need:
- Clear positive reasons grounded in evidence
- Explicit concerns with appropriate hedging
- Honest "we don't know" signals for gaps
- Confidence qualifiers that reflect evidence strength

This is the explanation layer from the context search spec — and it requires zero LLM calls because facts are self-describing.

## 3. Deliverables

### D1: `ExplanationCard` struct

In `backend/src/search/mod.rs`:

```rust
pub struct ExplanationCard {
    pub why_matches: Vec<ExplanationReason>,
    pub concerns: Vec<ExplanationConcern>,
    pub unmatched: Vec<String>,
    pub confidence_label: String,  // "high", "medium", "low"
    pub evidence_summary: EvidenceSummary,
}

pub struct ExplanationReason {
    pub text: String,          // "Residents report reliable maintenance"
    pub preference: String,    // "good maintenance"
    pub evidence_strength: String,  // "strong", "moderate", "limited"
    pub sources: Vec<String>,  // ["Reddit discussions", "Google reviews (4.3★)"]
}

pub struct ExplanationConcern {
    pub text: String,          // "Area-level waterlogging risk is moderate"
    pub preference: String,    // "avoid water issues"
    pub severity: String,      // "caution", "warning"
    pub source_level: String,  // "society-specific" or "area-level"
    pub note: Option<String>,  // "verify at society level"
}

pub struct EvidenceSummary {
    pub facts_consulted: usize,
    pub sources: Vec<String>,
    pub graph_driven_pct: f32,
}
```

### D2: `synthesize_explanation()` function

Template-based, no LLM:

```rust
fn synthesize_explanation(
    society_score: &SocietyScore,
    society_node: &Node,
    area_node: Option<&Node>,
) -> ExplanationCard
```

Logic:
1. For each matched_reason with score > 0.5, generate an ExplanationReason:
   - Use the fact's `display_template` as base
   - Prefix with confidence qualifier:
     - confidence >= 0.8 → no prefix (stated as fact)
     - confidence >= 0.5 → "Signals suggest..."
     - confidence < 0.5 → "Limited evidence suggests..."
   - Map source_type to human-readable source names
2. For each concern:
   - If Concern::Detected → direct concern text
   - If from area_node → add "(area-level signal — verify at society level)"
   - If Concern::NoData → "No data available for '{preference}'"
3. Compute confidence_label from overall SocietyScore confidence
4. Collect unique source types across all facts

### D3: Add to search response

Wire `ExplanationCard` into `SearchResult` alongside existing `match_explanation`:

```rust
pub struct SearchResult {
    // ... existing fields
    pub explanation_card: Option<ExplanationCard>,  // NEW
}
```

### D4: Frontend rendering of ExplanationCard

Update `frontend/src/pages/ResultsPageA.tsx` to render the new explanation card:

- **Why it matches** section: list of ExplanationReason items with evidence_strength indicator
- **Concerns** section: list of ExplanationConcern items styled as caution/warning
- **Gaps** section: "We don't have data on: {unmatched}" — honest, not hidden
- **Confidence badge**: high/medium/low with color coding
- **Evidence footer**: "Based on N facts from: Reddit, Google reviews, RERA"

Design: calm, muted colors. Concerns in amber, not red. Gaps in gray. Positive matches in subtle green. The overall feel should be "informed advisor" not "warning system."

### D5: Source name mapping

Map internal source types to user-friendly names:

```rust
fn source_display_name(source: &SourceType) -> &str {
    match source {
        Reddit => "Reddit resident discussions",
        Google => "Google reviews",
        Rera => "RERA registry",
        Bbmp => "BBMP records",
        News => "News coverage",
        Computed => "Computed from data",
        Manual => "Verified data",
        Llm => "AI analysis",
    }
}
```

## 4. Technical Guidance

**Files to modify:**
- `backend/src/search/mod.rs` — ExplanationCard, ExplanationReason, ExplanationConcern types
- `backend/src/search/text.rs` or `scoring.rs` — `synthesize_explanation()` function
- `backend/src/routes/search.rs` — wire explanation into response
- `frontend/src/pages/ResultsPageA.tsx` — render explanation card
- `frontend/src/lib/types.ts` — TypeScript types

**Key principle:** No LLM call for explanations. The display_templates are human-readable. Confidence qualifiers are deterministic. This keeps explanation generation at ~0ms.

**Tone calibration:** Explanations should sound like a knowledgeable friend, not a legal disclaimer. "Residents generally report reliable maintenance" not "Fact: maintenance_quality = good (0.6 confidence)".

## 5. Constraints

- Do NOT use LLM for explanation generation — strictly template-based
- Do NOT remove existing `match_explanation` field — keep for backward compatibility
- Do NOT over-style the frontend — keep it clean and calm
- Explanation generation must add <1ms to search latency

## 6. Success Criteria

- [ ] `ExplanationCard` struct defined with reasons, concerns, unmatched, confidence
- [ ] `synthesize_explanation()` generates cards from SocietyScore
- [ ] Confidence qualifiers correctly applied ("Signals suggest..." for medium confidence)
- [ ] Concerns show source level ("area-level signal" vs "society-specific")
- [ ] Unmatched preferences shown honestly ("No data on: quiet")
- [ ] Frontend renders explanation card with why/concerns/gaps sections
- [ ] Source names are user-friendly (not internal enum names)
- [ ] Explanation generation adds <1ms to search
- [ ] `cargo check` passes
- [ ] `npm run build` passes
