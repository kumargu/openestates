#!/usr/bin/env python3
"""Baseline product-semantic hardcoding audit for DAG convergence.

This M0 harness is warning-only. It intentionally scans code, config, docs,
and tests, then classifies findings so later milestones can distinguish runtime
product semantics from structural code, test fixtures, and known migration debt.
"""

from __future__ import annotations

import argparse
import json
import re
from collections import Counter, defaultdict
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable


SCAN_SUFFIXES = {".rs", ".ts", ".tsx", ".py", ".json", ".md", ".sh"}

DEFAULT_SCAN_ROOTS = (
    "app/config",
    "backend/src",
    "backend/tests",
    "frontend/src",
    "frontend/tests",
    "pipeline",
    "tests",
    "scripts",
    "docs",
    "data/validation/search_query_bank.json",
)

REQUIRED_CONFIG_FILES = (
    "app/config/dag/search_intent.json",
    "app/config/dag/search_guardrails.json",
    "app/config/dag/nearby_place_categories.json",
    "app/config/dag/ui_surfaces.json",
    "app/config/dag/evidence_sections.json",
    "app/config/dag/scoring_policy.json",
    "app/config/dag/fact_registry.json",
    "app/config/dag/resolution_policies.json",
    "app/config/dag/manifest.json",
)

REQUIRED_CONFIG_DIRS = (
    "app/config/dag/source_adapters",
)

SKIP_DIR_PARTS = {
    ".git",
    ".venv",
    "node_modules",
    "target",
    "dist",
    "build",
    ".vite",
}

SKIP_FILENAMES = {
    "package-lock.json",
    "Cargo.lock",
}

STRUCTURAL_PATH_MARKERS = (
    "app/config/dag/",
    "app/config/bootstrap/",
    "app/config/lake/",
    "app/config/runtime/",
    "backend/src/dag_config/",
    "frontend/src/styles/",
    "scripts/audit_dag_convergence.py",
    "scripts/audit_search_hardcoding.py",
)

API_CONTRACT_PATH_MARKERS = (
    "backend/src/models/",
    "backend/src/serving/types.rs",
    "frontend/src/lib/api.ts",
    "frontend/src/lib/types.ts",
)

TEST_PATH_MARKERS = (
    "/tests/",
    "frontend/tests/",
    "backend/tests/",
    "data/validation/",
    "fixtures/",
    "__tests__",
)

KNOWN_DEBT_PATHS = set()

KNOWN_DEBT_DOC_MARKERS = (
    "dag_convergence",
    "phase_",
    "hardcoding_audit",
    "roadmap",
    "handoff",
)

TERM_IGNORE = {
    "a",
    "an",
    "and",
    "api",
    "app",
    "area",
    "as",
    "at",
    "by",
    "config",
    "data",
    "default",
    "false",
    "for",
    "from",
    "home",
    "id",
    "in",
    "is",
    "key",
    "km",
    "label",
    "lake",
    "max",
    "m",
    "no",
    "of",
    "on",
    "one",
    "or",
    "source",
    "test",
    "the",
    "to",
    "true",
    "type",
    "ui",
    "value",
    "with",
}

CATEGORY_PRIORITY = {
    "policy constants": 0,
    "policy keys": 0,
    "warning/red-flag terms": 0,
    "source labels": 1,
    "map layer names": 2,
    "recommendation branch names": 3,
    "evidence section names": 4,
    "search vocabulary": 5,
}


@dataclass(frozen=True)
class Term:
    value: str
    category: str


@dataclass(frozen=True)
class Finding:
    path: Path
    line_no: int
    term: str
    term_category: str
    classification: str
    line: str


@dataclass(frozen=True)
class Matcher:
    term: str
    category: str
    pattern: re.Pattern[str]


@dataclass(frozen=True)
class PolicyConstantMatch:
    term: str
    category: str = "policy constants"


POLICY_CONSTANT_NAME = re.compile(
    r"\b(?:const|static|let)\s+[A-ZA-Z0-9_]*(?:LIMIT|CAP|MAX|MIN|RADIUS|THRESHOLD|WEIGHT|SCORE|RANK|SORT|FALLBACK|RECALL|DEFAULT)[A-Z0-9_]*\b"
)

POLICY_CONSTANT_LITERAL = re.compile(
    r"\b(?:limit|cap|max|min|radius|threshold|weight|score|rank|sort|fallback|recall|default)[A-Za-z0-9_]*\s*[:=]\s*(?:\d+(?:\.\d+)?|[\"'][^\"']+[\"']|\[)"
)

ARRAY_POLICY_LITERAL = re.compile(
    r"\b(?:const|static|let)\s+[A-ZA-Z0-9_]+\s*[:=][^;\n]*\[(?:\s*(?:\d|[\"']))"
)

RUNTIME_POLICY_EXTENSIONS = {".rs", ".ts", ".tsx", ".py"}

def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--root",
        default=Path(__file__).resolve().parents[1],
        type=Path,
        help="Repository root to scan.",
    )
    parser.add_argument(
        "--format",
        choices=("markdown", "json"),
        default="markdown",
        help="Output format.",
    )
    parser.add_argument(
        "--scan-root",
        action="append",
        default=[],
        help="Relative path to scan. May be repeated. Defaults to M0 source/config/doc/test roots.",
    )
    parser.add_argument(
        "--max-findings",
        type=int,
        default=250,
        help="Maximum finding rows to print; summaries always include all findings.",
    )
    args = parser.parse_args()

    root = args.root.resolve()
    validate_audit_inputs(root)
    terms = collect_terms(root)
    scan_roots = tuple(args.scan_root) if args.scan_root else DEFAULT_SCAN_ROOTS
    findings = audit(root, terms, scan_roots)

    if args.format == "json":
        print_json(root, terms, findings)
    else:
        print_markdown(root, terms, findings, args.max_findings)
    return 0


def collect_terms(root: Path) -> list[Term]:
    dag = root / "app" / "config" / "dag"
    terms: set[Term] = set()

    add_search_intent_terms(terms, read_json(dag / "search_intent.json"))
    add_policy_terms(terms, read_json(dag / "search_guardrails.json"), "search guardrails")
    add_nearby_category_terms(terms, read_json(dag / "nearby_place_categories.json"))
    add_ui_surface_terms(terms, read_json(dag / "ui_surfaces.json"))
    add_evidence_section_terms(terms, read_json(dag / "evidence_sections.json"))
    add_scoring_policy_terms(terms, read_json(dag / "scoring_policy.json"))
    add_fact_registry_terms(terms, read_json(dag / "fact_registry.json"))
    add_policy_terms(terms, read_json(dag / "resolution_policies.json"), "resolution policies")
    add_manifest_terms(terms, read_json(dag / "manifest.json"))
    add_source_terms(terms, dag / "source_adapters")
    add_source_terms(terms, dag / "source_scopes")

    unique_terms = dedupe_terms(terms)
    return sorted(
        (term for term in unique_terms if should_scan_term(term.value)),
        key=lambda item: (item.value.count(" "), len(item.value), item.value),
        reverse=True,
    )


def dedupe_terms(terms: set[Term]) -> list[Term]:
    by_value: dict[str, Term] = {}
    for term in terms:
        current = by_value.get(term.value)
        if current is None or category_rank(term.category) < category_rank(current.category):
            by_value[term.value] = term
    return list(by_value.values())


def category_rank(category: str) -> int:
    return CATEGORY_PRIORITY.get(category, 99)


def read_json(path: Path) -> object:
    with path.open(encoding="utf-8") as handle:
        return json.load(handle)


def validate_audit_inputs(root: Path) -> None:
    missing: list[str] = []
    for rel in REQUIRED_CONFIG_FILES:
        if not (root / rel).is_file():
            missing.append(rel)
    for rel in REQUIRED_CONFIG_DIRS:
        if not (root / rel).is_dir():
            missing.append(rel)
    if missing:
        formatted = "\n".join(f"- {rel}" for rel in missing)
        raise SystemExit(f"DAG convergence audit missing required config inputs:\n{formatted}")


def add_term(terms: set[Term], value: object, category: str) -> None:
    if not isinstance(value, str):
        return
    normalized = value.strip().lower()
    if normalized:
        terms.add(Term(normalized, category))
        if "_" in normalized or "." in normalized:
            words = normalized.replace(".", " ").replace("_", " ")
            terms.add(Term(words, category))


def add_nested_strings(terms: set[Term], value: object, category: str) -> None:
    if isinstance(value, str):
        add_term(terms, value, category)
    elif isinstance(value, list):
        for item in value:
            add_nested_strings(terms, item, category)
    elif isinstance(value, dict):
        for item in value.values():
            add_nested_strings(terms, item, category)


def add_policy_terms(terms: set[Term], value: object, category: str) -> None:
    if isinstance(value, str):
        add_term(terms, value, category)
    elif isinstance(value, (int, float, bool)) or value is None:
        return
    elif isinstance(value, list):
        for item in value:
            add_policy_terms(terms, item, category)
    elif isinstance(value, dict):
        for key, item in value.items():
            add_term(terms, key, "policy keys")
            add_policy_terms(terms, item, category)


def add_search_intent_terms(terms: set[Term], data: object) -> None:
    if not isinstance(data, dict):
        return
    add_policy_terms(terms, data, "search vocabulary")


def add_nearby_category_terms(terms: set[Term], data: object) -> None:
    if not isinstance(data, dict):
        return
    for category in data.get("categories", []):
        if not isinstance(category, dict):
            continue
        for key in (
            "fact_key",
            "category_aliases",
            "display_label",
            "answers_preferences",
            "relation_class",
            "name_markers",
            "name_block_markers",
        ):
            add_nested_strings(terms, category.get(key), "map layer names")
        for risk in category.get("derived_distance_risks", []):
            add_nested_strings(terms, risk, "warning/red-flag terms")


def add_ui_surface_terms(terms: set[Term], data: object) -> None:
    if not isinstance(data, dict):
        return
    for surface in data.get("surfaces", []):
        if not isinstance(surface, dict):
            continue
        for key in ("id", "title", "kicker", "leaf_keys", "primary_entity"):
            add_nested_strings(terms, surface.get(key), "evidence section names")
        scene = surface.get("scene")
        if not isinstance(scene, dict):
            continue
        for layer in scene.get("layers", []):
            if not isinstance(layer, dict):
                continue
            for key in (
                "id",
                "label",
                "factKeys",
                "linkedEntityFactKeys",
                "family",
                "relationClass",
                "includeNameMarkers",
                "sortPriorityFactKeys",
            ):
                add_nested_strings(terms, layer.get(key), "map layer names")


def add_scoring_policy_terms(terms: set[Term], data: object) -> None:
    if not isinstance(data, dict):
        return
    add_policy_terms(terms, data.get("missing_data"), "recommendation branch names")
    add_policy_terms(terms, data.get("search_ranking"), "recommendation branch names")
    add_policy_terms(terms, data.get("fact_groups"), "recommendation branch names")
    add_policy_terms(terms, data.get("runtime_fact_keys"), "recommendation branch names")
    add_policy_terms(terms, data.get("surfaces"), "recommendation branch names")
    add_policy_terms(terms, data.get("recommendation_branches"), "recommendation branch names")
    for signal in data.get("signals", []):
        if isinstance(signal, dict):
            add_policy_terms(terms, signal, "recommendation branch names")


def add_manifest_terms(terms: set[Term], data: object) -> None:
    if not isinstance(data, dict):
        return
    add_term(terms, "proof_labels", "proof labels")
    add_policy_terms(terms, data.get("proof_labels"), "proof labels")
    add_policy_terms(terms, data.get("agent_routing"), "policy keys")


def add_fact_registry_terms(terms: set[Term], data: object) -> None:
    if not isinstance(data, dict):
        return
    facts = data.get("facts")
    if not isinstance(facts, list):
        return
    for fact in facts:
        if not isinstance(fact, dict):
            continue
        for key in (
            "fact_key",
            "display_template",
            "answers_preferences",
            "entity_types",
            "source_priority",
            "enrichment_priority",
        ):
            add_nested_strings(terms, fact.get(key), "search vocabulary")
        add_nested_strings(terms, fact.get("ui"), "evidence section names")


def add_source_terms(terms: set[Term], directory: Path) -> None:
    if not directory.exists():
        return
    for path in sorted(directory.glob("*.json")):
        add_term(terms, path.stem, "source labels")
        add_nested_strings(terms, read_json(path), "source labels")


def add_evidence_section_terms(terms: set[Term], data: object) -> None:
    if not isinstance(data, list):
        return
    for section in data:
        if not isinstance(section, dict):
            continue
        for key in ("kind", "constellation", "title", "subtitle", "scope", "relationship", "media"):
            add_nested_strings(terms, section.get(key), "evidence section names")
        for fact in section.get("facts", []):
            add_nested_strings(terms, fact, "evidence section names")


def should_scan_term(term: str) -> bool:
    if len(term) < 3 or term in TERM_IGNORE:
        return False
    if len(term) > 72:
        return False
    if len(term.split()) > 7:
        return False
    if re.fullmatch(r"[0-9.]+", term):
        return False
    if re.fullmatch(r"[a-z]{1,2}", term):
        return False
    if term.startswith(("http://", "https://", "data/")):
        return False
    return any(ch.isalpha() for ch in term)


def audit(root: Path, terms: list[Term], scan_roots: tuple[str, ...]) -> list[Finding]:
    matchers = build_matchers(terms)
    findings: list[Finding] = []

    for path in scan_paths(root, scan_roots):
        if not should_scan_path(path, root):
            continue
        rel = path.relative_to(root)
        try:
            lines = path.read_text(encoding="utf-8").splitlines()
        except UnicodeDecodeError:
            continue
        ignored_lines = rust_cfg_test_lines(lines) if path.suffix == ".rs" else set()
        classification = classify_path(rel)
        for line_no, line in enumerate(lines, start=1):
            if line_no in ignored_lines:
                continue
            matcher = find_match(line.lower(), matchers) or find_policy_constant_match(
                rel, classification, line
            )
            if not matcher:
                continue
            findings.append(
                Finding(
                    path=rel,
                    line_no=line_no,
                    term=matcher.term,
                    term_category=matcher.category,
                    classification=classification,
                    line=line.strip(),
                )
            )
    return findings


def scan_paths(root: Path, scan_roots: tuple[str, ...]) -> Iterable[Path]:
    for scan_root in scan_roots:
        path = root / scan_root
        if path.is_file():
            yield path
        elif path.is_dir():
            yield from sorted(path.rglob("*"))


def build_matchers(terms: list[Term]) -> dict[str, list[Matcher]]:
    matchers: dict[str, list[Matcher]] = defaultdict(list)
    for term in terms:
        first_token = first_word_token(term.value)
        if not first_token:
            continue
        matchers[first_token].append(
            Matcher(
                term=term.value,
                category=term.category,
                pattern=compile_term_pattern(term.value),
            )
        )
    return matchers


def compile_term_pattern(term: str) -> re.Pattern[str]:
    escaped = re.escape(term)
    if re.fullmatch(r"[a-z0-9_.-]+", term):
        return re.compile(r"(?<![a-z0-9_.-])" + escaped + r"(?![a-z0-9_.-])", re.IGNORECASE)
    return re.compile(r"\b" + escaped + r"\b", re.IGNORECASE)


def first_word_token(value: str) -> str | None:
    match = re.search(r"[a-z0-9]+", value.lower())
    return match.group(0) if match else None


def find_match(line: str, matchers: dict[str, list[Matcher]]) -> Matcher | None:
    seen_tokens: set[str] = set()
    for token in re.findall(r"[a-z0-9]+", line):
        if token in seen_tokens:
            continue
        seen_tokens.add(token)
        for matcher in matchers.get(token, []):
            if matcher.pattern.search(line):
                return matcher
    return None


def find_policy_constant_match(
    rel: Path, classification: str, line: str
) -> PolicyConstantMatch | None:
    if rel.suffix not in RUNTIME_POLICY_EXTENSIONS:
        return None
    if classification in {"structural", "test_fixture"}:
        return None
    stripped = line.strip()
    if not stripped or stripped.startswith(("//", "#", "*")):
        return None
    if POLICY_CONSTANT_NAME.search(stripped):
        return PolicyConstantMatch("policy constant name")
    if POLICY_CONSTANT_LITERAL.search(stripped):
        return PolicyConstantMatch("policy constant literal")
    if ARRAY_POLICY_LITERAL.search(stripped):
        return PolicyConstantMatch("policy array literal")
    return None


def should_scan_path(path: Path, root: Path) -> bool:
    if not path.is_file() or path.suffix not in SCAN_SUFFIXES:
        return False
    if path.name in SKIP_FILENAMES:
        return False
    rel_parts = set(path.relative_to(root).parts)
    return not bool(rel_parts & SKIP_DIR_PARTS)


def classify_path(rel: Path) -> str:
    rel_posix = rel.as_posix()
    rel_with_slash = "/" + rel_posix
    if rel.name.startswith("test_") or ".test." in rel.name or "dev-fixtures" in rel.name:
        return "test_fixture"
    if any(marker in rel_with_slash for marker in TEST_PATH_MARKERS):
        return "test_fixture"
    if rel_posix in KNOWN_DEBT_PATHS:
        return "known_debt"
    if rel_posix.startswith("docs/"):
        if any(marker in rel_posix for marker in KNOWN_DEBT_DOC_MARKERS):
            return "known_debt"
        return "structural"
    if any(marker in rel_posix for marker in API_CONTRACT_PATH_MARKERS):
        return "api_contract"
    if any(marker in rel_posix for marker in STRUCTURAL_PATH_MARKERS):
        return "structural"
    if rel_posix.startswith("app/config/product/"):
        return "known_debt"
    if rel_posix.startswith("app/config/"):
        return "structural"
    return "product_semantic"


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


def print_json(root: Path, terms: list[Term], findings: list[Finding]) -> None:
    payload = {
        "root": str(root),
        "mode": "warning_only",
        "terms": len(terms),
        "summary": summarize(findings),
        "findings": [
            {
                "path": finding.path.as_posix(),
                "line": finding.line_no,
                "term": finding.term,
                "term_category": finding.term_category,
                "classification": finding.classification,
                "source": finding.line,
            }
            for finding in findings
        ],
    }
    print(json.dumps(payload, indent=2, sort_keys=True))


def print_markdown(root: Path, terms: list[Term], findings: list[Finding], max_findings: int) -> None:
    summary = summarize(findings)
    print("# DAG Convergence Hardcoding Audit")
    print()
    print("- Mode: warning only")
    print(f"- Root: `{root}`")
    print(f"- Config-derived terms: {len(terms)}")
    print(f"- Findings: {len(findings)}")
    print()
    print("## Summary By Classification")
    print()
    for classification in ("product_semantic", "api_contract", "known_debt", "test_fixture", "structural"):
        print(f"- `{classification}`: {summary['by_classification'].get(classification, 0)}")
    print()
    print("## Summary By Term Category")
    print()
    for category, count in summary["by_term_category"].most_common():
        print(f"- `{category}`: {count}")
    print()
    print("## Top Runtime Hotspots")
    print()
    hotspots = [
        (path, count)
        for path, count in summary["by_file"].most_common()
        if classify_path(Path(path)) in {"product_semantic", "api_contract", "known_debt"}
    ][:20]
    for path, count in hotspots:
        print(f"- `{path}`: {count}")
    print()
    print("## Findings")
    print()
    if len(findings) > max_findings:
        print(f"Showing first {max_findings} of {len(findings)} findings.")
        print()
    for finding in findings[:max_findings]:
        line = finding.line.replace("|", "\\|")
        print(
            f"- `{finding.classification}` `{finding.term_category}` "
            f"`{finding.path}:{finding.line_no}` `{finding.term}` :: {line}"
        )


def summarize(findings: Iterable[Finding]) -> dict[str, Counter[str]]:
    by_classification: Counter[str] = Counter()
    by_term_category: Counter[str] = Counter()
    by_file: Counter[str] = Counter()
    for finding in findings:
        by_classification[finding.classification] += 1
        by_term_category[finding.term_category] += 1
        by_file[finding.path.as_posix()] += 1
    return {
        "by_classification": by_classification,
        "by_term_category": by_term_category,
        "by_file": by_file,
    }


if __name__ == "__main__":
    raise SystemExit(main())
