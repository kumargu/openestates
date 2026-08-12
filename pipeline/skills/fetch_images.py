"""
FetchImages skill — find and download real photos for societies/properties.

Image source priority:
1. SerpAPI Google Image Search (if SERPAPI_API_KEY set) — best quality
2. DuckDuckGo Image Search (free, no key) — fallback

Features:
- Candidate scoring: prefers official sources, landscape, high-res, exterior shots
- Image type classification: exterior, interior, amenities, master_plan, floor_plan, unknown
- SerpAPI response caching to data/cache/serpapi/
- Provenance metadata per image
- Downloads to the rebuildable data/cache/media_ingest staging area. The Rust
  media materializer promotes selected bytes into the content-addressed lake.

Usage:
  python3 -m pipeline.skills.fetch_images                    # all societies
  python3 -m pipeline.skills.fetch_images --slug prestige_lakeside_habitat
  python3 -m pipeline.skills.fetch_images --force             # re-fetch even if photos exist
  python3 -m pipeline.skills.fetch_images --dry-run           # show candidates, don't download
"""

import hashlib
import json
import logging
import os
import re
import struct
import time
import urllib.parse
import urllib.request
from dataclasses import dataclass, field
from datetime import datetime, timezone
from pathlib import Path
from typing import List, Optional, Tuple

from pipeline.skills.base import BaseSkill, SkillCost, SkillResult

logger = logging.getLogger(__name__)

PROJECT_ROOT = Path(__file__).resolve().parent.parent.parent
PHOTOS_DIR = PROJECT_ROOT / "data" / "cache" / "media_ingest" / "societies"
SERPAPI_CACHE_DIR = PROJECT_ROOT / "data" / "cache" / "serpapi" / "image_search"
DDG_CACHE_DIR = PROJECT_ROOT / "data" / "cache" / "ddg" / "image_search"
METADATA_DIR = PROJECT_ROOT / "data" / "cache" / "image_metadata"
SEED_DIR = PROJECT_ROOT / "data" / "seed"

HEADERS = {
    "User-Agent": (
        "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) "
        "AppleWebKit/537.36 (KHTML, like Gecko) "
        "Chrome/122.0.0.0 Safari/537.36"
    ),
    "Accept": "text/html,application/xhtml+xml,application/xml;q=0.9,image/webp,*/*;q=0.8",
    "Accept-Language": "en-US,en;q=0.5",
}

# How many images to keep per society
TARGET_IMAGES = 5
# Minimum image size in bytes (skip tiny icons/spacers)
MIN_IMAGE_BYTES = 10_000
# Maximum image size in bytes
MAX_IMAGE_BYTES = 15_000_000

# Domains that are likely official or high-quality property sources
TRUSTED_DOMAINS = [
    "housing.com", "99acres.com", "magicbricks.com", "nobroker.in",
    "squareyards.com", "commonfloor.com", "proptiger.com",
    "sobha.com", "prestigeconstructions.com", "brigadegroup.com",
    "godrejproperties.com", "puravankara.com", "salarpuria.com",
    "embassy-group.com", "tottenvironment.com", "vaswanigroup.com",
    "roofandfloor.com", "nestoria.in",
]

# Domains to avoid
BLOCKED_DOMAINS = [
    "facebook.com", "twitter.com", "instagram.com", "youtube.com",
    "pinterest.com", "linkedin.com", "tiktok.com", "reddit.com",
    "wikipedia.org", "wikimedia.org",
    "shutterstock.com", "gettyimages.com", "istockphoto.com",
    "dreamstime.com", "123rf.com", "alamy.com",  # stock photo sites
]

# Keywords that suggest the image is NOT a useful property photo
NEGATIVE_URL_KEYWORDS = [
    "logo", "icon", "favicon", "avatar", "banner", "ad-",
    "advertisement", "sponsor", "thumbnail", "thumb_",
    "pixel", "tracking", "analytics",
    "floorplan", "floor-plan", "floor_plan",
    "sitemap", "site-map", "site_map",
    "brochure", "pdf", "document",
]

# Keywords that suggest the image IS a good property photo
POSITIVE_URL_KEYWORDS = [
    "exterior", "aerial", "facade", "elevation", "building",
    "clubhouse", "pool", "swimming", "garden", "landscape",
    "amenity", "amenities", "community", "entrance", "gate",
    "apartment", "tower", "complex", "society", "residential",
    "view", "night", "panorama",
]


# ──────────────────────────────────────────────────────────────
# Image candidate scoring
# ──────────────────────────────────────────────────────────────

@dataclass
class ImageCandidate:
    url: str
    source_page_url: str = ""
    title: str = ""
    width: int = 0
    height: int = 0
    source: str = ""  # "serpapi" | "ddg"
    score: float = 0.0
    classification: str = "unknown"  # exterior, interior, amenities, etc.
    penalties: list = field(default_factory=list)
    bonuses: list = field(default_factory=list)


def classify_image(url: str, title: str) -> str:
    """Classify image type from URL and title text."""
    text = f"{url} {title}".lower()

    if any(k in text for k in ["exterior", "elevation", "facade", "aerial", "building", "tower"]):
        return "exterior"
    if any(k in text for k in ["interior", "living", "bedroom", "kitchen", "bathroom", "hall"]):
        return "interior"
    if any(k in text for k in ["amenity", "amenities", "clubhouse", "pool", "gym", "garden", "playground"]):
        return "amenities"
    if any(k in text for k in ["master plan", "masterplan", "site plan", "siteplan", "layout plan"]):
        return "master_plan"
    if any(k in text for k in ["floor plan", "floorplan", "floor-plan"]):
        return "floor_plan"

    return "unknown"


def score_candidate(candidate: ImageCandidate, society_name: str) -> float:
    """Score an image candidate. Higher = better. Range roughly 0-100."""
    score = 50.0  # baseline
    url_lower = candidate.url.lower()
    title_lower = candidate.title.lower()
    society_lower = society_name.lower()

    # ── Source domain trust ──
    from urllib.parse import urlparse
    domain = urlparse(candidate.url).netloc.lower()

    if any(d in domain for d in BLOCKED_DOMAINS):
        candidate.penalties.append(f"blocked domain: {domain}")
        return 0.0  # hard reject

    if any(d in domain for d in TRUSTED_DOMAINS):
        score += 15
        candidate.bonuses.append(f"trusted domain: {domain}")

    # ── URL keyword signals ──
    for kw in NEGATIVE_URL_KEYWORDS:
        if kw in url_lower:
            score -= 20
            candidate.penalties.append(f"negative keyword: {kw}")

    for kw in POSITIVE_URL_KEYWORDS:
        if kw in url_lower or kw in title_lower:
            score += 5
            candidate.bonuses.append(f"positive keyword: {kw}")

    # ── Society name relevance ──
    name_parts = society_lower.split()
    matches = sum(1 for part in name_parts if len(part) > 2 and part in title_lower)
    if matches >= 2:
        score += 15
        candidate.bonuses.append(f"name match: {matches} words")
    elif matches == 1:
        score += 5
        candidate.bonuses.append("partial name match")

    # ── Resolution ──
    if candidate.width > 0 and candidate.height > 0:
        pixels = candidate.width * candidate.height
        if pixels >= 1_000_000:  # 1MP+
            score += 10
            candidate.bonuses.append("high res")
        elif pixels >= 500_000:
            score += 5
        elif pixels < 100_000:
            score -= 15
            candidate.penalties.append("very low res")
        elif pixels < 250_000:
            score -= 5
            candidate.penalties.append("low res")

        # Prefer landscape
        if candidate.width > candidate.height * 1.2:
            score += 5
            candidate.bonuses.append("landscape")
        elif candidate.height > candidate.width * 1.5:
            score -= 5
            candidate.penalties.append("portrait/tall")

    # ── Classification bonus ──
    if candidate.classification == "exterior":
        score += 10
    elif candidate.classification == "amenities":
        score += 5
    elif candidate.classification == "floor_plan":
        score -= 25
        candidate.penalties.append("floor plan")
    elif candidate.classification == "master_plan":
        score -= 15
        candidate.penalties.append("master plan")

    # ── File extension ──
    if url_lower.endswith(".svg") or url_lower.endswith(".gif"):
        score -= 30
        candidate.penalties.append("bad format")

    candidate.score = max(0, score)
    return candidate.score


# ──────────────────────────────────────────────────────────────
# Image search backends
# ──────────────────────────────────────────────────────────────

def _cache_key(query: str) -> str:
    return hashlib.sha256(query.encode()).hexdigest()[:16]


def search_serpapi(query: str, api_key: str, num: int = 20) -> List[ImageCandidate]:
    """Search Google Images via SerpAPI. Results are cached."""
    SERPAPI_CACHE_DIR.mkdir(parents=True, exist_ok=True)
    cache_file = SERPAPI_CACHE_DIR / f"{_cache_key(query)}.json"

    if cache_file.exists():
        data = json.loads(cache_file.read_text())
        logger.debug("SerpAPI cache hit: %s", query)
    else:
        encoded = urllib.parse.urlencode({
            "q": query,
            "engine": "google_images",
            "ijn": "0",
            "num": str(num),
            "api_key": api_key,
        })
        url = f"https://serpapi.com/search.json?{encoded}"
        req = urllib.request.Request(url)
        try:
            with urllib.request.urlopen(req, timeout=15) as resp:
                data = json.loads(resp.read().decode())
            cache_file.write_text(json.dumps(data, indent=2))
            logger.info("SerpAPI search: %s → %d results", query, len(data.get("images_results", [])))
        except Exception as e:
            logger.error("SerpAPI failed for '%s': %s", query, e)
            return []

    candidates = []
    for r in data.get("images_results", []):
        img_url = r.get("original", "")
        if not img_url or not img_url.startswith("http"):
            continue
        c = ImageCandidate(
            url=img_url,
            source_page_url=r.get("link", ""),
            title=r.get("title", ""),
            width=r.get("original_width", 0),
            height=r.get("original_height", 0),
            source="serpapi",
        )
        c.classification = classify_image(c.url, c.title)
        candidates.append(c)

    return candidates


def search_duckduckgo(query: str, max_results: int = 15) -> List[ImageCandidate]:
    """Search DuckDuckGo images. Free, no API key. Results are cached."""
    DDG_CACHE_DIR.mkdir(parents=True, exist_ok=True)
    cache_file = DDG_CACHE_DIR / f"{_cache_key(query)}.json"

    if cache_file.exists():
        data = json.loads(cache_file.read_text())
        logger.debug("DDG cache hit: %s", query)
    else:
        encoded = urllib.parse.quote(query)
        url = f"https://duckduckgo.com/?q={encoded}&iax=images&ia=images"
        try:
            req = urllib.request.Request(url, headers=HEADERS)
            with urllib.request.urlopen(req, timeout=10) as resp:
                html = resp.read().decode("utf-8", errors="replace")

            vqd_match = re.search(r'vqd=["\']([^"\']+)', html) or re.search(r'vqd=([a-zA-Z0-9_-]+)', html)
            if not vqd_match:
                logger.warning("DDG: no vqd token for '%s'", query)
                return []

            vqd = vqd_match.group(1)
            img_url = f"https://duckduckgo.com/i.js?l=us-en&o=json&q={encoded}&vqd={vqd}&f=size:Large"
            req = urllib.request.Request(img_url, headers={**HEADERS, "Referer": "https://duckduckgo.com/"})
            with urllib.request.urlopen(req, timeout=10) as resp:
                data = json.loads(resp.read().decode())

            cache_file.write_text(json.dumps(data, indent=2))
            logger.info("DDG search: %s → %d results", query, len(data.get("results", [])))
        except Exception as e:
            logger.error("DDG search failed for '%s': %s", query, e)
            return []

    candidates = []
    for r in data.get("results", [])[:max_results]:
        img_url = r.get("image", "")
        if not img_url or not img_url.startswith("http"):
            continue
        c = ImageCandidate(
            url=img_url,
            source_page_url=r.get("url", ""),
            title=r.get("title", ""),
            width=r.get("width", 0),
            height=r.get("height", 0),
            source="ddg",
        )
        c.classification = classify_image(c.url, c.title)
        candidates.append(c)

    return candidates


# ──────────────────────────────────────────────────────────────
# Image downloading
# ──────────────────────────────────────────────────────────────

def _detect_image_type(data: bytes) -> Optional[str]:
    """Detect image format from magic bytes."""
    if data[:3] == b"\xff\xd8\xff":
        return "jpg"
    if data[:8] == b"\x89PNG\r\n\x1a\n":
        return "png"
    if data[:4] == b"RIFF" and data[8:12] == b"WEBP":
        return "webp"
    return None


def _get_jpeg_dimensions(data: bytes) -> Tuple[int, int]:
    """Extract width/height from JPEG data without Pillow."""
    i = 2
    while i < len(data) - 1:
        if data[i] != 0xFF:
            break
        marker = data[i + 1]
        if marker == 0xD9:  # EOI
            break
        if marker == 0xDA:  # SOS — we're past headers
            break
        if 0xC0 <= marker <= 0xCF and marker not in (0xC4, 0xC8, 0xCC):
            # SOF marker — contains dimensions
            if i + 9 < len(data):
                h = struct.unpack(">H", data[i+5:i+7])[0]
                w = struct.unpack(">H", data[i+7:i+9])[0]
                return w, h
        # Skip to next marker
        if i + 3 < len(data):
            length = struct.unpack(">H", data[i+2:i+4])[0]
            i += 2 + length
        else:
            break
    return 0, 0


def _get_png_dimensions(data: bytes) -> Tuple[int, int]:
    """Extract width/height from PNG data."""
    if len(data) >= 24 and data[12:16] == b"IHDR":
        w = struct.unpack(">I", data[16:20])[0]
        h = struct.unpack(">I", data[20:24])[0]
        return w, h
    return 0, 0


def download_image(url: str, dest: Path, timeout: int = 15) -> Optional[dict]:
    """Download an image, validate it, return metadata or None."""
    try:
        req = urllib.request.Request(url, headers=HEADERS)
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            data = resp.read()

        size = len(data)
        if size < MIN_IMAGE_BYTES:
            logger.debug("Too small (%d bytes): %s", size, url[:80])
            return None
        if size > MAX_IMAGE_BYTES:
            logger.debug("Too large (%d bytes): %s", size, url[:80])
            return None

        img_type = _detect_image_type(data)
        if not img_type:
            logger.debug("Unknown image format: %s", url[:80])
            return None

        # Get dimensions
        w, h = 0, 0
        if img_type == "jpg":
            w, h = _get_jpeg_dimensions(data)
        elif img_type == "png":
            w, h = _get_png_dimensions(data)

        # Write with correct extension
        actual_dest = dest.with_suffix(f".{img_type}")
        actual_dest.write_bytes(data)

        return {
            "file": actual_dest.name,
            "format": img_type,
            "size_bytes": size,
            "width": w,
            "height": h,
            "source_url": url,
        }

    except Exception as e:
        logger.debug("Download failed %s: %s", url[:80], e)
        return None


# ──────────────────────────────────────────────────────────────
# Core fetch logic
# ──────────────────────────────────────────────────────────────

def _build_queries(name: str, area: str, city: str) -> List[str]:
    """Build search queries for a society."""
    base = f"{name} {area} {city}"
    return [
        f"{base} exterior",
        f"{base} apartment building",
        f"{name} {city} amenities clubhouse",
    ]


def fetch_images_for_entity(
    entity_id: str,
    entity_type: str,
    name: str,
    area: str = "",
    city: str = "Bangalore",
    serpapi_key: Optional[str] = None,
    project_root: Optional[Path] = None,
    target_images: int = TARGET_IMAGES,
    force: bool = False,
    dry_run: bool = False,
) -> dict:
    """
    Fetch and score images for a society or property.

    Returns metadata dict matching the Day 22 schema.
    """
    slug = entity_id.replace("society:", "").replace("soc-", "").replace(" ", "_")
    root = project_root or PROJECT_ROOT
    photos_dir = root / "data" / "cache" / "media_ingest" / "societies"
    metadata_dir = root / "data" / "cache" / "image_metadata"
    dest_dir = photos_dir / slug
    existing = sorted(dest_dir.glob("*.*")) if dest_dir.exists() else []

    # Skip if already fetched (unless force)
    if not force and existing:
        if len(existing) >= target_images:
            logger.info("[%s] Already has %d images — skipping", name, len(existing))
            return _build_metadata(entity_id, entity_type, slug, existing, [])
        logger.info(
            "[%s] Has %d/%d images — filling missing slots",
            name,
            len(existing),
            target_images,
        )

    queries = _build_queries(name, area, city)

    # Collect candidates from all queries
    all_candidates = []  # type: List[ImageCandidate]
    seen_urls = set()

    for query in queries:
        if serpapi_key:
            candidates = search_serpapi(query, serpapi_key)
        else:
            candidates = search_duckduckgo(query)
            time.sleep(1.5)  # rate limit DDG

        for c in candidates:
            url_hash = hashlib.sha256(c.url.encode()).hexdigest()
            if url_hash not in seen_urls:
                seen_urls.add(url_hash)
                score_candidate(c, name)
                all_candidates.append(c)

    # Sort by score, take top candidates
    all_candidates.sort(key=lambda c: c.score, reverse=True)

    if dry_run:
        print(f"\n  [{name}] Top candidates:")
        for i, c in enumerate(all_candidates[:10]):
            print(f"    {i+1}. score={c.score:.0f} type={c.classification} src={c.source}")
            print(f"       {c.url[:100]}")
            if c.bonuses:
                print(f"       + {', '.join(c.bonuses)}")
            if c.penalties:
                print(f"       - {', '.join(c.penalties)}")
        return {}

    # Filter: minimum score threshold
    viable = [c for c in all_candidates if c.score >= 30]
    if not viable:
        logger.warning("[%s] No viable candidates (all scored < 30)", name)
        return _build_metadata(entity_id, entity_type, slug, [], [])

    # Download: aim for a good mix
    dest_dir.mkdir(parents=True, exist_ok=True)
    downloaded = [] if force else list(existing)
    sources = []
    types_collected = set()

    # First pass: try to get diverse types (exterior first, then amenities, then others)
    type_priority = ["exterior", "amenities", "unknown", "interior"]
    for target_type in type_priority:
        if len(downloaded) >= target_images:
            break
        for c in viable:
            if len(downloaded) >= target_images:
                break
            if c.url in [s["original_image_url"] for s in sources]:
                continue
            # For first 2 images, prefer exterior; after that, mix
            if len(downloaded) < 2 and c.classification != "exterior" and target_type == "exterior":
                continue
            if target_type != "exterior" and c.classification != target_type:
                continue

            idx = len(downloaded) + 1
            dest = dest_dir / f"{idx}.jpg"  # extension corrected by download_image
            result = download_image(c.url, dest)
            if result:
                downloaded.append(dest_dir / result["file"])
                sources.append({
                    "file": result["file"],
                    "source_type": "search",
                    "source_page_url": c.source_page_url,
                    "original_image_url": c.url,
                    "search_engine": c.source,
                    "classification": c.classification,
                    "score": c.score,
                    "width": result["width"],
                    "height": result["height"],
                })
                types_collected.add(c.classification)
                logger.info("[%s] Downloaded %s (%s, score=%.0f)", name, result["file"], c.classification, c.score)

    # Second pass: fill remaining slots regardless of type
    if len(downloaded) < target_images:
        for c in viable:
            if len(downloaded) >= target_images:
                break
            if c.url in [s["original_image_url"] for s in sources]:
                continue
            idx = len(downloaded) + 1
            dest = dest_dir / f"{idx}.jpg"
            result = download_image(c.url, dest)
            if result:
                downloaded.append(dest_dir / result["file"])
                sources.append({
                    "file": result["file"],
                    "source_type": "search",
                    "source_page_url": c.source_page_url,
                    "original_image_url": c.url,
                    "search_engine": c.source,
                    "classification": c.classification,
                    "score": c.score,
                    "width": result["width"],
                    "height": result["height"],
                })
                logger.info("[%s] Downloaded %s (fill, score=%.0f)", name, result["file"], c.score)

    metadata = _build_metadata(entity_id, entity_type, slug, downloaded, sources)

    # Save metadata
    metadata_dir.mkdir(parents=True, exist_ok=True)
    meta_file = metadata_dir / f"{slug}.json"
    meta_file.write_text(json.dumps(metadata, indent=2))

    return metadata


def _build_metadata(
    entity_id: str,
    entity_type: str,
    slug: str,
    downloaded: List[Path],
    sources: List[dict],
) -> dict:
    """Build the structured metadata dict."""
    photos = [f"/_staged_media/societies/{slug}/{p.name}" for p in downloaded]
    hero = photos[0] if photos else ""
    gallery = photos[1:] if len(photos) > 1 else []

    confidence = "high" if len(photos) >= 3 else "medium" if len(photos) >= 1 else "none"

    return {
        "entity_id": entity_id,
        "entity_type": entity_type,
        "slug": slug,
        "hero_image": hero,
        "gallery": gallery,
        "all_photos": photos,
        "sources": sources,
        "selection_confidence": confidence,
        "fetched_at": datetime.now(timezone.utc).isoformat(),
    }


# ──────────────────────────────────────────────────────────────
# Main: run for all societies from seed data
# ──────────────────────────────────────────────────────────────

def load_societies() -> List[dict]:
    """Load societies from seed data."""
    path = SEED_DIR / "societies.json"
    if not path.exists():
        logger.error("societies.json not found at %s", path)
        return []
    return json.loads(path.read_text())


def populate_property_images():
    """
    After society images are fetched, populate property hero_image
    from their society's photos. Updates properties.json in place.
    """
    props_path = SEED_DIR / "properties.json"
    if not props_path.exists():
        return

    properties = json.loads(props_path.read_text())

    # Build society_id → slug mapping from societies.json
    societies = load_societies()
    soc_id_to_slug = {}
    for s in societies:
        sid = s["id"]
        slug = sid.replace("soc-", "")
        soc_id_to_slug[sid] = slug

    # Build slug → photos mapping from what's on disk
    slug_to_photos = {}
    if PHOTOS_DIR.exists():
        for d in PHOTOS_DIR.iterdir():
            if d.is_dir():
                photos = sorted(d.glob("*.*"))
                if photos:
                    slug_to_photos[d.name] = [
                        f"/_staged_media/societies/{d.name}/{p.name}" for p in photos
                    ]

    updated = 0
    for prop in properties:
        soc_id = prop.get("society_id", "")
        slug = soc_id_to_slug.get(soc_id, "")
        if not slug:
            continue

        photos = slug_to_photos.get(slug, [])
        if not photos:
            continue

        if not prop.get("hero_image"):
            prop["hero_image"] = photos[0]
            updated += 1
        if not prop.get("images"):
            prop["images"] = photos

    if updated > 0:
        props_path.write_text(json.dumps(properties, indent=2))
        logger.info("Updated %d properties with hero images", updated)

    return updated


# ---------------------------------------------------------------------------
# BaseSkill wrapper for enrichment engine integration
# ---------------------------------------------------------------------------

class FetchImagesSkill(BaseSkill):
    skill_id = "fetch_images"
    description = "Fetch and score images for societies from search engines"
    version = "1.0"
    output_keys = ["hero_image", "gallery"]

    def execute(self, input_data: dict) -> SkillResult:
        entity_id = input_data.get("entity_id", "")
        entity_type = input_data.get("entity_type", "society")
        name = input_data.get("name", "")
        area = input_data.get("area", "")

        if not entity_id or not name:
            logger.error("fetch_images requires entity_id and name")
            return SkillResult(confidence=0.0)

        serpapi_key = os.environ.get("SERPAPI_API_KEY") or os.environ.get("SERPAPI_KEY")
        metadata = fetch_images_for_entity(
            entity_id=entity_id,
            entity_type=entity_type,
            name=name,
            area=area,
            serpapi_key=serpapi_key,
        )

        # No SourcedFacts produced — images are stored on disk
        # Return a result with confidence based on images found
        photo_count = len(metadata.get("all_photos", []))
        confidence = 0.8 if photo_count >= 3 else 0.5 if photo_count >= 1 else 0.0
        return SkillResult(
            confidence=confidence,
            cost=SkillCost(api_calls=1 if serpapi_key else 0),
        )

    def estimated_cost(self) -> SkillCost:
        return SkillCost(api_calls=1, estimated_usd=0.0)


def main():
    import argparse

    parser = argparse.ArgumentParser(description="Fetch society/property images")
    parser.add_argument("--slug", help="Fetch for a single society slug")
    parser.add_argument("--force", action="store_true", help="Re-fetch even if images exist")
    parser.add_argument("--dry-run", action="store_true", help="Show candidates without downloading")
    parser.add_argument("--skip-properties", action="store_true", help="Don't update properties.json")
    args = parser.parse_args()

    logging.basicConfig(level=logging.INFO, format="%(message)s")

    serpapi_key = os.environ.get("SERPAPI_API_KEY") or os.environ.get("SERPAPI_KEY")
    if serpapi_key:
        print("  Using SerpAPI (Google Images)")
    else:
        print("  Using DuckDuckGo (no SERPAPI_API_KEY set)")

    societies = load_societies()
    if not societies:
        print("  No societies found in seed data")
        return

    if args.slug:
        societies = [s for s in societies if args.slug in s["id"]]
        if not societies:
            print(f"  No society matching slug '{args.slug}'")
            return

    print(f"\n  Fetching images for {len(societies)} societies\n")

    results = {}
    for i, soc in enumerate(societies, 1):
        name = soc["name"]
        area = soc.get("area", "")
        city = soc.get("city", "Bangalore")
        soc_id = soc["id"]

        print(f"  [{i}/{len(societies)}] {name}")

        metadata = fetch_images_for_entity(
            entity_id=soc_id,
            entity_type="society",
            name=name,
            area=area,
            city=city,
            serpapi_key=serpapi_key,
            force=args.force,
            dry_run=args.dry_run,
        )
        results[soc_id] = metadata

    # Summary
    if not args.dry_run:
        total_images = sum(len(m.get("all_photos", [])) for m in results.values())
        with_images = sum(1 for m in results.values() if m.get("all_photos"))
        print(f"\n  {'='*50}")
        print(f"  Summary: {total_images} images for {with_images}/{len(societies)} societies")
        print(f"  Staging: data/cache/media_ingest/societies/")
        print(f"  Metadata: data/cache/image_metadata/")

        # Update property hero images
        if not args.skip_properties:
            updated = populate_property_images()
            if updated:
                print(f"  Properties updated: {updated}")
        print()


if __name__ == "__main__":
    main()
