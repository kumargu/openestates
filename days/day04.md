# OpenEstates Matching Engine Prototype
## Day 4 – Ground Truth Evaluation + Reddit MarketSentimentResearcher (Minimal, High-Leverage)

Before starting today, read:
- CLAUDE.md
- LEARNING.md
- days/day03_learnings.md (or the Day 03 learnings text below)
- days/day01.md, days/day02.md, days/day03.md

We finished Day 3 with a working “Claude Code”-style chat TUI and a functioning learning loop:
conversation → OpenFang/Stub extraction → structured SignalUpdates → ContextGraph updates → event log.

Day 4 has two goals:
1) make evaluation possible by creating a hidden compatibility (“truth”) model, and
2) add the Reddit-based research idea as a lightweight agent pattern to continuously improve what we extract and what we match on.

We will keep both implementations minimal and local-first. No production infra, no heavy crawling, no bulk scraping.

---

## 0) Day 3 Learnings (quick summary)

Day 3 delivered:
- chat-first TUI with `/buyer`, `/extract`, `/context`, `/generate`, `/clear`, `/quit`
- strict `SignalUpdate` schema + JSON parsing/validation
- OpenFang REST client + graceful stub fallback
- ContextGraph with weight/confidence/provenance + basic reinforcement/weaken/remove
- `events.jsonl` audit log for conversation ingestion

Day 3 intentionally did NOT build:
- hidden truth model
- baseline search / matching engine evaluation
- watcher/nurture agents
- seller coaching

Day 4 will address the biggest blocker:
**without ground truth we cannot measure if learning improves matching**.

And Day 4 will also add:
**MarketSentimentResearcher** to keep feature taxonomy and coach prompts fresh.

---

## 1) Day 4 Goals

### Goal A — Hidden Truth Model (Ground Truth Compatibility)
Implement a hidden, synthetic compatibility model that produces a “true” compatibility score for buyer–property pairs. This is used ONLY for evaluation and simulation. The matching engine must never see it.

This makes “better” measurable via Precision@K, NDCG@K, and simulated closure rate (later).

### Goal B — Reddit MarketSentimentResearcher (Minimal, Safe Sampling)
Implement a lightweight Reddit research module and TUI command that:
- fetches a small recent sample from a subreddit (default: r/BangaloreRealEstates)
- extracts recurring themes, phrases, and “decision drivers”
- outputs a structured JSON report
- updates a local taxonomy file used to evolve feature design and coach prompts

This is not bulk crawling. It is “small sampling research” to build product intuition and language grounding.

---

## 2) Part A – Hidden Truth Compatibility Model

### Why we need this
If we don’t have a hidden truth model, we cannot prove that contextual matching beats baseline filter matching. The whole simulator becomes subjective. Day 4 makes evaluation possible.

### Core design
We will generate:
- `data/synthetic_market.json`  (observable)
- `data/synthetic_market_truth.json` (hidden + compatibility truth)

Day 2 may have produced a single file. If so, Day 4 must split it.

### Truth model concept
For each buyer and property, compute:
- `compatibility_score` in [0.0, 1.0]

This should incorporate:
- budget fit (observable price vs buyer hidden true budget limit)
- area fit (preferred areas + hidden flexibility)
- BHK fit
- metro proximity fit (based on buyer preference weight)
- society quality fit (based on buyer weight)
- noise/airport/graveyard/waterlogging fit (based on buyer sensitivities)
- document safety fit (based on buyer doc_safety_weight and property doc_completeness)
- seller urgency vs buyer timeline (seller hidden urgency + buyer timeline)
- negotiation compatibility (seller style vs buyer patience)
- and anything else that we get from reddit

The truth model can be simple, but must be:
- deterministic with a seed
- inspectable (score breakdown optional)
- separated from the matching engine

### Implementation tasks (Truth Model)
1) Create `simulation/truth_model.py`
   - `compute_truth_compatibility(buyer_hidden, seller_hidden, property_observable, buyer_observable) -> float`
   - optionally return breakdown components for debugging

2) Update `simulation/market_generator.py`
   - output two JSON files:
     - `synthetic_market.json`: visible only
     - `synthetic_market_truth.json`: hidden attributes + compatibility scores map

3) Add a debug TUI command:
   - `/truth buyer_<id>` prints top 5 property_ids by truth score (and optionally a breakdown)

This command is for debugging and should be clearly marked as “truth only”.

### Constraints
- Do NOT use truth values inside the matching engine. Truth access should live only in `simulation/` and debug commands.
- Keep computation light (do not compute full NxM if too large; cap at a reasonable size or compute on-demand per buyer).

---

## 3) Part B – Reddit MarketSentimentResearcher (Minimal + Safe)

### Why this matters
Reddit behaves like a “human nervous system” for this market: strong emotions, real fears, real language, real edge cases.
We will not treat Reddit as prevalence truth. We will treat it as:
- feature discovery (what matters in decisions)
- phrase discovery (how users talk)
- failure mode discovery (what breaks deals)
This helps improve coach prompts, extraction schema, and matching features.

### How we will “crawl” (Day 4 scope)
We will not build a crawler pipeline.
We will build a small sampler with strict limits.

Implementation approach:
- Use Reddit’s public JSON feed for subreddits, with a User-Agent.
  Example endpoint:
  `https://www.reddit.com/r/BangaloreRealEstates/new.json?limit=25`
- Fetch only small volume:
  - default limit: 25 posts
  - optionally fetch top-level comments for up to N posts (N small, like 10)
- Cache responses locally to avoid repeat hits during dev.
- Store only derived summaries, not full raw dumps.

No auth required. This keeps setup simple for local dev.
If we later want official API usage, we can add it.

### What the researcher outputs
Create a structured report:
- top recurring themes
- recurring phrases
- “decision drivers” (signals that should exist in our schema)
- coach prompt suggestions (questions to ask)
- extraction keyword suggestions
- feature suggestions for matching engine

Output file:
- `days/learning/reddit_reports/YYYY-MM-DD_reddit_report.json`
and update rolling taxonomy:
- `days/learning/reddit_taxonomy.json`

### Implementation tasks (Reddit Research)
1) Create `research/reddit_client.py`
   - `fetch_posts(subreddit, limit, sort)`
   - optional: `fetch_comments(post_permalink, limit)` (can be postponed if too much)

2) Create `research/reddit_sentiment_researcher.py`
   - input: subreddit + time window concept (for now just “recent”)
   - output: JSON report with:
     - themes
     - phrases
     - schema_suggestions
     - coach_prompt_suggestions

3) Add TUI command:
   - `/research reddit` (default subreddit)
   - `/research reddit r/BangaloreRealEstates 25`
It should print a short summary in chat and write the full JSON report to file.

### Safety/Compliance posture (Day 4)
- Keep volume low.
- Store derived summaries, not full raw content.
- Do not bulk scrape.
- Treat as qualitative research.

---

## 4) How this ties back to OpenFang (direction steering)

Truth model gives evaluation.
Reddit researcher gives evolving human signals and language.

OpenFang will later use the taxonomy to improve:
- extraction prompts (SignalUpdate keys)
- coach question library
- friction detection

In later days we may implement an OpenFang “Hand” that runs `/research reddit` on a schedule and writes taxonomy updates automatically.
For Day 4, we keep it manual via TUI.

---

## 5) Deliverables

By end of Day 4:

### Truth Model deliverables
- `simulation/truth_model.py` implemented
- generator outputs split into:
  - `data/synthetic_market.json` (visible)
  - `data/synthetic_market_truth.json` (hidden)
- TUI command `/truth <buyer_id>` showing top truth matches

### Reddit Research deliverables
- `research/reddit_client.py` and `research/reddit_sentiment_researcher.py`
- TUI command `/research reddit [subreddit] [limit]`
- output report saved to:
  - `days/learning/reddit_reports/<date>_reddit_report.json`
- rolling taxonomy file:
  - `days/learning/reddit_taxonomy.json`

---

## 6) Manual Verification Checklist

Truth model:
- Generate market
- Verify both JSON files exist
- Run `/truth buyer_XXXX` and see reasonable top matches
- Confirm matching engine code never loads truth file

Reddit research:
- Run `/research reddit r/BangaloreRealEstates 10`
- Confirm it fetches posts successfully (or fails gracefully offline)
- Confirm it produces a structured JSON report and saves it
- Confirm taxonomy file is updated (append/merge safely)

---

## 7) What we are NOT doing today
- baseline search engine
- contextual matching engine scoring
- simulated closure funnel (visit/offer/close)
- OpenFang scheduled Hands
- seller coaching

Day 4 is about building the foundation for evaluation AND adding the research agent pattern.

---

## 8) Suggested Day 4 Output Summary (what success looks like)
After Day 4, we can:
- learn signals via chat (`/extract`)
- see a buyer’s “true” best matches (`/truth`)
- start building baseline vs contextual ranking on Day 5 with measurable metrics
- run a research command that keeps feeding the system new human decision drivers

This keeps the project headed toward a system that is both:
- measurable (truth model)
- grounded in real human sentiment and language (Reddit research)