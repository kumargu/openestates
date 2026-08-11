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
from pathlib import Path
from typing import Any, Optional
from urllib.request import Request, urlopen

from pipeline.skills.rera_document_intelligence import (
    canonical_rera_society_entity_id,
    classify_rera_document,
    load_document_policy,
    select_rera_document_previews,
)


REPO_ROOT = Path(__file__).resolve().parents[2]


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


def _download_pdf(url: str, destination: Path) -> None:
    destination.parent.mkdir(parents=True, exist_ok=True)
    request = Request(url, headers={"User-Agent": "OpenEstates-RERA-Evidence/1.0"})
    with urlopen(request, timeout=90) as response:
        body = response.read()
    if not body.startswith(b"%PDF"):
        raise PreviewPreparationError("official document response is not a PDF")
    temporary = destination.with_suffix(destination.suffix + ".part")
    temporary.write_bytes(body)
    temporary.replace(destination)


def _extract_text(pdf_path: Path, max_pages: int) -> str:
    result = subprocess.run(
        ["pdftotext", "-f", "1", "-l", str(max_pages), "-layout", str(pdf_path), "-"],
        check=False,
        capture_output=True,
        text=True,
        timeout=60,
    )
    return result.stdout if result.returncode == 0 else ""


def _extract_ocr_text(preview_path: Path) -> str:
    if shutil.which("tesseract") is None:
        return ""
    result = subprocess.run(
        ["tesseract", str(preview_path), "stdout", "--psm", "6"],
        check=False,
        capture_output=True,
        text=True,
        timeout=120,
    )
    return result.stdout if result.returncode == 0 else ""


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


def _require_render_tools() -> None:
    missing = [command for command in ("pdftotext", "pdftoppm") if shutil.which(command) is None]
    if missing:
        raise PreviewPreparationError(
            "missing PDF tools: " + ", ".join(missing) + "; install Poppler before this offline step"
        )


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
    max_pages = int(policy["content_review"].get("max_pages", 2))
    project_dir = output_root / society_slug
    document_dir = project_dir / "documents"
    preview_dir = project_dir / "previews"
    rendered: dict[str, dict[str, Any]] = {}
    extracted_text: dict[str, str] = {}
    render_failures: list[dict[str, str]] = []
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
            embedded_text = _extract_text(pdf_path, max_pages)
            if not preview_path.is_file():
                _render_first_page(pdf_path, preview_path)
            ocr_text = _extract_ocr_text(preview_path) if len(embedded_text.strip()) < 80 else ""
            extracted_text[artifact_id] = "\n".join(
                text for text in (embedded_text, ocr_text) if text.strip()
            )
            artifact["content_review_method"] = (
                "embedded_text" if len(embedded_text.strip()) >= 80 else "ocr" if ocr_text.strip() else "visual_only"
            )
            preview_hash = _sha256(preview_path)
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
        extracted_text,
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
        "coverage_quality": "content_reviewed_document_previews",
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
