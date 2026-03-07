"""
Reddit Sentiment Researcher

Analyzes fetched Reddit posts using real text frequency analysis.
Discovers themes, phrases, and decision drivers from actual content —
not just matching against hardcoded keyword lists.

Two layers:
  1. Frequency analysis — unigrams, bigrams, trigrams from actual posts
  2. Domain mapping — maps discovered terms to our schema signals

Each run adds to the rolling taxonomy. Over time the taxonomy becomes
a genuine picture of what Bangalore real estate buyers care about.
"""

import datetime
import json
import os
import re
import string
from collections import Counter, defaultdict
from typing import Any, Dict, List, Optional, Set, Tuple

PROJECT_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
REPORTS_DIR = os.path.join(PROJECT_ROOT, "data", "reddit", "reports")
TAXONOMY_PATH = os.path.join(PROJECT_ROOT, "data", "reddit", "taxonomy.json")

# --- Stop words for text analysis ---

STOP_WORDS: Set[str] = {
    "the", "a", "an", "and", "or", "but", "in", "on", "at", "to", "for",
    "of", "with", "by", "from", "as", "is", "was", "are", "were", "be",
    "been", "being", "have", "has", "had", "do", "does", "did", "will",
    "would", "could", "should", "may", "might", "shall", "can", "i", "we",
    "you", "he", "she", "they", "it", "this", "that", "these", "those",
    "my", "our", "your", "his", "her", "their", "its", "me", "us", "him",
    "them", "what", "which", "who", "when", "where", "how", "why", "all",
    "any", "both", "each", "few", "more", "most", "other", "some", "such",
    "no", "not", "only", "own", "same", "so", "than", "too", "very",
    "just", "now", "also", "am", "if", "then", "there", "here", "about",
    "into", "through", "after", "before", "between", "out", "up", "down",
    "off", "over", "under", "again", "want", "need", "looking", "anyone",
    "anyone", "anyone", "please", "hi", "hello", "thanks", "thank", "help",
    "know", "like", "get", "got", "one", "two", "three", "four", "five",
    "well", "good", "new", "old", "near", "per", "sq", "ft", "sqft",
    "crore", "lakh", "lakhs", "rs", "inr", "vs", "et", "re", "https",
    "www", "com", "http", "www",
}

# Domain keywords that signal this is real-estate relevant
REAL_ESTATE_SIGNALS: Set[str] = {
    "flat", "apartment", "bhk", "property", "plot", "villa", "site",
    "builder", "society", "project", "floor", "price", "budget", "buy",
    "sell", "rent", "lease", "resale", "possession", "rera", "registration",
    "loan", "emi", "investment", "area", "location", "road", "phase",
    "gated", "community", "amenities", "parking", "maintenance", "sqft",
}


# --- Known Bangalore area names for price association ---

BANGALORE_AREAS = [
    "whitefield", "sarjapur", "hsr", "bellandur", "electronic city", "electroniccity",
    "koramangala", "indiranagar", "marathahalli", "yelahanka", "hebbal", "devanahalli",
    "jp nagar", "bannerghatta", "banashankari", "kr puram", "krpuram", "horamavu",
    "hal", "domlur", "old airport", "rr nagar", "rrnagar", "chandapura", "chandrapura",
    "kanakapura", "tumkur", "magadi", "mysore road", "prestige city", "sobha",
    "north bangalore", "south bangalore", "east bangalore", "west bangalore",
    "central bangalore",
]


def _extract_price_signals(posts: List[Dict]) -> Dict[str, Any]:
    """
    Extract price mentions from post titles and bodies.
    Associates prices with areas when both appear in the same post.

    Recognizes:
      - X Cr / X crore / X crores
      - X L / X lakh / X lakhs
      - X/sqft / X per sq ft / X per sqft
      - Budget: X / upto X / under X

    Returns a dict with:
      area_prices: {area -> [price_in_INR, ...]}
      sqft_rates:  {area -> [rate_per_sqft, ...]}
      raw_mentions: [{"price_str", "price_inr", "area", "title"}, ...]
    """
    # Price pattern: handles "1.45 Cr", "1.13 crs", "30 Lakhs", "13k", "7000/sqft"
    CRORE_PAT  = re.compile(r'(?:rs\.?\s*)?(\d+(?:\.\d+)?)\s*(?:cr(?:ores?)?|crore)', re.I)
    LAKH_PAT   = re.compile(r'(?:rs\.?\s*)?(\d+(?:\.\d+)?)\s*(?:l(?:akhs?)?)', re.I)
    SQFT_PAT   = re.compile(r'(?:rs\.?\s*)?(\d{3,6})\s*(?:per\s+sq\.?\s*ft|/\s*sqft|per\s*sqft|/sqft)', re.I)
    K_SQFT_PAT = re.compile(r'(?:₹\s*)?(\d+(?:\.\d+)?)k\s*(?:per\s+sq\.?\s*ft|/\s*sqft|per\s*sqft)', re.I)

    area_prices: Dict[str, List[int]] = defaultdict(list)
    sqft_rates:  Dict[str, List[int]] = defaultdict(list)
    raw_mentions: List[Dict] = []

    for post in posts:
        title = post.get("title", "")
        body  = post.get("selftext", "") or ""
        combined = title + " " + body
        combined_lower = combined.lower()

        # Find which areas are mentioned in this post
        post_areas = [area for area in BANGALORE_AREAS if area in combined_lower]

        # Extract crore prices
        for m in CRORE_PAT.finditer(combined):
            price_inr = int(float(m.group(1)) * 1_00_00_000)
            if 500_000 < price_inr < 100_00_00_000:  # 5L to 100Cr sanity check
                area = post_areas[0] if post_areas else "unknown"
                area_prices[area].append(price_inr)
                raw_mentions.append({"price_str": m.group(0), "price_inr": price_inr,
                                     "area": area, "title": title[:80]})

        # Extract lakh prices
        for m in LAKH_PAT.finditer(combined):
            price_inr = int(float(m.group(1)) * 1_00_000)
            if 5_00_000 < price_inr < 30_00_00_000:  # 5L to 300Cr sanity check
                area = post_areas[0] if post_areas else "unknown"
                area_prices[area].append(price_inr)
                raw_mentions.append({"price_str": m.group(0), "price_inr": price_inr,
                                     "area": area, "title": title[:80]})

        # Extract per-sqft rates (e.g. 7000/sqft)
        for m in SQFT_PAT.finditer(combined):
            rate = int(m.group(1))
            if 2000 < rate < 50000:  # sanity: 2k to 50k per sqft
                area = post_areas[0] if post_areas else "unknown"
                sqft_rates[area].append(rate)
                raw_mentions.append({"price_str": m.group(0), "price_inr": rate,
                                     "area": area, "title": title[:80], "type": "per_sqft"})

        # Extract "13k per sqft" style
        for m in K_SQFT_PAT.finditer(combined):
            rate = int(float(m.group(1)) * 1000)
            if 2000 < rate < 50000:
                area = post_areas[0] if post_areas else "unknown"
                sqft_rates[area].append(rate)
                raw_mentions.append({"price_str": m.group(0), "price_inr": rate,
                                     "area": area, "title": title[:80], "type": "per_sqft"})

    # Summarize: median price per area
    def _fmt_price(p: int) -> str:
        if p >= 1_00_00_000:
            return f"{p/1_00_00_000:.2f} Cr"
        return f"{p/1_00_000:.0f} L"

    def _median(lst: List[int]) -> int:
        s = sorted(lst)
        n = len(s)
        return s[n // 2] if n % 2 == 1 else (s[n // 2 - 1] + s[n // 2]) // 2

    area_summary: Dict[str, Dict] = {}
    all_areas = set(area_prices) | set(sqft_rates)
    for area in all_areas:
        prices = area_prices.get(area, [])
        rates  = sqft_rates.get(area, [])
        if prices or rates:
            area_summary[area] = {
                "price_mentions": len(prices),
                "median_price": _fmt_price(_median(prices)) if prices else None,
                "price_range": [_fmt_price(min(prices)), _fmt_price(max(prices))] if len(prices) > 1 else None,
                "sqft_rate_mentions": len(rates),
                "median_sqft_rate": f"₹{_median(rates):,}/sqft" if rates else None,
            }

    return {
        "area_price_summary": area_summary,
        "raw_mentions": raw_mentions,
    }


def _clean_text(text: str) -> str:
    """Lowercase, remove punctuation, clean up."""
    text = text.lower()
    # Preserve hyphenated words like "ready-to-move"
    text = re.sub(r'-', ' ', text)
    text = re.sub(r'[^\w\s]', ' ', text)
    text = re.sub(r'\s+', ' ', text)
    return text.strip()


def _tokenize(text: str) -> List[str]:
    """Tokenize and filter stop words and short tokens."""
    tokens = _clean_text(text).split()
    return [t for t in tokens if t not in STOP_WORDS and len(t) > 2 and not t.isdigit()]


def _ngrams(tokens: List[str], n: int) -> List[Tuple[str, ...]]:
    return [tuple(tokens[i:i+n]) for i in range(len(tokens) - n + 1)]


def _extract_terms(posts: List[Dict]) -> Tuple[Counter, Counter, Counter]:
    """
    Extract unigram, bigram, trigram frequencies from all posts.
    Returns (unigrams, bigrams, trigrams) as Counters.
    """
    unigrams: Counter = Counter()
    bigrams: Counter = Counter()
    trigrams: Counter = Counter()

    for post in posts:
        title = post.get("title", "")
        body = post.get("selftext", "") or ""
        text = title + " " + body
        tokens = _tokenize(text)

        unigrams.update(tokens)
        bigrams.update(_ngrams(tokens, 2))
        trigrams.update(_ngrams(tokens, 3))

    return unigrams, bigrams, trigrams


def _filter_relevant_phrases(
    bigrams: Counter,
    trigrams: Counter,
    min_count: int = 2,
) -> List[str]:
    """
    Extract meaningful recurring phrases from bigrams + trigrams.
    Filter to those that appear >= min_count times.
    """
    phrases = []

    # Trigrams first (more specific)
    for tokens, count in trigrams.most_common(30):
        if count >= min_count:
            phrase = " ".join(tokens)
            phrases.append(phrase)

    # Bigrams
    for tokens, count in bigrams.most_common(40):
        if count >= min_count:
            phrase = " ".join(tokens)
            # Skip if this bigram is already covered by a trigram above
            already_covered = any(phrase in p for p in phrases)
            if not already_covered:
                phrases.append(phrase)

    return phrases[:20]  # Cap to avoid noise


def _extract_top_terms(unigrams: Counter, top_n: int = 30) -> List[str]:
    """Return most frequent single terms, excluding noise."""
    return [term for term, _ in unigrams.most_common(top_n + 20)
            if term not in STOP_WORDS and len(term) > 3][:top_n]


def _map_terms_to_signals(terms: List[str], phrases: List[str]) -> List[str]:
    """
    Map discovered terms to schema signal suggestions.
    This is a soft mapping — output is human-readable suggestions, not code.
    """
    # Domain-area groupings derived from actual Reddit content patterns
    domain_map = {
        # Location signals
        frozenset(["whitefield", "sarjapur", "hsr", "bellandur", "marathahalli",
                   "koramangala", "yelahanka", "hebbal", "domlur", "bannerghatta",
                   "electronic", "banashankari", "jp", "nagar"]):
            "area_preference — specific locality matters",
        frozenset(["metro", "purple", "green", "yellow", "line", "station",
                   "commute", "traffic", "office", "distance", "travel"]):
            "commute_sensitivity — proximity to metro/office is a factor",
        # Property type signals
        frozenset(["flat", "apartment", "villa", "plot", "site", "house",
                   "independent", "gated", "community"]):
            "property_type_preference — flat vs villa vs plot",
        frozenset(["bhk", "1bhk", "2bhk", "3bhk", "4bhk", "bedroom",
                   "sqft", "carpet", "area", "spacious"]):
            "size_requirement — BHK and area preference",
        # Financial signals
        frozenset(["budget", "afford", "crore", "lakh", "price", "cost",
                   "emi", "loan", "expensive", "overpriced", "undervalued"]):
            "budget_sensitivity — price consciousness in decisions",
        frozenset(["investment", "rental", "appreciation", "yield", "returns",
                   "capital", "gain", "flip", "resale"]):
            "purpose — investment vs self-use",
        # Trust and risk signals
        frozenset(["builder", "developer", "reputation", "delay", "rera",
                   "possession", "handover", "cancelled", "stuck"]):
            "builder_trust — concern about builder reliability",
        frozenset(["legal", "khata", "oc", "cc", "document", "title",
                   "encumbrance", "registration", "stamp"]):
            "legal_risk_concern — documentation and legal safety",
        frozenset(["sewage", "dump", "waste", "noise", "highway", "airport",
                   "pollution", "traffic", "garbage", "smell"]):
            "environment_sensitivity — noise, pollution, surroundings",
        frozenset(["vastu", "east", "north", "facing", "sunlight", "wind",
                   "direction"]):
            "vastu_preference — directional and cultural preferences",
        frozenset(["society", "amenities", "gym", "pool", "parking",
                   "security", "maintenance", "clubhouse", "lift"]):
            "amenity_priority — importance of society features",
        frozenset(["resale", "ready", "move", "under", "construction",
                   "pre", "launch", "ocp"]):
            "construction_stage_preference — ready-to-move vs under-construction",
        frozenset(["negotiate", "bargain", "discount", "below", "market",
                   "quoted", "fair", "price"]):
            "negotiation_mindset — willingness to negotiate",
    }

    all_terms_set = set(terms) | set(t for phrase in phrases for t in phrase.split())
    matched_signals = set()

    for keyword_set, signal in domain_map.items():
        if keyword_set & all_terms_set:
            matched_signals.add(signal)

    return sorted(matched_signals)


def _infer_coach_prompts(signals: List[str]) -> List[str]:
    """Generate coach prompt suggestions based on discovered signals."""
    prompt_map = {
        "area_preference": "Which specific areas are you targeting? Open to adjacent localities?",
        "commute_sensitivity": "How important is commute time? Do you need to be near a metro station?",
        "property_type_preference": "Are you looking for a flat, villa, or plot?",
        "size_requirement": "What's your minimum BHK and carpet area requirement?",
        "budget_sensitivity": "What's your hard budget limit? Is there flexibility if the property is right?",
        "purpose": "Is this for self-use, rental income, or long-term investment?",
        "builder_trust": "How important is builder reputation vs price? Any builders you'd avoid?",
        "legal_risk_concern": "Do you have a lawyer to verify documents, or do you need guidance on that?",
        "environment_sensitivity": "Any specific environmental concerns — noise, road proximity, dump yards?",
        "vastu_preference": "Does vastu/facing matter to you? Any hard requirements?",
        "amenity_priority": "Which society amenities matter most to you — parking, gym, pool, security?",
        "construction_stage_preference": "Ready-to-move or open to under-construction? What's your timeline?",
        "negotiation_mindset": "Are you comfortable negotiating hard, or do you prefer transparent pricing?",
    }

    prompts = []
    for signal in signals:
        signal_key = signal.split(" — ")[0].strip()
        if signal_key in prompt_map:
            prompts.append(prompt_map[signal_key])
    return prompts


def analyze_posts(posts: List[Dict], subreddit: str) -> Dict:
    """
    Analyze a list of Reddit posts using real frequency analysis + price extraction.

    Returns a structured report with:
      - top_terms: most frequent domain terms
      - phrases: recurring multi-word expressions
      - decision_drivers: schema signal suggestions
      - coach_prompt_suggestions: questions the coach should ask
      - price_signals: area -> price ranges extracted from actual posts
      - sample_titles: a few actual post titles for human review
    """
    unigrams, bigrams, trigrams = _extract_terms(posts)

    top_terms = _extract_top_terms(unigrams, top_n=25)
    phrases = _filter_relevant_phrases(bigrams, trigrams, min_count=1)
    signals = _map_terms_to_signals(top_terms, phrases)
    coach_prompts = _infer_coach_prompts(signals)
    price_data = _extract_price_signals(posts)

    sample_titles = [p.get("title", "")[:100] for p in posts[:10] if p.get("title")]

    return {
        "meta": {
            "subreddit": subreddit,
            "post_count": len(posts),
            "analyzed_at": datetime.datetime.utcnow().isoformat() + "Z",
        },
        "top_terms": top_terms[:15],
        "phrases": phrases[:12],
        "decision_drivers": signals,
        "coach_prompt_suggestions": coach_prompts,
        "price_signals": price_data["area_price_summary"],
        "price_mentions_raw": price_data["raw_mentions"],
        "sample_titles": sample_titles,
        # Private: raw frequencies for taxonomy accumulation
        "_term_frequencies": dict(unigrams.most_common(50)),
        "_phrase_frequencies": {
            " ".join(k): v for k, v in bigrams.most_common(30)
        },
        "_price_raw": price_data["raw_mentions"],
    }


def save_report(report: Dict) -> str:
    """Save the report to a dated JSON file. Returns the file path."""
    os.makedirs(REPORTS_DIR, exist_ok=True)
    date_str = datetime.date.today().strftime("%Y-%m-%d")

    # If a file for today already exists, append a sequence number
    base_path = os.path.join(REPORTS_DIR, f"{date_str}_reddit_report.json")
    path = base_path
    seq = 1
    while os.path.exists(path):
        path = os.path.join(REPORTS_DIR, f"{date_str}_reddit_report_{seq}.json")
        seq += 1

    # Don't store raw frequencies in saved report (keep private)
    public_report = {k: v for k, v in report.items() if not k.startswith("_")}
    with open(path, "w") as f:
        json.dump(public_report, f, indent=2)
    return path


def update_taxonomy(report: Dict) -> str:
    """
    Merge new report findings into the rolling taxonomy.

    Taxonomy accumulates:
      - term_counts: unigram frequencies summed across all runs
      - phrase_counts: bigram/phrase frequencies summed across all runs
      - decision_drivers: deduplicated list of schema signals discovered
      - coach_prompts: deduplicated list of coach questions
      - run_count: number of research runs
    """
    os.makedirs(os.path.dirname(TAXONOMY_PATH), exist_ok=True)

    if os.path.exists(TAXONOMY_PATH):
        with open(TAXONOMY_PATH) as f:
            taxonomy = json.load(f)
    else:
        taxonomy = {
            "term_counts": {},
            "phrase_counts": {},
            "decision_drivers": [],
            "coach_prompts": [],
            "area_prices": {},      # area -> list of price_inr observations
            "area_sqft_rates": {},  # area -> list of per-sqft rate observations
            "run_count": 0,
            "subreddits_crawled": [],
            "last_updated": None,
        }

    # Ensure price keys exist for older taxonomy files
    if "area_prices" not in taxonomy:
        taxonomy["area_prices"] = {}
    if "area_sqft_rates" not in taxonomy:
        taxonomy["area_sqft_rates"] = {}

    # Accumulate term frequencies
    for term, count in report.get("_term_frequencies", {}).items():
        taxonomy["term_counts"][term] = taxonomy["term_counts"].get(term, 0) + count

    # Accumulate phrase frequencies
    for phrase, count in report.get("_phrase_frequencies", {}).items():
        taxonomy["phrase_counts"][phrase] = taxonomy["phrase_counts"].get(phrase, 0) + count

    # Merge decision drivers (deduplicate)
    existing_drivers = set(taxonomy["decision_drivers"])
    for d in report.get("decision_drivers", []):
        if d not in existing_drivers:
            taxonomy["decision_drivers"].append(d)

    # Merge coach prompts (deduplicate)
    existing_prompts = set(taxonomy["coach_prompts"])
    for p in report.get("coach_prompt_suggestions", []):
        if p not in existing_prompts:
            taxonomy["coach_prompts"].append(p)

    # Accumulate raw price observations (for future median recalculation)
    for mention in report.get("_price_raw", []):
        area = mention.get("area", "unknown")
        price = mention.get("price_inr", 0)
        if mention.get("type") == "per_sqft":
            taxonomy["area_sqft_rates"].setdefault(area, [])
            taxonomy["area_sqft_rates"][area].append(price)
        else:
            taxonomy["area_prices"].setdefault(area, [])
            taxonomy["area_prices"][area].append(price)

    # Track subreddits
    subreddit = report.get("meta", {}).get("subreddit", "")
    if subreddit and subreddit not in taxonomy["subreddits_crawled"]:
        taxonomy["subreddits_crawled"].append(subreddit)

    taxonomy["run_count"] = taxonomy.get("run_count", 0) + 1
    taxonomy["last_updated"] = datetime.datetime.utcnow().isoformat() + "Z"

    # Sort term_counts for readability
    taxonomy["term_counts"] = dict(
        sorted(taxonomy["term_counts"].items(), key=lambda x: x[1], reverse=True)
    )
    taxonomy["phrase_counts"] = dict(
        sorted(taxonomy["phrase_counts"].items(), key=lambda x: x[1], reverse=True)
    )

    with open(TAXONOMY_PATH, "w") as f:
        json.dump(taxonomy, f, indent=2)

    # Auto-compact every 10 runs to keep data fresh
    if taxonomy["run_count"] % 10 == 0:
        compact_taxonomy(verbose=False)

    return TAXONOMY_PATH


# --- Compaction constants ---

PRICE_WINDOW = 100       # max raw price observations to keep per area
TERM_DECAY = 0.88        # multiply all term/phrase counts by this on compaction
MIN_TERM_COUNT = 2       # drop terms below this after decay
ARCHIVE_DIR = os.path.join(REPORTS_DIR, "archive")


def compact_taxonomy(verbose: bool = True) -> Dict[str, Any]:
    """
    Compact the taxonomy to keep it fresh and bounded.

    What happens:
      1. Snapshot current taxonomy to archive before touching it
      2. Price lists: keep only the last PRICE_WINDOW observations per area
         (sliding window — newest data matters most for market pricing)
      3. Term/phrase counts: apply TERM_DECAY multiplier
         (recent crawls outweigh older ones over time)
      4. Drop terms below MIN_TERM_COUNT after decay (prune long-tail noise)
      5. Merge all report files from today into a single merged report,
         delete the originals, save the merged one to archive/

    Returns a summary dict with what changed.
    """
    if not os.path.exists(TAXONOMY_PATH):
        return {"status": "skipped", "reason": "no taxonomy file"}

    with open(TAXONOMY_PATH) as f:
        taxonomy = json.load(f)

    # --- 1. Snapshot before compaction ---
    os.makedirs(ARCHIVE_DIR, exist_ok=True)
    ts = datetime.datetime.utcnow().strftime("%Y%m%dT%H%M%S")
    snapshot_path = os.path.join(ARCHIVE_DIR, f"taxonomy_snapshot_{ts}.json")
    with open(snapshot_path, "w") as f:
        json.dump(taxonomy, f, indent=2)

    stats: Dict[str, Any] = {
        "snapshot": snapshot_path,
        "compacted_at": datetime.datetime.utcnow().isoformat() + "Z",
    }

    # --- 2. Price list sliding window ---
    price_trimmed = 0
    for area, prices in taxonomy.get("area_prices", {}).items():
        if len(prices) > PRICE_WINDOW:
            removed = len(prices) - PRICE_WINDOW
            taxonomy["area_prices"][area] = prices[-PRICE_WINDOW:]
            price_trimmed += removed

    for area, rates in taxonomy.get("area_sqft_rates", {}).items():
        if len(rates) > PRICE_WINDOW:
            removed = len(rates) - PRICE_WINDOW
            taxonomy["area_sqft_rates"][area] = rates[-PRICE_WINDOW:]
            price_trimmed += removed

    stats["price_observations_trimmed"] = price_trimmed

    # --- 3. Term count decay ---
    terms_before = len(taxonomy.get("term_counts", {}))
    taxonomy["term_counts"] = {
        term: int(count * TERM_DECAY)
        for term, count in taxonomy.get("term_counts", {}).items()
        if int(count * TERM_DECAY) >= MIN_TERM_COUNT
    }
    taxonomy["phrase_counts"] = {
        phrase: int(count * TERM_DECAY)
        for phrase, count in taxonomy.get("phrase_counts", {}).items()
        if int(count * TERM_DECAY) >= MIN_TERM_COUNT
    }
    terms_after = len(taxonomy.get("term_counts", {}))
    stats["terms_before"] = terms_before
    stats["terms_after"] = terms_after
    stats["terms_pruned"] = terms_before - terms_after

    # Re-sort after decay
    taxonomy["term_counts"] = dict(
        sorted(taxonomy["term_counts"].items(), key=lambda x: x[1], reverse=True)
    )
    taxonomy["phrase_counts"] = dict(
        sorted(taxonomy["phrase_counts"].items(), key=lambda x: x[1], reverse=True)
    )

    # --- 4. Merge today's report files ---
    date_str = datetime.date.today().strftime("%Y-%m-%d")
    report_files = sorted([
        f for f in os.listdir(REPORTS_DIR)
        if f.startswith(date_str) and f.endswith(".json") and "merged" not in f
    ])

    merged_reports_count = 0
    if len(report_files) > 1:
        merged: Dict[str, Any] = {
            "meta": {
                "merged_at": datetime.datetime.utcnow().isoformat() + "Z",
                "source_files": report_files,
                "total_posts": 0,
            },
            "price_signals_merged": {},
            "decision_drivers": [],
            "coach_prompt_suggestions": [],
            "sample_titles": [],
        }

        seen_drivers: set = set()
        seen_prompts: set = set()

        for fname in report_files:
            fpath = os.path.join(REPORTS_DIR, fname)
            with open(fpath) as f:
                r = json.load(f)

            merged["meta"]["total_posts"] += r.get("meta", {}).get("post_count", 0)
            merged["sample_titles"].extend(r.get("sample_titles", []))

            # Merge price signals (keep latest per area)
            for area, data in r.get("price_signals", {}).items():
                merged["price_signals_merged"][area] = data

            for d in r.get("decision_drivers", []):
                if d not in seen_drivers:
                    merged["decision_drivers"].append(d)
                    seen_drivers.add(d)

            for p in r.get("coach_prompt_suggestions", []):
                if p not in seen_prompts:
                    merged["coach_prompt_suggestions"].append(p)
                    seen_prompts.add(p)

        # Save merged to archive
        merged_path = os.path.join(ARCHIVE_DIR, f"{date_str}_merged_report.json")
        with open(merged_path, "w") as f:
            json.dump(merged, f, indent=2)

        # Delete originals
        for fname in report_files:
            os.remove(os.path.join(REPORTS_DIR, fname))

        merged_reports_count = len(report_files)
        stats["reports_merged"] = merged_reports_count
        stats["merged_report"] = merged_path

    taxonomy["last_compacted"] = datetime.datetime.utcnow().isoformat() + "Z"

    with open(TAXONOMY_PATH, "w") as f:
        json.dump(taxonomy, f, indent=2)

    if verbose:
        print(f"  Compacted: {stats.get('terms_pruned', 0)} terms pruned, "
              f"{price_trimmed} price obs trimmed, "
              f"{merged_reports_count} reports merged → archive")

    return stats
