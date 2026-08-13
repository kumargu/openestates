#!/usr/bin/env python3
"""Download, inspect, render, and select useful RERA document previews.

This is an offline preparation step. It reads a cached ``fetch_rera`` result,
reclassifies its document manifest with DAG config, renders eligible PDFs, and
writes a promotion manifest consumed by ``promote_rera_project_plans``.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import shutil
import subprocess
import tempfile
from pathlib import Path
from typing import Any, Optional, Sequence
from urllib.parse import urljoin
from urllib.request import Request, urlopen

from pipeline.skills.rera_document_intelligence import (
    canonical_rera_society_entity_id,
    classify_rera_document,
    load_document_policy,
    select_rera_document_previews,
)


REPO_ROOT = Path(__file__).resolve().parents[2]
PLAN_FRAME_CACHE_ROOT = REPO_ROOT / "data" / "cache" / "rera_project_plan_frames"
RERA_DOCUMENT_BASE_URL = "https://rera.karnataka.gov.in/"


class PreviewPreparationError(ValueError):
    """Raised when preview preparation cannot preserve its input contract."""


def _string(value: Any, field: str) -> str:
    if not isinstance(value, str) or not value.strip():
        raise PreviewPreparationError(f"{field} must be a non-empty string")
    return value.strip()


def _load_json(path: Path) -> dict[str, Any]:
    with path.open("r", encoding="utf-8") as handle:
        payload = json.load(handle)
    if not isinstance(payload, dict):
        raise PreviewPreparationError(f"{path} must contain an object")
    return payload


def document_artifacts_from_skill_cache(path: Path) -> list[dict[str, Any]]:
    payload = _load_json(path)
    facts = payload.get("facts")
    if not isinstance(facts, list):
        raise PreviewPreparationError(f"{path} has no facts list")
    for fact in facts:
        if not isinstance(fact, dict) or fact.get("key") != "rera_document_manifest":
            continue
        value = fact.get("value")
        raw = value.get("data") if isinstance(value, dict) else value
        try:
            artifacts = json.loads(raw)
        except (TypeError, json.JSONDecodeError) as error:
            raise PreviewPreparationError("rera_document_manifest is not valid JSON") from error
        if not isinstance(artifacts, list):
            raise PreviewPreparationError("rera_document_manifest must contain a list")
        return [dict(item) for item in artifacts if isinstance(item, dict)]
    raise PreviewPreparationError(f"{path} has no rera_document_manifest fact")


def document_artifacts_from_skill_result(result: Any) -> list[dict[str, Any]]:
    """Read the document manifest from a cached or freshly collected skill result."""
    for fact in getattr(result, "facts", []):
        if getattr(fact, "key", None) != "rera_document_manifest":
            continue
        value = getattr(fact, "value", None)
        raw = value.get("data") if isinstance(value, dict) else value
        try:
            artifacts = json.loads(raw)
        except (TypeError, json.JSONDecodeError) as error:
            raise PreviewPreparationError("rera_document_manifest is not valid JSON") from error
        if not isinstance(artifacts, list):
            raise PreviewPreparationError("rera_document_manifest must contain a list")
        return [dict(item) for item in artifacts if isinstance(item, dict)]
    raise PreviewPreparationError("skill result has no rera_document_manifest fact")


def reclassify_artifacts(artifacts: list[dict[str, Any]]) -> list[dict[str, Any]]:
    classified: list[dict[str, Any]] = []
    for artifact in artifacts:
        result = classify_rera_document(
            artifact.get("label"),
            artifact.get("source_field_label"),
            artifact.get("source_url"),
        )
        updated = dict(artifact)
        updated.update(result)
        updated["document_group"] = result["group"]
        updated.pop("group", None)
        classified.append(updated)
    return classified


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _safe_name(value: str) -> str:
    normalized = "".join(ch.lower() if ch.isalnum() else "-" for ch in value)
    return "-".join(part for part in normalized.split("-") if part)[:120]


def _official_document_url(value: str) -> str:
    return urljoin(RERA_DOCUMENT_BASE_URL, value.strip())


def _download_pdf(url: str, destination: Path) -> None:
    destination.parent.mkdir(parents=True, exist_ok=True)
    request = Request(
        _official_document_url(url),
        headers={"User-Agent": "OpenEstates-RERA-Evidence/1.0"},
    )
    with urlopen(request, timeout=90) as response:
        body = response.read()
    if not body.startswith(b"%PDF"):
        raise PreviewPreparationError("official document response is not a PDF")
    temporary = destination.with_suffix(destination.suffix + ".part")
    temporary.write_bytes(body)
    temporary.replace(destination)


def _cached_pdf(source_url: str, cache_root: Path) -> tuple[Path, str]:
    request_hash = hashlib.sha256(source_url.encode("utf-8")).hexdigest()
    request_path = cache_root / "requests" / f"{request_hash}.json"
    if request_path.is_file():
        try:
            metadata = _load_json(request_path)
            source_hash = str(metadata.get("source_hash") or "")
            cached = cache_root / "objects" / "documents" / f"{source_hash}.pdf"
            if source_hash and cached.is_file() and _sha256(cached) == source_hash:
                return cached, source_hash
        except (OSError, ValueError, TypeError):
            pass

    with tempfile.TemporaryDirectory() as raw_directory:
        temporary = Path(raw_directory) / "document.pdf"
        _download_pdf(source_url, temporary)
        source_hash = _sha256(temporary)
        destination = cache_root / "objects" / "documents" / f"{source_hash}.pdf"
        destination.parent.mkdir(parents=True, exist_ok=True)
        if not destination.is_file():
            shutil.copyfile(temporary, destination)
    request_path.parent.mkdir(parents=True, exist_ok=True)
    request_path.write_text(
        json.dumps(
            {"source_hash": source_hash, "source_url": source_url},
            sort_keys=True,
            separators=(",", ":"),
        )
        + "\n",
        encoding="utf-8",
    )
    return destination, source_hash


def _render_first_page(pdf_path: Path, preview_path: Path) -> None:
    preview_path.parent.mkdir(parents=True, exist_ok=True)
    prefix = preview_path.with_suffix("")
    subprocess.run(
        [
            "pdftoppm",
            "-png",
            "-f",
            "1",
            "-singlefile",
            "-scale-to",
            "1600",
            str(pdf_path),
            str(prefix),
        ],
        check=True,
        capture_output=True,
        timeout=120,
    )
    generated = prefix.with_suffix(".png")
    if generated != preview_path:
        generated.replace(preview_path)
    if not preview_path.is_file() or preview_path.stat().st_size == 0:
        raise PreviewPreparationError(f"renderer did not produce {preview_path}")


def _cached_preview(pdf_path: Path, source_hash: str, cache_root: Path) -> tuple[Path, str]:
    render_path = cache_root / "renders" / f"{source_hash}.json"
    if render_path.is_file():
        try:
            metadata = _load_json(render_path)
            preview_hash = str(metadata.get("preview_hash") or "")
            cached = cache_root / "objects" / "previews" / f"{preview_hash}.png"
            if preview_hash and cached.is_file() and _sha256(cached) == preview_hash:
                return cached, preview_hash
        except (OSError, ValueError, TypeError):
            pass

    with tempfile.TemporaryDirectory() as raw_directory:
        temporary = Path(raw_directory) / "page-1.png"
        _render_first_page(pdf_path, temporary)
        preview_hash = _sha256(temporary)
        destination = cache_root / "objects" / "previews" / f"{preview_hash}.png"
        destination.parent.mkdir(parents=True, exist_ok=True)
        if not destination.is_file():
            shutil.copyfile(temporary, destination)
    render_path.parent.mkdir(parents=True, exist_ok=True)
    render_path.write_text(
        json.dumps(
            {"page": 1, "preview_hash": preview_hash, "source_hash": source_hash},
            sort_keys=True,
            separators=(",", ":"),
        )
        + "\n",
        encoding="utf-8",
    )
    return destination, preview_hash


def _read_raw_pgm(path: Path) -> tuple[int, int, bytes]:
    data = path.read_bytes()
    cursor = 0

    def token() -> bytes:
        nonlocal cursor
        while cursor < len(data) and data[cursor] in b" \t\r\n":
            cursor += 1
        if cursor < len(data) and data[cursor] == ord("#"):
            while cursor < len(data) and data[cursor] not in b"\r\n":
                cursor += 1
            return token()
        start = cursor
        while cursor < len(data) and data[cursor] not in b" \t\r\n":
            cursor += 1
        return data[start:cursor]

    if token() != b"P5":
        raise PreviewPreparationError("analysis render is not a raw grayscale PGM")
    width = int(token())
    height = int(token())
    if int(token()) != 255:
        raise PreviewPreparationError("analysis render has an unsupported pixel range")
    while cursor < len(data) and data[cursor] in b" \t\r\n":
        cursor += 1
    pixels = data[cursor:]
    if len(pixels) != width * height:
        raise PreviewPreparationError("analysis render has inconsistent pixel data")
    return width, height, pixels


def _analyze_first_page(pdf_path: Path, analysis_size: int) -> dict[str, float]:
    with tempfile.TemporaryDirectory() as raw_directory:
        prefix = Path(raw_directory) / "analysis"
        result = subprocess.run(
            [
                "pdftoppm",
                "-gray",
                "-f",
                "1",
                "-l",
                "1",
                "-singlefile",
                "-scale-to",
                str(analysis_size),
                str(pdf_path),
                str(prefix),
            ],
            check=False,
            capture_output=True,
            text=True,
            timeout=120,
        )
        if result.returncode != 0:
            raise PreviewPreparationError(result.stderr.strip() or "analysis render failed")
        width, height, pixels = _read_raw_pgm(prefix.with_suffix(".pgm"))

    total = max(len(pixels), 1)
    dark_ratio = sum(1 for value in pixels if value < 220) / total
    mid_tone_ratio = sum(1 for value in pixels if 80 <= value < 220) / total
    very_dark_ratio = sum(1 for value in pixels if value < 80) / total
    edge_count = 0
    comparisons = 0
    step = max(width // 160, 1)
    for y in range(0, height - 1, step):
        row = y * width
        next_row = (y + 1) * width
        for x in range(0, width - 1, step):
            value = pixels[row + x]
            if (
                abs(value - pixels[row + x + 1]) > 35
                or abs(value - pixels[next_row + x]) > 35
            ):
                edge_count += 1
            comparisons += 1
    return {
        "dark_ratio": round(dark_ratio, 6),
        "mid_tone_ratio": round(mid_tone_ratio, 6),
        "very_dark_ratio": round(very_dark_ratio, 6),
        "edge_ratio": round(edge_count / max(comparisons, 1), 6),
    }


def _render_rejection_reason(
    image_signals: dict[str, float], policy: dict[str, Any]
) -> Optional[str]:
    review = policy["render_review"]
    if image_signals["dark_ratio"] < float(review["min_dark_ratio"]):
        return "blank_render"
    if image_signals["edge_ratio"] < float(review["min_edge_ratio"]):
        return "not_line_drawing_like"
    if (
        image_signals["dark_ratio"] > float(review["max_dark_ratio"])
        or image_signals["mid_tone_ratio"] > float(review["max_mid_tone_ratio"])
        or image_signals["very_dark_ratio"] > float(review["max_very_dark_ratio"])
    ):
        return "photo_or_dense_render"
    return None


def _require_render_tools() -> None:
    missing = [command for command in ("pdftoppm",) if shutil.which(command) is None]
    if missing:
        raise PreviewPreparationError(
            "missing PDF tools: " + ", ".join(missing) + "; install Poppler before this offline step"
        )


def prepare_rera_plan_previews(
    artifacts: Sequence[dict[str, Any]],
    registration_number: str,
    cache_root: Path = PLAN_FRAME_CACHE_ROOT,
) -> dict[str, Any]:
    """Render and select plan frames into the content-addressed DAG input cache."""
    _require_render_tools()
    policy = load_document_policy()
    classified = reclassify_artifacts([dict(artifact) for artifact in artifacts])
    role_order = set(policy["selection"]["role_order"])
    inspection_role_caps = policy["selection"].get("inspection_role_caps", {})
    rendered: dict[str, dict[str, Any]] = {}
    reviews: list[dict[str, Any]] = []
    inspected_by_role: dict[str, int] = {}

    for artifact in classified:
        artifact_id = artifact.get("artifact_id")
        role = artifact.get("preview_role")
        source_url = artifact.get("source_url")
        if (
            not isinstance(artifact_id, str)
            or role not in role_order
            or artifact.get("preview_policy") != "content_review_required"
            or not isinstance(source_url, str)
            or not source_url.strip()
        ):
            continue
        source_url = _official_document_url(source_url)
        artifact["source_url"] = source_url
        inspection_cap = int(inspection_role_caps.get(role, 0))
        if inspected_by_role.get(role, 0) >= inspection_cap:
            continue
        inspected_by_role[role] = inspected_by_role.get(role, 0) + 1
        review: dict[str, Any] = {
            "artifact_id": artifact_id,
            "kind": artifact.get("kind"),
            "role": role,
            "label": artifact.get("label"),
            "source_url": source_url,
            "page": 1,
            "confidence": float(artifact.get("confidence") or 0.85),
        }
        try:
            pdf_path, source_hash = _cached_pdf(source_url, cache_root)
            preview_path, preview_hash = _cached_preview(pdf_path, source_hash, cache_root)
            image_signals = _analyze_first_page(
                pdf_path, int(policy["render_review"]["analysis_size_px"])
            )
            rejection_reason = _render_rejection_reason(image_signals, policy)
            review.update(
                {
                    "status": "rejected" if rejection_reason else "accepted",
                    "rejection_reason": rejection_reason,
                    "source_hash": source_hash,
                    "source_cache_relative_path": (
                        f"rera_project_plan_frames/objects/documents/{source_hash}.pdf"
                    ),
                    "preview_hash": preview_hash,
                    "cache_relative_path": (
                        f"rera_project_plan_frames/objects/previews/{preview_hash}.png"
                    ),
                    "image_signals": image_signals,
                }
            )
            if rejection_reason is None:
                rendered[artifact_id] = {
                    "preview_url": preview_path.name,
                    "source_hash": source_hash,
                    "preview_hash": preview_hash,
                    "page": 1,
                }
        except (OSError, PreviewPreparationError, subprocess.SubprocessError) as error:
            review.update({"status": "failed", "rejection_reason": str(error)})
        reviews.append(review)

    selection = select_rera_document_previews(classified, rendered, policy=policy)
    review_by_id = {review["artifact_id"]: review for review in reviews}
    previews = []
    for selected in selection["selected"]:
        review = review_by_id[selected["artifact_id"]]
        previews.append(
            {
                "artifact_id": selected["artifact_id"],
                "kind": selected["kind"],
                "role": selected["role"],
                "buyer_label": selected.get("label") or selected["kind"],
                "source_url": selected["source_url"],
                "source_hash": review["source_hash"],
                "source_cache_relative_path": review["source_cache_relative_path"],
                "preview_hash": review["preview_hash"],
                "cache_relative_path": review["cache_relative_path"],
                "page": selected["page"],
                "confidence": review["confidence"],
                "status": "accepted",
                "rejection_reason": None,
            }
        )

    payload = json.dumps(previews, sort_keys=True, separators=(",", ":")).encode("utf-8")
    return {
        "registration_number": registration_number,
        "previews": previews,
        "document_reviews": reviews,
        "selection_exclusions": selection["excluded"],
        "payload_hash": hashlib.sha256(payload).hexdigest(),
    }


def prepare_project(
    *,
    skill_cache_path: Path,
    society_slug: str,
    registration_number: str,
    output_root: Path,
) -> dict[str, Any]:
    _require_render_tools()
    policy = load_document_policy()
    artifacts = reclassify_artifacts(document_artifacts_from_skill_cache(skill_cache_path))
    role_order = set(policy["selection"]["role_order"])
    inspection_role_caps = policy["selection"].get("inspection_role_caps", {})
    project_dir = output_root / society_slug
    document_dir = project_dir / "documents"
    preview_dir = project_dir / "previews"
    rendered: dict[str, dict[str, Any]] = {}
    render_failures: list[dict[str, str]] = []
    render_reviews: list[dict[str, Any]] = []
    inspected_by_role: dict[str, int] = {}

    for artifact in artifacts:
        artifact_id = artifact.get("artifact_id")
        role = artifact.get("preview_role")
        source_url = artifact.get("source_url")
        if (
            not isinstance(artifact_id, str)
            or role not in role_order
            or artifact.get("preview_policy") != "content_review_required"
            or not isinstance(source_url, str)
            or not source_url.strip()
        ):
            continue
        inspection_cap = int(inspection_role_caps.get(role, 0))
        if inspected_by_role.get(role, 0) >= inspection_cap:
            continue
        inspected_by_role[role] = inspected_by_role.get(role, 0) + 1
        stem = _safe_name(artifact_id)
        pdf_path = document_dir / f"{stem}.pdf"
        preview_path = preview_dir / f"{stem}.png"
        try:
            if not pdf_path.is_file():
                _download_pdf(source_url, pdf_path)
            source_hash = _sha256(pdf_path)
            if not preview_path.is_file():
                _render_first_page(pdf_path, preview_path)
            preview_hash = _sha256(preview_path)
            image_signals = _analyze_first_page(
                pdf_path, int(policy["render_review"]["analysis_size_px"])
            )
            rejection_reason = _render_rejection_reason(image_signals, policy)
            render_reviews.append(
                {
                    "artifact_id": artifact_id,
                    "status": "rejected" if rejection_reason else "accepted",
                    "rejection_reason": rejection_reason,
                    "source_hash": source_hash,
                    "preview_hash": preview_hash,
                    "image_signals": image_signals,
                }
            )
            if rejection_reason:
                continue
            artifact["source_hash"] = source_hash
            rendered[artifact_id] = {
                "preview_url": preview_path.name,
                "source_hash": source_hash,
                "preview_hash": preview_hash,
                "page": 1,
            }
        except (OSError, PreviewPreparationError, subprocess.SubprocessError) as error:
            render_failures.append({"artifact_id": artifact_id, "reason": str(error)})

    selection = select_rera_document_previews(
        artifacts,
        rendered,
        policy=policy,
    )
    selected = selection["selected"]
    site_overview = next(
        (item for item in selected if item["role"] in ("master_plan", "site_plan")),
        None,
    )
    filed_previews = [item for item in selected if item is not site_overview]

    project: dict[str, Any] = {
        "society_slug": society_slug,
        "society_entity_id": canonical_rera_society_entity_id(registration_number),
        "provider": "RERA",
        "coverage_quality": "render_validated_document_previews",
        "registration_number": registration_number,
        "source_dirs": [str(preview_dir.resolve())],
        "document_artifacts": artifacts,
        "floor_plans": [],
        "filed_plan_previews": [
            {
                "artifact_id": item["artifact_id"],
                "source_name": rendered[item["artifact_id"]]["preview_url"],
                "page": item["page"],
                "role": item["role"],
                "selection_reason": item["selection_reason"],
            }
            for item in filed_previews
        ],
    }
    if site_overview:
        project["site_overview"] = {
            "artifact_id": site_overview["artifact_id"],
            "source_name": rendered[site_overview["artifact_id"]]["preview_url"],
            "page": site_overview["page"],
            "role": site_overview["role"],
            "selection_reason": site_overview["selection_reason"],
        }

    promotion_manifest_path = project_dir / "promotion_manifest.json"
    audit_path = project_dir / "selection_audit.json"
    promotion_manifest_path.parent.mkdir(parents=True, exist_ok=True)
    promotion_manifest_path.write_text(
        json.dumps({"projects": [project]}, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    audit = {
        "policy_version": policy["version"],
        "skill_cache_path": str(skill_cache_path),
        "registration_number": registration_number,
        "document_count": len(artifacts),
        "rendered_count": len(rendered),
        "selected": selected,
        "excluded": selection["excluded"],
        "render_failures": render_failures,
        "render_reviews": render_reviews,
    }
    audit_path.write_text(json.dumps(audit, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    return {
        "promotion_manifest_path": str(promotion_manifest_path),
        "audit_path": str(audit_path),
        "document_count": len(artifacts),
        "rendered_count": len(rendered),
        "selected_count": len(selected),
        "render_failure_count": len(render_failures),
    }


def main(argv: Optional[list[str]] = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--skill-cache", type=Path, required=True)
    parser.add_argument("--society-slug", required=True)
    parser.add_argument("--registration-number", required=True)
    parser.add_argument(
        "--output-root",
        type=Path,
        default=REPO_ROOT / "data" / "cache" / "rera_document_previews",
    )
    args = parser.parse_args(argv)
    result = prepare_project(
        skill_cache_path=args.skill_cache,
        society_slug=_string(args.society_slug, "society_slug"),
        registration_number=_string(args.registration_number, "registration_number"),
        output_root=args.output_root,
    )
    print(json.dumps(result, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
