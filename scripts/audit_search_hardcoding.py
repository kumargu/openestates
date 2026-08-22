#!/usr/bin/env python3
"""Audit buyer/product vocabulary outside config and tests.

The default mode is a review aid. ``--gate`` returns a failing exit code for
production hardcoding so the same audit can protect CI and merge checks.
"""

from __future__ import annotations

import argparse
import json
import re
from pathlib import Path


SOURCE_SUFFIXES = {".rs", ".ts", ".tsx"}
DEFAULT_EXCLUDES = (
    "/target/",
    "/node_modules/",
    "/dist/",
    "/build/",
    "/tests/",
    "/test/",
    "/__tests__/",
)

PRODUCTION_SEARCH_PATHS = (
    "/backend/src/search/",
    "/backend/src/routes/search.rs",
)

APPROVED_SUBSTRINGS = (
    "/app/config/dag/",
    "/backend/src/dag_config/",
    "/frontend/src/components/evidence/",
    "/frontend/src/lib/types.ts",
)

STRUCTURAL_SEARCH_TERMS = {
    "bhk",
    "budget",
    "cr",
    "crore",
    "crores",
    "l",
    "lakh",
    "lakhs",
    "km",
    "kms",
    "kilometer",
    "kilometers",
    "m",
    "meter",
    "meters",
    "metre",
    "metres",
    "under",
    "below",
    "within",
    "upto",
    "up to",
    "less than",
    "inside",
    "limit",
    "max",
    "near",
    "nearby",
    "near by",
    "close to",
}

STRUCTURAL_FACT_KEYS = {
    "geo.latitude",
    "geo.longitude",
    "place.category",
}

BLOCKED_SEARCH_CONFIG_ALIASES = {
    "aster",
    "bagalur",
    "bagmane",
    "bellandur",
    "deens",
    "electronic city",
    "gopalan",
    "hebbal",
    "hoodi",
    "indiranagar",
    "itpl",
    "jayanagar",
    "jp nagar",
    "kadugodi",
    "koramangala",
    "manipal",
    "manyata",
    "marathahalli",
    "phoenix",
    "sarjapur",
    "varthur",
    "whitefield",
}

SEARCH_CONFIG_BLOCKLIST_PATHS = (
    "app/config/dag/search_intent.json",
    "app/config/dag/search_guardrails.json",
)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--root",
        default=Path(__file__).resolve().parents[1],
        type=Path,
        help="Repository root to scan.",
    )
    parser.add_argument(
        "--mode",
        choices=("all", "production-search"),
        default="all",
        help="Scan all review-aid sources or only production search runtime.",
    )
    parser.add_argument(
        "--gate",
        action="store_true",
        help="Exit non-zero when product-semantic or blocked-alias findings exist.",
    )
    args = parser.parse_args()

    root = args.root.resolve()
    terms = sorted(load_product_terms(root), key=len, reverse=True)
    if not terms:
        raise SystemExit("No product vocabulary terms found in app/config/dag")
    pattern = re.compile(
        r"\b(" + "|".join(re.escape(term) for term in terms) + r")\b",
        re.IGNORECASE,
    )

    fact_keys = load_fact_keys(root) - STRUCTURAL_FACT_KEYS
    findings: list[tuple[Path, int, str, str]] = []
    fact_key_findings: list[tuple[Path, int, str, str]] = []
    for path in sorted(root.rglob("*")):
        if path.suffix not in SOURCE_SUFFIXES or not path.is_file():
            continue
        rel = "/" + path.relative_to(root).as_posix()
        if args.mode == "production-search" and not any(
            rel == allowed or rel.startswith(allowed) for allowed in PRODUCTION_SEARCH_PATHS
        ):
            continue
        if any(exclude in rel for exclude in DEFAULT_EXCLUDES):
            continue
        if any(approved in rel for approved in APPROVED_SUBSTRINGS):
            continue
        try:
            lines = path.read_text(encoding="utf-8").splitlines()
        except UnicodeDecodeError:
            continue
        ignored_lines = rust_cfg_test_lines(lines) if path.suffix == ".rs" else set()
        for line_no, line in enumerate(lines, start=1):
            if line_no in ignored_lines:
                continue
            if line.lstrip().startswith("//"):
                continue
            match = pattern.search(line)
            if match:
                findings.append((path.relative_to(root), line_no, match.group(0), line.strip()))
        fact_key_findings.extend(
            find_fact_key_comparisons(
                path.relative_to(root),
                lines,
                fact_keys,
                ignored_lines,
            )
        )

    blocked_config_aliases = find_blocked_search_config_aliases(root)

    print("Search hardcoding audit report")
    print("===============================")
    print(f"Mode: {'gate' if args.gate else 'warning only'}")
    print(f"Scope: {args.mode}")
    print(f"Config-derived terms: {len(terms)}")
    print(f"Findings: {len(findings)}")
    for rel, line_no, term, line in findings:
        print(f"{rel}:{line_no}: product_semantic? `{term}` :: {line}")
    print(f"Fact-key comparison findings: {len(fact_key_findings)}")
    for rel, line_no, fact_key, line in fact_key_findings:
        print(f"{rel}:{line_no}: hardcoded_fact_key? `{fact_key}` :: {line}")
    print(f"Blocked search-config alias findings: {len(blocked_config_aliases)}")
    for rel, json_path, term, value in blocked_config_aliases:
        print(f"{rel}:{json_path}: blocked_alias? `{term}` :: {value}")
    if args.gate and (findings or fact_key_findings or blocked_config_aliases):
        return 1
    return 0


def load_product_terms(root: Path) -> set[str]:
    dag = root / "app" / "config" / "dag"
    terms: set[str] = set()
    add_search_intent_terms(terms, read_json(dag / "search_intent.json"))
    add_nearby_category_terms(terms, read_json(dag / "nearby_place_categories.json"))
    return {term for term in terms if should_scan_term(term)}


def load_fact_keys(root: Path) -> set[str]:
    data = read_json(root / "app" / "config" / "dag" / "fact_registry.json")
    keys: set[str] = set()
    collect_fact_keys(data, keys)
    return keys


def collect_fact_keys(value: object, keys: set[str], parent_key: str = "") -> None:
    if isinstance(value, dict):
        for key, item in value.items():
            collect_fact_keys(item, keys, key)
    elif isinstance(value, list):
        for item in value:
            collect_fact_keys(item, keys, parent_key)
    elif isinstance(value, str) and parent_key in {"fact_key", "fact_keys"}:
        keys.add(value)


def find_fact_key_comparisons(
    path: Path,
    lines: list[str],
    fact_keys: set[str],
    ignored_lines: set[int],
) -> list[tuple[Path, int, str, str]]:
    findings: list[tuple[Path, int, str, str]] = []
    for line_no, line in enumerate(lines, start=1):
        if line_no in ignored_lines or line.lstrip().startswith("//"):
            continue
        for fact_key in fact_keys:
            if not re.search(rf"(['\"])({re.escape(fact_key)})\1", line):
                continue
            comparison = (
                "eq_ignore_ascii_case" in line
                or "==" in line
                or "!=" in line
                or re.search(rf"(['\"]){re.escape(fact_key)}\1\s*=>", line)
            )
            if comparison:
                findings.append((path, line_no, fact_key, line.strip()))
    return findings


def read_json(path: Path) -> object:
    with path.open(encoding="utf-8") as handle:
        return json.load(handle)


def add_search_intent_terms(terms: set[str], data: object) -> None:
    if not isinstance(data, dict):
        return
    resolution = data.get("resolution")
    if isinstance(resolution, dict):
        add_nested_strings(terms, resolution.get("place_families"))


def add_nearby_category_terms(terms: set[str], data: object) -> None:
    if not isinstance(data, dict):
        return
    for category in data.get("categories", []):
        if not isinstance(category, dict):
            continue
        add_nested_strings(terms, category.get("category_aliases"))
        add_nested_strings(terms, category.get("answers_preferences"))


def add_nested_strings(terms: set[str], value: object) -> None:
    if isinstance(value, str):
        terms.add(value.strip().lower())
    elif isinstance(value, list):
        for item in value:
            add_nested_strings(terms, item)
    elif isinstance(value, dict):
        for item in value.values():
            add_nested_strings(terms, item)


def rust_cfg_test_lines(lines: list[str]) -> set[int]:
    ignored: set[int] = set()
    pending_cfg_test = False
    in_test_module = False
    brace_depth = 0
    start_depth = 0

    for index, line in enumerate(lines, start=1):
        stripped = line.strip()
        if stripped == "#[cfg(test)]":
            pending_cfg_test = True
            ignored.add(index)
            continue

        opens = line.count("{")
        closes = line.count("}")

        if pending_cfg_test:
            ignored.add(index)
            if "mod tests" in line and "{" in line:
                in_test_module = True
                start_depth = brace_depth
            pending_cfg_test = False
        elif in_test_module:
            ignored.add(index)

        brace_depth += opens - closes
        if in_test_module and brace_depth <= start_depth:
            in_test_module = False

    return ignored


def should_scan_term(term: str) -> bool:
    ignored = {
        "one",
        "two",
        "three",
        "four",
        "five",
        "six",
        "lake",
        "inside",
        "limit",
        "max",
        "m",
        "km",
    }
    if len(term) < 2 or term in ignored or term in STRUCTURAL_SEARCH_TERMS:
        return False
    if term.startswith("app/config/"):
        return False
    if re.fullmatch(r"[0-9.]+", term):
        return False
    if re.fullmatch(r"[a-z_]+\.[a-z0-9_]+", term):
        return False
    return any(ch.isalpha() for ch in term)


def find_blocked_search_config_aliases(root: Path) -> list[tuple[Path, str, str, str]]:
    findings: list[tuple[Path, str, str, str]] = []
    for rel_path in SEARCH_CONFIG_BLOCKLIST_PATHS:
        path = root / rel_path
        if not path.exists():
            continue
        data = read_json(path)
        for json_path, value in iter_json_strings(data):
            value_lower = value.lower()
            for term in sorted(BLOCKED_SEARCH_CONFIG_ALIASES, key=len, reverse=True):
                if contains_term(value_lower, term):
                    findings.append((Path(rel_path), json_path, term, value))
                    break
    return findings


def iter_json_strings(value: object, path: str = "$") -> list[tuple[str, str]]:
    strings: list[tuple[str, str]] = []
    if isinstance(value, str):
        strings.append((path, value))
    elif isinstance(value, list):
        for index, item in enumerate(value):
            strings.extend(iter_json_strings(item, f"{path}[{index}]"))
    elif isinstance(value, dict):
        for key, item in value.items():
            strings.extend(iter_json_strings(item, f"{path}.{key}"))
    return strings


def contains_term(text: str, term: str) -> bool:
    return re.search(r"\b" + re.escape(term) + r"\b", text, re.IGNORECASE) is not None


if __name__ == "__main__":
    raise SystemExit(main())
