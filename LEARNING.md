# OpenEstates — Learning and Memory Instructions

OpenEstates improves over time because the **system** accumulates structured knowledge and learns from behavior, not because a model magically remembers more.

Durable learning must live in system-owned state such as:
- parsed search intent
- structured preference signals
- context profiles
- ranking explanations
- market intelligence snapshots
- shortlist and comparison events
- interaction outcomes
- evaluation results

Always prefer inspectable learning over hidden magic.

The purpose of learning in OpenEstates is not to make the system feel “AI-powered.”  
The purpose is to make the product become **more transparent, more accurate, and more useful** over time.

---

## 1. What the System Should Learn

OpenEstates should learn what actually matters in home-buying decisions and how that varies by user, area, and property type.

The system should gradually become better at understanding:

- which preferences are hard constraints versus soft preferences
- which tradeoffs a user is willing to make
- which sensitivities matter most for a given user
- which ranking signals are useful versus noisy
- which transparency widgets actually help decision-making
- which area/society signals repeatedly affect conviction
- which matches lead to shortlisting, revisits, and eventual contact
- where the system was wrong and why

The goal is not just to recommend “more properties.”  
The goal is to help users reach conviction faster and with less ambiguity.

---

## 2. Main Categories of Learning

### 2.1 Intent Learning

The system should learn from what users explicitly ask for and what they imply indirectly.

Examples:
- “near metro”
- “quiet area”
- “okay stretching budget if commute improves”
- “good society matters more than amenities”
- “don’t want legal uncertainty”
- “want stronger resale than just a nice flat”

These should become structured signals, not remain raw text.

For every learned preference or constraint, the system should store:
- `value`
- `confidence`
- `weight`
- `repetition_count`
- `recency`
- `source`

This allows the system to separate:
- one-off phrasing
- stable preferences
- uncertain preferences
- recently changing intent

---

### 2.2 Behavioral Learning

OpenEstates should learn not only from what users say, but from what they do.

Examples of behavior:
- which search results they click
- which cards they ignore
- how long they stay on a property page
- which properties they shortlist
- which properties they compare
- what they remove from shortlist
- what they revisit multiple times

Behavior often reveals preference strength more accurately than initial words.

For example:
- a user may say metro matters, but repeatedly shortlist stronger societies farther away
- a user may say budget is flexible, but consistently avoid properties above a certain threshold
- a user may claim openness to multiple areas, but only deeply inspect one area

The system should learn from these patterns carefully and update inferred weighting over time.

---

### 2.3 Market Intelligence Learning

OpenEstates should continuously improve its understanding of areas, societies, and recurring decision drivers.

This includes learning from:
- price/sqft distributions
- area trend changes
- repeated review themes
- Reddit or discussion-based fear patterns
- recurring society complaints
- recurring externality concerns such as airport noise, waterlogging, graveyard proximity, congestion, maintenance quality, or legal ambiguity

This learning is not about replacing human judgment.  
It is about enriching the transparency layer so users can see better context around a property.

Where possible, the system should convert noisy external information into structured signals and concise summaries.

---

### 2.4 Outcome Learning

Over time, OpenEstates should compare what it recommended with what actually happened.

Examples of outcomes:
- ignored
- clicked
- shortlisted
- compared
- revisited
- interest expressed
- contact unlocked later
- dropped
- moved forward

In later versions, this may expand into:
- visit requested
- offer made
- negotiation stalled
- deal closed

Every important outcome should be linked back to:
- context at recommendation time
- ranking inputs
- score breakdown
- explanation shown
- listing attributes
- transparency signals displayed

This allows the system to learn whether certain signals actually helped users make decisions.

---

### 2.5 Mistake Learning

Whenever the system gets something wrong, it should leave enough evidence to inspect why.

Examples:
- search intent parser overfit a soft preference into a hard constraint
- ranking overweighted one signal such as price/sqft
- explanation emphasized the wrong reason
- alerts or recommendations were repeatedly ignored
- user repeatedly rejected results that the system believed were strong matches
- market intelligence signals were too noisy to be helpful

Mistakes should not disappear into logs.  
They should become structured opportunities for refinement.

---

## 3. Durable Memory Rules

Do not rely on raw model memory for anything important.

Anything OpenEstates may need later must be stored in a structured and app-owned form.

Valid durable memory includes:
- user search context
- extracted preference objects
- confidence values
- weight values
- timestamps
- event history
- ranking snapshots
- shortlist states
- comparison history
- outcome markers
- market intelligence summaries
- taxonomy updates from external research

If a model generates useful understanding, convert it into structured data before storing it.

Never let free-form model output become the authoritative source of truth.

---

## 4. Provenance

Every signal that enters the system should carry provenance.

Examples of valid sources:
- `search_query_v1`
- `property_click`
- `shortlist_add`
- `comparison_event`
- `property_revisit`
- `conversation_turn_n`
- `market_sentiment_report`
- `review_summary`
- `synthetic_hidden_profile`
- `prior_match_outcome`

Provenance is important because the system must be able to revise beliefs.

A weak signal from early search input should be overridable by stronger behavioral evidence later.

---

## 5. Confidence, Weight, and Decay

Not all learned signals are equal.

Each signal should carry:
- a confidence score (how sure the system is)
- a weight (how important the signal seems)
- a recency dimension (how fresh it is)
- optionally a decay rule

Examples:
- “Maybe Sarjapur also” should start weak and decay quickly
- repeated shortlisting of metro-adjacent properties should increase metro weight
- a one-time concern about noise may remain weak unless repeated
- repeated rejection of properties with low society quality should strengthen that preference
- old urgency should decay unless reinforced

This is critical because user preferences evolve.

The system should remain adaptive, not fossilized.

---

## 6. Evaluation Mindset

Every learning mechanism should support evaluation.

The core product question remains:

**Does context-based discovery and ranking outperform traditional filter-based search in helping users reach better decisions?**

Support this through:
- baseline vs contextual ranking comparison
- top-k ranking evaluation
- inspection of false positives
- inspection of missed strong matches
- shortlist conversion comparison
- revisit behavior analysis
- eventual downstream outcome analysis

Learning is only valuable if it improves measurable usefulness or reduces recurring failure patterns.

---

## 7. Learning from Transparency Features

Because transparency is the core of OpenEstates, the system should also learn which transparency elements are actually useful.

Examples:
- do users engage with price/sqft context?
- do trend charts affect shortlisting?
- do review summaries affect decision time?
- do local externality badges matter?
- which transparency blocks correlate with stronger conviction?

This matters because transparency is not just a principle — it is a product surface.

The system should learn which kinds of transparency reduce ambiguity and help decisions.

---

## 8. External Research and Sentiment Learning

OpenEstates may use sources like Reddit or review systems as qualitative research inputs.

These sources should be used to learn:
- recurring anxieties
- common decision drivers
- real user phrasing
- market myths and misconceptions
- feature ideas for the ranking and transparency layers

These sources should not be treated as prevalence truth.

They are most useful for:
- updating taxonomies
- improving prompt libraries
- identifying missing signals
- refining explanation language

Derived learnings should be stored in structured form, such as:
- theme taxonomies
- phrase libraries
- feature suggestions
- prompt suggestions

---

## 9. System Watching

OpenEstates should be able to inspect itself.

For any important subsystem, it should be possible to answer:
- what does the system currently believe about this user?
- why was this property ranked highly?
- what changed after this search or interaction?
- which signals came from search text vs behavior?
- why was this alert or recommendation shown?
- where has the system been consistently wrong?

This means every important subsystem should eventually have some inspect/debug pathway.

The system should feel debuggable, not mystical.

---

## 10. Design Habit

Before implementing any new subsystem, answer these questions:

1. What should this subsystem remember?
2. What can it get wrong?
3. How will we know it was wrong?
4. What should be stored for future improvement?
5. What inspect or debug surface would help us trust it?
6. Does this help transparency, matching quality, or user conviction?

If the answer to the last question is unclear, the feature may not be worth building yet.

---

## 11. Final Learning Rule

OpenEstates should become better over time because it can:
- observe user intent
- observe user behavior
- enrich market understanding
- compare rankings to outcomes
- revise beliefs cleanly
- expose its reasoning transparently

The goal is not to appear intelligent once.

The goal is to build a system that becomes **more transparent, more accurate, and more decision-useful over time**.