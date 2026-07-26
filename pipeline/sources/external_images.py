"""Source-neutral external image collection.

The collector emits image observations only. Rust owns the durable Parquet,
KG facts, and later binary derivative assets. MagicBricks is the first source
adapter because those URLs already appear in market-pricing provenance.
"""

import json
import os
import re
import hashlib
import io
import urllib.parse
import urllib.request
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Callable, Dict, Iterable, List, Optional, Tuple

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
READER_PREFIX = "https://r.jina.ai/http://"
PREVIEW_MAX_SIZE = (1280, 960)
PREVIEW_QUALITY = 78
MAX_OPTIMIZED_IMAGES_PER_ENTITY = 8
LOCAL_SOCIETY_PHOTO_EXTENSIONS = ("jpg", "jpeg", "png", "webp")
PROJECT_ROOT = Path(__file__).resolve().parents[2]
DAG_ROOT = PROJECT_ROOT / "app" / "config" / "dag"


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
        "source_watermarks": source_watermarks(deduped, observed_at),
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
    if not source_pages:
        source_pages = magicbricks_source_pages(input_data, society_name)
    ensure_local_society_photos(project_root, input_data, entity_id, society_name)
    records = local_society_photo_records(
        project_root, entity_id, society_name, observed_at
    )
    if len(records) >= local_society_photo_target(project_root):
        return records[:MAX_OPTIMIZED_IMAGES_PER_ENTITY]
    optimized_count = 0
    external_rank_offset = len(records) + 20 if records else 0
    for source_page in source_pages:
        if len(records) >= MAX_OPTIMIZED_IMAGES_PER_ENTITY:
            break
        page_url = required_page_url(source_page)
        if not page_url:
            continue
        html = optional_string(source_page.get("html")) if isinstance(source_page, dict) else None
        if html is None:
            html = fetch_page_text(page_url, optional_string(source_page.get("source_name")))
        if not html:
            continue
        candidates = image_candidates_from_html(html, page_url, society_name)
        for rank, candidate in enumerate(candidates[:8], start=1 + external_rank_offset):
            if len(records) >= MAX_OPTIMIZED_IMAGES_PER_ENTITY:
                break
            optimized = None
            if optimized_count < MAX_OPTIMIZED_IMAGES_PER_ENTITY:
                optimized = optimized_preview_for_candidate(
                    project_root,
                    entity_id,
                    candidate["image_url"],
                    page_url,
                )
            image_url = candidate["image_url"]
            storage_policy = "link_only"
            content_sha256 = None
            width = optional_int(candidate.get("width"))
            height = optional_int(candidate.get("height"))
            if optimized:
                optimized_count += 1
                image_url = optimized["preview_url"]
                storage_policy = "optimized_preview"
                content_sha256 = optimized["content_sha256"]
                width = optimized["width"]
                height = optimized["height"]
            records.append(
                {
                    "entity_id": entity_id,
                    "project_key": optional_string(input_data.get("project_key")),
                    "source_name": optional_string(source_page.get("source_name"))
                    if isinstance(source_page, dict)
                    else "magicbricks",
                    "source_page_url": page_url,
                    "image_url": image_url,
                    "original_image_url": candidate["image_url"],
                    "image_kind": classify_image(candidate["image_url"], candidate.get("alt_text")),
                    "width": width,
                    "height": height,
                    "rank": rank,
                    "score": candidate["score"],
                    "alt_text": optional_string(candidate.get("alt_text")),
                    "storage_policy": storage_policy,
                    "content_sha256": content_sha256,
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


def ensure_local_society_photos(
    project_root: Path, input_data: Dict[str, Any], entity_id: str, society_name: str
) -> None:
    policy = load_project_crawl_policy(project_root, "local_society_photo_collection")
    if not policy or not bool(policy.get("enabled", False)):
        return
    if str(os.environ.get(policy.get("skip_env", "")) or "").lower() in ("1", "true", "yes"):
        return

    target_images = positive_int(policy.get("target_images"), 5)
    photo_dir = project_root / "frontend" / "public" / "societies" / entity_slug(
        entity_id, society_name
    )
    if len(local_society_photo_paths(photo_dir)) >= target_images:
        return

    try:
        from pipeline.skills.fetch_images import fetch_images_for_entity
    except Exception:
        return

    fetch_images_for_entity(
        entity_id=entity_id,
        entity_type="society",
        name=society_name,
        area=optional_string(input_data.get("area")) or "",
        city=optional_string(input_data.get("city")) or "Bangalore",
        serpapi_key=os.environ.get("SERPAPI_API_KEY") or os.environ.get("SERPAPI_KEY"),
        project_root=project_root,
        target_images=target_images,
        force=False,
        dry_run=False,
    )


def local_society_photo_target(project_root: Path) -> int:
    policy = load_project_crawl_policy(project_root, "local_society_photo_collection")
    if not policy:
        return 5
    return positive_int(policy.get("target_images"), 5)


def local_society_photo_records(
    project_root: Path, entity_id: str, society_name: str, observed_at: str
) -> List[Dict[str, Any]]:
    """Promote previously downloaded high-quality society photos into DAG media.

    These files are produced by the legacy fetch_images skill. They are still
    useful as curated local assets while the newer DAG collector backfills
    optimized remote previews.
    """
    photo_dir = project_root / "frontend" / "public" / "societies" / entity_slug(
        entity_id, society_name
    )
    if not photo_dir.exists():
        return []

    records = []
    for rank, path in enumerate(local_society_photo_paths(photo_dir), start=1):
        image_url = public_society_photo_url(photo_dir.name, path.name)
        width, height = image_dimensions(path)
        records.append(
            {
                "entity_id": entity_id,
                "project_key": None,
                "source_name": "LocalSocietyPhotos",
                "source_page_url": image_url,
                "image_url": image_url,
                "original_image_url": image_url,
                "image_kind": classify_image(image_url, path.name),
                "width": width,
                "height": height,
                "rank": rank,
                "score": 100.0 + max(0, 10 - rank),
                "alt_text": "{} photo {}".format(society_name, rank),
                "storage_policy": "static_public_asset",
                "content_sha256": file_sha256(path),
                "observed_at": observed_at,
            }
        )
    return records


def local_society_photo_paths(photo_dir: Path) -> List[Path]:
    if not photo_dir.exists():
        return []
    paths = []
    for path in photo_dir.iterdir():
        if path.is_file() and path.suffix.lower().lstrip(".") in LOCAL_SOCIETY_PHOTO_EXTENSIONS:
            paths.append(path)
    return sorted(paths, key=local_society_photo_sort_key)


def local_society_photo_sort_key(path: Path) -> tuple:
    stem = path.stem
    try:
        return (0, int(stem), path.name)
    except ValueError:
        return (1, stem, path.name)


def public_society_photo_url(slug_value: str, filename: str) -> str:
    return "/societies/{}/{}".format(slug_value, filename)


def image_dimensions(path: Path) -> Tuple[Optional[int], Optional[int]]:
    try:
        from PIL import Image

        with Image.open(path) as image:
            return image.width, image.height
    except Exception:
        pass
    try:
        return image_dimensions_from_bytes(path.read_bytes()[:256_000])
    except OSError:
        return None, None


def image_dimensions_from_bytes(data: bytes) -> Tuple[Optional[int], Optional[int]]:
    if len(data) >= 24 and data.startswith(b"\x89PNG\r\n\x1a\n"):
        return int.from_bytes(data[16:20], "big"), int.from_bytes(data[20:24], "big")
    if len(data) >= 4 and data.startswith(b"\xff\xd8"):
        return jpeg_dimensions(data)
    return None, None


def jpeg_dimensions(data: bytes) -> Tuple[Optional[int], Optional[int]]:
    start_of_frame_markers = set(
        [
            0xC0,
            0xC1,
            0xC2,
            0xC3,
            0xC5,
            0xC6,
            0xC7,
            0xC9,
            0xCA,
            0xCB,
            0xCD,
            0xCE,
            0xCF,
        ]
    )
    index = 2
    while index + 9 < len(data):
        if data[index] != 0xFF:
            index += 1
            continue
        while index < len(data) and data[index] == 0xFF:
            index += 1
        if index >= len(data):
            break
        marker = data[index]
        index += 1
        if marker in (0xD8, 0xD9):
            continue
        if index + 2 > len(data):
            break
        segment_length = int.from_bytes(data[index : index + 2], "big")
        if segment_length < 2:
            break
        if marker in start_of_frame_markers and index + 7 < len(data):
            height = int.from_bytes(data[index + 3 : index + 5], "big")
            width = int.from_bytes(data[index + 5 : index + 7], "big")
            return width, height
        index += segment_length
    return None, None


def file_sha256(path: Path) -> Optional[str]:
    try:
        hasher = hashlib.sha256()
        with path.open("rb") as handle:
            for chunk in iter(lambda: handle.read(1024 * 1024), b""):
                hasher.update(chunk)
        return "sha256:{}".format(hasher.hexdigest())
    except OSError:
        return None


def optimized_preview_for_candidate(
    project_root: Path,
    entity_id: str,
    image_url: str,
    source_page_url: str,
    fetch: Optional[Callable[[str], Optional[bytes]]] = None,
) -> Optional[Dict[str, Any]]:
    if skip_image_optimization() or not image_optimizer_available():
        return None
    image_bytes = (fetch or fetch_image_bytes)(image_url)
    if not image_bytes:
        return None
    return write_optimized_preview(
        project_root=project_root,
        entity_id=entity_id,
        image_url=image_url,
        source_page_url=source_page_url,
        image_bytes=image_bytes,
    )


def image_optimizer_available() -> bool:
    try:
        import PIL  # noqa: F401

        return True
    except Exception:
        return False


def write_optimized_preview(
    project_root: Path,
    entity_id: str,
    image_url: str,
    source_page_url: str,
    image_bytes: bytes,
) -> Optional[Dict[str, Any]]:
    try:
        from PIL import Image, ImageOps
    except Exception:
        return None

    content_sha256 = hashlib.sha256(image_bytes).hexdigest()
    entity_slug = slug(entity_id.split(":", 1)[-1])
    url_slug = slug(urllib.parse.urlparse(image_url).path)[:48] or "image"
    filename = "{}-{}.webp".format(url_slug, content_sha256[:16])
    preview_dir = (
        project_root
        / "data"
        / "lake"
        / "media"
        / "previews"
        / "external_images"
        / entity_slug
    )
    preview_path = preview_dir / filename
    preview_url = "/media/previews/external_images/{}/{}".format(entity_slug, filename)

    try:
        if preview_path.exists():
            with Image.open(preview_path) as existing:
                return {
                    "preview_url": preview_url,
                    "preview_path": str(preview_path),
                    "content_sha256": content_sha256,
                    "width": existing.width,
                    "height": existing.height,
                }

        with Image.open(io.BytesIO(image_bytes)) as image:
            image = ImageOps.exif_transpose(image)
            image.thumbnail(PREVIEW_MAX_SIZE)
            if image.mode not in ("RGB", "RGBA"):
                image = image.convert("RGB")
            preview_dir.mkdir(parents=True, exist_ok=True)
            image.save(preview_path, format="WEBP", quality=PREVIEW_QUALITY, method=6)
            return {
                "preview_url": preview_url,
                "preview_path": str(preview_path),
                "content_sha256": content_sha256,
                "width": image.width,
                "height": image.height,
            }
    except Exception:
        return None


def explicit_source_pages(input_data: Dict[str, Any]) -> List[Dict[str, Any]]:
    pages = input_data.get("image_source_pages")
    if isinstance(pages, list):
        return [page for page in pages if isinstance(page, dict)]
    urls = input_data.get("image_source_urls") or input_data.get(
        "google_project_image_source_urls"
    )
    if isinstance(urls, list):
        return [
            {"source_page_url": url, "source_name": image_source_name(url)}
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


def magicbricks_source_pages(
    input_data: Dict[str, Any], society_name: str
) -> List[Dict[str, Any]]:
    city = magicbricks_city_slug(optional_string(input_data.get("city")))
    project_slug = slug(society_name)
    if not project_slug or not city:
        return []
    return [
        {
            "source_name": "MagicBricks",
            "source_page_url": "https://www.magicbricks.com/project-{}-for-sale-in-{}-pppfs".format(
                project_slug, city
            ),
        },
        {
            "source_name": "GoogleImages",
            "source_page_url": "https://www.google.com/search?tbm=isch&q={}".format(
                urllib.parse.quote_plus("{} {} project photos".format(society_name, city))
            ),
        },
        {
            "source_name": "SquareYards",
            "source_page_url": "https://www.squareyards.com/sale/resale-properties-in-{}-{}".format(
                project_slug, city
            ),
        },
        {
            "source_name": "SquareYards",
            "source_page_url": "https://www.squareyards.com/rent/property-for-rent-in-{}-{}".format(
                project_slug, city
            ),
        },
    ]


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
    if any(
        word in text
        for word in (
            "logo",
            "icon",
            "sprite",
            "favicon",
            "floorplan",
            "profile_pic",
            "profilepic",
            "/employee/",
            "/connect/profile",
        )
    ):
        score -= 40
    if is_low_resolution_derivative_url(image_url):
        score -= 28
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
    if "squareyards" in image_url.lower() and "secondaryportal" in image_url.lower():
        score += 8
    return max(score, 0.0)


def is_low_resolution_derivative_url(image_url: str) -> bool:
    lower = image_url.lower()
    return any(
        marker in lower
        for marker in (
            "aio=w-300",
            "aio=w-320",
            "h310_w462",
            "_310_462",
            "w300_h300",
            "300x300",
        )
    )


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


def source_watermarks(
    records: Iterable[Dict[str, Any]], default_watermark: str
) -> List[Dict[str, str]]:
    watermarks = {}
    for record in records:
        source_name = slug(optional_string(record.get("source_name")) or "external")
        key = "external_image_{}".format(source_name.replace("-", "_"))
        observed_at = optional_string(record.get("observed_at")) or default_watermark
        watermarks[key] = max(watermarks.get(key, default_watermark), observed_at)
    if not watermarks:
        watermarks["external_image_empty"] = default_watermark
    return [
        {"source": source, "high_watermark": watermark}
        for source, watermark in sorted(watermarks.items())
    ]


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


def skip_image_optimization() -> bool:
    explicit_enable = os.environ.get("OPENESTATES_ENABLE_IMAGE_OPTIMIZATION")
    if explicit_enable is not None:
        return str(explicit_enable).lower() not in ("1", "true", "yes")

    explicit_skip = os.environ.get("OPENESTATES_SKIP_IMAGE_OPTIMIZATION")
    if explicit_skip is not None:
        return str(explicit_skip).lower() in ("1", "true", "yes")

    policy = load_crawl_policy("external_image_optimization")
    if policy is not None:
        return not bool(policy.get("enabled", True))

    return True


def load_crawl_policy(policy_id: str) -> Optional[Dict[str, Any]]:
    return load_project_crawl_policy(PROJECT_ROOT, policy_id, fallback_to_default=True)


def load_project_crawl_policy(
    project_root: Path, policy_id: str, fallback_to_default: bool = False
) -> Optional[Dict[str, Any]]:
    project_path = (
        project_root
        / "app"
        / "config"
        / "dag"
        / "crawl_policies"
        / "{}.json".format(policy_id)
    )
    if project_path.exists():
        return read_json_file(project_path)
    if not fallback_to_default:
        return None
    path = DAG_ROOT / "crawl_policies" / "{}.json".format(policy_id)
    if not path.exists():
        return None
    return read_json_file(path)


def read_json_file(path: Path) -> Optional[Dict[str, Any]]:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return None


def fetch_page_text(url: str, source_name: Optional[str]) -> Optional[str]:
    fetch_url = url
    if should_use_reader(source_name, url):
        reader_prefix = os.environ.get("OPENESTATES_IMAGE_READER_PREFIX") or READER_PREFIX
        fetch_url = "{}{}".format(reader_prefix, url)
    try:
        request = urllib.request.Request(fetch_url, headers=HEADERS)
        with urllib.request.urlopen(request, timeout=15) as response:
            return response.read().decode("utf-8", errors="replace")
    except Exception:
        return None


def fetch_image_bytes(url: str) -> Optional[bytes]:
    try:
        request = urllib.request.Request(url, headers=HEADERS)
        with urllib.request.urlopen(request, timeout=15) as response:
            content_type = response.headers.get("Content-Type", "").lower()
            if content_type and not content_type.startswith("image/"):
                return None
            return response.read(8_000_000)
    except Exception:
        return None


def required_page_url(source_page: Any) -> Optional[str]:
    if isinstance(source_page, dict):
        return optional_string(source_page.get("source_page_url") or source_page.get("url"))
    return optional_string(source_page)


def should_use_reader(source_name: Optional[str], url: str) -> bool:
    if str(os.environ.get("OPENESTATES_IMAGE_READER_PREFIX") or "").lower() == "none":
        return False
    marker = "{} {}".format(source_name or "", url).lower()
    return any(source in marker for source in ("magicbricks", "google.com", "squareyards"))


def image_source_name(url: Any) -> str:
    value = optional_string(url) or ""
    lower = value.lower()
    if "google." in lower:
        return "GoogleImages"
    if "squareyards" in lower:
        return "SquareYards"
    return "MagicBricks"


def normalize_image_url(value: str, page_url: str) -> str:
    value = value.strip().replace("\\/", "/")
    if ")" in value:
        value = value.split(")", 1)[0]
    value = re.sub(r"[\)\]\}\.,\*]+$", "", value)
    if value.startswith("//"):
        return "https:" + value
    return urllib.parse.urljoin(page_url, value)


def usable_image_url(value: str) -> bool:
    lower = value.lower()
    if any(
        marker in lower
        for marker in (
            "profile_pic",
            "profilepic",
            "/employee/",
            "/connect/profile",
            "app-store",
            "google-play",
            "qr-code",
            "search-no-result",
            "/ui-assets/",
            "/assets/images/squareyards",
        )
    ):
        return False
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


def entity_slug(entity_id: str, society_name: str) -> str:
    if entity_id.startswith("society:"):
        value = slug(entity_id.split(":", 1)[1])
        if value.startswith("rera-") and society_name:
            return slug(society_name)
        return value
    return slug(society_name)


def partition_values(request: Dict[str, Any]) -> Dict[str, str]:
    partition = request.get("partition", {})
    return {str(key): str(value) for key, value in partition.get("parts", [])}


def normalized_planned_at(request: Dict[str, Any]) -> str:
    value = str(request.get("planned_at") or "").strip()
    return value or datetime.now(timezone.utc).isoformat()


def magicbricks_city_slug(value: Optional[str]) -> str:
    text = slug(value or "bangalore")
    if text in ("bengaluru", "bangaluru"):
        return "bangalore"
    return text or "bangalore"


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


def positive_int(value: Any, default: int) -> int:
    parsed = optional_int(value)
    if parsed is None or parsed <= 0:
        return default
    return parsed


def slug(value: str) -> str:
    return re.sub(r"[^a-z0-9]+", "-", str(value or "").lower()).strip("-")
