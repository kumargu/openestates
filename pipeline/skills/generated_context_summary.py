from __future__ import annotations

import argparse
import hashlib
import json
import os
import sys
import time
import urllib.error
import urllib.request
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Protocol


PROJECT_ROOT = Path(__file__).resolve().parents[2]
DEFAULT_MAX_TOKENS = 180
DEFAULT_TEMPERATURE = 0.0
MAX_SUMMARY_ATTEMPTS = 2
DEFAULT_STYLE_PROFILE = PROJECT_ROOT / "app" / "config" / "dag" / "summary_styles.json"
DEFAULT_PROVIDER_CONFIG = PROJECT_ROOT / "app" / "config" / "dag" / "summary_provider.json"
APPROACH_ROAD_VISUALS_PATH = PROJECT_ROOT / "data" / "validation" / "approach_road_visuals.json"
LOCAL_CONTEXT_PATH = PROJECT_ROOT / "data" / "validation" / "society_local_context.json"
REDDIT_POC_SIGNALS_PATH = PROJECT_ROOT / "data" / "validation" / "reddit_poc_society_signals.json"
SEED_PROPERTIES_PATH = PROJECT_ROOT / "data" / "seed" / "properties.json"
SUMMARY_FACT_KEY = "generated_context_summary"
SUMMARY_METADATA_FACT_KEY = "generated_context_summary_metadata"
SUMMARY_SKILL_ID = "generated_context_summary_local"
SUMMARY_SOURCE = "local_summary"


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
            raise ValueError(f"summary model path does not exist: {model_path}")
        load_started = time.perf_counter()
        try:
            from llama_cpp import Llama
        except ImportError as error:
            raise ValueError(
                "llama-cpp-python is not installed; install it outside the backend first"
            ) from error
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
        except urllib.error.URLError as error:
            raise ValueError(f"summary endpoint request failed: {error}") from error
        generation_ms = elapsed_ms(started)
        summary = body["choices"][0]["message"]["content"].strip()
        return SummaryResult(self.name, self.model, summary, 0, generation_ms)

    def _headers(self) -> dict[str, str]:
        headers = {"Content-Type": "application/json"}
        if self._api_key:
            headers["Authorization"] = f"Bearer {self._api_key}"
        return headers


def collect_generated_context_summaries(
    snapshot_date: str,
    provider: SummaryProvider | None = None,
    source_entities: list[dict[str, Any]] | None = None,
) -> dict[str, Any]:
    briefs = build_location_image_evidence_briefs(source_entities)
    if not briefs:
        raise ValueError("generated context summaries found no eligible evidence briefs")
    summary_provider = provider or build_provider_from_env()
    style = load_style_profile(DEFAULT_STYLE_PROFILE, "buyer_human_v1")
    reports = []
    for brief in briefs:
        result, evaluation = generate_checked_summary(summary_provider, brief, style)
        reports.append({"brief": brief, "result": result, "evaluation": evaluation})
    payload = build_skill_facts_payload(reports, snapshot_date=snapshot_date, style_id="buyer_human_v1")
    validate_generated_context_summaries(payload)
    return payload


def build_provider_from_env() -> SummaryProvider:
    model_path = os.environ.get("OPENESTATES_SUMMARY_GGUF")
    if model_path:
        return LlamaCppProvider(Path(model_path))

    base_url = os.environ.get("OPENESTATES_SUMMARY_BASE_URL")
    model = os.environ.get("OPENESTATES_SUMMARY_MODEL")
    if base_url and model:
        return OpenAICompatibleProvider(
            base_url,
            model,
            os.environ.get("OPENESTATES_SUMMARY_API_KEY"),
        )
    config = load_provider_config(DEFAULT_PROVIDER_CONFIG)
    detected = detect_openai_compatible_provider(config, base_url, model)
    if detected:
        return detected

    raise ValueError(
        "generated context summaries require a real local summary provider. "
        "Set OPENESTATES_SUMMARY_GGUF, set OPENESTATES_SUMMARY_BASE_URL plus "
        "OPENESTATES_SUMMARY_MODEL, or start one of the endpoints in "
        "app/config/dag/summary_provider.json."
    )


def load_provider_config(path: Path) -> dict[str, Any]:
    return read_json(path, {})


def detect_openai_compatible_provider(
    config: dict[str, Any],
    explicit_base_url: str | None,
    explicit_model: str | None,
) -> SummaryProvider | None:
    if explicit_base_url and explicit_model:
        return OpenAICompatibleProvider(
            explicit_base_url,
            explicit_model,
            os.environ.get("OPENESTATES_SUMMARY_API_KEY"),
        )
    if not explicit_base_url and (
        not config.get("auto_detect", True)
        or os.environ.get("OPENESTATES_SUMMARY_AUTODETECT") == "0"
    ):
        return None

    timeout = float(config.get("probe_timeout_seconds") or 0.6)
    preferred_models = [str(model) for model in config.get("preferred_model_order", [])]
    endpoints = configured_summary_endpoints(config, explicit_base_url)
    for endpoint in endpoints:
        models = discover_endpoint_models(endpoint, timeout)
        selected_model = select_summary_model(models, preferred_models, explicit_model)
        if selected_model:
            return OpenAICompatibleProvider(
                str(endpoint["base_url"]),
                selected_model,
                os.environ.get("OPENESTATES_SUMMARY_API_KEY"),
            )
    return None


def configured_summary_endpoints(
    config: dict[str, Any],
    explicit_base_url: str | None,
) -> list[dict[str, Any]]:
    if explicit_base_url:
        return [
            {
                "id": "explicit_openai_compatible",
                "base_url": explicit_base_url,
                "model_list_url": f"{explicit_base_url.rstrip('/')}/models",
                "model_list_shape": "openai_models",
            }
        ]
    return [
        endpoint
        for endpoint in config.get("openai_compatible_endpoints", [])
        if endpoint.get("base_url") and endpoint.get("model_list_url")
    ]


def discover_endpoint_models(endpoint: dict[str, Any], timeout: float) -> list[str]:
    try:
        payload = fetch_json(str(endpoint["model_list_url"]), timeout)
    except ValueError:
        return []
    shape = endpoint.get("model_list_shape")
    if shape == "ollama_tags":
        return [
            str(model.get("name"))
            for model in payload.get("models", [])
            if isinstance(model, dict) and model.get("name")
        ]
    if shape == "openai_models":
        return [
            str(model.get("id"))
            for model in payload.get("data", [])
            if isinstance(model, dict) and model.get("id")
        ]
    return []


def fetch_json(url: str, timeout: float) -> dict[str, Any]:
    headers = {}
    api_key = os.environ.get("OPENESTATES_SUMMARY_API_KEY")
    if api_key:
        headers["Authorization"] = f"Bearer {api_key}"
    request = urllib.request.Request(url, headers=headers, method="GET")
    try:
        with urllib.request.urlopen(request, timeout=timeout) as response:
            payload = json.loads(response.read().decode("utf-8"))
    except (urllib.error.URLError, TimeoutError, json.JSONDecodeError) as error:
        raise ValueError(f"summary provider probe failed for {url}: {error}") from error
    if not isinstance(payload, dict):
        raise ValueError(f"summary provider probe returned non-object JSON for {url}")
    return payload


def select_summary_model(
    available_models: list[str],
    preferred_models: list[str],
    explicit_model: str | None,
) -> str | None:
    if explicit_model:
        return explicit_model if explicit_model in available_models else None
    available = {model.lower(): model for model in available_models}
    for preferred in preferred_models:
        if preferred.lower() in available:
            return available[preferred.lower()]
    for model in available_models:
        lowered = model.lower()
        if "embed" not in lowered and "rerank" not in lowered:
            return model
    return None


def build_location_image_evidence_briefs(
    source_entities: list[dict[str, Any]] | None = None,
) -> list[EvidenceBrief]:
    visuals = read_json(APPROACH_ROAD_VISUALS_PATH, [])
    seed_by_entity = seed_project_basics_by_entity_id()
    local_context_by_entity = local_context_by_entity_id()
    reddit_fact_by_entity = reddit_fact_by_entity_id()
    requested_keys = requested_summary_keys(source_entities or [])
    briefs = []
    for visual in visuals:
        if visual.get("coverage_quality") != "usable" or not visual.get("frames"):
            continue
        entity_id = str(visual["entity_id"])
        title = str(visual.get("title") or title_from_entity_id(entity_id))
        if requested_keys and requested_keys.isdisjoint(visual_summary_keys(visual, entity_id, title)):
            continue
        seed = seed_by_entity.get(entity_id, {})
        evidence = [
            project_basics_evidence(title, seed),
            location_image_evidence(visual),
        ]
        local_context = local_context_by_entity.get(entity_id)
        if local_context:
            evidence.append(local_context_evidence(local_context))
            forbidden_terms = base_forbidden_terms()
        else:
            evidence.append(
                "Nearby graph context: this evidence brief does not list nearby schools, hospitals, malls, metro stations, or work hubs."
            )
            forbidden_terms = base_forbidden_terms() + [
                "hospital",
                "school",
                "mall",
                "shopping",
                "metro",
                "tech park",
                "office hub",
                "nearby anchors",
                "local amenities",
            ]
        reddit_fact = reddit_fact_by_entity.get(entity_id)
        if reddit_fact:
            evidence.append(review_caution_evidence(reddit_fact))
        expected = [title, "road"]
        if seed.get("area"):
            expected.append(str(seed["area"]))
        briefs.append(
            EvidenceBrief(
                entity_id=entity_id,
                property_name=title,
                summary_scope="society",
                summary_kind="livability_context",
                target_words=TargetWords(min=70, max=130),
                evidence=evidence,
                expected_terms=expected,
                forbidden_terms=forbidden_terms,
            )
        )
    return briefs


def base_forbidden_terms() -> list[str]:
    return [
        "guaranteed",
        "investment upside",
        "appreciation",
        "minutes away",
        "walking distance",
        "short commute",
        "close to",
        "friction-free",
        "confidence",
        "low-confidence",
        "fact_key",
        "json",
    ]


def requested_summary_keys(source_entities: list[dict[str, Any]]) -> set[str]:
    keys: set[str] = set()
    for seed in source_entities:
        for field in ("entity_id", "alias_entity_id", "project_key", "name"):
            value = seed.get(field)
            if isinstance(value, str):
                keys.update(summary_match_keys(value))
    return keys


def visual_summary_keys(visual: dict[str, Any], entity_id: str, title: str) -> set[str]:
    keys = set()
    for value in (entity_id, visual.get("slug"), title):
        if isinstance(value, str):
            keys.update(summary_match_keys(value))
    return keys


def summary_match_keys(value: str) -> set[str]:
    raw = value.strip()
    if not raw:
        return set()
    normalized = normalize_summary_key(raw)
    keys = {normalized} if normalized else set()
    slug = normalized.replace(" ", "-")
    if ":" in raw:
        prefix, suffix = raw.split(":", 1)
        keys.add(f"{normalize_summary_key(prefix)}:{normalize_summary_key(suffix)}")
        keys.add(normalize_summary_key(suffix))
        keys.add(normalize_summary_key(suffix).replace(" ", "-"))
    if slug:
        keys.update({slug, f"society:{slug}", f"soc-{slug}"})
    return {key for key in keys if key}


def normalize_summary_key(value: str) -> str:
    return " ".join(
        "".join(ch.lower() if ch.isalnum() else " " for ch in value).split()
    )


def project_basics_evidence(title: str, seed: dict[str, Any]) -> str:
    parts = [f"{title} appears in seed listings"]
    if seed.get("area"):
        parts.append(f"area {seed['area']}")
    if seed.get("builder_name"):
        parts.append(f"builder {seed['builder_name']}")
    if seed.get("possession_status"):
        parts.append(f"possession/status {seed['possession_status']}")
    bhks = seed.get("bhks") or []
    if bhks:
        parts.append(f"observed configurations include {', '.join(str(bhk) for bhk in bhks)} BHK")
    return "Project basics: " + "; ".join(parts) + "."


def location_image_evidence(visual: dict[str, Any]) -> str:
    frames = visual.get("frames") or []
    distances = sorted(
        int(frame["distance_from_gate_m"])
        for frame in frames
        if isinstance(frame.get("distance_from_gate_m"), (int, float))
    )
    dates = sorted(
        {
            str(frame["capture_date"])
            for frame in frames
            if frame.get("capture_date")
        }
    )
    roles = sorted({str(frame.get("view_role")) for frame in frames if frame.get("view_role")})
    distance_text = (
        f"about {distances[0]}-{distances[-1]}m from the gate"
        if distances
        else "near the gate"
    )
    date_text = f" captured {', '.join(dates)}" if dates else ""
    role_text = f" with {', '.join(roles).replace('_', ' ')} views" if roles else ""
    return (
        f"Location-image evidence: {len(frames)} Google Street View road-context frames "
        f"from {distance_text}{date_text}{role_text}; coverage quality is {visual.get('coverage_quality', 'available')}."
    )


def local_context_evidence(local_context: dict[str, Any]) -> str:
    names = [
        entity.get("name")
        for entity in local_context.get("entities", [])
        if entity.get("name")
    ]
    return "Nearby graph context: local graph links include {}.".format(", ".join(names))


def review_caution_evidence(fact: dict[str, Any]) -> str:
    fact_key = str(fact.get("fact_key", "review caution"))
    label = fact_key.replace(".", " ").replace("_", " ")
    value = str(fact.get("value") or "mentioned")
    return f"Caution/review evidence: {label} {value}."


def seed_project_basics_by_entity_id() -> dict[str, dict[str, Any]]:
    properties = read_json(SEED_PROPERTIES_PATH, [])
    by_entity: dict[str, dict[str, Any]] = {}
    for prop in properties:
        society_id = prop.get("society_id")
        if not society_id:
            continue
        entity_id = society_entity_id_from_seed_id(str(society_id))
        basics = by_entity.setdefault(
            entity_id,
            {
                "area": prop.get("area"),
                "builder_name": prop.get("builder_name"),
                "possession_status": prop.get("possession_status"),
                "bhks": set(),
            },
        )
        if prop.get("bhk"):
            basics["bhks"].add(prop["bhk"])
    for basics in by_entity.values():
        basics["bhks"] = sorted(basics["bhks"])
    return by_entity


def society_entity_id_from_seed_id(seed_id: str) -> str:
    if seed_id.startswith("soc-"):
        return "society:" + seed_id.removeprefix("soc-")
    if seed_id.startswith("society:"):
        return seed_id
    return "society:" + seed_id


def local_context_by_entity_id() -> dict[str, dict[str, Any]]:
    payload = read_json(LOCAL_CONTEXT_PATH, {})
    return {
        graph["society_id"]: graph
        for graph in payload.get("graphs", [])
        if graph.get("society_id")
    }


def reddit_fact_by_entity_id() -> dict[str, dict[str, Any]]:
    payload = read_json(REDDIT_POC_SIGNALS_PATH, {})
    facts: dict[str, dict[str, Any]] = {}
    for fact in payload.get("facts", []):
        entity_id = fact.get("entity_id")
        if entity_id and entity_id not in facts:
            facts[str(entity_id)] = fact
    return facts


def load_style_profile(path: Path, style_id: str) -> dict[str, Any]:
    payload = read_json(path, {})
    for profile in payload.get("profiles", []):
        if profile.get("id") == style_id:
            return profile
    raise ValueError(f"style profile {style_id!r} not found in {path}")


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
        "- Include the property name and area when the area appears in the evidence.\n"
        "- Include the main buyer fit and the main caution.\n"
        "- If a category is absent from the evidence, do not mention that absence; stay with the evidence that exists.\n"
        "- Voice examples are style examples only; do not copy their place names, cautions, or claims.\n"
        "- Return only the final paragraph; do not add labels, headings, or introductions.\n"
        "- Use road images as road-context evidence only; do not turn them into proof of traffic speed, flooding, noise, or commute time.\n"
        "- Do not say walking distance, short commute, close to, or minutes away unless the evidence gives that exact relationship.\n"
        "- Do not invent distances, prices, ratings, guarantees, or appreciation claims.\n"
        "- Do not mention raw fact keys, slugs, JSON, confidence scores, or internal graph terms.\n\n"
        f"{style_block}\n"
        f"Property: {brief.property_name}\n"
        f"Evidence:\n{evidence_lines}\n\n"
        "Summary:"
    )


def generate_checked_summary(
    provider: SummaryProvider,
    brief: EvidenceBrief,
    style: dict[str, Any],
) -> tuple[SummaryResult, dict[str, Any]]:
    result = provider.generate(
        build_prompt(brief, style),
        DEFAULT_MAX_TOKENS,
        DEFAULT_TEMPERATURE,
    )
    evaluation = evaluate_summary(brief, result.summary)
    attempts = 1
    while attempts < MAX_SUMMARY_ATTEMPTS and not summary_quality_passed(evaluation):
        result = provider.generate(
            build_repair_prompt(brief, result.summary, evaluation),
            DEFAULT_MAX_TOKENS,
            DEFAULT_TEMPERATURE,
        )
        evaluation = evaluate_summary(brief, result.summary)
        attempts += 1
    evaluation["attempts"] = attempts
    return result, evaluation


def build_repair_prompt(
    brief: EvidenceBrief,
    draft: str,
    evaluation: dict[str, Any],
) -> str:
    evidence_lines = "\n".join(f"- {item}" for item in brief.evidence)
    missing = ", ".join(evaluation.get("expected_terms_missing") or []) or "none"
    forbidden = ", ".join(evaluation.get("forbidden_terms_found") or []) or "none"
    return (
        "Rewrite the property context summary so it passes the quality checks.\n"
        "Use only the evidence below. Return only one paragraph, no heading.\n"
        f"Target length: {brief.target_words.min}-{brief.target_words.max} words.\n"
        f"Required terms to include if natural: {', '.join(brief.expected_terms)}.\n"
        f"Missing terms in the draft: {missing}.\n"
        f"Forbidden terms found in the draft: {forbidden}.\n"
        "Do not mention any forbidden term. Do not invent nearby places, commute speed, flooding, noise, or distance.\n\n"
        f"Evidence:\n{evidence_lines}\n\n"
        f"Failed draft:\n{draft}\n\n"
        "Rewritten summary:"
    )


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
    snapshot_date: str,
    style_id: str,
) -> dict[str, Any]:
    facts = []
    annotations = []
    learned_at = datetime.now(timezone.utc).isoformat()
    for report in reports:
        brief: EvidenceBrief = report["brief"]
        result: SummaryResult = report["result"]
        evaluation: dict[str, Any] = report["evaluation"]
        quality_status = "passed" if summary_quality_passed(evaluation) else "failed"
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
                SUMMARY_METADATA_FACT_KEY,
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
        annotations.extend(fact_annotations(brief.entity_id))
    return {
        "source": SUMMARY_SOURCE,
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


def fact_annotations(entity_id: str) -> list[dict[str, Any]]:
    return [
        {
            "entity_id": entity_id,
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
        },
        {
            "entity_id": entity_id,
            "fact_key": SUMMARY_METADATA_FACT_KEY,
            "display_template": None,
            "answers_preferences_json": "[]",
            "scoring_direction": None,
            "scoring_weight": None,
            "scoring_thresholds_json": "[]",
        },
    ]


def validate_generated_context_summaries(payload: dict[str, Any]) -> None:
    rejected_providers = {"mock"}
    facts = payload.get("facts", [])
    if not isinstance(facts, list):
        raise ValueError("generated context summaries facts must be a list")
    summary_entities = set()
    metadata_by_entity = {}
    for fact in facts:
        if not isinstance(fact, dict):
            raise ValueError("generated context summaries facts must be objects")
        entity_id = fact.get("entity_id")
        if fact.get("fact_key") == SUMMARY_FACT_KEY:
            if isinstance(entity_id, str) and entity_id:
                summary_entities.add(entity_id)
            continue
        if fact.get("fact_key") != SUMMARY_METADATA_FACT_KEY:
            continue
        if not isinstance(entity_id, str) or not entity_id:
            raise ValueError("generated context summary metadata must have entity_id")
        metadata = decode_metadata(fact)
        provider = str(metadata.get("provider", "")).strip().lower()
        if not provider:
            raise ValueError("generated context summaries require a provider in metadata")
        if provider in rejected_providers:
            raise ValueError(
                f"generated context summaries cannot be collected from provider {provider!r}"
            )
        if str(metadata.get("quality_status", "")).strip().lower() != "passed":
            raise ValueError("generated context summaries require passed metadata quality")
        metadata_by_entity[entity_id] = metadata
    missing_metadata = sorted(summary_entities.difference(metadata_by_entity))
    if missing_metadata:
        raise ValueError(
            "generated context summaries missing metadata for: {}".format(
                ", ".join(missing_metadata)
            )
        )


def decode_metadata(fact: dict[str, Any]) -> dict[str, Any]:
    value_json = fact.get("value_json")
    if not isinstance(value_json, str):
        raise ValueError("generated context summary metadata must have value_json")
    try:
        value = json.loads(value_json)
        metadata = json.loads(value["data"])
    except (KeyError, TypeError, json.JSONDecodeError) as error:
        raise ValueError("generated context summary metadata is invalid JSON") from error
    if not isinstance(metadata, dict):
        raise ValueError("generated context summary metadata must be an object")
    return metadata


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
    return {
        "entity_id": entity_id,
        "fact_key": fact_key,
        "value_type": value["type"].lower(),
        "value_json": json.dumps(value, sort_keys=True, separators=(",", ":")),
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


def read_json(path: Path, default: Any) -> Any:
    if not path.exists():
        return default
    return json.loads(path.read_text(encoding="utf-8"))


def title_from_entity_id(entity_id: str) -> str:
    slug = entity_id.split(":", 1)[-1]
    return " ".join(part.capitalize() for part in slug.split("-"))


def elapsed_ms(started: float) -> int:
    return int((time.perf_counter() - started) * 1000)


def preflight_summary_provider() -> dict[str, Any]:
    provider = build_provider_from_env()
    result = provider.generate(
        "Write one short sentence saying the summary provider is ready.",
        max_tokens=32,
        temperature=0.0,
    )
    return {
        "ok": True,
        "provider": result.provider,
        "model": result.model,
        "load_ms": result.load_ms,
        "generation_ms": result.generation_ms,
        "sample": result.summary,
    }


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Generated context summary provider utilities."
    )
    parser.add_argument(
        "--preflight",
        action="store_true",
        help="Probe and call the configured or auto-detected summary provider.",
    )
    parser.add_argument(
        "--list-briefs",
        action="store_true",
        help="Print eligible evidence briefs without calling a model.",
    )
    parser.add_argument(
        "--source-entity",
        action="append",
        default=[],
        help="Limit --list-briefs to one entity id, alias, project key, slug, or name.",
    )
    args = parser.parse_args()

    if args.preflight:
        try:
            print(json.dumps(preflight_summary_provider(), indent=2, sort_keys=True))
            return 0
        except Exception as error:
            print(
                json.dumps(
                    {
                        "ok": False,
                        "error": f"{type(error).__name__}: {error}",
                        "config": str(DEFAULT_PROVIDER_CONFIG),
                    },
                    indent=2,
                    sort_keys=True,
                ),
                file=sys.stderr,
            )
            return 1

    if args.list_briefs:
        source_entities = [
            {"entity_id": entity_id, "name": entity_id}
            for entity_id in args.source_entity
        ]
        briefs = build_location_image_evidence_briefs(source_entities)
        print(
            json.dumps(
                [
                    {
                        "entity_id": brief.entity_id,
                        "property_name": brief.property_name,
                        "evidence_count": len(brief.evidence),
                        "expected_terms": brief.expected_terms,
                    }
                    for brief in briefs
                ],
                indent=2,
                sort_keys=True,
            )
        )
        return 0

    parser.print_help()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
