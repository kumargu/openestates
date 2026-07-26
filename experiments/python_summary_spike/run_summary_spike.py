#!/usr/bin/env python3
from __future__ import annotations

import argparse
import hashlib
import json
import os
import time
import urllib.error
import urllib.request
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Protocol


DEFAULT_MAX_TOKENS = 180
DEFAULT_TEMPERATURE = 0.0
DEFAULT_STYLE_PROFILE = Path("app/config/dag/summary_styles.json")
SUMMARY_FACT_KEY = "generated_context_summary"
SUMMARY_SKILL_ID = "generated_context_summary_local"


@dataclass(frozen=True)
class TargetWords:
    min: int
    max: int


@dataclass(frozen=True)
class EvidenceBrief:
    entity_id: str
    property_name: str
    summary_scope: str
    summary_kind: str
    target_words: TargetWords
    evidence: list[str]
    expected_terms: list[str]
    forbidden_terms: list[str]


@dataclass(frozen=True)
class SummaryResult:
    provider: str
    model: str
    summary: str
    load_ms: int
    generation_ms: int


class SummaryProvider(Protocol):
    name: str
    model: str

    def generate(self, prompt: str, max_tokens: int, temperature: float) -> SummaryResult:
        ...


class LlamaCppProvider:
    name = "llama-cpp"

    def __init__(self, model_path: Path) -> None:
        if not model_path.exists():
            raise SystemExit(f"model path does not exist: {model_path}")
        load_started = time.perf_counter()
        try:
            from llama_cpp import Llama
        except ImportError as err:
            raise SystemExit(
                "llama-cpp-python is not installed. Install it outside the backend, then rerun."
            ) from err
        self._llm = Llama(model_path=str(model_path), n_ctx=2048, verbose=False)
        self._load_ms = elapsed_ms(load_started)
        self.model = str(model_path)

    def generate(self, prompt: str, max_tokens: int, temperature: float) -> SummaryResult:
        started = time.perf_counter()
        response = self._llm(
            prompt,
            max_tokens=max_tokens,
            temperature=temperature,
            stop=["\n\n"],
        )
        generation_ms = elapsed_ms(started)
        summary = response["choices"][0]["text"].strip()
        return SummaryResult(self.name, self.model, summary, self._load_ms, generation_ms)


class OpenAICompatibleProvider:
    name = "openai-compatible"

    def __init__(self, base_url: str, model: str, api_key: str | None) -> None:
        self._base_url = base_url.rstrip("/")
        self.model = model
        self._api_key = api_key

    def generate(self, prompt: str, max_tokens: int, temperature: float) -> SummaryResult:
        payload = {
            "model": self.model,
            "messages": [{"role": "user", "content": prompt}],
            "max_tokens": max_tokens,
            "temperature": temperature,
        }
        request = urllib.request.Request(
            f"{self._base_url}/chat/completions",
            data=json.dumps(payload).encode("utf-8"),
            headers=self._headers(),
            method="POST",
        )
        started = time.perf_counter()
        try:
            with urllib.request.urlopen(request, timeout=120) as response:
                body = json.loads(response.read().decode("utf-8"))
        except urllib.error.URLError as err:
            raise SystemExit(f"summary endpoint request failed: {err}") from err
        generation_ms = elapsed_ms(started)
        summary = body["choices"][0]["message"]["content"].strip()
        return SummaryResult(self.name, self.model, summary, 0, generation_ms)

    def _headers(self) -> dict[str, str]:
        headers = {"Content-Type": "application/json"}
        if self._api_key:
            headers["Authorization"] = f"Bearer {self._api_key}"
        return headers


def main() -> None:
    args = parse_args()
    briefs = load_briefs(args)
    style = load_style_profile(args.style_profile, args.style_id)
    provider = build_provider(args)

    reports = []
    for brief in briefs:
        prompt = build_prompt(brief, style)
        result = provider.generate(prompt, args.max_tokens, args.temperature)
        evaluation = evaluate_summary(brief, result.summary)
        reports.append({"brief": brief, "result": result, "evaluation": evaluation})

    if args.output_format == "skill-facts":
        payload = build_skill_facts_payload(
            reports,
            source=args.source,
            snapshot_date=args.snapshot_date,
            style_id=args.style_id,
        )
    else:
        payload = {
            "results": [
                {
                    "entity_id": report["brief"].entity_id,
                    "property_name": report["brief"].property_name,
                    "result": report["result"].__dict__,
                    "evaluation": report["evaluation"],
                }
                for report in reports
            ]
        }
    rendered = json.dumps(payload, indent=2)
    print(rendered)

    if args.output:
        args.output.write_text(rendered + "\n", encoding="utf-8")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Run a Python LLM summary spike.")
    parser.add_argument("--provider", choices=["llama-cpp", "openai-compatible"], required=True)
    parser.add_argument("--evidence", type=Path, action="append", default=[])
    parser.add_argument("--evidence-dir", type=Path)
    parser.add_argument("--style-profile", type=Path, default=DEFAULT_STYLE_PROFILE)
    parser.add_argument("--style-id", default="buyer_human_v1")
    parser.add_argument("--output-format", choices=["report", "skill-facts"], default="report")
    parser.add_argument("--source", default="local_summary")
    parser.add_argument("--snapshot-date", default=datetime.now(timezone.utc).date().isoformat())
    parser.add_argument("--model-path", type=Path)
    parser.add_argument("--max-tokens", type=int, default=DEFAULT_MAX_TOKENS)
    parser.add_argument("--temperature", type=float, default=DEFAULT_TEMPERATURE)
    parser.add_argument("--output", type=Path)
    return parser.parse_args()


def load_briefs(args: argparse.Namespace) -> list[EvidenceBrief]:
    paths = list(args.evidence)
    if args.evidence_dir:
        paths.extend(sorted(args.evidence_dir.glob("*.json")))
    if not paths:
        raise SystemExit("--evidence or --evidence-dir is required")
    return [load_brief(path) for path in paths]


def load_brief(path: Path) -> EvidenceBrief:
    raw = json.loads(path.read_text(encoding="utf-8"))
    words = raw.get("target_words", {})
    property_name = raw["property_name"]
    return EvidenceBrief(
        entity_id=str(raw.get("entity_id") or entity_id_from_name(property_name)),
        property_name=property_name,
        summary_scope=str(raw.get("summary_scope") or "society"),
        summary_kind=str(raw.get("summary_kind") or "livability_context"),
        target_words=TargetWords(min=int(words.get("min", 80)), max=int(words.get("max", 120))),
        evidence=list(raw["evidence"]),
        expected_terms=list(raw.get("expected_terms", [])),
        forbidden_terms=list(raw.get("forbidden_terms", [])),
    )


def load_style_profile(path: Path, style_id: str) -> dict[str, Any]:
    if not path.exists():
        return {}
    payload = json.loads(path.read_text(encoding="utf-8"))
    for profile in payload.get("profiles", []):
        if profile.get("id") == style_id:
            return profile
    raise SystemExit(f"style profile {style_id!r} not found in {path}")


def build_prompt(brief: EvidenceBrief, style: dict[str, Any]) -> str:
    evidence_lines = "\n".join(f"- {item}" for item in brief.evidence)
    style_rules = "\n".join(f"- {rule}" for rule in style.get("rules", []))
    examples = "\n\n".join(
        f"Example {index + 1}:\n{example}"
        for index, example in enumerate(style.get("few_shot_examples", []))
    )
    style_block = ""
    if style_rules:
        style_block += f"\nStyle rules:\n{style_rules}\n"
    if examples:
        style_block += f"\nVoice examples:\n{examples}\n"
    return (
        "You write concise evidence-grounded property context for OpenEstates.\n"
        "Rules:\n"
        "- Use only the evidence below.\n"
        f"- Write one paragraph, {brief.target_words.min}-{brief.target_words.max} words.\n"
        "- Include the main buyer fit and the main caution.\n"
        "- Do not invent distances, prices, ratings, guarantees, or appreciation claims.\n"
        "- Do not mention raw fact keys, slugs, JSON, or internal graph terms.\n\n"
        f"{style_block}\n"
        f"Property: {brief.property_name}\n"
        f"Evidence:\n{evidence_lines}\n\n"
        "Summary:"
    )


def build_provider(args: argparse.Namespace) -> SummaryProvider:
    if args.provider == "llama-cpp":
        model_path = args.model_path or env_path("OPENESTATES_SUMMARY_GGUF")
        if not model_path:
            raise SystemExit("--model-path or OPENESTATES_SUMMARY_GGUF is required for llama-cpp")
        return LlamaCppProvider(model_path)
    base_url = os.environ.get("OPENESTATES_SUMMARY_BASE_URL")
    model = os.environ.get("OPENESTATES_SUMMARY_MODEL")
    if not base_url or not model:
        raise SystemExit(
            "OPENESTATES_SUMMARY_BASE_URL and OPENESTATES_SUMMARY_MODEL are required "
            "for openai-compatible"
        )
    return OpenAICompatibleProvider(base_url, model, os.environ.get("OPENESTATES_SUMMARY_API_KEY"))


def env_path(name: str) -> Path | None:
    value = os.environ.get(name)
    return Path(value) if value else None


def evaluate_summary(brief: EvidenceBrief, summary: str) -> dict[str, Any]:
    normalized = summary.lower()
    word_count = len(summary.split())
    return {
        "word_count": word_count,
        "word_count_ok": brief.target_words.min <= word_count <= brief.target_words.max,
        "expected_terms_found": [
            term for term in brief.expected_terms if term.lower() in normalized
        ],
        "expected_terms_missing": [
            term for term in brief.expected_terms if term.lower() not in normalized
        ],
        "forbidden_terms_found": [
            term for term in brief.forbidden_terms if term.lower() in normalized
        ],
    }


def build_skill_facts_payload(
    reports: list[dict[str, Any]],
    source: str,
    snapshot_date: str,
    style_id: str,
) -> dict[str, Any]:
    facts = []
    annotations = []
    for report in reports:
        brief: EvidenceBrief = report["brief"]
        result: SummaryResult = report["result"]
        evaluation: dict[str, Any] = report["evaluation"]
        quality_status = "passed" if summary_quality_passed(evaluation) else "failed"
        learned_at = datetime.now(timezone.utc).isoformat()
        evidence_hash = evidence_hash_for_brief(brief)
        common = {
            "confidence": 0.72 if quality_status == "passed" else 0.35,
            "source_type": "Computed",
            "source_url": None,
            "model": result.model,
            "skill_id": SUMMARY_SKILL_ID,
            "triggered_by": "asset_dag",
            "learned_at": learned_at,
            "run_id": evidence_hash,
        }
        facts.append(
            skill_fact_row(
                brief.entity_id,
                SUMMARY_FACT_KEY,
                {"type": "Text", "data": result.summary},
                input_hash=evidence_hash,
                **common,
            )
        )
        facts.append(
            skill_fact_row(
                brief.entity_id,
                "generated_context_summary_metadata",
                {
                    "type": "Text",
                    "data": json.dumps(
                        {
                            "summary_scope": brief.summary_scope,
                            "summary_kind": brief.summary_kind,
                            "style_profile": style_id,
                            "provider": result.provider,
                            "load_ms": result.load_ms,
                            "generation_ms": result.generation_ms,
                            "quality_status": quality_status,
                            "quality_checks": evaluation,
                            "evidence_hash": evidence_hash,
                        },
                        sort_keys=True,
                        separators=(",", ":"),
                    ),
                },
                input_hash=evidence_hash,
                **common,
            )
        )
        annotations.append(
            {
                "entity_id": brief.entity_id,
                "fact_key": SUMMARY_FACT_KEY,
                "display_template": "{value}",
                "answers_preferences_json": json.dumps(
                    [
                        "before you shortlist",
                        "livability summary",
                        "buyer context",
                        "daily life",
                    ],
                    separators=(",", ":"),
                ),
                "scoring_direction": "TextMatch",
                "scoring_weight": 0.4,
                "scoring_thresholds_json": "[]",
            }
        )
        annotations.append(
            {
                "entity_id": brief.entity_id,
                "fact_key": "generated_context_summary_metadata",
                "display_template": None,
                "answers_preferences_json": "[]",
                "scoring_direction": None,
                "scoring_weight": None,
                "scoring_thresholds_json": "[]",
            }
        )
    return {
        "source": source,
        "snapshot_date": snapshot_date,
        "facts": facts,
        "fact_annotations": annotations,
        "source_watermarks": [
            {
                "source": "generated_context_summaries",
                "high_watermark": datetime.now(timezone.utc).isoformat(),
            }
        ],
    }


def summary_quality_passed(evaluation: dict[str, Any]) -> bool:
    return (
        bool(evaluation["word_count_ok"])
        and not evaluation["expected_terms_missing"]
        and not evaluation["forbidden_terms_found"]
    )


def skill_fact_row(
    entity_id: str,
    fact_key: str,
    value: dict[str, Any],
    *,
    confidence: float,
    source_type: str,
    source_url: str | None,
    model: str,
    skill_id: str,
    triggered_by: str,
    learned_at: str,
    run_id: str,
    input_hash: str,
) -> dict[str, Any]:
    value_json = json.dumps(value, sort_keys=True, separators=(",", ":"))
    return {
        "entity_id": entity_id,
        "fact_key": fact_key,
        "value_type": value["type"].lower(),
        "value_json": value_json,
        "confidence": confidence,
        "source_type": source_type,
        "source_url": source_url,
        "model": model,
        "skill_id": skill_id,
        "triggered_by": triggered_by,
        "learned_at": learned_at,
        "run_id": run_id,
        "input_hash": f"sha256:{input_hash}",
    }


def evidence_hash_for_brief(brief: EvidenceBrief) -> str:
    payload = {
        "entity_id": brief.entity_id,
        "property_name": brief.property_name,
        "summary_scope": brief.summary_scope,
        "summary_kind": brief.summary_kind,
        "evidence": brief.evidence,
        "expected_terms": brief.expected_terms,
        "forbidden_terms": brief.forbidden_terms,
    }
    encoded = json.dumps(payload, sort_keys=True, separators=(",", ":")).encode("utf-8")
    return hashlib.sha256(encoded).hexdigest()


def entity_id_from_name(name: str) -> str:
    slug = "-".join(
        part
        for part in "".join(ch.lower() if ch.isalnum() else " " for ch in name).split()
        if part
    )
    return f"society:{slug}"


def elapsed_ms(started: float) -> int:
    return int((time.perf_counter() - started) * 1000)


if __name__ == "__main__":
    main()
