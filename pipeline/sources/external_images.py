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
import shutil
import tempfile
import urllib.parse
import urllib.request
import subprocess
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Callable, Dict, Iterable, List, Optional, Set, Tuple

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
DEFAULT_MAX_CANDIDATES_PER_ENTITY = 48
DEFAULT_MAX_OPTIMIZED_IMAGES_PER_ENTITY = 24
DEFAULT_MAX_PROMOTED_GALLERY_FRAMES = 18
LOCAL_SOCIETY_PHOTO_EXTENSIONS = ("jpg", "jpeg", "png", "webp")
PROJECT_ROOT = Path(__file__).resolve().parents[2]
DAG_ROOT = PROJECT_ROOT / "app" / "config" / "dag"
MEDIA_SOURCE_POLICY_ID = "media_source_policy"


def collect_external_images(request: Dict[str, Any]) -> Dict[str, Any]:
    observed_at = normalized_planned_at(request)
    snapshot_date = partition_values(request).get("dt") or observed_at[:10]
    if skip_image_fetch(request):
        return skipped_snapshot(snapshot_date, observed_at)

    project_root = Path(request.get("project_root") or ".").resolve()
    records = []  # type: List[Dict[str, Any]]
    source_health = []  # type: List[Dict[str, Any]]
    for input_data in request.get("source_entities", []):
        entity_records, entity_health = records_for_entity(project_root, input_data, observed_at)
        records.extend(entity_records)
        source_health.extend(entity_health)

    policy = load_project_crawl_policy(project_root, MEDIA_SOURCE_POLICY_ID)
    deduped = curate_gallery_records(dedupe_records(records), policy)
    if not deduped:
        skipped = skipped_snapshot(snapshot_date, observed_at)
        if source_health:
            skipped["source_health"] = source_health
            skipped["media_qa_report"] = media_qa_report([], source_health)
        return skipped

    return {
        "snapshot_date": snapshot_date,
        "records": deduped,
        "max_promoted_gallery_frames": max_promoted_gallery_frames(policy),
        "source_watermarks": source_watermarks(deduped, observed_at),
        "source_health": source_health,
        "media_qa_report": media_qa_report(deduped, source_health),
    }


def records_for_entity(
    project_root: Path, input_data: Dict[str, Any], observed_at: str
) -> Tuple[List[Dict[str, Any]], List[Dict[str, Any]]]:
    entity_id = optional_string(input_data.get("entity_id"))
    society_name = optional_string(
        input_data.get("society_name")
        or input_data.get("name")
        or input_data.get("project_name")
    )
    if not entity_id or not society_name:
        return [], []

    policy = load_project_crawl_policy(project_root, MEDIA_SOURCE_POLICY_ID)
    source_pages = explicit_source_pages(input_data)
    if not source_pages:
        source_pages = magicbricks_pages_from_kg(project_root, entity_id, society_name)
    if not source_pages:
        source_pages = configured_source_pages(project_root, input_data, society_name)
    alias_entity_id = optional_string(input_data.get("alias_entity_id"))
    ensure_local_society_photos(
        project_root, input_data, entity_id, society_name, alias_entity_id
    )
    records = local_society_photo_records(
        project_root,
        entity_id,
        society_name,
        observed_at,
        policy,
        alias_entity_id,
    )
    max_candidates = max_candidates_per_entity(policy)
    max_optimized = max_optimized_images_per_entity(policy)
    optimized_count = 0
    external_rank_offset = len(records) + 20 if records else 0
    source_health = []  # type: List[Dict[str, Any]]
    for source_page in source_pages:
        if len(records) >= max_candidates:
            break
        page_url = required_page_url(source_page)
        if not page_url:
            continue
        html = optional_string(source_page.get("html")) if isinstance(source_page, dict) else None
        if html is None:
            html = fetch_page_text(page_url, optional_string(source_page.get("source_name")))
        if not html:
            source_health.append(
                media_source_health(entity_id, source_page, "fetch_failed", observed_at, 0)
            )
            continue
        candidates = image_candidates_from_html(html, page_url, society_name)
        budget = source_crawl_budget(source_page, 8)
        source_health.append(
            media_source_health(
                entity_id, source_page, "ok", observed_at, min(len(candidates), budget)
            )
        )
        for rank, candidate in enumerate(candidates[:budget], start=1 + external_rank_offset):
            if len(records) >= max_candidates:
                break
            optimized = None
            if optimized_count < max_optimized:
                optimized = optimized_preview_for_candidate(
                    project_root,
                    entity_id,
                    candidate["image_url"],
                    page_url,
                    policy=policy,
                )
            image_url = candidate["image_url"]
            storage_policy = "link_only"
            content_sha256 = None
            width = optional_int(candidate.get("width"))
            height = optional_int(candidate.get("height"))
            if optimized:
                optimized_count += 1
                image_url = optimized["preview_url"]
                storage_policy = "staged_local_asset"
                content_sha256 = optimized["content_sha256"]
                width = optimized["width"]
                height = optimized["height"]
            source_name = (
                optional_string(source_page.get("source_name"))
                if isinstance(source_page, dict)
                else "external"
            )
            source_bucket = source_bucket_for_url(candidate["image_url"]) or source_bucket_for_url(
                page_url
            )
            qa = classify_media_candidate(
                image_url=image_url,
                original_image_url=candidate["image_url"],
                alt_text=optional_string(candidate.get("alt_text")),
                width=width,
                height=height,
                source_name=source_name or "external",
                source_bucket=source_bucket,
                source_page=source_page if isinstance(source_page, dict) else {},
                policy=policy,
                score=candidate["score"],
                content_sha256=content_sha256,
                content_reject_reason=(optimized or {}).get("content_reject_reason"),
            )
            records.append(
                {
                    "entity_id": entity_id,
                    "project_key": optional_string(input_data.get("project_key")),
                    "source_name": source_name or "external",
                    "source_page_url": page_url,
                    "image_url": image_url,
                    "original_image_url": candidate["image_url"],
                    "image_kind": qa["candidate_kind"],
                    "source_bucket": source_bucket,
                    "candidate_kind": qa["candidate_kind"],
                    "quality_score": qa["quality_score"],
                    "relevance_score": qa["relevance_score"],
                    "reject_reason": qa["reject_reason"],
                    "allowed_slots": qa["allowed_slots"],
                    "dedupe_key": qa["dedupe_key"],
                    "classification_method": qa["classification_method"],
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
    return records, source_health


def ensure_local_society_photos(
    project_root: Path,
    input_data: Dict[str, Any],
    entity_id: str,
    society_name: str,
    alias_entity_id: Optional[str] = None,
) -> None:
    policy = load_project_crawl_policy(project_root, "local_society_photo_collection")
    if not policy or not bool(policy.get("enabled", False)):
        return
    if str(os.environ.get(policy.get("skip_env", "")) or "").lower() in ("1", "true", "yes"):
        return

    target_images = positive_int(policy.get("target_images"), 18)
    photo_dir = local_society_photo_dir(
        project_root, entity_id, society_name, alias_entity_id
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


def local_society_photo_records(
    project_root: Path,
    entity_id: str,
    society_name: str,
    observed_at: str,
    policy: Optional[Dict[str, Any]] = None,
    alias_entity_id: Optional[str] = None,
) -> List[Dict[str, Any]]:
    """Promote previously downloaded high-quality society photos into DAG media.

    Collector files are rebuildable staging inputs. The Rust materializer moves
    their bounded delivery copies into immutable content-addressed lake keys.
    """
    photo_dir = local_society_photo_dir(
        project_root, entity_id, society_name, alias_entity_id
    )
    if not photo_dir.exists():
        return []

    records = []
    provenance_by_file = local_society_photo_provenance(project_root, photo_dir.name)
    for rank, path in enumerate(local_society_photo_paths(photo_dir), start=1):
        image_url, storage_policy = local_society_photo_reference(project_root, path)
        provenance = provenance_by_file.get(path.name) or {}
        original_image_url = optional_string(provenance.get("original_image_url")) or image_url
        source_page_url = optional_string(provenance.get("source_page_url")) or image_url
        provenance_source_name = image_source_name(
            original_image_url if original_image_url != image_url else source_page_url
        )
        width, height = image_dimensions(path)
        content_sha256 = file_sha256(path)
        try:
            optimized = write_optimized_preview(
                project_root,
                entity_id,
                image_url,
                source_page_url,
                path.read_bytes(),
                policy=policy,
            )
        except OSError:
            optimized = None
        if optimized:
            image_url = optimized["preview_url"]
            storage_policy = "staged_local_asset"
            content_sha256 = optimized["content_sha256"]
            width = optimized["width"]
            height = optimized["height"]
        qa = classify_media_candidate(
            image_url=image_url,
            original_image_url=original_image_url,
            alt_text=optional_string(provenance.get("title"))
            or "{} photo {}".format(society_name, rank),
            width=width,
            height=height,
            source_name="LocalSocietyPhotos",
            source_bucket="local_society_photo",
            source_page={
                "source_name": provenance_source_name,
                "source_page_url": source_page_url,
            },
            policy=policy,
            score=100.0 + max(0, 10 - rank),
            content_sha256=content_sha256,
            content_reject_reason=(optimized or {}).get("content_reject_reason"),
            declared_kind=normalized_candidate_kind(provenance.get("classification")),
        )
        records.append(
            {
                "entity_id": entity_id,
                "project_key": None,
                "source_name": "LocalSocietyPhotos",
                "source_page_url": source_page_url,
                "image_url": image_url,
                "original_image_url": original_image_url,
                "image_kind": qa["candidate_kind"],
                "source_bucket": "local_society_photo",
                "candidate_kind": qa["candidate_kind"],
                "quality_score": qa["quality_score"],
                "relevance_score": qa["relevance_score"],
                "reject_reason": qa["reject_reason"],
                "allowed_slots": qa["allowed_slots"],
                "dedupe_key": qa["dedupe_key"],
                "classification_method": qa["classification_method"],
                "width": width,
                "height": height,
                "rank": rank,
                "score": 100.0 + max(0, 10 - rank),
                "alt_text": optional_string(provenance.get("title"))
                or "{} photo {}".format(society_name, rank),
                "storage_policy": storage_policy,
                "content_sha256": content_sha256,
                "observed_at": observed_at,
            }
        )
    return records


def local_society_photo_provenance(
    project_root: Path, society_slug: str
) -> Dict[str, Dict[str, Any]]:
    path = project_root / "data" / "cache" / "image_metadata" / "{}.json".format(society_slug)
    metadata = read_json_file(path)
    if not metadata:
        return {}
    by_file = {}
    for source in metadata.get("sources") or []:
        if not isinstance(source, dict):
            continue
        filename = optional_string(source.get("file"))
        if filename:
            by_file[filename] = source
    return by_file


def local_society_photo_paths(photo_dir: Path) -> List[Path]:
    if not photo_dir.exists():
        return []
    paths = []
    for path in photo_dir.iterdir():
        if path.is_file() and path.suffix.lower().lstrip(".") in LOCAL_SOCIETY_PHOTO_EXTENSIONS:
            paths.append(path)
    return sorted(paths, key=local_society_photo_sort_key)


def local_society_photo_dir(
    project_root: Path,
    entity_id: str,
    society_name: str,
    alias_entity_id: Optional[str] = None,
) -> Path:
    staged_dir = project_root / "data" / "cache" / "media_ingest" / "societies"
    named_dir = staged_dir / entity_slug(entity_id, society_name)
    if local_society_photo_paths(named_dir):
        return named_dir

    canonical_slug = slug(entity_id.split(":", 1)[-1])
    canonical_dir = staged_dir / canonical_slug
    if canonical_dir != named_dir and local_society_photo_paths(canonical_dir):
        return canonical_dir

    if alias_entity_id:
        alias_dir = staged_dir / slug(alias_entity_id.split(":", 1)[-1])
        if local_society_photo_paths(alias_dir):
            return alias_dir
    return named_dir


def local_society_photo_sort_key(path: Path) -> tuple:
    stem = path.stem
    try:
        return (0, int(stem), path.name)
    except ValueError:
        return (1, stem, path.name)


def local_society_photo_reference(project_root: Path, path: Path) -> Tuple[str, str]:
    staged_root = project_root / "data" / "cache" / "media_ingest"
    relative = path.relative_to(staged_root)
    return "/_staged_media/{}".format(relative.as_posix()), "staged_local_asset"


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
    policy: Optional[Dict[str, Any]] = None,
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
        policy=policy,
    )


def image_optimizer_available() -> bool:
    try:
        import PIL  # noqa: F401

        return True
    except Exception:
        return shutil.which("sips") is not None


def write_optimized_preview(
    project_root: Path,
    entity_id: str,
    image_url: str,
    source_page_url: str,
    image_bytes: bytes,
    policy: Optional[Dict[str, Any]] = None,
) -> Optional[Dict[str, Any]]:
    try:
        from PIL import Image, ImageFilter, ImageOps, ImageStat
    except Exception:
        return write_sips_preview(project_root, entity_id, image_url, image_bytes)

    entity_slug = slug(entity_id.split(":", 1)[-1])
    url_slug = slug(urllib.parse.urlparse(image_url).path)[:48] or "image"
    preview_dir = (
        project_root
        / "data"
        / "cache"
        / "media_ingest"
        / "external_images"
        / entity_slug
    )

    try:
        with Image.open(io.BytesIO(image_bytes)) as image:
            image = ImageOps.exif_transpose(image)
            image.thumbnail(PREVIEW_MAX_SIZE)
            if image.mode not in ("RGB", "RGBA"):
                image = image.convert("RGB")
            grayscale = image.convert("L")
            grayscale.thumbnail((256, 256))
            edges = grayscale.filter(ImageFilter.FIND_EDGES)
            if edges.width > 4 and edges.height > 4:
                edges = edges.crop((2, 2, edges.width - 2, edges.height - 2))
            edge_variance = float(ImageStat.Stat(edges).var[0])
            blur_threshold = float(
                ((policy or {}).get("classification") or {}).get(
                    "blur_edge_variance_min", 8.0
                )
            )
            output = io.BytesIO()
            image.save(output, format="WEBP", quality=PREVIEW_QUALITY, method=6)
            encoded = output.getvalue()
            content_sha256 = hashlib.sha256(encoded).hexdigest()
            filename = "{}-{}.webp".format(url_slug, content_sha256[:16])
            preview_path = preview_dir / filename
            preview_url = "/_staged_media/external_images/{}/{}".format(
                entity_slug, filename
            )
            preview_dir.mkdir(parents=True, exist_ok=True)
            if not preview_path.exists():
                preview_path.write_bytes(encoded)
            return {
                "preview_url": preview_url,
                "preview_path": str(preview_path),
                "content_sha256": content_sha256,
                "width": image.width,
                "height": image.height,
                "content_reject_reason": (
                    "blur_or_low_detail" if edge_variance < blur_threshold else None
                ),
            }
    except Exception:
        return None


def write_sips_preview(
    project_root: Path,
    entity_id: str,
    image_url: str,
    image_bytes: bytes,
) -> Optional[Dict[str, Any]]:
    """Write a browser-safe JPEG when Pillow is unavailable on macOS.

    Apple ImageIO can emit AVIF files that its own decoders accept while
    Chromium exposes dimensions but renders transparent pixels. JPEG is the
    conservative fallback here; the normal Pillow path continues to emit WebP.
    """
    if shutil.which("sips") is None:
        return None
    entity_slug = slug(entity_id.split(":", 1)[-1])
    url_slug = slug(urllib.parse.urlparse(image_url).path)[:48] or "image"
    try:
        with tempfile.TemporaryDirectory(prefix="openestates-media-") as temp_dir:
            input_path = Path(temp_dir) / "input-image"
            output_path = Path(temp_dir) / "output.jpg"
            input_path.write_bytes(image_bytes)
            subprocess.run(
                [
                    "sips",
                    "-s",
                    "format",
                    "jpeg",
                    "-s",
                    "formatOptions",
                    "78",
                    "--resampleHeightWidthMax",
                    "1280",
                    str(input_path),
                    "--out",
                    str(output_path),
                ],
                check=True,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                timeout=60,
            )
            encoded = output_path.read_bytes()
            dimensions = subprocess.run(
                ["sips", "-g", "pixelWidth", "-g", "pixelHeight", str(output_path)],
                check=True,
                capture_output=True,
                text=True,
                timeout=10,
            ).stdout
        width_match = re.search(r"pixelWidth:\s*(\d+)", dimensions)
        height_match = re.search(r"pixelHeight:\s*(\d+)", dimensions)
        if not width_match or not height_match:
            return None
        content_sha256 = hashlib.sha256(encoded).hexdigest()
        filename = "{}-{}.jpg".format(url_slug, content_sha256[:16])
        preview_dir = (
            project_root
            / "data"
            / "cache"
            / "media_ingest"
            / "external_images"
            / entity_slug
        )
        preview_path = preview_dir / filename
        preview_dir.mkdir(parents=True, exist_ok=True)
        if not preview_path.exists():
            preview_path.write_bytes(encoded)
        return {
            "preview_url": "/_staged_media/external_images/{}/{}".format(
                entity_slug, filename
            ),
            "preview_path": str(preview_path),
            "content_sha256": content_sha256,
            "width": int(width_match.group(1)),
            "height": int(height_match.group(1)),
        }
    except (OSError, subprocess.SubprocessError):
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


def configured_source_pages(
    project_root: Path, input_data: Dict[str, Any], society_name: str
) -> List[Dict[str, Any]]:
    policy = load_project_crawl_policy(project_root, MEDIA_SOURCE_POLICY_ID)
    if not policy or not bool(policy.get("enabled", True)):
        return magicbricks_source_pages(input_data, society_name)

    city = magicbricks_city_slug(optional_string(input_data.get("city")))
    project_slug = slug(society_name)
    pages = []  # type: List[Dict[str, Any]]
    for source in sorted(
        policy.get("sources") or [],
        key=lambda item: -positive_int(item.get("priority"), 0),
    ):
        if not bool(source.get("enabled", True)):
            continue
        for template in source.get("url_templates") or []:
            if not project_slug or not city:
                continue
            url = str(template).format(project_slug=project_slug, city_slug=city)
            pages.append(
                {
                    "source_id": optional_string(source.get("id")),
                    "source_name": optional_string(source.get("source_name"))
                    or optional_string(source.get("id"))
                    or image_source_name(url),
                    "source_page_url": url,
                    "priority": positive_int(source.get("priority"), 0),
                    "enabled": bool(source.get("enabled", True)),
                    "crawl_budget_per_run": positive_int(
                        source.get("crawl_budget_per_run"), 8
                    ),
                    "min_interval_hours": positive_int(
                        source.get("min_interval_hours"), 168
                    ),
                    "backoff_on_block_hours": positive_int(
                        source.get("backoff_on_block_hours"), 336
                    ),
                    "trust_profile": optional_string(source.get("trust_profile")),
                    "path_kind_rules": source.get("path_kind_rules") or {},
                    "reject_url_patterns": source.get("reject_url_patterns") or [],
                }
            )
    if not pages:
        return magicbricks_source_pages(input_data, society_name)
    return pages


def source_crawl_budget(source_page: Any, default: int) -> int:
    if not isinstance(source_page, dict):
        return default
    return min(positive_int(source_page.get("crawl_budget_per_run"), default), 24)


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
    return deterministic_candidate_kind(
        image_url=image_url,
        alt_text=alt_text,
        source_bucket=None,
        source_page={},
        policy=None,
    )


def classify_media_candidate(
    image_url: str,
    original_image_url: str,
    alt_text: Optional[str],
    width: Optional[int],
    height: Optional[int],
    source_name: str,
    source_bucket: Optional[str],
    source_page: Dict[str, Any],
    policy: Optional[Dict[str, Any]],
    score: float,
    content_sha256: Optional[str] = None,
    content_reject_reason: Optional[str] = None,
    declared_kind: Optional[str] = None,
) -> Dict[str, Any]:
    kind = normalized_candidate_kind(declared_kind) or deterministic_candidate_kind(
        image_url=original_image_url or image_url,
        alt_text=alt_text,
        source_bucket=source_bucket,
        source_page=source_page,
        policy=policy,
    )
    classification_method = "fetch_images_heuristic" if declared_kind else "heuristic"
    if should_run_vision(kind, policy):
        vision = vision_classification(
            image_url=image_url,
            original_image_url=original_image_url,
            alt_text=alt_text,
            source_name=source_name,
            source_bucket=source_bucket,
            width=width,
            height=height,
            policy=policy,
        )
        if vision:
            kind = optional_string(vision.get("candidate_kind")) or kind
            classification_method = "vision"
        else:
            classification_method = "heuristic_only"

    reject_reason = reject_reason_for(
        kind=kind,
        image_url=original_image_url or image_url,
        alt_text=alt_text,
        width=width,
        height=height,
        source_name=source_name,
        source_page=source_page,
        policy=policy,
        content_sha256=content_sha256,
        content_reject_reason=content_reject_reason,
    )
    allowed_slots = allowed_slots_for(kind, width, reject_reason, policy)
    quality_score = media_quality_score(kind, width, height, source_name, reject_reason, score)
    relevance_score = media_relevance_score(kind, source_name, source_bucket, reject_reason, score)
    return {
        "candidate_kind": kind,
        "quality_score": round(quality_score, 4),
        "relevance_score": round(relevance_score, 4),
        "reject_reason": reject_reason,
        "allowed_slots": allowed_slots,
        "dedupe_key": dedupe_key_for(
            original_image_url or image_url, image_url, content_sha256
        ),
        "classification_method": classification_method,
    }


def deterministic_candidate_kind(
    image_url: str,
    alt_text: Optional[str],
    source_bucket: Optional[str],
    source_page: Dict[str, Any],
    policy: Optional[Dict[str, Any]],
) -> str:
    decoded_url = urllib.parse.unquote(image_url or "")
    text = "{} {} {}".format(decoded_url, alt_text or "", source_bucket or "").lower()
    source_rules = {}
    if isinstance(source_page, dict):
        source_rules.update(source_page.get("path_kind_rules") or {})
    source_rules.update(policy_path_kind_rules(policy, source_page, image_url))
    for marker, kind in source_rules.items():
        if str(marker).lower() in text:
            return normalized_candidate_kind(kind) or "unknown"
    if any(word in text for word in ("collage", "montage", "contact sheet", "contact-sheet")):
        return "collage"
    if any(word in text for word in ("floorplan", "floor-plan", "floor_plan", "bhk_configuration")):
        return "floor_plan"
    if any(word in text for word in ("master plan", "master%20plan", "site plan", "layout plan")):
        return "site_plan"
    if any(word in text for word in ("location map", "route map", "map screenshot", "nearby map")):
        return "location_context"
    if any(word in text for word in ("neighbourhood", "neighborhood", "approach road", "street view", "surroundings")):
        return "neighbourhood"
    if any(word in text for word in ("amenity", "clubhouse", "pool", "gym", "garden", "landscape")):
        return "amenity"
    if any(word in text for word in ("entrance", "gate")):
        return "entrance"
    if any(word in text for word in ("elevation", "tower", "building", "podium", "facade")):
        return "building"
    if any(word in text for word in ("exterior", "aerial", "project image")):
        return "exterior"
    if any(word in text for word in ("bedroom", "bathroom", "kitchen", "living-room", "living room")):
        return "interior_room"
    if any(word in text for word in ("logo", "favicon", "sprite")):
        return "logo"
    if any(word in text for word in ("stock", "shutterstock", "getty", "representation")):
        return "stock_or_marketing"
    if "localsocietyphotos" in text or source_bucket == "local_society_photo":
        return "exterior"
    return "unknown"


def normalized_candidate_kind(value: Any) -> Optional[str]:
    kind = optional_string(value)
    if not kind:
        return None
    aliases = {
        "amenities": "amenity",
        "master_plan": "site_plan",
        "interior": "interior_room",
        "neighborhood": "neighbourhood",
    }
    return aliases.get(kind.lower(), kind.lower())


def policy_path_kind_rules(
    policy: Optional[Dict[str, Any]], source_page: Dict[str, Any], image_url: str
) -> Dict[str, str]:
    if not policy:
        return {}
    source_name = optional_string(source_page.get("source_name")) or image_source_name(image_url)
    source_id = optional_string(source_page.get("source_id"))
    rules = {}
    for source in policy.get("sources") or []:
        if source_id and source_id == optional_string(source.get("id")):
            rules.update(source.get("path_kind_rules") or {})
        elif source_name and source_name.lower() == (
            optional_string(source.get("source_name")) or ""
        ).lower():
            rules.update(source.get("path_kind_rules") or {})
    return rules


def reject_reason_for(
    kind: str,
    image_url: str,
    alt_text: Optional[str],
    width: Optional[int],
    height: Optional[int],
    source_name: str,
    source_page: Dict[str, Any],
    policy: Optional[Dict[str, Any]],
    content_sha256: Optional[str] = None,
    content_reject_reason: Optional[str] = None,
) -> Optional[str]:
    if content_reject_reason:
        return "content:{}".format(content_reject_reason)
    lower = "{} {} {}".format(image_url or "", alt_text or "", source_name or "").lower()
    content_reject = watermark_content_reject_reason(policy, content_sha256)
    if content_reject:
        return content_reject
    watermark_reject = watermark_reject_reason(
        policy=policy,
        source_page=source_page,
        lower=lower,
        source_name=source_name,
    )
    if watermark_reject:
        return watermark_reject
    for pattern in reject_url_patterns(policy, source_page):
        if str(pattern).lower() in lower:
            return "reject_pattern:{}".format(pattern)
    if kind in reject_kinds(policy):
        return "kind:{}".format(kind)
    if width is not None and height is not None:
        if width < 240 or height < 160:
            return "too_small"
        ratio = float(width) / float(height or 1)
        if ratio < 0.35 or ratio > 4.5:
            return "bad_aspect_ratio"
    return None


def allowed_slots_for(
    kind: str, width: Optional[int], reject_reason: Optional[str], policy: Optional[Dict[str, Any]]
) -> List[str]:
    if reject_reason:
        return []
    slots = []  # type: List[str]
    slot_policy = (policy or {}).get("promotion_slots") or default_promotion_slots()
    if kind in slot_policy.get("hero", []) and width is not None and width >= hero_min_width(policy):
        slots.append("hero")
    if kind in slot_policy.get("gallery", []) and (
        width is None or width >= gallery_min_width(policy)
    ):
        slots.append("gallery")
    if kind in slot_policy.get("floor_plan", []):
        slots.append("floor_plan")
    if kind in slot_policy.get("site_plan", []):
        slots.append("site_plan")
    if kind in slot_policy.get("location", []):
        slots.append("location")
    return slots


def default_promotion_slots() -> Dict[str, List[str]]:
    return {
        "hero": ["exterior", "building", "tower", "entrance"],
        "gallery": [
            "exterior",
            "building",
            "tower",
            "entrance",
            "amenity",
            "neighbourhood",
        ],
        "floor_plan": ["floor_plan"],
        "site_plan": ["site_plan"],
        "location": ["location_context"],
    }


def media_quality_score(
    kind: str,
    width: Optional[int],
    height: Optional[int],
    source_name: str,
    reject_reason: Optional[str],
    source_score: float,
) -> float:
    if reject_reason:
        return 0.0
    score = 0.35 + min(max(source_score, 0.0), 100.0) / 300.0
    if width is not None and height is not None:
        if width >= 1200:
            score += 0.18
        elif width >= 900:
            score += 0.12
        elif width >= 600:
            score += 0.06
    if kind in ("exterior", "tower", "entrance"):
        score += 0.12
    elif kind == "amenity":
        score += 0.08
    elif kind in ("floor_plan", "site_plan"):
        score += 0.1
    if source_name.lower() in ("localsocietyphotos", "googleplacephotos", "builderofficial"):
        score += 0.08
    return max(0.0, min(score, 1.0))


def media_relevance_score(
    kind: str,
    source_name: str,
    source_bucket: Optional[str],
    reject_reason: Optional[str],
    source_score: float,
) -> float:
    if reject_reason:
        return 0.0
    score = 0.4 + min(max(source_score, 0.0), 100.0) / 350.0
    if kind != "unknown":
        score += 0.15
    if source_bucket:
        score += 0.08
    if source_name.lower() in ("houssed", "builderofficial", "googleplacephotos", "reradocuments"):
        score += 0.06
    return max(0.0, min(score, 1.0))


def should_run_vision(kind: str, policy: Optional[Dict[str, Any]]) -> bool:
    command = optional_string(os.environ.get(vision_command_env(policy)))
    if not command:
        return False
    ambiguous = (policy or {}).get("classification", {}).get("ambiguous_kinds") or [
        "unknown"
    ]
    return kind in ambiguous


def vision_classification(
    image_url: str,
    original_image_url: str,
    alt_text: Optional[str],
    source_name: str,
    source_bucket: Optional[str],
    width: Optional[int],
    height: Optional[int],
    policy: Optional[Dict[str, Any]],
) -> Optional[Dict[str, Any]]:
    command = optional_string(os.environ.get(vision_command_env(policy)))
    if not command:
        return None
    payload = json.dumps(
        {
            "image_url": image_url,
            "original_image_url": original_image_url,
            "alt_text": alt_text,
            "source_name": source_name,
            "source_bucket": source_bucket,
            "width": width,
            "height": height,
        },
        separators=(",", ":"),
    )
    try:
        result = subprocess.run(
            command,
            input=payload,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            universal_newlines=True,
            shell=True,
            timeout=positive_int(
                (policy or {}).get("classification", {}).get("vision_timeout_seconds"), 20
            ),
        )
        if result.returncode != 0:
            return None
        decoded = json.loads(result.stdout)
        return decoded if isinstance(decoded, dict) else None
    except Exception:
        return None


def source_bucket_for_url(url: str) -> Optional[str]:
    decoded = urllib.parse.unquote(url or "")
    parts = [part for part in urllib.parse.urlparse(decoded).path.split("/") if part]
    known = (
        "Project Image",
        "Amenities",
        "BHK_Configuration",
        "Master Plan",
        "Location",
    )
    for part in parts:
        if part in known:
            return part
    return None


def reject_url_patterns(
    policy: Optional[Dict[str, Any]], source_page: Optional[Dict[str, Any]] = None
) -> List[str]:
    patterns = []  # type: List[str]
    if policy:
        patterns.extend(policy.get("global_reject_url_patterns") or [])
    source_page = source_page or {}
    patterns.extend(source_page.get("reject_url_patterns") or [])
    source_id = optional_string(source_page.get("source_id"))
    source_name = optional_string(source_page.get("source_name"))
    if policy:
        for source in policy.get("sources") or []:
            if source_id and source_id == optional_string(source.get("id")):
                patterns.extend(source.get("reject_url_patterns") or [])
            elif source_name and source_name.lower() == (
                optional_string(source.get("source_name")) or ""
            ).lower():
                patterns.extend(source.get("reject_url_patterns") or [])
    return patterns


def watermark_reject_reason(
    policy: Optional[Dict[str, Any]],
    source_page: Optional[Dict[str, Any]],
    lower: str,
    source_name: str,
) -> Optional[str]:
    classification = (policy or {}).get("classification") or {}
    source_names = set(
        str(name).strip().lower()
        for name in classification.get("watermark_source_rejects") or []
        if str(name).strip()
    )
    page_source_name = optional_string((source_page or {}).get("source_name"))
    for name in (source_name, page_source_name):
        normalized = (name or "").strip().lower()
        if normalized and normalized in source_names:
            return "watermark:{}".format(normalized)
    for pattern in classification.get("watermark_reject_patterns") or []:
        marker = str(pattern).strip().lower()
        if marker and marker in lower:
            return "watermark:{}".format(marker.replace(" ", "_"))
    return None


def watermark_content_reject_reason(
    policy: Optional[Dict[str, Any]], content_sha256: Optional[str]
) -> Optional[str]:
    if not content_sha256:
        return None
    rejected = set(
        str(value).strip().lower()
        for value in ((policy or {}).get("classification") or {}).get(
            "rejected_content_sha256"
        )
        or []
        if str(value).strip()
    )
    digest = str(content_sha256).strip().lower()
    if digest in rejected or "sha256:{}".format(digest) in rejected:
        return "watermark:content_sha256"
    return None


def reject_kinds(policy: Optional[Dict[str, Any]]) -> Set[str]:
    if not policy:
        return {"logo", "thumbnail", "stock_or_marketing", "interior_room", "bad_crop", "qr"}
    return set(str(kind) for kind in policy.get("reject_kinds") or [])


def hero_min_width(policy: Optional[Dict[str, Any]]) -> int:
    return positive_int((policy or {}).get("classification", {}).get("hero_min_width"), 900)


def gallery_min_width(policy: Optional[Dict[str, Any]]) -> int:
    return positive_int((policy or {}).get("classification", {}).get("gallery_min_width"), 600)


def vision_command_env(policy: Optional[Dict[str, Any]]) -> str:
    return (
        optional_string((policy or {}).get("classification", {}).get("vision_command_env"))
        or "OPENESTATES_MEDIA_VISION_COMMAND"
    )


def dedupe_key_for(
    original_image_url: str, image_url: str, content_sha256: Optional[str] = None
) -> str:
    if content_sha256:
        digest = str(content_sha256).lower().removeprefix("sha256:")
        return "sha256:{}".format(digest)
    source = original_image_url or image_url
    parsed = urllib.parse.urlparse(source)
    normalized = urllib.parse.urlunparse(
        (parsed.scheme.lower(), parsed.netloc.lower(), parsed.path, "", "", "")
    )
    return "url:{}".format(normalized)


def dedupe_records(records: Iterable[Dict[str, Any]]) -> List[Dict[str, Any]]:
    deduped = []
    seen = set()
    for record in records:
        key = (record["entity_id"], record.get("dedupe_key") or record["image_url"])
        if key in seen:
            continue
        seen.add(key)
        deduped.append(record)
    return deduped


def curate_gallery_records(
    records: List[Dict[str, Any]], policy: Optional[Dict[str, Any]]
) -> List[Dict[str, Any]]:
    """Assign deterministic role-aware order to approved walkable frames."""
    by_entity = {}  # type: Dict[str, List[Dict[str, Any]]]
    for record in records:
        record.pop("gallery_order", None)
        record.pop("curation_confidence", None)
        by_entity.setdefault(record["entity_id"], []).append(record)

    max_frames = max_promoted_gallery_frames(policy)
    ordered_kinds = gallery_kind_order(policy)
    hero_kinds = hero_kind_order(policy)

    for entity_records in by_entity.values():
        eligible = [
            record
            for record in entity_records
            if not record.get("reject_reason")
            and "gallery" in (record.get("allowed_slots") or [])
        ]
        hero_candidates = [
            record for record in eligible if "hero" in (record.get("allowed_slots") or [])
        ]
        hero = None
        for kind in hero_kinds:
            candidates = [
                record
                for record in hero_candidates
                if normalized_candidate_kind(record.get("candidate_kind")) == kind
            ]
            if candidates:
                hero = min(candidates, key=gallery_quality_sort_key)
                break
        if hero is None and hero_candidates:
            hero = min(hero_candidates, key=gallery_quality_sort_key)
        ordered = [hero] if hero is not None else []
        for kind in ordered_kinds:
            candidates = [
                record
                for record in eligible
                if record not in ordered
                and normalized_candidate_kind(record.get("candidate_kind")) == kind
            ]
            if candidates:
                ordered.append(min(candidates, key=gallery_quality_sort_key))
        remaining = [record for record in eligible if record not in ordered]
        remaining.sort(key=gallery_quality_sort_key)
        ordered.extend(remaining)
        selected = ordered[:max_frames]
        selected_ids = {id(record) for record in selected}

        for record in entity_records:
            slots = list(record.get("allowed_slots") or [])
            if id(record) not in selected_ids:
                slots = [slot for slot in slots if slot != "gallery"]
            if record is not hero:
                slots = [slot for slot in slots if slot != "hero"]
            record["allowed_slots"] = slots
        for order, record in enumerate(selected):
            record["gallery_order"] = order
            record["curation_confidence"] = round(
                min(
                    float(record.get("quality_score") or 0.0),
                    float(record.get("relevance_score") or 0.0),
                ),
                4,
            )
    return records


def gallery_quality_sort_key(record: Dict[str, Any]) -> tuple:
    return (
        -float(record.get("quality_score") or 0.0),
        -float(record.get("relevance_score") or 0.0),
        int(record.get("rank") or 999999),
        str(record.get("image_url") or ""),
    )


def collection_policy(policy: Optional[Dict[str, Any]]) -> Dict[str, Any]:
    return (policy or {}).get("collection") or {}


def max_candidates_per_entity(policy: Optional[Dict[str, Any]]) -> int:
    return positive_int(
        collection_policy(policy).get("max_candidates_per_entity"),
        DEFAULT_MAX_CANDIDATES_PER_ENTITY,
    )


def max_optimized_images_per_entity(policy: Optional[Dict[str, Any]]) -> int:
    return positive_int(
        collection_policy(policy).get("max_optimized_images_per_entity"),
        DEFAULT_MAX_OPTIMIZED_IMAGES_PER_ENTITY,
    )


def max_promoted_gallery_frames(policy: Optional[Dict[str, Any]]) -> int:
    return positive_int(
        collection_policy(policy).get("max_promoted_gallery_frames"),
        DEFAULT_MAX_PROMOTED_GALLERY_FRAMES,
    )


def gallery_kind_order(policy: Optional[Dict[str, Any]]) -> List[str]:
    configured = collection_policy(policy).get("gallery_kind_order") or []
    normalized = [normalized_candidate_kind(kind) for kind in configured]
    return [kind for kind in normalized if kind] or [
        "building",
        "amenity",
        "neighbourhood",
        "exterior",
        "tower",
        "entrance",
    ]


def hero_kind_order(policy: Optional[Dict[str, Any]]) -> List[str]:
    configured = collection_policy(policy).get("hero_kind_order") or []
    normalized = [normalized_candidate_kind(kind) for kind in configured]
    return [kind for kind in normalized if kind] or [
        "exterior",
        "tower",
        "entrance",
        "building",
    ]


def media_source_health(
    entity_id: str,
    source_page: Any,
    status: str,
    observed_at: str,
    candidate_count: int,
) -> Dict[str, Any]:
    if not isinstance(source_page, dict):
        source_page = {"source_page_url": optional_string(source_page)}
    return {
        "entity_id": entity_id,
        "source_id": optional_string(source_page.get("source_id")),
        "enabled": bool(source_page.get("enabled", True)),
        "priority": optional_int(source_page.get("priority")),
        "crawl_budget": optional_int(source_page.get("crawl_budget_per_run")),
        "min_interval_hours": optional_int(source_page.get("min_interval_hours")),
        "backoff_on_block_hours": optional_int(source_page.get("backoff_on_block_hours")),
        "source_name": optional_string(source_page.get("source_name"))
        or image_source_name(source_page.get("source_page_url")),
        "source_page_url": required_page_url(source_page),
        "status": status,
        "last_status": status,
        "last_success_at": observed_at if status == "ok" else None,
        "candidate_count": candidate_count,
        "observed_at": observed_at,
    }


def media_qa_report(
    records: List[Dict[str, Any]], source_health: List[Dict[str, Any]]
) -> Dict[str, Any]:
    by_entity = {}  # type: Dict[str, Dict[str, Any]]
    for record in records:
        entity = by_entity.setdefault(
            record["entity_id"],
            {
                "candidate_count": 0,
                "approved_count": 0,
                "rejected_count": 0,
                "by_source": {},
                "by_kind": {},
                "selected": {
                    "hero": None,
                    "gallery": [],
                    "floor_plan": [],
                    "site_plan": [],
                    "location": [],
                },
                "validation_failures": [],
            },
        )
        entity["candidate_count"] += 1
        source = optional_string(record.get("source_name")) or "external"
        entity["by_source"][source] = entity["by_source"].get(source, 0) + 1
        kind = optional_string(record.get("candidate_kind") or record.get("image_kind")) or "unknown"
        entity["by_kind"][kind] = entity["by_kind"].get(kind, 0) + 1
        if record.get("reject_reason"):
            entity["rejected_count"] += 1
            continue
        entity["approved_count"] += 1
        for slot in record.get("allowed_slots") or []:
            if slot == "hero" and entity["selected"]["hero"] is None:
                entity["selected"]["hero"] = record.get("image_url")
            elif slot in entity["selected"] and slot != "hero":
                entity["selected"][slot].append(record.get("image_url"))
    for entity_id, entity in by_entity.items():
        if not entity["selected"]["hero"]:
            entity["validation_failures"].append("missing_approved_hero")
    health_by_entity = {}  # type: Dict[str, List[Dict[str, Any]]]
    for health in source_health:
        health_by_entity.setdefault(health.get("entity_id") or "unknown", []).append(health)
    return {
        "entities": by_entity,
        "source_health": health_by_entity,
    }


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
        return optional_string(
            source_page.get("source_page_url")
            or source_page.get("source_url")
            or source_page.get("url")
        )
    return optional_string(source_page)


def should_use_reader(source_name: Optional[str], url: str) -> bool:
    if str(os.environ.get("OPENESTATES_IMAGE_READER_PREFIX") or "").lower() == "none":
        return False
    marker = "{} {}".format(source_name or "", url).lower()
    return any(source in marker for source in ("magicbricks", "google.com", "squareyards"))


def image_source_name(url: Any) -> str:
    value = optional_string(url) or ""
    lower = value.lower()
    if "houssed" in lower:
        return "Houssed"
    if "housing.com" in lower:
        return "Housing"
    if "99acres" in lower:
        return "99acres"
    if "nobroker" in lower:
        return "NoBroker"
    if "squareyards" in lower:
        return "SquareYards"
    if "magicbricks" in lower or "staticmb" in lower:
        return "MagicBricks"
    if "godrejproperties" in lower:
        return "BuilderOfficial"
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
