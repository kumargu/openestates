"""Build privacy-safe, promotion-gated RERA regulatory source records.

OCR and model calls are offline. Extractors receive redacted public text only;
the request path consumes promoted Parquet records and never invokes this code.
"""

from __future__ import annotations

import hashlib
import json
import re
import subprocess
from copy import deepcopy
from datetime import datetime, timezone
from functools import lru_cache
from html import unescape
from pathlib import Path
from typing import Any, Literal, Mapping, Optional, Protocol, Sequence
from urllib.parse import urljoin

from pydantic import BaseModel, ConfigDict, Field, field_validator


REPO_ROOT = Path(__file__).resolve().parents[2]
DEFAULT_POLICY_PATH = (
    REPO_ROOT
    / "app"
    / "config"
    / "dag"
    / "source_adapters"
    / "rera_regulatory_intelligence.json"
)
DEFAULT_OCR_CACHE = REPO_ROOT / "data" / "cache" / "skills" / "rera_regulatory_ocr"


class RegulatoryIntelligenceError(ValueError):
    """Raised when a regulatory record cannot be represented safely."""


class DocumentScope(BaseModel):
    model_config = ConfigDict(extra="forbid")

    registration_number: Optional[str] = None
    proceeding_ref: Optional[str] = None
    project_match: bool = False
    promoter_match: bool = False
    location_match: bool = False

    def resolution_method(self) -> Optional[str]:
        if self.registration_number and self.registration_number.strip():
            return "exact_registration"
        if self.proceeding_ref and self.proceeding_ref.strip():
            return "official_proceeding_relationship"
        if self.project_match and self.promoter_match and self.location_match:
            return "unregistered_triplet_exact"
        return None


class EventDraft(BaseModel):
    model_config = ConfigDict(extra="forbid")

    event_id: str
    promoter_id: Optional[str] = None
    event_class: str
    event_type: str
    occurred_at: str
    issuer: str
    proceeding_ref: Optional[str] = None
    decision_stage: str
    disposition: Optional[str] = None
    current_effect: str
    affected_scope: Optional[str] = None
    assertion_mode: Literal[
        "registry_record",
        "promoter_declaration",
        "complainant_allegation",
        "authority_order",
    ]
    source_trust: Literal["primary_authority", "promoter_filing", "party_filing"]
    supporting_quote: Optional[str] = None
    page: Optional[int] = Field(default=None, ge=1)
    extraction_confidence: float = Field(default=0.0, ge=0.0, le=1.0)

    @field_validator(
        "event_id",
        "event_class",
        "event_type",
        "occurred_at",
        "issuer",
        "decision_stage",
        "current_effect",
    )
    @classmethod
    def non_blank(cls, value: str) -> str:
        if not value.strip():
            raise ValueError("regulatory event fields must be non-blank")
        return value.strip()


class RelationshipDraft(BaseModel):
    model_config = ConfigDict(extra="forbid")

    relationship_id: str
    relationship_type: str
    from_event_id: str
    to_event_id: str
    occurred_at: str
    issuer: str
    effect_text: Optional[str] = None
    supporting_quote: Optional[str] = None
    page: Optional[int] = Field(default=None, ge=1)
    assertion_mode: Literal["registry_record", "authority_order"] = "authority_order"
    source_trust: Literal["primary_authority", "party_filing"] = "primary_authority"
    extraction_confidence: float = Field(default=0.0, ge=0.0, le=1.0)


class RedactedDocument(BaseModel):
    model_config = ConfigDict(extra="forbid")

    content_sha256: str
    source_url: str
    issuer: str
    document_format: Literal["pdf", "structured_list"]
    pages: list[str]


class StructuredListEventCandidate(BaseModel):
    """Exact-registration event parsed from one configured official list."""

    model_config = ConfigDict(extra="forbid")

    list_id: str
    document_url: str
    event: EventDraft


class ExtractionProvider(Protocol):
    """Provider-neutral extractor; input text is already redacted."""

    def extract_events(
        self,
        document: RedactedDocument,
        scope: DocumentScope,
    ) -> Sequence[EventDraft]: ...

    def extract_relationships(
        self,
        document: RedactedDocument,
        scope: DocumentScope,
    ) -> Sequence[RelationshipDraft]: ...


class VerificationProvider(Protocol):
    """Independent verifier using the same redacted document."""

    def verify_events(
        self,
        document: RedactedDocument,
        scope: DocumentScope,
        candidates: Sequence[EventDraft],
    ) -> Sequence[EventDraft]: ...

    def verify_relationships(
        self,
        document: RedactedDocument,
        scope: DocumentScope,
        candidates: Sequence[RelationshipDraft],
    ) -> Sequence[RelationshipDraft]: ...


@lru_cache(maxsize=4)
def load_policy(path: str = str(DEFAULT_POLICY_PATH)) -> dict[str, Any]:
    with Path(path).open("r", encoding="utf-8") as handle:
        policy = json.load(handle)
    if not isinstance(policy, dict) or not policy.get("covered_sources"):
        raise RegulatoryIntelligenceError("regulatory policy must define covered_sources")
    return policy


def _sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def ocr_pdf_locally(
    pdf_path: Path,
    *,
    cache_root: Path = DEFAULT_OCR_CACHE,
) -> list[str]:
    """Run local pdftotext once per content hash and return page text."""

    body = pdf_path.read_bytes()
    if not body:
        raise RegulatoryIntelligenceError(f"empty regulatory PDF: {pdf_path}")
    digest = _sha256(body)
    cache_path = cache_root / digest[:2] / f"{digest}.txt"
    if cache_path.exists():
        text = cache_path.read_text(encoding="utf-8")
    else:
        result = subprocess.run(
            ["pdftotext", "-layout", str(pdf_path), "-"],
            check=False,
            capture_output=True,
            text=True,
            timeout=180,
        )
        if result.returncode != 0 or not result.stdout.strip():
            raise RegulatoryIntelligenceError(
                f"local OCR failed for {pdf_path}: {result.stderr.strip()}"
            )
        text = result.stdout
        cache_path.parent.mkdir(parents=True, exist_ok=True)
        cache_path.write_text(text, encoding="utf-8")
    return [page.strip() for page in text.split("\f") if page.strip()]


_PII_PATTERNS = (
    re.compile(r"\b[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}\b", re.IGNORECASE),
    re.compile(r"(?<!\d)(?:\+?91[-\s]?)?[6-9]\d{9}(?!\d)"),
    re.compile(r"(?<!\d)\d{4}[\s-]?\d{4}[\s-]?\d{4}(?!\d)"),
    re.compile(r"\b[A-Z]{5}\d{4}[A-Z]\b", re.IGNORECASE),
    re.compile(r"\b(?:account|a/c)\s*(?:no\.?|number)?\s*[:#-]?\s*[A-Z0-9-]{6,}\b", re.IGNORECASE),
)


def redact_public_text(
    text: str,
    *,
    natural_person_names: Sequence[str] = (),
    replacement: str = "[redacted]",
) -> str:
    """Redact contacts, identifiers, signatures, addresses, and known people."""

    redacted = text
    for pattern in _PII_PATTERNS:
        redacted = pattern.sub(replacement, redacted)
    for name in sorted(natural_person_names, key=len, reverse=True):
        if name.strip():
            redacted = re.sub(re.escape(name.strip()), replacement, redacted, flags=re.IGNORECASE)
    redacted = re.sub(
        r"(?im)^\s*(?:residential\s+)?address\s*:\s*.*$",
        f"Address: {replacement}",
        redacted,
    )
    redacted = re.sub(
        r"(?im)^\s*(?:signed|signature)\s*(?:by)?\s*:?.*$",
        f"Signature: {replacement}",
        redacted,
    )
    return redacted


def privacy_is_valid(text: str, natural_person_names: Sequence[str] = ()) -> bool:
    if any(pattern.search(text) for pattern in _PII_PATTERNS):
        return False
    lowered = text.casefold()
    return not any(name.strip().casefold() in lowered for name in natural_person_names if name.strip())


def redacted_document(
    *,
    source_url: str,
    issuer: str,
    pages: Sequence[str],
    document_format: Literal["pdf", "structured_list"],
    natural_person_names: Sequence[str] = (),
) -> RedactedDocument:
    redacted_pages = [
        redact_public_text(page, natural_person_names=natural_person_names) for page in pages
    ]
    content = "\f".join(redacted_pages).encode("utf-8")
    return RedactedDocument(
        content_sha256=_sha256(content),
        source_url=source_url,
        issuer=issuer,
        document_format=document_format,
        pages=redacted_pages,
    )


def _clean_html_fragment(value: str) -> str:
    return re.sub(
        r"\s+",
        " ",
        unescape(re.sub(r"<[^>]+>", " ", value or "")),
    ).strip()


def _normalized_column(value: str) -> str:
    return re.sub(r"[^a-z0-9]+", " ", value.casefold()).strip()


def _normalized_registration(value: str) -> str:
    return " ".join(value.strip().upper().split())


def _table_rows_with_links(html: str, table_id: str) -> list[list[tuple[str, Optional[str]]]]:
    table_match = re.search(
        r"<table\b(?=[^>]*\bid\s*=\s*['\"]{}['\"])[^>]*>(.*?)</table>".format(
            re.escape(table_id)
        ),
        html,
        re.IGNORECASE | re.DOTALL,
    )
    if table_match is None:
        raise RegulatoryIntelligenceError(
            f"configured regulatory table {table_id!r} was not found"
        )
    rows: list[list[tuple[str, Optional[str]]]] = []
    for row_html in re.findall(
        r"<tr\b[^>]*>(.*?)</tr>",
        table_match.group(1),
        re.IGNORECASE | re.DOTALL,
    ):
        cells: list[tuple[str, Optional[str]]] = []
        for cell_html in re.findall(
            r"<t[hd]\b[^>]*>(.*?)</t[hd]>",
            row_html,
            re.IGNORECASE | re.DOTALL,
        ):
            link_match = re.search(
                r"<a\b[^>]*\bhref\s*=\s*['\"]([^'\"]+)['\"]",
                cell_html,
                re.IGNORECASE | re.DOTALL,
            )
            cells.append(
                (
                    _clean_html_fragment(cell_html),
                    unescape(link_match.group(1)).strip() if link_match else None,
                )
            )
        if cells:
            rows.append(cells)
    return rows


def structured_list_event_candidates(
    *,
    registration_number: str,
    issuer: str,
    base_url: str,
    list_config: Mapping[str, Any],
    html: str,
) -> list[StructuredListEventCandidate]:
    """Parse only rows whose official registration value is an exact match."""

    parser = list_config.get("exact_registration_table")
    if not isinstance(parser, Mapping):
        return []
    table_id = str(parser.get("table_id") or "").strip()
    if not table_id:
        raise RegulatoryIntelligenceError("exact-registration table requires table_id")
    rows = _table_rows_with_links(html, table_id)
    if len(rows) < 2:
        raise RegulatoryIntelligenceError(f"regulatory table {table_id!r} has no data rows")

    headers = [_normalized_column(cell[0]) for cell in rows[0]]

    def column(label_key: str) -> int:
        label = _normalized_column(str(parser.get(label_key) or ""))
        if not label or label not in headers:
            raise RegulatoryIntelligenceError(
                f"regulatory table {table_id!r} is missing configured {label_key}"
            )
        return headers.index(label)

    registration_index = column("registration_header")
    event_type_index = column("event_type_header")
    occurred_at_index = column("occurred_at_header")
    document_index = column("document_header")
    mappings = {
        str(item.get("source_value") or "").strip().upper(): item
        for item in parser.get("event_mappings") or []
        if isinstance(item, Mapping) and str(item.get("source_value") or "").strip()
    }
    if not mappings:
        raise RegulatoryIntelligenceError(
            f"regulatory table {table_id!r} requires event_mappings"
        )

    normalized_registration = _normalized_registration(registration_number)
    date_format = str(parser.get("date_format") or "%d-%m-%Y")
    decision_stage = str(parser.get("decision_stage") or "registry_order").strip()
    assertion_mode = str(parser.get("assertion_mode") or "registry_record").strip()
    list_id = str(list_config.get("id") or table_id).strip()
    candidates: list[StructuredListEventCandidate] = []
    for row in rows[1:]:
        if len(row) != len(headers):
            continue
        row_registration = _normalized_registration(row[registration_index][0])
        if row_registration != normalized_registration:
            continue
        source_value = row[event_type_index][0].strip().upper()
        mapping = mappings.get(source_value)
        if mapping is None:
            raise RegulatoryIntelligenceError(
                f"unmapped regulatory event category {source_value!r} for {registration_number}"
            )
        try:
            occurred_at = datetime.strptime(
                row[occurred_at_index][0].strip(), date_format
            ).date().isoformat()
        except ValueError as error:
            raise RegulatoryIntelligenceError(
                f"invalid regulatory event date for {registration_number}: "
                f"{row[occurred_at_index][0]!r}"
            ) from error
        document_href = row[document_index][1]
        if not document_href:
            raise RegulatoryIntelligenceError(
                f"regulatory event lacks an official document link for {registration_number}"
            )
        document_url = urljoin(base_url.rstrip("/") + "/", document_href)
        event_material = "\n".join(
            (issuer, list_id, normalized_registration, source_value, occurred_at, document_url)
        )
        event_id = "regulatory_event:sha256:{}".format(
            _sha256(event_material.encode("utf-8"))
        )
        candidates.append(
            StructuredListEventCandidate(
                list_id=list_id,
                document_url=document_url,
                event=EventDraft(
                    event_id=event_id,
                    event_class=str(mapping.get("event_class") or "").strip(),
                    event_type=str(mapping.get("event_type") or "").strip(),
                    occurred_at=occurred_at,
                    issuer=issuer,
                    decision_stage=decision_stage,
                    disposition=(
                        str(mapping.get("disposition")).strip()
                        if mapping.get("disposition") is not None
                        else None
                    ),
                    current_effect=str(mapping.get("current_effect") or "").strip(),
                    affected_scope="registered project",
                    assertion_mode=assertion_mode,
                    source_trust="primary_authority",
                    extraction_confidence=1.0,
                ),
            )
        )
    return candidates


def _canonical(model: BaseModel, *, exclude: set[str]) -> str:
    return json.dumps(
        model.model_dump(exclude=exclude, exclude_none=True),
        ensure_ascii=False,
        sort_keys=True,
        separators=(",", ":"),
    )


def _agreement_by_id(
    extracted: Sequence[BaseModel],
    verified: Sequence[BaseModel],
    *,
    id_field: str,
    excluded_fields: set[str],
) -> dict[str, bool]:
    verified_by_id = {str(getattr(item, id_field)): item for item in verified}
    return {
        str(getattr(item, id_field)): (
            str(getattr(item, id_field)) in verified_by_id
            and _canonical(item, exclude=excluded_fields)
            == _canonical(verified_by_id[str(getattr(item, id_field))], exclude=excluded_fields)
        )
        for item in extracted
    }


def _official_source(document: RedactedDocument, policy: Mapping[str, Any]) -> bool:
    return any(
        document.issuer == source.get("id")
        and any(host in document.source_url for host in source.get("authority_hosts", []))
        for source in policy.get("covered_sources", [])
    )


def _structured_fields_valid(event: EventDraft) -> bool:
    try:
        datetime.fromisoformat(event.occurred_at.replace("Z", "+00:00"))
    except ValueError:
        return False
    if event.proceeding_ref is not None and not event.proceeding_ref.strip():
        return False
    return True


def resolve_current_effects(
    events: Sequence[EventDraft],
    relationships: Sequence[RelationshipDraft],
) -> list[EventDraft]:
    """Apply official relationships; an appeal filing alone never implies a stay."""

    by_id = {event.event_id: event.model_copy(deep=True) for event in events}
    effect_relationships = {"stays", "modifies", "sets_aside", "supersedes"}
    for relation in sorted(relationships, key=lambda item: (item.occurred_at, item.relationship_id)):
        if relation.relationship_type not in effect_relationships:
            continue
        target = by_id.get(relation.to_event_id)
        if target is None or not relation.effect_text or not relation.effect_text.strip():
            continue
        target.current_effect = relation.effect_text.strip()
    return list(by_id.values())


def build_regulatory_source_records(
    *,
    registration_number: str,
    receipt_id: str,
    capture_id: str,
    observed_at: str,
    document: RedactedDocument,
    scope: DocumentScope,
    extracted_events: Sequence[EventDraft],
    verified_events: Sequence[EventDraft],
    extracted_relationships: Sequence[RelationshipDraft] = (),
    verified_relationships: Sequence[RelationshipDraft] = (),
    parser_version: str = "rera_regulatory_intelligence.v1",
    policy: Optional[Mapping[str, Any]] = None,
    include_coverage: bool = True,
) -> list[dict[str, Any]]:
    """Return deterministic L1 records; failed gates remain quarantinable."""

    active_policy = policy or load_policy()
    resolution_method = scope.resolution_method()
    event_agreement = _agreement_by_id(
        extracted_events,
        verified_events,
        id_field="event_id",
        excluded_fields={"extraction_confidence"},
    )
    relationship_agreement = _agreement_by_id(
        extracted_relationships,
        verified_relationships,
        id_field="relationship_id",
        excluded_fields={"extraction_confidence"},
    )
    resolved_events = resolve_current_effects(extracted_events, extracted_relationships)
    official = _official_source(document, active_policy)
    privacy_valid = all(privacy_is_valid(page) for page in document.pages)
    records: list[dict[str, Any]] = []

    def base(kind: str, locator: str, label: str, raw_value: Mapping[str, Any]) -> dict[str, Any]:
        return {
            "kind": kind,
            "registration_number": registration_number,
            "receipt_id": receipt_id,
            "capture_id": capture_id,
            "source_locator": locator,
            "raw_label": label,
            "raw_value": json.dumps(raw_value, ensure_ascii=False, sort_keys=True, separators=(",", ":")),
            "observed_at": observed_at,
            "parser_version": parser_version,
        }

    for event in sorted(resolved_events, key=lambda item: (item.occurred_at, item.event_id)):
        payload = event.model_dump(exclude_none=True)
        agreement = document.document_format == "structured_list" or event_agreement.get(
            event.event_id, False
        )
        payload["promotion"] = {
            "official_source": official,
            "issuer_verified": event.issuer == document.issuer,
            "stage_verified": bool(event.decision_stage.strip()),
            "scope_resolution": resolution_method or "unresolved",
            "extractor_verifier_agreement": agreement,
            "structured_fields_valid": _structured_fields_valid(event),
            "privacy_validated": privacy_valid,
            "unresolved_contradiction": not agreement,
            "document_format": document.document_format,
        }
        records.append(
            base(
                "regulatory_event",
                f"page:{event.page}" if event.page else "structured-list",
                "Regulatory event",
                payload,
            )
        )

    allowed_relationships = set(active_policy.get("relationship_types", []))
    for relation in sorted(
        extracted_relationships,
        key=lambda item: (item.occurred_at, item.relationship_id),
    ):
        payload = relation.model_dump(exclude={"occurred_at", "issuer", "effect_text"}, exclude_none=True)
        agreement = document.document_format == "structured_list" or relationship_agreement.get(
            relation.relationship_id, False
        )
        payload["promotion"] = {
            "official_source": official,
            "issuer_verified": relation.issuer == document.issuer,
            "stage_verified": relation.relationship_type in allowed_relationships,
            "scope_resolution": resolution_method or "unresolved",
            "extractor_verifier_agreement": agreement,
            "structured_fields_valid": relation.relationship_type in allowed_relationships,
            "privacy_validated": privacy_valid,
            "unresolved_contradiction": not agreement,
            "document_format": document.document_format,
        }
        row = base(
            "regulatory_relationship",
            f"page:{relation.page}" if relation.page else "structured-list",
            "Regulatory relationship",
            payload,
        )
        row["effective_at"] = relation.occurred_at
        records.append(row)

    if include_coverage:
        records.append(
            base(
                "regulatory_coverage",
                f"coverage/{document.issuer.lower()}",
                "Regulatory coverage",
                {
                    "source": document.issuer,
                    "checked_at": observed_at,
                    "status": "checked" if official else "unverified_source",
                },
            )
        )
    return records


def extract_and_verify(
    *,
    document: RedactedDocument,
    scope: DocumentScope,
    extractor: ExtractionProvider,
    verifier: VerificationProvider,
) -> tuple[list[EventDraft], list[EventDraft], list[RelationshipDraft], list[RelationshipDraft]]:
    """Run independent provider passes over redacted text only."""

    extracted_events = list(extractor.extract_events(document, scope))
    extracted_relationships = list(extractor.extract_relationships(document, scope))
    verified_events = list(verifier.verify_events(document, scope, deepcopy(extracted_events)))
    verified_relationships = list(
        verifier.verify_relationships(document, scope, deepcopy(extracted_relationships))
    )
    return extracted_events, verified_events, extracted_relationships, verified_relationships


def utc_now() -> str:
    return datetime.now(timezone.utc).isoformat()
