#!/usr/bin/env python3.11
"""Promote RERA site overview + floor plan previews into lake media facts.

This offline writer materializes `media.project_plan_frames` from a small
project manifest and already-rendered RERA document preview images.

It writes generated outputs under:

  data/lake/media/previews/rera_plans/{society_slug}/
  data/lake/media/rera_plans/{society_slug}/project_plan_frames.json
  data/lake/media/rera_plans/{society_slug}/serving_facts.jsonl

Request paths must only read promoted preview refs and JSON fact payloads.
"""

import argparse
import hashlib
import json
import shutil
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Iterable, Optional

REPO_ROOT = Path(__file__).resolve().parents[2]
DEFAULT_MANIFEST = REPO_ROOT / "app" / "config" / "dag" / "rera_project_plan_targets.json"
PROJECT_PLAN_FACT_KEY = "media.project_plan_frames"


class ManifestError(ValueError):
    pass


@dataclass(frozen=True)
class DocumentArtifact:
    artifact_id: str
    kind: str
    label: str
    source_url: Optional[str]
    configuration_type: Optional[str]
    bedroom_count: Optional[float]
    confidence: float


def _string(value: Any, field: str) -> str:
    if not isinstance(value, str) or not value.strip():
        raise ManifestError(f"{field} must be a non-empty string")
    return value.strip()


def _optional_string(value: Any) -> Optional[str]:
    if isinstance(value, str) and value.strip():
        return value.strip()
    return None


def _number(value: Any, field: str) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise ManifestError(f"{field} must be numeric")
    return float(value)


def _optional_number(value: Any, field: str) -> Optional[float]:
    if value is None:
        return None
    return _number(value, field)


def _optional_int(value: Any, field: str) -> Optional[int]:
    number = _optional_number(value, field)
    if number is None:
        return None
    return int(number)


def _slug(value: str) -> str:
    normalized = "".join(ch.lower() if ch.isalnum() else "-" for ch in value.strip())
    return "-".join(part for part in normalized.split("-") if part)


def _load_json(path: Path) -> dict[str, Any]:
    with path.open("r", encoding="utf-8") as handle:
        payload = json.load(handle)
    if not isinstance(payload, dict):
        raise ManifestError(f"{path} must contain a JSON object")
    return payload


def _artifact_from_raw(raw: dict[str, Any]) -> DocumentArtifact:
    return DocumentArtifact(
        artifact_id=_string(raw.get("artifact_id"), "document_artifacts[].artifact_id"),
        kind=_string(raw.get("kind") or raw.get("document_kind"), "document_artifacts[].kind"),
        label=_string(raw.get("label"), "document_artifacts[].label"),
        source_url=_optional_string(raw.get("source_url")),
        configuration_type=_optional_string(raw.get("configuration_type")),
        bedroom_count=_optional_number(raw.get("bedroom_count"), "document_artifacts[].bedroom_count"),
        confidence=_optional_number(raw.get("confidence"), "document_artifacts[].confidence") or 0.7,
    )


def _document_artifacts(project: dict[str, Any], *, required: bool) -> dict[str, DocumentArtifact]:
    raw_artifacts = project.get("document_artifacts")
    if raw_artifacts is None and isinstance(project.get("rera_detail"), dict):
        raw_artifacts = project["rera_detail"].get("document_artifacts")
    if not isinstance(raw_artifacts, list) or not raw_artifacts:
        if not required:
            return {}
        raise ManifestError("project must include non-empty RERA document_artifacts")

    artifacts: dict[str, DocumentArtifact] = {}
    for raw in raw_artifacts:
        if not isinstance(raw, dict):
            raise ManifestError("document_artifacts entries must be objects")
        artifact = _artifact_from_raw(raw)
        artifacts[artifact.artifact_id] = artifact
    return artifacts


def _source_dirs(repo_root: Path, project: dict[str, Any], manifest_path: Path) -> list[Path]:
    raw_dirs = project.get("source_dirs", [])
    if not isinstance(raw_dirs, list):
        raise ManifestError("source_dirs must be a list")
    dirs = []
    for raw in raw_dirs:
        directory = _string(raw, "source_dirs[]")
        path = Path(directory)
        if not path.is_absolute():
            path = (manifest_path.parent / path).resolve()
        dirs.append(path)

    society_slug = _string(project.get("society_slug"), "society_slug")
    dirs.extend(
        [
            repo_root / "data" / "cache" / "rera_ocr_poc" / society_slug / "previews",
            repo_root / "data" / "cache" / "rera_project_plans" / society_slug / "previews",
        ]
    )
    return dirs


def _find_source_image(source_dirs: Iterable[Path], filename: str) -> Path:
    for directory in source_dirs:
        path = directory / filename
        if path.is_file():
            return path
    searched = ", ".join(str(path) for path in source_dirs)
    raise FileNotFoundError(f"missing plan preview {filename!r}; looked in {searched}")


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _copy_preview(source: Path, preview_dir: Path, society_slug: str, dest_name: str) -> str:
    preview_dir.mkdir(parents=True, exist_ok=True)
    dest = preview_dir / dest_name
    shutil.copy2(source, dest)
    return f"/media/previews/rera_plans/{society_slug}/{dest_name}"


def _artifact(
    artifacts: dict[str, DocumentArtifact],
    artifact_id: str,
    expected_kinds: set[str],
) -> DocumentArtifact:
    artifact = artifacts.get(artifact_id)
    if artifact is None:
        raise ManifestError(f"{artifact_id!r} is not present in RERA document_artifacts")
    if artifact.kind not in expected_kinds:
        expected = ", ".join(sorted(expected_kinds))
        raise ManifestError(f"{artifact_id!r} has kind {artifact.kind!r}; expected {expected}")
    return artifact


def _source_url(project: dict[str, Any], artifact: DocumentArtifact, override: Any = None) -> Optional[str]:
    return (
        _optional_string(override)
        or artifact.source_url
        or _optional_string(project.get("source_url"))
        or _optional_string(project.get("rera_source_url"))
    )


def _site_overview(
    repo_root: Path,
    manifest_path: Path,
    project: dict[str, Any],
    artifacts: dict[str, DocumentArtifact],
    preview_dir: Path,
) -> Optional[dict[str, Any]]:
    raw = project.get("site_overview")
    if raw is None:
        return None
    if not isinstance(raw, dict):
        raise ManifestError("site_overview must be an object")

    society_slug = _string(project.get("society_slug"), "society_slug")
    artifact_id = _string(raw.get("artifact_id"), "site_overview.artifact_id")
    artifact = _artifact(artifacts, artifact_id, {"site_plan", "brochure", "site_overview"})
    source_name = _string(raw.get("source_name"), "site_overview.source_name")
    source = _find_source_image(_source_dirs(repo_root, project, manifest_path), source_name)
    dest_name = _optional_string(raw.get("dest_name")) or "site-overview.png"
    preview_url = _copy_preview(source, preview_dir, society_slug, dest_name)

    return {
        "id": _optional_string(raw.get("id")) or "site-overview",
        "plan_kind": "site_overview",
        "artifact_id": artifact_id,
        "label": _optional_string(raw.get("label")) or artifact.label,
        "preview_url": preview_url,
        "thumbnail_url": preview_url,
        "source_url": _source_url(project, artifact, raw.get("source_url")),
        "source_hash": _sha256(source),
        "page": _optional_int(raw.get("page"), "site_overview.page"),
        "confidence": _optional_number(raw.get("confidence"), "site_overview.confidence") or artifact.confidence,
    }


def _floor_plan(
    repo_root: Path,
    manifest_path: Path,
    project: dict[str, Any],
    artifacts: dict[str, DocumentArtifact],
    preview_dir: Path,
    raw: dict[str, Any],
) -> dict[str, Any]:
    society_slug = _string(project.get("society_slug"), "society_slug")
    artifact_id = _string(raw.get("artifact_id"), "floor_plans[].artifact_id")
    artifact = _artifact(artifacts, artifact_id, {"floor_plan", "brochure"})
    source_name = _string(raw.get("source_name"), "floor_plans[].source_name")
    source = _find_source_image(_source_dirs(repo_root, project, manifest_path), source_name)

    configuration_type = _optional_string(raw.get("configuration_type")) or artifact.configuration_type
    if configuration_type is None:
        raise ManifestError(f"{artifact_id!r} needs configuration_type")
    bedroom_count = _optional_int(raw.get("bedroom_count"), "floor_plans[].bedroom_count")
    if bedroom_count is None and artifact.bedroom_count is not None:
        bedroom_count = int(artifact.bedroom_count)
    if bedroom_count is None:
        raise ManifestError(f"{artifact_id!r} needs bedroom_count")

    unit_label = _optional_string(raw.get("unit_type_label"))
    plan_id = _optional_string(raw.get("id")) or _slug(
        " ".join(part for part in [configuration_type, unit_label, str(raw.get("page") or "")] if part)
    )
    dest_name = _optional_string(raw.get("dest_name")) or f"{_slug(plan_id)}.png"
    preview_url = _copy_preview(source, preview_dir, society_slug, dest_name)

    carpet_sqft = _optional_int(raw.get("carpet_area_sqft"), "floor_plans[].carpet_area_sqft")
    sale_sqft = _optional_int(raw.get("sale_area_sqft"), "floor_plans[].sale_area_sqft")
    usable_ratio = _optional_number(raw.get("usable_area_ratio"), "floor_plans[].usable_area_ratio")
    if usable_ratio is None and carpet_sqft and sale_sqft:
        usable_ratio = round(carpet_sqft / sale_sqft, 3)

    title = _optional_string(raw.get("title"))
    if title is None:
        title = f"{unit_label} · {configuration_type}" if unit_label else configuration_type

    return {
        "id": plan_id,
        "plan_kind": "floor_plan",
        "artifact_id": artifact_id,
        "configuration_type": configuration_type,
        "unit_type_label": unit_label,
        "bedroom_count": bedroom_count,
        "tab_label": _optional_string(raw.get("tab_label")) or configuration_type,
        "title": title,
        "preview_url": preview_url,
        "thumbnail_url": preview_url,
        "source_url": _source_url(project, artifact, raw.get("source_url")),
        "source_hash": _sha256(source),
        "page": _optional_int(raw.get("page"), "floor_plans[].page"),
        "carpet_area_sqft": carpet_sqft,
        "carpet_area_sqm": _optional_number(raw.get("carpet_area_sqm"), "floor_plans[].carpet_area_sqm"),
        "sale_area_sqft": sale_sqft,
        "sale_area_sqm": _optional_number(raw.get("sale_area_sqm"), "floor_plans[].sale_area_sqm"),
        "usable_area_ratio": usable_ratio,
        "confidence": _optional_number(raw.get("confidence"), "floor_plans[].confidence") or artifact.confidence,
    }


def _drop_none(value: Any) -> Any:
    if isinstance(value, dict):
        return {key: _drop_none(item) for key, item in value.items() if item is not None}
    if isinstance(value, list):
        return [_drop_none(item) for item in value]
    return value


def materialize_project(
    repo_root: Path,
    manifest_path: Path,
    project: dict[str, Any],
) -> dict[str, Any]:
    if _optional_string(project.get("provider")) not in (None, "RERA"):
        raise ManifestError("rera_project_plan_targets only accepts provider=RERA")

    society_slug = _string(project.get("society_slug"), "society_slug")
    society_entity_id = _string(project.get("society_entity_id"), "society_entity_id")
    registration_number = _optional_string(project.get("registration_number"))
    source_url = _optional_string(project.get("source_url")) or _optional_string(project.get("rera_source_url"))
    raw_floor_plans = project.get("floor_plans", [])
    if not isinstance(raw_floor_plans, list):
        raise ManifestError("floor_plans must be a list")
    artifacts = _document_artifacts(
        project,
        required=project.get("site_overview") is not None or bool(raw_floor_plans),
    )
    preview_dir = repo_root / "data" / "lake" / "media" / "previews" / "rera_plans" / society_slug
    fact_dir = repo_root / "data" / "lake" / "media" / "rera_plans" / society_slug
    fact_dir.mkdir(parents=True, exist_ok=True)

    site_overview = _site_overview(repo_root, manifest_path, project, artifacts, preview_dir)
    floor_plans = [
        _floor_plan(repo_root, manifest_path, project, artifacts, preview_dir, raw)
        for raw in raw_floor_plans
        if isinstance(raw, dict)
    ]

    payload = _drop_none(
        {
            "provider": "RERA",
            "coverage_quality": _optional_string(project.get("coverage_quality")) or "usable",
            "source_url": source_url,
            "registration_number": registration_number,
            "society_entity_id": society_entity_id,
            "site_overview": site_overview,
            "floor_plans": floor_plans,
        }
    )

    fact_path = fact_dir / "project_plan_frames.json"
    fact_path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")

    confidences = [plan["confidence"] for plan in floor_plans]
    if site_overview:
        confidences.append(site_overview["confidence"])

    serving_fact = {
        "entity_id": society_entity_id,
        "fact_key": PROJECT_PLAN_FACT_KEY,
        "value_type": "text",
        "value_text": json.dumps(payload, sort_keys=True, separators=(",", ":")),
        "value": {"type": "Text", "data": json.dumps(payload, sort_keys=True, separators=(",", ":"))},
        "confidence": min(0.9, max([0.0] + confidences)),
        "source_type": "Rera",
        "source_url": source_url,
        "model": None,
        "skill_id": "promote_rera_project_plans",
        "learned_at": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
    }
    serving_path = fact_dir / "serving_facts.jsonl"
    serving_path.write_text(json.dumps(serving_fact, sort_keys=True) + "\n", encoding="utf-8")

    return {
        "society_slug": society_slug,
        "fact_path": str(fact_path),
        "serving_fact_path": str(serving_path),
        "preview_dir": str(preview_dir),
        "floor_plan_count": len(floor_plans),
        "site_overview": site_overview["preview_url"] if site_overview else None,
    }


def load_manifest(path: Path) -> list[dict[str, Any]]:
    manifest = _load_json(path)
    projects = manifest.get("projects")
    if not isinstance(projects, list) or not projects:
        raise ManifestError("manifest must contain a non-empty projects list")
    for project in projects:
        if not isinstance(project, dict):
            raise ManifestError("projects entries must be objects")
    return projects


def main(argv: Optional[list[str]] = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, default=DEFAULT_MANIFEST)
    parser.add_argument("--project", default="all", help="Society slug to promote, or all")
    parser.add_argument("--repo-root", type=Path, default=REPO_ROOT)
    args = parser.parse_args(argv)

    repo_root = args.repo_root.resolve()
    manifest_path = args.manifest.resolve()
    projects = load_manifest(manifest_path)
    if args.project != "all":
        projects = [
            project
            for project in projects
            if _string(project.get("society_slug"), "society_slug") == args.project
        ]
    if not projects:
        raise SystemExit(f"no projects matched {args.project!r}")

    results = [materialize_project(repo_root, manifest_path, project) for project in projects]
    print(json.dumps({"projects": results}, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
