#!/usr/bin/env python3.11
"""Promote buyer-facing RERA site overview + floor plan previews into the lake.

This is the first-slice offline writer for media.project_plan_frames.
It copies already-rendered brochure page images into:

  data/lake/media/previews/rera_plans/{society_slug}/
  data/lake/media/rera_plans/{society_slug}/project_plan_frames.json

Request paths must only read the resulting preview refs + JSON fact payload.
"""

import argparse
import json
import shutil
from pathlib import Path
from typing import Any, Dict, List, Optional

REPO_ROOT = Path(__file__).resolve().parents[2]

WATERFORD = {
    "society_slug": "prestige-waterford",
    "society_entity_id": "society:prestige-waterford",
    "provider": "RERA",
    "coverage_quality": "usable",
    "source_url": (
        "https://rera.karnataka.gov.in/download_jc?DOC_ID=JkzIpz4CjT0vIYgn3o%2BFyQ%3D%3D"
    ),
    "registration_number": "PRM/KA/RERA/1251/446/PR/200811/003528",
    "site_overview": {
        "id": "site-overview",
        "artifact_id": "prestige-waterford:brochure:page-3",
        "label": "Site overview",
        "page": 3,
        "source_name": "brochure-page-3.png",
        "confidence": 0.82,
    },
    "floor_plans": [
        {
            "id": "type-a2-1bed",
            "artifact_id": "prestige-waterford:brochure:page-12",
            "configuration_type": "1BHK",
            "unit_type_label": "A2",
            "bedroom_count": 1,
            "tab_label": "1 bedroom",
            "title": "A2 · 1 bedroom",
            "page": 12,
            "source_name": "brochure-page-12.png",
            "carpet_area_sqft": 413,
            "carpet_area_sqm": 38.35,
            "sale_area_sqft": 631,
            "sale_area_sqm": 58.59,
            "confidence": 0.86,
        },
        {
            "id": "type-b1a-2bed",
            "artifact_id": "prestige-waterford:brochure:page-15",
            "configuration_type": "2BHK",
            "unit_type_label": "B1A",
            "bedroom_count": 2,
            "tab_label": "2 bedroom",
            "title": "B1A · 2 bedroom",
            "page": 15,
            "source_name": "brochure-page-15.png",
            "carpet_area_sqft": 999,
            "carpet_area_sqm": 92.84,
            "sale_area_sqft": 1515,
            "sale_area_sqm": 140.79,
            "confidence": 0.86,
        },
        {
            "id": "type-b1-3bed",
            "artifact_id": "prestige-waterford:brochure:page-14",
            "configuration_type": "3BHK",
            "unit_type_label": "B1",
            "bedroom_count": 3,
            "tab_label": "3 bed compact",
            "title": "B1 · 3 bedroom compact",
            "page": 14,
            "source_name": "brochure-page-14.png",
            "carpet_area_sqft": 1197,
            "carpet_area_sqm": 111.25,
            "sale_area_sqft": 1775,
            "sale_area_sqm": 164.9,
            "confidence": 0.86,
        },
        {
            "id": "type-c2-3bed",
            "artifact_id": "prestige-waterford:brochure:page-22",
            "configuration_type": "3BHK",
            "unit_type_label": "C2",
            "bedroom_count": 3,
            "tab_label": "3 bed large",
            "title": "C2 · 3 bedroom large",
            "page": 22,
            "source_name": "brochure-page-22.png",
            "carpet_area_sqft": 1382,
            "carpet_area_sqm": 128.35,
            "sale_area_sqft": 2027,
            "sale_area_sqm": 188.31,
            "confidence": 0.86,
        },
        {
            "id": "type-d1-4bed",
            "artifact_id": "prestige-waterford:brochure:page-24",
            "configuration_type": "4BHK",
            "unit_type_label": "D1",
            "bedroom_count": 4,
            "tab_label": "4 bedroom",
            "title": "D1 · 4 bedroom",
            "page": 24,
            "source_name": "brochure-page-24.png",
            "carpet_area_sqft": 1740,
            "carpet_area_sqm": 161.67,
            "sale_area_sqft": 2525,
            "sale_area_sqm": 234.53,
            "confidence": 0.86,
        },
    ],
}


def _candidate_source_dirs(repo_root: Path) -> List[Path]:
    return [
        repo_root
        / "frontend"
        / "prototypes"
        / "rera-waterford-intelligence"
        / "public"
        / "previews",
        repo_root / "data" / "cache" / "rera_ocr_poc" / "prestige-waterford" / "previews",
    ]


def _find_source_image(source_dirs: List[Path], filename: str) -> Path:
    for directory in source_dirs:
        path = directory / filename
        if path.is_file():
            return path
    raise FileNotFoundError(
        "missing plan preview {!r}; looked in {}".format(
            filename, ", ".join(str(path) for path in source_dirs)
        )
    )


def _copy_preview(
    source: Path,
    preview_dir: Path,
    society_slug: str,
    dest_name: str,
) -> str:
    preview_dir.mkdir(parents=True, exist_ok=True)
    dest = preview_dir / dest_name
    shutil.copy2(source, dest)
    return "/media/previews/rera_plans/{}/{}".format(society_slug, dest_name)


def promote_waterford(repo_root: Path) -> Dict[str, Any]:
    project = WATERFORD
    society_slug = project["society_slug"]
    source_dirs = _candidate_source_dirs(repo_root)
    preview_dir = (
        repo_root / "data" / "lake" / "media" / "previews" / "rera_plans" / society_slug
    )
    fact_dir = repo_root / "data" / "lake" / "media" / "rera_plans" / society_slug
    fact_dir.mkdir(parents=True, exist_ok=True)

    site = dict(project["site_overview"])
    site_source = _find_source_image(source_dirs, site["source_name"])
    site_preview = _copy_preview(
        site_source,
        preview_dir,
        society_slug,
        "site-overview.png",
    )
    site_overview = {
        "artifact_id": site["artifact_id"],
        "label": site["label"],
        "preview_url": site_preview,
        "thumbnail_url": site_preview,
        "source_url": project["source_url"],
        "page": site["page"],
        "confidence": site["confidence"],
    }

    floor_plans: List[Dict[str, Any]] = []
    for plan in project["floor_plans"]:
        source = _find_source_image(source_dirs, plan["source_name"])
        dest_name = "{}-{}.png".format(
            plan["configuration_type"].lower(),
            plan["unit_type_label"].lower(),
        )
        preview_url = _copy_preview(source, preview_dir, society_slug, dest_name)
        floor_plans.append(
            {
                "id": plan["id"],
                "artifact_id": plan["artifact_id"],
                "configuration_type": plan["configuration_type"],
                "unit_type_label": plan["unit_type_label"],
                "bedroom_count": plan["bedroom_count"],
                "tab_label": plan["tab_label"],
                "title": plan["title"],
                "preview_url": preview_url,
                "thumbnail_url": preview_url,
                "source_url": project["source_url"],
                "page": plan["page"],
                "carpet_area_sqft": plan["carpet_area_sqft"],
                "carpet_area_sqm": plan["carpet_area_sqm"],
                "sale_area_sqft": plan["sale_area_sqft"],
                "sale_area_sqm": plan["sale_area_sqm"],
                "usable_area_ratio": round(
                    plan["carpet_area_sqft"] / plan["sale_area_sqft"], 3
                ),
                "confidence": plan["confidence"],
            }
        )

    payload = {
        "provider": project["provider"],
        "coverage_quality": project["coverage_quality"],
        "source_url": project["source_url"],
        "registration_number": project["registration_number"],
        "society_entity_id": project["society_entity_id"],
        "site_overview": site_overview,
        "floor_plans": floor_plans,
    }

    fact_path = fact_dir / "project_plan_frames.json"
    fact_path.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
    return {
        "fact_path": str(fact_path),
        "preview_dir": str(preview_dir),
        "floor_plan_count": len(floor_plans),
        "site_overview": site_overview["preview_url"],
    }


def main(argv: Optional[List[str]] = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--project",
        default="prestige-waterford",
        choices=["prestige-waterford"],
        help="Project slug to promote",
    )
    parser.add_argument(
        "--repo-root",
        type=Path,
        default=REPO_ROOT,
        help="Repository root",
    )
    args = parser.parse_args(argv)
    if args.project != "prestige-waterford":
        raise SystemExit("unsupported project")
    result = promote_waterford(args.repo_root.resolve())
    print(json.dumps(result, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
