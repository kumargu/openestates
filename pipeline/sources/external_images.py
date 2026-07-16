"""Source-neutral external image collection.

The collector emits image observations only. Rust owns the durable Parquet,
KG facts, and later binary derivative assets. MagicBricks is the first source
adapter because those URLs already appear in market-pricing provenance.
"""

import json
import os
import re
import urllib.parse
import urllib.request
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Dict, Iterable, List, Optional

HEADERS = {
    "User-Agent": (
        "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) "
        "AppleWebKit/537.36 (KHTML, like Gecko) "
        "Chrome/122.0.0.0 Safari/537.36"
    ),
    "Accept": "text/html,application/xhtml+xml,application/xml;q=0.9,image/webp,*/*;q=0.8",
    "Accept-Language": "en-US,en;q=0.8",
}

IMAGE_URL_RE = re.compile(
    r"https?:\/\/[^\"'<>\s]+?\.(?:jpg|jpeg|png|webp)(?:\?[^\"'<>\s]*)?",
    re.IGNORECASE,
)
IMG_TAG_RE = re.compile(r"<img\b[^>]*>", re.IGNORECASE)
ATTR_RE = re.compile(r"([a-zA-Z0-9_:-]+)\s*=\s*([\"'])(.*?)\2", re.DOTALL)
IMAGE_ATTRS = ("data-src", "data-original", "data-lazy", "src")


def collect_external_images(request: Dict[str, Any]) -> Dict[str, Any]:
    observed_at = normalized_planned_at(request)
    snapshot_date = partition_values(request).get("dt") or observed_at[:10]
    if skip_image_fetch(request):
        return skipped_snapshot(snapshot_date, observed_at)

    project_root = Path(request.get("project_root") or ".").resolve()
    records = []  # type: List[Dict[str, Any]]
    for input_data in request.get("source_entities", []):
        records.extend(records_for_entity(project_root, input_data, observed_at))

    deduped = dedupe_records(records)
    if not deduped:
        return skipped_snapshot(snapshot_date, observed_at)

    return {
        "snapshot_date": snapshot_date,
        "records": deduped,
        "source_watermarks": [
            {
                "source": "external_image_magicbricks",
                "high_watermark": max(
                    (record["observed_at"] for record in deduped), default=observed_at
                ),
            }
        ],
    }


def records_for_entity(
    project_root: Path, input_data: Dict[str, Any], observed_at: str
) -> List[Dict[str, Any]]:
    entity_id = optional_string(input_data.get("entity_id"))
    society_name = optional_string(
        input_data.get("society_name")
        or input_data.get("name")
        or input_data.get("project_name")
    )
    if not entity_id or not society_name:
        return []

    source_pages = explicit_source_pages(input_data)
    if not source_pages:
        source_pages = magicbricks_pages_from_kg(project_root, entity_id, society_name)
    records = []  # type: List[Dict[str, Any]]
    for source_page in source_pages:
        page_url = required_page_url(source_page)
        if not page_url:
            continue
        html = optional_string(source_page.get("html")) if isinstance(source_page, dict) else None
        if html is None:
            html = fetch_html(page_url)
        if not html:
            continue
        candidates = image_candidates_from_html(html, page_url, society_name)
        for rank, candidate in enumerate(candidates[:8], start=1):
            records.append(
                {
                    "entity_id": entity_id,
                    "project_key": optional_string(input_data.get("project_key")),
                    "source_name": optional_string(source_page.get("source_name"))
                    if isinstance(source_page, dict)
                    else "magicbricks",
                    "source_page_url": page_url,
                    "image_url": candidate["image_url"],
                    "image_kind": classify_image(candidate["image_url"], candidate.get("alt_text")),
                    "width": optional_int(candidate.get("width")),
                    "height": optional_int(candidate.get("height")),
                    "rank": rank,
                    "score": candidate["score"],
                    "alt_text": optional_string(candidate.get("alt_text")),
                    "storage_policy": "link_only",
                    "content_sha256": None,
                    "observed_at": observed_at,
                }
            )
    records.sort(
        key=lambda record: (
            record["entity_id"],
            record.get("rank") or 999,
            -(record.get("score") or 0),
            record["image_url"],
        )
    )
    return records


def explicit_source_pages(input_data: Dict[str, Any]) -> List[Dict[str, Any]]:
    pages = input_data.get("image_source_pages")
    if isinstance(pages, list):
        return [page for page in pages if isinstance(page, dict)]
    urls = input_data.get("image_source_urls")
    if isinstance(urls, list):
        return [
            {"source_page_url": url, "source_name": "magicbricks"}
            for url in urls
            if optional_string(url)
        ]
    return []


def magicbricks_pages_from_kg(
    project_root: Path, entity_id: str, society_name: str
) -> List[Dict[str, Any]]:
    node = legacy_society_node(project_root, entity_id, society_name)
    if not node:
        return []
    pages = []
    seen = set()
    for fact in node.get("facts", []):
        source = fact.get("source") or {}
        url = optional_string(source.get("url"))
        skill_id = optional_string(source.get("skill_id"))
        marker = "{} {}".format(url or "", skill_id or "").lower()
        if not url or "magicbricks" not in marker or url in seen:
            continue
        seen.add(url)
        pages.append({"source_page_url": url, "source_name": "magicbricks"})
    return pages


def image_candidates_from_html(
    html: str, page_url: str, society_name: str
) -> List[Dict[str, Any]]:
    candidates = []
    seen = set()
    for tag in IMG_TAG_RE.findall(html):
        attrs = {key.lower(): value for key, _, value in ATTR_RE.findall(tag)}
        raw_url = next((attrs.get(attr) for attr in IMAGE_ATTRS if attrs.get(attr)), None)
        if not raw_url:
            continue
        image_url = normalize_image_url(raw_url, page_url)
        if not usable_image_url(image_url) or image_url in seen:
            continue
        seen.add(image_url)
        alt_text = attrs.get("alt") or attrs.get("title")
        candidates.append(
            {
                "image_url": image_url,
                "alt_text": alt_text,
                "width": attrs.get("width"),
                "height": attrs.get("height"),
                "score": score_image(image_url, alt_text, society_name),
            }
        )
    for raw_url in IMAGE_URL_RE.findall(html):
        image_url = normalize_image_url(raw_url.replace("\\/", "/"), page_url)
        if not usable_image_url(image_url) or image_url in seen:
            continue
        seen.add(image_url)
        candidates.append(
            {
                "image_url": image_url,
                "alt_text": None,
                "width": None,
                "height": None,
                "score": score_image(image_url, None, society_name),
            }
        )
    candidates.sort(key=lambda candidate: (-candidate["score"], candidate["image_url"]))
    return candidates


def score_image(image_url: str, alt_text: Optional[str], society_name: str) -> float:
    text = "{} {}".format(image_url, alt_text or "").lower()
    score = 50.0
    if any(word in text for word in ("logo", "icon", "sprite", "favicon", "floorplan")):
        score -= 40
    if any(word in text for word in ("project", "gallery", "photo", "elevation", "amenity")):
        score += 10
    if any(word in text for word in ("elevation", "exterior", "tower", "building")):
        score += 6
    name_matches = sum(
        1
        for part in re.split(r"[^a-z0-9]+", society_name.lower())
        if len(part) > 2 and part in text
    )
    score += min(name_matches * 8, 24)
    if "magicbricks" in image_url.lower() or "mbimgs" in image_url.lower():
        score += 8
    return max(score, 0.0)


def classify_image(image_url: str, alt_text: Optional[str]) -> str:
    text = "{} {}".format(image_url, alt_text or "").lower()
    if any(word in text for word in ("floorplan", "floor-plan", "floor_plan")):
        return "floor_plan"
    if any(word in text for word in ("amenity", "clubhouse", "pool", "gym", "garden")):
        return "amenities"
    if any(word in text for word in ("elevation", "exterior", "tower", "building", "project")):
        return "exterior"
    return "unknown"


def dedupe_records(records: Iterable[Dict[str, Any]]) -> List[Dict[str, Any]]:
    deduped = []
    seen = set()
    for record in records:
        key = (record["entity_id"], record["image_url"])
        if key in seen:
            continue
        seen.add(key)
        deduped.append(record)
    return deduped


def skipped_snapshot(snapshot_date: str, observed_at: str) -> Dict[str, Any]:
    return {
        "snapshot_date": snapshot_date,
        "records": [],
        "source_watermarks": [
            {
                "source": "external_images_skipped",
                "high_watermark": observed_at,
            }
        ],
    }


def skip_image_fetch(request: Dict[str, Any]) -> bool:
    if request.get("skip_image_fetch"):
        return True
    return str(os.environ.get("OPENESTATES_SKIP_IMAGE_FETCH") or "").lower() in (
        "1",
        "true",
        "yes",
    )


def fetch_html(url: str) -> Optional[str]:
    try:
        request = urllib.request.Request(url, headers=HEADERS)
        with urllib.request.urlopen(request, timeout=15) as response:
            return response.read().decode("utf-8", errors="replace")
    except Exception:
        return None


def required_page_url(source_page: Any) -> Optional[str]:
    if isinstance(source_page, dict):
        return optional_string(source_page.get("source_page_url") or source_page.get("url"))
    return optional_string(source_page)


def normalize_image_url(value: str, page_url: str) -> str:
    value = value.strip().replace("\\/", "/")
    if value.startswith("//"):
        return "https:" + value
    return urllib.parse.urljoin(page_url, value)


def usable_image_url(value: str) -> bool:
    lower = value.lower()
    return lower.startswith("http") and any(
        ext in lower for ext in (".jpg", ".jpeg", ".png", ".webp")
    )


def legacy_society_node(
    project_root: Path, entity_id: str, society_name: str
) -> Optional[Dict[str, Any]]:
    candidates = []
    if entity_id.startswith("society:"):
        candidates.append(entity_id.split(":", 1)[1])
    candidates.append(slug(society_name))
    node_dir = project_root / "data" / "knowledge" / "nodes" / "society"
    for candidate in candidates:
        path = node_dir / "{}.json".format(candidate)
        if not path.exists():
            continue
        try:
            return json.loads(path.read_text())
        except (OSError, ValueError):
            continue
    return None


def partition_values(request: Dict[str, Any]) -> Dict[str, str]:
    partition = request.get("partition", {})
    return {str(key): str(value) for key, value in partition.get("parts", [])}


def normalized_planned_at(request: Dict[str, Any]) -> str:
    value = str(request.get("planned_at") or "").strip()
    return value or datetime.now(timezone.utc).isoformat()


def optional_string(value: Any) -> Optional[str]:
    if value is None:
        return None
    text = str(value).strip()
    return text or None


def optional_int(value: Any) -> Optional[int]:
    if value is None or value == "":
        return None
    try:
        return int(float(value))
    except (TypeError, ValueError):
        return None


def slug(value: str) -> str:
    return re.sub(r"[^a-z0-9]+", "-", str(value or "").lower()).strip("-")
