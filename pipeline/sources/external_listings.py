"""Source-neutral external listing collection.

The collector produces generic listing observations. Portal-specific adapters
normalize into this shape before Rust materializes durable DAG assets.
"""

import math
import os
import re
import statistics
import urllib.error
import urllib.parse
import urllib.request
from datetime import datetime, timezone
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
LISTING_HEADING_RE = re.compile(
    r"^\s*##\s+(?P<bhk>\d+(?:\.\d+)?)\s+BHK\s+Flat\s+for\s+Sale\s+in\s+"
    r"(?P<location>.+?)\s*$",
    re.IGNORECASE | re.MULTILINE,
)
AREA_RE = re.compile(
    r"(?P<label>Carpet Area|Super Area)\s*\n\s*\n\s*(?P<area>[\d,]+(?:\.\d+)?)\s*sq\.?ft",
    re.IGNORECASE,
)
PRICE_LINE_RE = re.compile(r"^₹\s*[\d,.]+\s*(?:Cr|Lac|Lakh)\s*$", re.IGNORECASE)
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
    return {
        "snapshot_date": snapshot_date,
        "records": records,
        "source_watermarks": [
            {
                "source": "external_listing_magicbricks",
                "high_watermark": max(
                    (record["observed_at"] for record in records), default=observed_at
                ),
            }
        ],
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
        source_pages = magicbricks_source_pages(input_data, society_name)

    observations = []  # type: List[Dict[str, Any]]
    for source_page in source_pages:
        source_url = required_page_url(source_page)
        if not source_url:
            continue
        page_text = optional_string(source_page.get("text")) or optional_string(
            source_page.get("html")
        )
        if page_text is None:
            page_text = fetch_magicbricks_page_text(source_url)
        if not page_text:
            continue
        observations.extend(
            listing_observations_from_magicbricks_text(
                page_text,
                entity_id=entity_id,
                project_key=optional_string(input_data.get("project_key")),
                society_name=society_name,
                fallback_locality=optional_string(input_data.get("area")),
                source_url=source_url,
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
            {"source_url": url, "source_name": "MagicBricks"}
            for url in urls
            if optional_string(url)
        ]
    return []


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
        return pages
    return [
        {
            "source_name": "MagicBricks",
            "source_url": "https://www.magicbricks.com/project-{}-for-sale-in-{}-pppfs".format(
                project_slug, city
            ),
        }
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


def fetch_magicbricks_page_text(source_url: str) -> Optional[str]:
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


def listing_observations_from_magicbricks_text(
    page_text: str,
    *,
    entity_id: str,
    project_key: Optional[str],
    society_name: str,
    fallback_locality: Optional[str],
    source_url: str,
    observed_at: str,
) -> List[Dict[str, Any]]:
    observations = []
    matches = list(LISTING_HEADING_RE.finditer(page_text))
    for index, match in enumerate(matches):
        block_start = match.end()
        block_end = matches[index + 1].start() if index + 1 < len(matches) else len(page_text)
        block = page_text[block_start:block_end]
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
        observations.append(
            {
                "entity_id": entity_id,
                "project_key": project_key,
                "source_name": "MagicBricks",
                "source_url": source_url,
                "price": price[0],
                "price_display": price[1],
                "area_sqft": area[0],
                "area_display": area[1],
                "price_per_sqft": ppsf[0] if ppsf else None,
                "price_per_sqft_display": ppsf[1] if ppsf else None,
                "configuration": "{} BHK".format(format_bhk(bhk)),
                "area_type": area[2],
                "bhk": bhk,
                "bathrooms": labeled_number(block, "Bathroom"),
                "floor": labeled_text(block, "Floor"),
                "society": society_name,
                "locality": locality,
                "observed_at": observed_at,
            }
        )
    return observations


def aggregate_listing_records(observations: Iterable[Dict[str, Any]]) -> List[Dict[str, Any]]:
    grouped = {}  # type: Dict[Tuple[str, float], List[Dict[str, Any]]]
    for observation in observations:
        bhk = observation.get("bhk")
        entity_id = observation.get("entity_id")
        if not entity_id or bhk is None:
            continue
        grouped.setdefault((str(entity_id), float(bhk)), []).append(observation)

    records = []
    for (_entity_id, bhk), group in grouped.items():
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
    if not match:
        return None
    area = optional_float(match.group("area").replace(",", ""))
    if area is None or area <= 0:
        return None
    label = "carpet" if "carpet" in match.group("label").lower() else "super built-up"
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
    normalized = text.lower().replace(",", "")
    match = re.search(r"([\d.]+)\s*(cr|lac|lakh)", normalized)
    if not match:
        return None
    value = optional_float(match.group(1))
    if value is None:
        return None
    unit = match.group(2)
    multiplier = 10_000_000 if unit == "cr" else 100_000
    return value * multiplier


def locality_from_heading(
    heading_location: str, society_name: str, fallback_locality: Optional[str]
) -> Optional[str]:
    location = re.sub(r"\s+", " ", heading_location).strip(" ,")
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


def labeled_number(block: str, label: str) -> Optional[float]:
    value = labeled_text(block, label)
    return optional_float(value) if value is not None else None


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
