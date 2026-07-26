"""Source-neutral external listing collection.

The collector produces generic listing observations. Portal-specific adapters
normalize into this shape before Rust materializes durable DAG assets.
"""

import json
import math
import os
import re
import statistics
import urllib.error
import urllib.parse
import urllib.request
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Dict, Iterable, List, Optional, Tuple


HEADERS = {
    "User-Agent": (
        "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) "
        "AppleWebKit/537.36 (KHTML, like Gecko) "
        "Chrome/122.0.0.0 Safari/537.36"
    ),
    "Accept": "text/markdown,text/plain,text/html;q=0.9,*/*;q=0.8",
    "Accept-Language": "en-US,en;q=0.8",
}

MAGICBRICKS_READER_PREFIX = "https://r.jina.ai/http://"
SQUAREYARDS_READER_PREFIX = "https://r.jina.ai/http://"
LISTING_HEADING_RE = re.compile(
    r"^\s*##\s+(?P<bhk>\d+(?:\.\d+)?)\s+BHK\s+Flat\s+for\s+(?P<kind>Sale|Rent)\s+in\s+"
    r"(?P<location>.+?)\s*$",
    re.IGNORECASE | re.MULTILINE,
)
AREA_RE = re.compile(
    r"(?P<label>Carpet Area|Super Area)\s*\n\s*\n\s*(?P<area>[\d,]+(?:\.\d+)?)\s*sq\.?ft",
    re.IGNORECASE,
)
PRICE_LINE_RE = re.compile(
    r"^\*{0,2}₹\s*[\d,.]+\s*(?:Cr|L|Lac|Lakh|K)?\*{0,2}"
    r"(?:\s*/\s*(?:month|per\s+month))?(?:\s*\+\s*charges)?\s*$",
    re.IGNORECASE,
)
PPSF_LINE_RE = re.compile(r"^₹\s*[\d,]+(?:\.\d+)?\s*per\s+sqft\s*$", re.IGNORECASE)


def collect_external_listings(request: Dict[str, Any]) -> Dict[str, Any]:
    observed_at = normalized_planned_at(request)
    snapshot_date = partition_values(request).get("dt") or observed_at[:10]
    records = []  # type: List[Dict[str, Any]]

    for input_data in request.get("source_entities", []):
        records.extend(records_for_entity(input_data, observed_at, request))

    records = dedupe_records(records)
    records.sort(
        key=lambda record: (
            record.get("entity_id") or "",
            record.get("bhk") or 0,
            record.get("source_name") or "",
            record.get("source_url") or "",
        )
    )
    watermarks = source_watermarks(records, "external_listing", observed_at)
    watermarks.extend(listing_coverage_watermarks(records, request, observed_at))
    return {
        "snapshot_date": snapshot_date,
        "records": records,
        "source_watermarks": watermarks,
    }


def records_for_entity(
    input_data: Dict[str, Any], observed_at: str, request: Dict[str, Any]
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
    if not source_pages and not skip_listing_fetch(request):
        source_pages = external_listing_source_pages(input_data, society_name)

    observations = []  # type: List[Dict[str, Any]]
    for source_page in source_pages:
        source_url = required_page_url(source_page)
        if not source_url:
            continue
        page_text = optional_string(source_page.get("text")) or optional_string(
            source_page.get("html")
        )
        source_name = source_name_from_page(source_page)
        query_kind = listing_type_from_page(source_page)
        if page_text is None:
            page_text = fetch_listing_page_text(source_url, source_name)
        if not page_text:
            continue
        observations.extend(
            listing_observations_from_page_text(
                page_text,
                entity_id=entity_id,
                project_key=optional_string(input_data.get("project_key")),
                society_name=society_name,
                fallback_locality=optional_string(input_data.get("area")),
                source_url=source_url,
                source_name=source_name,
                query_kind=query_kind,
                observed_at=observed_at,
            )
        )

    return aggregate_listing_records(observations)


def explicit_source_pages(input_data: Dict[str, Any]) -> List[Dict[str, Any]]:
    for key in ("external_listing_source_pages", "listing_source_pages"):
        pages = input_data.get(key)
        if isinstance(pages, list):
            return [page for page in pages if isinstance(page, dict)]
    urls = input_data.get("external_listing_source_urls") or input_data.get(
        "listing_source_urls"
    )
    if isinstance(urls, list):
        return [
            {"source_url": url, "source_name": "MagicBricks", "query_kind": "sale"}
            for url in urls
            if optional_string(url)
        ]
    return []


def external_listing_source_pages(
    input_data: Dict[str, Any], society_name: str
) -> List[Dict[str, Any]]:
    return magicbricks_source_pages(input_data, society_name) + squareyards_source_pages(
        input_data, society_name
    )


def magicbricks_source_pages(
    input_data: Dict[str, Any], society_name: str
) -> List[Dict[str, Any]]:
    city = magicbricks_city_slug(optional_string(input_data.get("city")))
    project_slug = slug(society_name)
    if not project_slug or not city:
        return []
    bhk_values = rera_configuration_bhks(input_data)
    if bhk_values:
        pages = []
        for bhk in bhk_values:
            bhk_slug = format_bhk(bhk).replace(".", "-")
            pages.append(
                {
                    "source_name": "MagicBricks",
                    "source_url": "https://www.magicbricks.com/{}-bhk-flats-for-sale-in-{}-{}-pppfs".format(
                        bhk_slug, project_slug, city
                    ),
                    "query_kind": "sale",
                    "bhk": bhk,
                    "query_basis": "rera_configuration",
                }
            )
            if bhk in (2.0, 3.0):
                pages.append(
                    {
                        "source_name": "MagicBricks",
                        "source_url": "https://www.magicbricks.com/{}-bhk-flats-for-rent-in-{}-{}-pppfr".format(
                            bhk_slug, project_slug, city
                        ),
                        "query_kind": "rent",
                        "bhk": bhk,
                        "query_basis": "rera_configuration",
                    }
                )
        pages.extend(magicbricks_project_pages(project_slug, city, include_rent=True))
        return dedupe_source_pages(pages)
    return [
        *magicbricks_project_pages(project_slug, city, include_rent=True)
    ]


def magicbricks_project_pages(
    project_slug: str, city: str, *, include_rent: bool
) -> List[Dict[str, Any]]:
    pages = [
        {
            "source_name": "MagicBricks",
            "source_url": "https://www.magicbricks.com/project-{}-for-sale-in-{}-pppfs".format(
                project_slug, city
            ),
            "query_kind": "sale",
            "query_basis": "project_fallback",
        }
    ]
    if include_rent:
        pages.append(
            {
                "source_name": "MagicBricks",
                "source_url": "https://www.magicbricks.com/project-{}-for-rent-in-{}-pppfr".format(
                    project_slug, city
                ),
                "query_kind": "rent",
                "query_basis": "project_fallback",
            }
        )
    return pages


def squareyards_source_pages(
    input_data: Dict[str, Any], society_name: str
) -> List[Dict[str, Any]]:
    city = squareyards_city_slug(optional_string(input_data.get("city")))
    project_slug = slug(society_name)
    if not project_slug or not city:
        return []
    return [
        {
            "source_name": "SquareYards",
            "source_url": "https://www.squareyards.com/sale/resale-properties-in-{}-{}".format(
                project_slug, city
            ),
            "query_kind": "sale",
            "query_basis": "project_focused",
        },
        {
            "source_name": "SquareYards",
            "source_url": "https://www.squareyards.com/rent/property-for-rent-in-{}-{}".format(
                project_slug, city
            ),
            "query_kind": "rent",
            "query_basis": "project_focused",
        },
    ]


def rera_configuration_bhks(input_data: Dict[str, Any]) -> List[float]:
    values = (
        input_data.get("rera_configurations")
        or input_data.get("available_configurations")
        or input_data.get("configurations")
        or []
    )
    if isinstance(values, str):
        values = [part.strip() for part in values.split(",")]
    if not isinstance(values, list):
        return []
    bhks = []
    for value in values:
        if isinstance(value, dict):
            raw = value.get("bedroom_count") or value.get("bhk") or value.get("configuration_type")
        else:
            raw = value
        parsed = configuration_bhk(raw)
        if parsed is not None and parsed not in bhks:
            bhks.append(parsed)
    return sorted(bhks)


def configuration_bhk(value: Any) -> Optional[float]:
    if value is None:
        return None
    if isinstance(value, (int, float)):
        number = float(value)
        return number if 0 < number <= 6 else None
    match = re.search(r"\b([1-6](?:\.5)?)\s*(?:bhk|b h k|bed)?\b", str(value), re.IGNORECASE)
    if not match:
        return None
    return optional_float(match.group(1))


def fetch_listing_page_text(source_url: str, source_name: str) -> Optional[str]:
    if source_name.lower() == "squareyards":
        reader_prefix = os.environ.get("OPENESTATES_SQUAREYARDS_READER_PREFIX")
        if reader_prefix is None:
            reader_prefix = SQUAREYARDS_READER_PREFIX
    else:
        reader_prefix = os.environ.get("OPENESTATES_MAGICBRICKS_READER_PREFIX")
        if reader_prefix is None:
            reader_prefix = MAGICBRICKS_READER_PREFIX
    fetch_url = "{}{}".format(reader_prefix, source_url) if reader_prefix else source_url
    request = urllib.request.Request(fetch_url, headers=HEADERS)
    timeout = optional_float(os.environ.get("OPENESTATES_LISTING_FETCH_TIMEOUT")) or 30.0
    try:
        with urllib.request.urlopen(request, timeout=timeout) as response:
            return response.read().decode("utf-8", "replace")
    except (OSError, urllib.error.URLError, urllib.error.HTTPError, TimeoutError):
        return None


def listing_observations_from_page_text(
    page_text: str,
    *,
    entity_id: str,
    project_key: Optional[str],
    society_name: str,
    fallback_locality: Optional[str],
    source_url: str,
    source_name: str,
    query_kind: str,
    observed_at: str,
) -> List[Dict[str, Any]]:
    observations = []
    matches = list(LISTING_HEADING_RE.finditer(page_text))
    for index, match in enumerate(matches):
        block_start = match.end()
        block_end = matches[index + 1].start() if index + 1 < len(matches) else len(page_text)
        block = page_text[block_start:block_end]
        if source_name.lower() == "squareyards" and not block_mentions_project(
            squareyards_listing_project_context(page_text, block, match, index, matches),
            society_name,
        ):
            continue
        area = area_from_block(block)
        price = price_from_block(block)
        if not area or not price:
            continue
        bhk = optional_float(match.group("bhk"))
        if bhk is None:
            continue
        locality = locality_from_heading(
            match.group("location"), society_name, fallback_locality
        )
        ppsf = price_per_sqft_from_block(block)
        listing_type = listing_type_from_heading(match.group("kind"), query_kind)
        observations.append(
            {
                "entity_id": entity_id,
                "project_key": project_key,
                "source_name": source_name,
                "source_url": source_url,
                "listing_type": listing_type,
                "price": price[0],
                "price_display": price[1],
                "area_sqft": area[0],
                "area_display": area[1],
                "price_per_sqft": ppsf[0] if ppsf else None,
                "price_per_sqft_display": ppsf[1] if ppsf else None,
                "configuration": "{} BHK".format(format_bhk(bhk)),
                "area_type": area[2],
                "bhk": bhk,
                "bathrooms": bathrooms_from_block(block),
                "floor": labeled_text(block, "Floor"),
                "society": society_name,
                "locality": locality,
                "observed_at": observed_at,
            }
        )
    return observations


def aggregate_listing_records(observations: Iterable[Dict[str, Any]]) -> List[Dict[str, Any]]:
    grouped = {}  # type: Dict[Tuple[str, float, str, str], List[Dict[str, Any]]]
    for observation in observations:
        bhk = observation.get("bhk")
        entity_id = observation.get("entity_id")
        if not entity_id or bhk is None:
            continue
        listing_type = listing_type_from_page(observation)
        source_name = optional_string(observation.get("source_name")) or "ExternalListing"
        grouped.setdefault((str(entity_id), float(bhk), listing_type, source_name), []).append(
            observation
        )

    records = []
    for (_entity_id, bhk, listing_type, _source_name), group in grouped.items():
        prices = sorted_float_values(record.get("price") for record in group)
        areas = sorted_float_values(record.get("area_sqft") for record in group)
        ppsf_values = sorted_float_values(record.get("price_per_sqft") for record in group)
        if not prices or not areas:
            continue
        first = group[0]
        price = statistics.median(prices)
        area_sqft = representative_area_sqft(price, areas, ppsf_values)
        records.append(
            {
                "entity_id": first["entity_id"],
                "project_key": first.get("project_key"),
                "source_name": first["source_name"],
                "source_url": first.get("source_url"),
                "listing_type": listing_type,
                "price": round(price),
                "price_min": round(prices[0]),
                "price_max": round(prices[-1]),
                "area_sqft": round(area_sqft),
                "area_sqft_min": round(areas[0]),
                "area_sqft_max": round(areas[-1]),
                "price_per_sqft_min": round(ppsf_values[0]) if ppsf_values else None,
                "price_per_sqft_max": round(ppsf_values[-1]) if ppsf_values else None,
                "price_display": inr_range_display(prices[0], prices[-1]),
                "area_display": sqft_range_display(areas[0], areas[-1]),
                "price_per_sqft_display": ppsf_range_display(ppsf_values),
                "configuration": "{} BHK".format(format_bhk(bhk)),
                "area_type": aggregate_area_type(group),
                "bhk": bhk,
                "bathrooms": median_optional(record.get("bathrooms") for record in group),
                "floor": representative_text(record.get("floor") for record in group),
                "society": first.get("society"),
                "locality": representative_text(record.get("locality") for record in group),
                "observed_at": max(record["observed_at"] for record in group),
            }
        )
    return records


def area_from_block(block: str) -> Optional[Tuple[float, str, str]]:
    match = AREA_RE.search(block)
    if match:
        area = optional_float(match.group("area").replace(",", ""))
        label = "carpet" if "carpet" in match.group("label").lower() else "super built-up"
    else:
        fallback = squareyards_area_match(block) or re.search(
            r"(?P<area>[\d,]+(?:\.\d+)?)\s*(?:sq\.?\s*ft|sqft|sq\.?\s*feet)",
            block,
            re.IGNORECASE,
        )
        if not fallback:
            return None
        area = optional_float(fallback.group("area").replace(",", ""))
        label = area_label_from_match(fallback) or "listed area"
    if area is None or area <= 0:
        return None
    return area, "{} sqft".format(clean_number(area)), label


def price_from_block(block: str) -> Optional[Tuple[float, str]]:
    for line in block.splitlines():
        text = line.strip()
        if PRICE_LINE_RE.match(text):
            value = parse_inr_price(text)
            if value:
                return value, text
    return None


def price_per_sqft_from_block(block: str) -> Optional[Tuple[float, str]]:
    for line in block.splitlines():
        text = line.strip()
        if PPSF_LINE_RE.match(text):
            value = optional_float(re.sub(r"[^\d.]", "", text))
            if value and value > 0:
                return value, text
    return None


def parse_inr_price(text: str) -> Optional[float]:
    normalized = text.lower().replace(",", "").replace("*", "")
    match = re.search(r"([\d.]+)\s*(cr|l|lac|lakh|k)?", normalized)
    if not match:
        return None
    value = optional_float(match.group(1))
    if value is None:
        return None
    unit = match.group(2)
    if unit == "cr":
        multiplier = 10_000_000
    elif unit in ("l", "lac", "lakh"):
        multiplier = 100_000
    elif unit == "k":
        multiplier = 1_000
    else:
        multiplier = 1
    return value * multiplier


def locality_from_heading(
    heading_location: str, society_name: str, fallback_locality: Optional[str]
) -> Optional[str]:
    location = clean_heading_location(heading_location)
    if not location:
        return fallback_locality
    lower = location.lower()
    society_lower = society_name.lower()
    if lower.startswith(society_lower):
        location = location[len(society_name) :].strip(" ,")
    parts = [
        part.strip()
        for part in location.split(",")
        if part.strip() and part.strip().lower() not in ("bangalore", "bengaluru")
    ]
    return ", ".join(parts) if parts else fallback_locality


def clean_heading_location(value: str) -> str:
    location = re.sub(r"!\[[^\]]*\]\([^)]+\)", " ", value or "")
    location = re.sub(r"\[[^\]]*\]\([^)]+\)", " ", location)
    location = re.sub(r"https?://\S+", " ", location)
    location = re.sub(r"_+", " ", location)
    location = re.sub(r"\s+", " ", location).strip(" ,")
    marker = re.search(r"\bImage\s+\d+\s*:", location, re.IGNORECASE)
    if marker:
        location = location[: marker.start()].strip(" ,")
    return location


def labeled_number(block: str, label: str) -> Optional[float]:
    value = labeled_text(block, label)
    return optional_float(value) if value is not None else None


def bathrooms_from_block(block: str) -> Optional[float]:
    labeled = labeled_number(block, "Bathroom")
    if labeled is not None:
        return labeled
    match = re.search(r"\bConfig\s+[^\n]*?\+\s*(?P<baths>\d+(?:\.\d+)?)\s*Bath\b", block, re.IGNORECASE)
    return optional_float(match.group("baths")) if match else None


def squareyards_area_match(block: str):
    return re.search(
        r"Area\s+(?P<label>Built-up Area|Carpet Area|Super Area)\s*\n+\s*"
        r"(?P<area>[\d,]+(?:\.\d+)?)\s*\n+\s*Sq\.?Ft\.?",
        block,
        re.IGNORECASE,
    )


def area_label_from_match(match) -> Optional[str]:
    label = match.groupdict().get("label")
    if not label:
        return None
    lowered = label.lower()
    if "built" in lowered:
        return "built-up"
    if "carpet" in lowered:
        return "carpet"
    if "super" in lowered:
        return "super built-up"
    return None


def block_mentions_project(block: str, society_name: str) -> bool:
    text = re.sub(r"[^a-z0-9]+", " ", block.lower())
    project_tokens = [
        token
        for token in re.split(r"[^a-z0-9]+", society_name.lower())
        if len(token) > 2
    ]
    if not project_tokens:
        return True
    return all(token in text for token in project_tokens)


def squareyards_listing_project_context(
    page_text: str, block: str, match, index: int, matches: List[Any]
) -> str:
    previous_boundary = matches[index - 1].end() if index > 0 else 0
    prelude_start = max(previous_boundary, match.start() - 1200)
    body_preview = drop_squareyards_trailing_project_label(block[:1200])
    return "{}\n{}".format(page_text[prelude_start : match.start()], body_preview)


def drop_squareyards_trailing_project_label(text: str) -> str:
    lines = text.splitlines()
    while lines and not lines[-1].strip():
        lines.pop()
    if not lines:
        return ""
    last = lines[-1].strip().lower()
    factual_markers = ("sq", "₹", "bath", "floor", "read more", "furnishing", "facing")
    if len(last) <= 120 and not any(marker in last for marker in factual_markers):
        lines.pop()
    return "\n".join(lines)


def labeled_text(block: str, label: str) -> Optional[str]:
    pattern = re.compile(
        r"^\s*{}\s*\n\s*\n\s*(?P<value>[^\n]+)".format(re.escape(label)),
        re.IGNORECASE | re.MULTILINE,
    )
    match = pattern.search(block)
    return optional_string(match.group("value")) if match else None


def dedupe_records(records: Iterable[Dict[str, Any]]) -> List[Dict[str, Any]]:
    deduped = []
    seen = set()
    for record in records:
        key = (
            record["entity_id"],
            record.get("bhk"),
            record.get("listing_type"),
            record.get("source_name"),
            record.get("source_url"),
            record.get("price_min"),
            record.get("price_max"),
            record.get("area_sqft_min"),
            record.get("area_sqft_max"),
        )
        if key in seen:
            continue
        seen.add(key)
        deduped.append(record)
    return deduped


def source_watermarks(
    records: Iterable[Dict[str, Any]], prefix: str, default_watermark: str
) -> List[Dict[str, str]]:
    watermarks = {}
    for record in records:
        source_name = slug(optional_string(record.get("source_name")) or "external")
        key = "{}_{}".format(prefix, source_name.replace("-", "_"))
        observed_at = optional_string(record.get("observed_at")) or default_watermark
        watermarks[key] = max(watermarks.get(key, default_watermark), observed_at)
    if not watermarks:
        watermarks["{}_empty".format(prefix)] = default_watermark
    return [
        {"source": source, "high_watermark": watermark}
        for source, watermark in sorted(watermarks.items())
    ]


def listing_coverage_watermarks(
    records: Iterable[Dict[str, Any]], request: Dict[str, Any], observed_at: str
) -> List[Dict[str, str]]:
    source_entities = [
        entity
        for entity in request.get("source_entities", [])
        if isinstance(entity, dict) and optional_string(entity.get("entity_id"))
    ]
    if not source_entities:
        return []
    records_by_entity = {}  # type: Dict[str, int]
    for record in records:
        entity_id = optional_string(record.get("entity_id"))
        if entity_id:
            records_by_entity[entity_id] = records_by_entity.get(entity_id, 0) + 1
    min_records = external_listing_min_records_per_entity(request)
    thin_entities = [
        optional_string(entity.get("entity_id")) or ""
        for entity in source_entities
        if records_by_entity.get(optional_string(entity.get("entity_id")) or "", 0) < min_records
    ]
    return [
        {
            "source": "external_listing_coverage",
            "high_watermark": (
                "entities={};records={};entities_below_min={};min_records_per_entity={}".format(
                    len(source_entities),
                    sum(records_by_entity.values()),
                    len(thin_entities),
                    min_records,
                )
            ),
        },
        {
            "source": "external_listing_coverage_at",
            "high_watermark": observed_at,
        },
    ]


def external_listing_min_records_per_entity(request: Dict[str, Any]) -> int:
    project_root = Path(request.get("project_root") or ".").resolve()
    policy = load_project_crawl_policy(project_root, "external_listing_coverage")
    if not policy:
        return 4
    return max(1, positive_int(policy.get("min_records_per_entity"), 4))


def load_project_crawl_policy(project_root: Path, policy_id: str) -> Optional[Dict[str, Any]]:
    path = project_root / "app" / "config" / "dag" / "crawl_policies" / "{}.json".format(policy_id)
    try:
        return json.loads(path.read_text())
    except (OSError, ValueError, TypeError):
        return None


def positive_int(value: Any, default: int) -> int:
    try:
        parsed = int(value)
    except (TypeError, ValueError):
        return default
    return parsed if parsed > 0 else default


def dedupe_source_pages(pages: Iterable[Dict[str, Any]]) -> List[Dict[str, Any]]:
    deduped = []
    seen = set()
    for page in pages:
        url = required_page_url(page)
        if not url or url in seen:
            continue
        seen.add(url)
        deduped.append(page)
    return deduped


def skip_listing_fetch(request: Dict[str, Any]) -> bool:
    if request.get("skip_external_listing_fetch"):
        return True
    return str(os.environ.get("OPENESTATES_SKIP_EXTERNAL_LISTING_FETCH") or "").lower() in (
        "1",
        "true",
        "yes",
    )


def aggregate_area_type(group: List[Dict[str, Any]]) -> str:
    values = {
        value
        for value in (optional_string(record.get("area_type")) for record in group)
        if value
    }
    if len(values) == 1:
        return next(iter(values))
    return "mixed listed area" if values else "unknown"


def sorted_float_values(values: Iterable[Any]) -> List[float]:
    parsed = []
    for value in values:
        number = optional_float(value)
        if number is not None and math.isfinite(number) and number > 0:
            parsed.append(number)
    return sorted(parsed)


def representative_area_sqft(
    price: float, areas: List[float], ppsf_values: List[float]
) -> float:
    if ppsf_values:
        ppsf = statistics.median(ppsf_values)
        if ppsf > 0:
            return price / ppsf
    return statistics.median(areas)


def median_optional(values: Iterable[Any]) -> Optional[float]:
    parsed = sorted_float_values(values)
    return statistics.median(parsed) if parsed else None


def representative_text(values: Iterable[Any]) -> Optional[str]:
    counts = {}  # type: Dict[str, int]
    for value in values:
        text = optional_string(value)
        if text:
            counts[text] = counts.get(text, 0) + 1
    if not counts:
        return None
    return sorted(counts.items(), key=lambda item: (-item[1], item[0]))[0][0]


def inr_range_display(min_price: float, max_price: float) -> str:
    if round(min_price) == round(max_price):
        return inr_display(min_price)
    return "{} - {}".format(inr_display(min_price), inr_display(max_price))


def inr_display(value: float) -> str:
    if value >= 10_000_000:
        return "₹{} Cr".format(clean_number(value / 10_000_000.0))
    return "₹{} Lac".format(clean_number(value / 100_000.0))


def sqft_range_display(min_area: float, max_area: float) -> str:
    if round(min_area) == round(max_area):
        return "{} sqft".format(clean_number(min_area))
    return "{}-{} sqft".format(clean_number(min_area), clean_number(max_area))


def ppsf_range_display(values: List[float]) -> Optional[str]:
    if not values:
        return None
    if round(values[0]) == round(values[-1]):
        return "₹{} per sqft".format(format_int(values[0]))
    return "₹{}-{} per sqft".format(format_int(values[0]), format_int(values[-1]))


def clean_number(value: float) -> str:
    rounded = round(value, 2)
    if float(rounded).is_integer():
        return str(int(rounded))
    return "{:.2f}".format(rounded).rstrip("0").rstrip(".")


def format_int(value: float) -> str:
    return "{:,}".format(round(value))


def format_bhk(value: float) -> str:
    if float(value).is_integer():
        return str(int(value))
    return clean_number(value)


def source_name_from_page(source_page: Dict[str, Any]) -> str:
    return optional_string(source_page.get("source_name")) or "MagicBricks"


def listing_type_from_page(source_page: Dict[str, Any]) -> str:
    value = optional_string(source_page.get("listing_type") or source_page.get("query_kind"))
    if value and value.lower() == "rent":
        return "rent"
    return "sale"


def listing_type_from_heading(heading_kind: str, fallback: str) -> str:
    if heading_kind and heading_kind.strip().lower() == "rent":
        return "rent"
    return "rent" if fallback == "rent" else "sale"


def optional_string(value: Any) -> Optional[str]:
    if value is None:
        return None
    text = str(value).strip()
    return text or None


def optional_float(value: Any) -> Optional[float]:
    if value is None or value == "":
        return None
    try:
        return float(str(value).replace(",", "").strip())
    except (TypeError, ValueError):
        return None


def slug(value: str) -> str:
    return re.sub(r"[^a-z0-9]+", "-", str(value or "").lower()).strip("-")


def magicbricks_city_slug(city: Optional[str]) -> str:
    if not city:
        return "bangalore"
    normalized = city.strip().lower()
    if normalized in ("bengaluru", "bangalore"):
        return "bangalore"
    return slug(normalized)


def squareyards_city_slug(city: Optional[str]) -> str:
    normalized = slug(city or "bangalore")
    if normalized in ("bengaluru", "bangaluru"):
        return "bangalore"
    return normalized or "bangalore"


def required_page_url(source_page: Dict[str, Any]) -> Optional[str]:
    return optional_string(
        source_page.get("source_url")
        or source_page.get("source_page_url")
        or source_page.get("url")
    )


def partition_values(request: Dict[str, Any]) -> Dict[str, str]:
    partition = request.get("partition", {})
    return {str(key): str(value) for key, value in partition.get("parts", [])}


def normalized_planned_at(request: Dict[str, Any]) -> str:
    value = str(request.get("planned_at") or "").strip()
    return value or datetime.now(timezone.utc).isoformat()
