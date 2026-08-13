"""Classify RERA documents and choose a small, useful preview set.

Product vocabulary and selection limits live in DAG config. This module owns
only generic matching, validation, de-duplication, and deterministic ordering.
Downloading and rendering remain offline concerns.
"""

from __future__ import annotations

import json
import hashlib
import re
import unicodedata
from functools import lru_cache
from pathlib import Path
from typing import Any, Mapping, Optional, Sequence


REPO_ROOT = Path(__file__).resolve().parents[2]
DEFAULT_POLICY_PATH = (
    REPO_ROOT
    / "app"
    / "config"
    / "dag"
    / "source_adapters"
    / "rera_document_previews.json"
)


class ReraDocumentPolicyError(ValueError):
    """Raised when document intelligence config is incomplete or invalid."""


def _non_empty_string(value: Any, field: str) -> str:
    if not isinstance(value, str) or not value.strip():
        raise ReraDocumentPolicyError(f"{field} must be a non-empty string")
    return value.strip()


@lru_cache(maxsize=4)
def load_document_policy(path: str = str(DEFAULT_POLICY_PATH)) -> dict[str, Any]:
    policy_path = Path(path)
    with policy_path.open("r", encoding="utf-8") as handle:
        policy = json.load(handle)
    if not isinstance(policy, dict):
        raise ReraDocumentPolicyError(f"{policy_path} must contain an object")
    roles = policy.get("roles")
    if not isinstance(roles, list) or not roles:
        raise ReraDocumentPolicyError("roles must be a non-empty list")
    for index, role in enumerate(roles):
        if not isinstance(role, dict):
            raise ReraDocumentPolicyError(f"roles[{index}] must be an object")
        for field in ("id", "kind", "group", "buyer_visibility", "preview_policy"):
            _non_empty_string(role.get(field), f"roles[{index}].{field}")
        patterns = role.get("label_patterns")
        if not isinstance(patterns, list) or not patterns:
            raise ReraDocumentPolicyError(f"roles[{index}].label_patterns must be non-empty")
        for pattern in patterns:
            re.compile(_non_empty_string(pattern, f"roles[{index}].label_patterns[]"), re.IGNORECASE)
    render_review = policy.get("render_review")
    required_render_fields = (
        "analysis_size_px",
        "min_dark_ratio",
        "max_dark_ratio",
        "max_mid_tone_ratio",
        "max_very_dark_ratio",
        "min_edge_ratio",
    )
    if not isinstance(render_review, dict) or any(
        not isinstance(render_review.get(field), (int, float))
        for field in required_render_fields
    ):
        raise ReraDocumentPolicyError("render_review must define numeric render thresholds")
    return policy


def _matching_role(text: Optional[str], policy: Mapping[str, Any]) -> Optional[dict[str, Any]]:
    if not isinstance(text, str) or not text.strip():
        return None
    normalized = re.sub(r"[^a-zA-Z0-9]+", " ", text).strip()
    for role in policy["roles"]:
        if any(
            re.search(pattern, candidate, re.IGNORECASE)
            for pattern in role["label_patterns"]
            for candidate in (text, normalized)
        ):
            return role
    return None


def canonical_rera_society_entity_id(registration_number: str) -> str:
    """Return the canonical society ID used by the RERA registry DAG.

    Registration identity is the only stable input. Project names and manually
    supplied slugs must never decide which serving entity receives plan facts.
    """

    normalized = " ".join(
        unicodedata.normalize("NFKC", registration_number).strip().upper().split()
    )
    if not normalized:
        raise ReraDocumentPolicyError("registration_number must be non-empty")
    digest = hashlib.sha256(normalized.encode("utf-8")).hexdigest()
    return f"society:rera-{digest[:16]}"


def classify_rera_document(
    label: Optional[str],
    source_field_label: Optional[str] = None,
    href: Optional[str] = None,
    *,
    policy: Optional[Mapping[str, Any]] = None,
) -> dict[str, Optional[str]]:
    """Classify independent evidence fields without contaminating filenames.

    The uploaded filename is authoritative for document role when it contains
    a recognizable description. Portal field context is only a fallback for
    generic filenames such as ``site.pdf``.
    """

    active_policy = policy or load_document_policy()
    for basis, text in (
        ("label", label),
        ("source_field_label", source_field_label),
        ("href", href),
    ):
        role = _matching_role(text, active_policy)
        if role is None:
            continue
        return {
            "kind": role["kind"],
            "group": role["group"],
            "buyer_visibility": role["buyer_visibility"],
            "preview_policy": role["preview_policy"],
            "preview_role": role["id"],
            "classification_basis": basis,
        }
    return {
        "kind": "unclassified_document",
        "group": "other",
        "buyer_visibility": "list_only",
        "preview_policy": "list_only",
        "preview_role": None,
        "classification_basis": None,
    }


def _artifact_text(artifact: Mapping[str, Any], key: str) -> Optional[str]:
    value = artifact.get(key)
    return value.strip() if isinstance(value, str) and value.strip() else None


def _dedupe_key(artifact: Mapping[str, Any]) -> tuple[str, str]:
    source_url = (_artifact_text(artifact, "source_url") or "").lower()
    label = re.sub(r"[^a-z0-9]+", " ", (_artifact_text(artifact, "label") or "").lower()).strip()
    return source_url, label


def select_rera_document_previews(
    artifacts: Sequence[Mapping[str, Any]],
    rendered_previews: Mapping[str, Mapping[str, Any]],
    *,
    policy: Optional[Mapping[str, Any]] = None,
) -> dict[str, list[dict[str, Any]]]:
    """Select deterministic previews from rendered, public RERA evidence.

    ``rendered_previews`` is deliberately required as a separate input: a URL
    or a promising filename never becomes buyer-facing preview evidence until
    the offline renderer produced an actual image.
    """

    active_policy = policy or load_document_policy()
    selection = active_policy.get("selection")
    if not isinstance(selection, dict):
        raise ReraDocumentPolicyError("selection must be an object")
    role_order = selection.get("role_order")
    role_caps = selection.get("role_caps")
    if not isinstance(role_order, list) or not isinstance(role_caps, dict):
        raise ReraDocumentPolicyError("selection role_order and role_caps are required")

    role_rank = {role_id: index for index, role_id in enumerate(role_order)}
    eligible: list[dict[str, Any]] = []
    excluded: list[dict[str, Any]] = []
    seen_documents: set[tuple[str, str]] = set()

    for source_index, artifact in enumerate(artifacts):
        artifact_id = _artifact_text(artifact, "artifact_id")
        role = _artifact_text(artifact, "preview_role")
        reason: Optional[str] = None
        if artifact_id is None:
            reason = "missing_artifact_id"
        elif role not in role_rank:
            reason = "role_not_selected"
        elif artifact.get("preview_policy") != "content_review_required":
            reason = "preview_policy_denied"
        elif selection.get("require_source_url", True) and not _artifact_text(artifact, "source_url"):
            reason = "missing_source_url"
        elif selection.get("require_rendered_preview", True) and artifact_id not in rendered_previews:
            reason = "missing_rendered_preview"
        elif selection.get("require_source_hash", True) and not _artifact_text(
            rendered_previews.get(artifact_id, {}), "source_hash"
        ):
            reason = "missing_source_hash"
        if reason:
            excluded.append({"artifact_id": artifact_id, "reason": reason})
            continue

        dedupe_key = _dedupe_key(artifact)
        if dedupe_key in seen_documents:
            excluded.append({"artifact_id": artifact_id, "reason": "duplicate_document"})
            continue
        seen_documents.add(dedupe_key)
        preview = dict(rendered_previews[artifact_id])
        eligible.append(
            {
                "artifact": dict(artifact),
                "preview": preview,
                "role": role,
                "role_rank": role_rank[role],
                "source_index": source_index,
            }
        )

    eligible.sort(
        key=lambda item: (
            item["role_rank"],
            (_artifact_text(item["artifact"], "label") or "").casefold(),
            item["source_index"],
        )
    )

    max_total = int(selection.get("max_total", len(eligible)))
    dedupe_buckets = selection.get("dedupe_buckets", {})
    selected: list[dict[str, Any]] = []
    role_counts: dict[str, int] = {}
    used_buckets: set[str] = set()
    used_source_hashes: set[str] = set()
    fallback_roles = set(selection.get("fallback_roles", []))
    minimum_preview_count = int(selection.get("minimum_preview_count", 0))

    def append_if_allowed(item: Mapping[str, Any]) -> bool:
        artifact = item["artifact"]
        artifact_id = artifact["artifact_id"]
        role = item["role"]
        cap = int(role_caps.get(role, 0))
        bucket = dedupe_buckets.get(role) if isinstance(dedupe_buckets, dict) else None
        preview = item["preview"]
        source_hash = _artifact_text(preview, "source_hash")
        if role_counts.get(role, 0) >= cap:
            excluded.append({"artifact_id": artifact_id, "reason": "role_cap"})
            return False
        if isinstance(bucket, str) and bucket in used_buckets:
            excluded.append({"artifact_id": artifact_id, "reason": "dedupe_bucket"})
            return False
        if source_hash and source_hash in used_source_hashes:
            excluded.append({"artifact_id": artifact_id, "reason": "duplicate_content"})
            return False
        if len(selected) >= max_total:
            excluded.append({"artifact_id": artifact_id, "reason": "total_cap"})
            return False

        selected.append(
            {
                "artifact_id": artifact_id,
                "kind": artifact.get("kind"),
                "role": role,
                "label": artifact.get("label"),
                "source_url": artifact.get("source_url"),
                "preview_url": preview.get("preview_url"),
                "source_hash": preview.get("source_hash"),
                "page": preview.get("page", 1),
                "selection_reason": f"configured_role:{role}",
            }
        )
        role_counts[role] = role_counts.get(role, 0) + 1
        if isinstance(bucket, str):
            used_buckets.add(bucket)
        if source_hash:
            used_source_hashes.add(source_hash)
        return True

    primary = [item for item in eligible if item["role"] not in fallback_roles]
    fallback = [item for item in eligible if item["role"] in fallback_roles]
    for item in primary:
        append_if_allowed(item)

    for item in fallback:
        if len(selected) >= minimum_preview_count:
            excluded.append(
                {"artifact_id": item["artifact"]["artifact_id"], "reason": "fallback_not_needed"}
            )
            continue
        append_if_allowed(item)

    return {"selected": selected, "excluded": excluded}
