"""
Compliance audit — Reddit-derived silver/serving facts must not contain raw comment bodies.

Usage:
    python3 -m pipeline.audit_reddit_compliance [--lake-root data/lake]
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Iterable, List, Tuple

FORBIDDEN_VALUE_SUBSTRINGS = (
    "selftext",
    "comment_body",
    "permalink",
)
DERIVED_TOKENS = {"mentioned", "signal", "concern", "positive", "negative"}
MAX_DERIVED_VALUE_LEN = 64


def iter_parquet_files(root: Path) -> Iterable[Path]:
    if not root.exists():
        return
    for path in sorted(root.rglob("*.parquet")):
        if "reddit_resident_facts" in str(path) or "search_serving_bundle" in str(path):
            yield path


def scan_parquet(path: Path) -> List[str]:
    try:
        import pyarrow.parquet as pq
    except ImportError:
        return []

    violations: List[str] = []
    table = pq.read_table(path)
    columns = set(table.column_names)
    if "value_json" not in columns and "value_text" not in columns:
        return violations

    source_types = None
    if "source_type" in columns:
        source_types = [str(value or "") for value in table.column("source_type").to_pylist()]

    value_jsons = (
        [str(value or "") for value in table.column("value_json").to_pylist()]
        if "value_json" in columns
        else []
    )
    value_texts = (
        [str(value or "") for value in table.column("value_text").to_pylist()]
        if "value_text" in columns
        else []
    )

    row_count = max(len(value_jsons), len(value_texts), len(source_types or []))
    for index in range(row_count):
        source_type = (source_types or [""])[index] if source_types else ""
        if source_type and "reddit" not in source_type.lower():
            continue

        raw_value = value_texts[index] if index < len(value_texts) else ""
        if not raw_value and index < len(value_jsons):
            payload = value_jsons[index]
            try:
                parsed = json.loads(payload)
                raw_value = str(parsed.get("data") or "")
            except (TypeError, ValueError, AttributeError):
                raw_value = payload

        lowered = raw_value.lower().strip()
        if not lowered:
            continue
        if any(token in lowered for token in FORBIDDEN_VALUE_SUBSTRINGS):
            violations.append("{} row {}: forbidden field name in value".format(path, index))
            continue
        if len(raw_value) > MAX_DERIVED_VALUE_LEN and lowered not in DERIVED_TOKENS:
            violations.append(
                "{} row {}: value too long for derived RedditTheme fact ({})".format(
                    path, index, len(raw_value)
                )
            )
    return violations


def run_audit(lake_root: Path) -> Tuple[int, List[str]]:
    try:
        import pyarrow.parquet  # noqa: F401
    except ImportError:
        print("WARN — pyarrow not installed; skipping lake Parquet scan (POC JSON only)")

    violations: List[str] = []
    scanned = 0
    for path in iter_parquet_files(lake_root):
        scanned += 1
        violations.extend(scan_parquet(path))

    poc_path = Path("data/validation/reddit_poc_society_signals.json")
    if poc_path.exists():
        payload = json.loads(poc_path.read_text(encoding="utf-8"))
        for entry in payload.get("facts", []):
            value = str(entry.get("value") or "")
            if len(value) > MAX_DERIVED_VALUE_LEN:
                violations.append("POC JSON {}: value too long".format(entry.get("fact_key")))

    return scanned, violations


def main(argv: List[str] = None) -> int:
    parser = argparse.ArgumentParser(description="Audit Reddit fact compliance in lake Parquet")
    parser.add_argument(
        "--lake-root",
        default="data/lake",
        help="Lake root directory (default: data/lake)",
    )
    args = parser.parse_args(argv)
    scanned, violations = run_audit(Path(args.lake_root))
    if violations:
        print("FAIL — {} violation(s) across {} parquet file(s):".format(len(violations), scanned))
        for line in violations:
            print("  -", line)
        return 1
    print("OK — no Reddit compliance violations (scanned {} parquet file(s))".format(scanned))
    return 0


if __name__ == "__main__":
    sys.exit(main())
