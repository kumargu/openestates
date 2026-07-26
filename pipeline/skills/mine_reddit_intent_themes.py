"""
Mine buyer-intent themes from Reddit text and compare them with the search schema registry.

Usage:
  python3 -m pipeline.skills.mine_reddit_intent_themes
  python3 -m pipeline.skills.mine_reddit_intent_themes --sample-text "clubhouse and pool maintained..."
  python3 -m pipeline.skills.mine_reddit_intent_themes --fetch-reddit --subreddit BangaloreRealEstates --days 30

This is an offline schema-discovery helper. It never runs on /api/search.
"""

import argparse
import json
import re
import time
from collections import Counter
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Dict, List, NamedTuple, Set, Tuple
from urllib.parse import urlencode
from urllib.request import Request, urlopen

PROJECT_ROOT = Path(__file__).resolve().parents[2]
REGISTRY_PATH = PROJECT_ROOT / "app" / "config" / "dag" / "fact_registry.json"
LOCAL_TAXONOMY_PATH = PROJECT_ROOT / "data" / "reddit" / "taxonomy.json"
LOCAL_REPORTS_DIR = PROJECT_ROOT / "data" / "reddit" / "reports"
CACHE_DIR = PROJECT_ROOT / "data" / "cache" / "reddit_intent_themes"
OUTPUT_PATH = PROJECT_ROOT / "data" / "search" / "theme_inventory_report.json"

STOPWORDS = {
    "about",
    "above",
    "across",
    "after",
    "also",
    "around",
    "been",
    "being",
    "between",
    "but",
    "can",
    "could",
    "does",
    "few",
    "for",
    "from",
    "had",
    "has",
    "have",
    "how",
    "into",
    "just",
    "like",
    "looks",
    "many",
    "maybe",
    "more",
    "much",
    "near",
    "not",
    "now",
    "off",
    "one",
    "only",
    "out",
    "over",
    "really",
    "road",
    "same",
    "should",
    "site",
    "some",
    "than",
    "that",
    "the",
    "their",
    "there",
    "this",
    "today",
    "was",
    "were",
    "what",
    "when",
    "where",
    "which",
    "will",
    "with",
    "worth",
    "would",
    "years",
}


class TextCorpus(NamedTuple):
    texts: List[str]
    sources: List[str]


def load_registry() -> Dict[str, Any]:
    return json.loads(REGISTRY_PATH.read_text())


def load_local_corpus(sample_texts: List[str]) -> TextCorpus:
    texts = []  # type: List[str]
    sources = []  # type: List[str]

    for text in sample_texts:
        if text.strip():
            texts.append(text)
            sources.append("sample_text")

    if LOCAL_TAXONOMY_PATH.exists():
        taxonomy = json.loads(LOCAL_TAXONOMY_PATH.read_text())
        texts.extend(taxonomy.get("decision_drivers", []))
        texts.extend(taxonomy.get("coach_prompts", []))
        texts.extend(taxonomy.get("term_counts", {}).keys())
        texts.extend(taxonomy.get("phrase_counts", {}).keys())
        sources.append(str(LOCAL_TAXONOMY_PATH.relative_to(PROJECT_ROOT)))

    if LOCAL_REPORTS_DIR.exists():
        for path in sorted(LOCAL_REPORTS_DIR.glob("*.json")):
            data = json.loads(path.read_text())
            strings = list(walk_strings(data))
            texts.extend(strings)
            sources.append(str(path.relative_to(PROJECT_ROOT)))

    return TextCorpus(texts=texts, sources=sources)


def fetch_reddit_corpus(subreddit: str, days: int, limit: int) -> TextCorpus:
    CACHE_DIR.mkdir(parents=True, exist_ok=True)
    cache_path = CACHE_DIR / f"{subreddit}_{days}d_{limit}.json"
    if cache_path.exists():
        data = json.loads(cache_path.read_text())
        return TextCorpus(texts=data["texts"], sources=data["sources"])

    cutoff = time.time() - days * 24 * 60 * 60
    listing = reddit_get_json(
        f"https://www.reddit.com/r/{subreddit}/new.json?"
        + urlencode({"limit": min(limit, 100)})
    )

    texts = []  # type: List[str]
    sources = []  # type: List[str]
    posts = listing.get("data", {}).get("children", [])
    for child in posts:
        post = child.get("data", {})
        created = float(post.get("created_utc") or 0)
        if created < cutoff:
            continue

        title = post.get("title") or ""
        selftext = post.get("selftext") or ""
        permalink = post.get("permalink") or ""
        post_id = post.get("id") or ""
        if title:
            texts.append(title)
        if selftext:
            texts.append(selftext)
        if permalink:
            sources.append(f"https://reddit.com{permalink}")

        if post_id:
            time.sleep(1.5)
            comments = reddit_get_json(
                f"https://www.reddit.com/r/{subreddit}/comments/{post_id}.json?"
                + urlencode({"limit": 100, "sort": "top"})
            )
            for body in extract_comment_bodies(comments):
                texts.append(body)

    payload = {"texts": texts, "sources": sources, "fetched_at": now_iso()}
    cache_path.write_text(json.dumps(payload, indent=2))
    return TextCorpus(texts=texts, sources=sources)


def reddit_get_json(url: str) -> Any:
    req = Request(
        url,
        headers={
            "User-Agent": "python:openestates:intent-theme-miner:v1.0",
            "Accept": "application/json",
        },
    )
    with urlopen(req, timeout=20) as resp:
        return json.loads(resp.read().decode("utf-8"))


def extract_comment_bodies(payload: Any) -> List[str]:
    bodies = []  # type: List[str]

    def visit(value: Any) -> None:
        if isinstance(value, dict):
            if isinstance(value.get("body"), str):
                bodies.append(value["body"])
            for child in value.values():
                visit(child)
        elif isinstance(value, list):
            for child in value:
                visit(child)

    visit(payload)
    return bodies


def walk_strings(value: Any) -> List[str]:
    found = []  # type: List[str]

    def visit(item: Any) -> None:
        if isinstance(item, str):
            found.append(item)
        elif isinstance(item, dict):
            for child in item.values():
                visit(child)
        elif isinstance(item, list):
            for child in item:
                visit(child)

    visit(value)
    return found


def tokenize(text: str) -> List[str]:
    tokens = re.findall(r"[a-z][a-z0-9+.-]*", text.lower())
    return [token for token in tokens if len(token) >= 3 and token not in STOPWORDS]


def count_terms(texts: List[str]) -> Tuple[Counter, Counter]:
    terms: Counter[str] = Counter()
    phrases: Counter[str] = Counter()
    for text in texts:
        tokens = tokenize(text)
        terms.update(tokens)
        for size in (2, 3):
            for i in range(0, max(0, len(tokens) - size + 1)):
                phrase_tokens = tokens[i : i + size]
                if any(token in STOPWORDS for token in phrase_tokens):
                    continue
                phrases[" ".join(phrase_tokens)] += 1
    return terms, phrases


def registry_terms_for_theme(registry: Dict[str, Any], dimension: str, label: str) -> Set[str]:
    terms = set()  # type: Set[str]
    for theme in registry.get("theme_layers", []):
        if theme.get("dimension") == dimension:
            terms.update(normalize_terms(theme.get("intent_terms", [])))

    for pattern in registry.get("positive_preference_patterns", []):
        if pattern.get("label") == label or pattern.get("label") == dimension.replace("_", " "):
            terms.update(normalize_terms(pattern.get("patterns", [])))

    for evidence in registry.get("text_evidence", []):
        if evidence.get("dimension") == dimension or evidence.get("label") == label:
            terms.update(normalize_terms(evidence.get("aliases", [])))
            terms.update(normalize_terms(evidence.get("positive_terms", [])))

    return terms


def normalize_terms(values: List[str]) -> Set[str]:
    return {value.lower().strip() for value in values if value and value.strip()}


def score_themes(
    registry: Dict[str, Any], terms: Counter, phrases: Counter
) -> Tuple[List[Dict[str, Any]], List[Dict[str, Any]]]:
    ranked = []  # type: List[Dict[str, Any]]
    covered_terms = set()  # type: Set[str]

    for theme in registry.get("theme_layers", []):
        theme_terms = registry_terms_for_theme(
            registry, theme["dimension"], theme.get("label", "")
        )
        hit_terms = Counter()
        for term in theme_terms:
            count = phrases[term] if " " in term else terms[term]
            if count > 0:
                hit_terms[term] = count
                covered_terms.add(term)

        ranked.append(
            {
                "rank": theme["rank"],
                "dimension": theme["dimension"],
                "label": theme["label"],
                "layer": theme["layer"],
                "score": sum(hit_terms.values()),
                "hits": hit_terms.most_common(12),
                "fact_keys": theme.get("fact_keys", []),
                "source_priority": theme.get("source_priority", []),
            }
        )

    ranked.sort(key=lambda item: (-item["score"], item["rank"]))
    unmapped = suggest_unmapped_terms(registry, terms, phrases, covered_terms)
    return ranked, unmapped


def suggest_unmapped_terms(
    registry: Dict[str, Any],
    terms: Counter,
    phrases: Counter,
    covered_terms: Set[str],
) -> List[Dict[str, Any]]:
    known = set(covered_terms)  # type: Set[str]
    for theme in registry.get("theme_layers", []):
        known.update(normalize_terms(theme.get("intent_terms", [])))
        known.update(normalize_terms(theme.get("fact_keys", [])))
    for pattern in registry.get("positive_preference_patterns", []):
        known.update(normalize_terms(pattern.get("patterns", [])))
        known.update(normalize_terms(pattern.get("expanded_keys", [])))
    for evidence in registry.get("text_evidence", []):
        known.update(normalize_terms(evidence.get("aliases", [])))
        known.update(normalize_terms(evidence.get("positive_terms", [])))
        known.update(normalize_terms(evidence.get("negative_terms", [])))

    candidates = []  # type: List[Tuple[str, int]]
    for phrase, count in phrases.most_common(100):
        if count >= 2 and phrase not in known:
            candidates.append((phrase, count))
    for term, count in terms.most_common(100):
        if count >= 2 and term not in known:
            candidates.append((term, count))

    return [{"term": term, "count": count} for term, count in candidates[:40]]


def now_iso() -> str:
    return datetime.now(timezone.utc).isoformat()


def build_report(args: argparse.Namespace) -> Dict[str, Any]:
    registry = load_registry()
    corpus = load_local_corpus(args.sample_text)

    if args.fetch_reddit:
        try:
            live = fetch_reddit_corpus(args.subreddit, args.days, args.limit)
            corpus.texts.extend(live.texts)
            corpus.sources.extend(live.sources)
        except Exception as exc:
            corpus.sources.append(f"reddit_fetch_failed:{type(exc).__name__}:{exc}")

    terms, phrases = count_terms(corpus.texts)
    theme_scores, unmapped_terms = score_themes(registry, terms, phrases)

    return {
        "generated_at": now_iso(),
        "registry": str(REGISTRY_PATH.relative_to(PROJECT_ROOT)),
        "source_count": len(corpus.sources),
        "text_count": len(corpus.texts),
        "sources": corpus.sources[:50],
        "theme_scores": theme_scores,
        "unmapped_terms": unmapped_terms,
        "top_terms": terms.most_common(60),
        "top_phrases": phrases.most_common(60),
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--sample-text", action="append", default=[])
    parser.add_argument("--fetch-reddit", action="store_true")
    parser.add_argument("--subreddit", default="BangaloreRealEstates")
    parser.add_argument("--days", type=int, default=30)
    parser.add_argument("--limit", type=int, default=100)
    parser.add_argument("--output", type=Path, default=OUTPUT_PATH)
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    report = build_report(args)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2))

    print(f"Wrote {args.output.relative_to(PROJECT_ROOT)}")
    for theme in report["theme_scores"][:12]:
        print(
            f"{theme['score']:>3}  #{theme['rank']:<2} "
            f"{theme['dimension']:<24} {theme['layer']}"
        )
    if report["unmapped_terms"]:
        print("Unmapped candidates:")
        for candidate in report["unmapped_terms"][:10]:
            print(f"  {candidate['count']:>3}  {candidate['term']}")


if __name__ == "__main__":
    main()
