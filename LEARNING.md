# OpenEstates — Learning and Memory Instructions

OpenEstates improves over time because the **system** accumulates structured knowledge — not because a model magically remembers more.

Durable learning lives in: stored context · context graphs · extracted signals · score components · observed outcomes · evaluation comparisons

Always prefer inspectable learning over hidden magic.

---

## What the System Should Learn

- Which buyer/seller preferences are hard constraints vs soft preferences
- Which signals are stable vs temporary
- Which match types lead to visits → offers → closures
- Which extracted signals are reliable
- Which coach prompts produce useful information
- Where the system was wrong and why

---

## Three Categories of Learning

### 1. Repeated Signal Learning

When a preference is expressed repeatedly, reinforced, or contradicted across time, the context graph should reflect that.

Store for every signal:
- `value` — the preference or constraint
- `confidence` — how strongly to weight it
- `repetition_count` — how many times it has appeared
- `recency` — when it was last seen
- `source` — where it came from (conversation turn, onboarding, rejection pattern, etc.)

This separates stable preferences from one-off statements.

### 2. Outcome Learning

Compare what was recommended against what actually happened.

Outcome stages to track: ignored · inspected · visit requested · visit happened · offer made · negotiation stalled · deal closed · deal failed

Link each outcome back to: context at recommendation time · score breakdown · explanation shown · buyer/seller traits · listing attributes

### 3. Mistake Learning

When the system gets something wrong, leave inspectable evidence.

Examples of mistakes to track:
- Coach extracted a hard constraint from a soft statement
- System over-ranked because one dimension was weighted too strongly
- Alerts were repeatedly ignored
- Seriousness was misread

Mistakes are not invisible events. They are learning opportunities with a structured paper trail.

---

## Durable Memory Rules

- Do not rely on raw model memory for anything the system needs later
- Every learned fact must be representable as structured state
- If a model generates useful understanding, convert it before storing

Valid durable memory: structured attributes · graph edges · signal objects · confidence values · timestamps · event history · score snapshots · interaction outcomes

---

## Provenance

Every signal that enters the system should know where it came from.

Valid sources: `conversation_turn_N` · `onboarding_answer` · `visit_rejection` · `synthetic_hidden_profile` · `prior_match_outcome`

Provenance matters because the system may need to revise beliefs later. Early vague signals should be overrideable by later strong signals.

---

## Confidence and Decay

Not all signals are equal. Signals should carry confidence and decay unless reinforced.

- `"Maybe Sarjapur also"` → low confidence, decays fast
- `"Whitefield only"` repeated five times → high confidence, stable
- Urgency stated once months ago → decays unless reinforced
- Negotiation anxiety across multiple conversations → remains important

---

## Evaluation Mindset

Every learning mechanism should support evaluation. The core question is: does contextual matching outperform traditional filter-based search?

Support this by:
- Comparing baseline ranking vs contextual ranking
- Comparing top-k results against hidden synthetic compatibility scores
- Inspecting false positives and missed good matches
- Tracking closure rates over time

Learning is only useful if it improves measurable outcomes or reduces clear failure patterns.

---

## Design Habit

When implementing any new subsystem, answer these before writing code:

1. What should this subsystem remember?
2. What can it get wrong?
3. How will we know it was wrong?
4. What should be stored for future improvement?
5. What inspect or debug command would help us trust it?
