"""Deterministic project inventory and metro station collectors."""

import json
import re
from datetime import date, datetime
from typing import Any, Callable, Dict, Iterable, List
from urllib.error import HTTPError, URLError
from urllib.parse import urlencode
from urllib.request import Request, urlopen


PRESTIGE_API_URL = "https://www.prestigeconstructions.com/api/apicall"
PRESTIGE_PROJECT_URL = (
    "https://www.prestigeconstructions.com/residential-projects/{city}/{slug}"
)
OVERPASS_API_URLS = (
    "https://overpass-api.de/api/interpreter",
    "https://overpass.private.coffee/api/interpreter",
)
OVERPASS_QUERY = """[out:json][timeout:25];
node(12.75,77.35,13.25,78.00)["station"="subway"]["railway"="station"];
out body;
"""


def collect_prestige_inventory(
    request: Dict[str, Any],
    society_inputs: Dict[str, Dict[str, Any]],
    fetch_projects: Callable[[str], List[Dict[str, Any]]] = None,
) -> Dict[str, Any]:
    observed_at = normalized_planned_at(request)
    snapshot_date = partition_values(request).get("dt") or observed_at[:10]
    fetch = fetch_projects or fetch_prestige_projects
    records = []

    for _, input_data in sorted(society_inputs.items()):
        project_name = optional_string(
            input_data.get("society_name")
            or input_data.get("name")
            or input_data.get("project_name")
        )
        entity_id = optional_string(input_data.get("entity_id"))
        if not project_name or not entity_id:
            continue
        candidates = fetch(project_name)
        exact = [
            candidate
            for candidate in candidates
            if normalized_name(candidate.get("ProjectName")) == normalized_name(project_name)
        ]
        if len(exact) != 1:
            raise ValueError(
                "Prestige inventory expected one exact project for {!r}, found {}".format(
                    project_name, len(exact)
                )
            )
        project = exact[0]
        slug = required_string(project.get("Project_slug"), "Project_slug")
        city = optional_string(project.get("CityText")) or "Bangalore"
        coordinates = (project.get("LatLong") or {}).get("coordinates") or []
        latitude = optional_float(coordinates[0] if len(coordinates) > 0 else None)
        longitude = optional_float(coordinates[1] if len(coordinates) > 1 else None)
        records.append(
            {
                "entity_id": entity_id,
                "project_key": optional_string(input_data.get("project_key")),
                "source_project_id": required_string(
                    project.get("ProjectID") or project.get("_id"), "ProjectID"
                ),
                "source_project_name": required_string(
                    project.get("ProjectName"), "ProjectName"
                ),
                "source_project_slug": slug,
                "source_url": PRESTIGE_PROJECT_URL.format(
                    city=normalized_slug(city), slug=slug
                ),
                "status": optional_string(project.get("ProjectStatus")),
                "land_area_acres": parse_acres(project.get("Size")),
                "starting_price_inr": parse_price_inr(project.get("DisplayPrice")),
                "price_display": optional_string(project.get("DisplayPrice")),
                "bhk_options": parse_bhk_options(project.get("bedroomdisplaytext")),
                "total_units": optional_int(project.get("total_unit")),
                "latitude": latitude,
                "longitude": longitude,
                "maps_url": optional_string(project.get("location_url_link")),
                "address": optional_string(project.get("Address")),
                "observed_at": observed_at,
            }
        )

    return {
        "snapshot_date": snapshot_date,
        "records": records,
        "source_watermarks": [
            {"source": "prestige_project_inventory_api", "high_watermark": observed_at}
        ],
    }


def fetch_prestige_projects(project_name: str) -> List[Dict[str, Any]]:
    body = urlencode(
        {
            "dynamicurl": "managecontent/v2/projectinventorycms/list",
            "propertycategory": "Residential",
            "is_available": "true",
            "CityText": "bangalore",
            "search": project_name,
            "page": "1",
            "size": "20",
        }
    ).encode("utf-8")
    request = Request(
        PRESTIGE_API_URL,
        data=body,
        headers={
            "Accept": "application/json",
            "Content-Type": "application/x-www-form-urlencoded",
            "User-Agent": "OpenEstates/1.0 (+https://github.com/kumargu/openestates)",
        },
    )
    with urlopen(request, timeout=30) as response:
        payload = json.load(response)
    if not payload.get("success") or not isinstance(payload.get("data"), list):
        raise ValueError("Prestige inventory returned an invalid response")
    return payload["data"]


def collect_metro_stations(
    request: Dict[str, Any],
    fetch_payload: Callable[[], Dict[str, Any]] = None,
) -> Dict[str, Any]:
    observed_at = normalized_planned_at(request)
    snapshot_date = partition_values(request).get("dt") or observed_at[:10]
    payload = (fetch_payload or fetch_overpass_stations)()
    records = []
    for element in payload.get("elements", []):
        tags = element.get("tags") or {}
        name = optional_string(tags.get("name"))
        latitude = optional_float(element.get("lat"))
        longitude = optional_float(element.get("lon"))
        if not name or latitude is None or longitude is None:
            continue
        if not is_namma_metro(tags):
            continue
        station_id = "{}:{}".format(element.get("type") or "node", element.get("id"))
        records.append(
            {
                "station_id": station_id,
                "name": name,
                "network": optional_string(tags.get("network")),
                "operator": optional_string(tags.get("operator")),
                "status": station_status(tags, observed_at[:10]),
                "latitude": latitude,
                "longitude": longitude,
                "source_url": "https://www.openstreetmap.org/{}/{}".format(
                    element.get("type") or "node", element.get("id")
                ),
                "observed_at": observed_at,
            }
        )
    records.sort(key=lambda record: (record["name"].lower(), record["station_id"]))
    if not records:
        raise ValueError("Overpass returned no usable Namma Metro stations")
    watermark = (
        (payload.get("osm3s") or {}).get("timestamp_osm_base") or observed_at
    )
    return {
        "snapshot_date": snapshot_date,
        "records": records,
        "source_watermarks": [
            {"source": "openstreetmap_overpass", "high_watermark": watermark}
        ],
    }


def fetch_overpass_stations() -> Dict[str, Any]:
    request_body = urlencode({"data": OVERPASS_QUERY}).encode("utf-8")
    failures = []
    for endpoint in OVERPASS_API_URLS:
        request = Request(
            endpoint,
            data=request_body,
            headers={
                "Content-Type": "application/x-www-form-urlencoded",
                "User-Agent": "OpenEstates/1.0 (+https://github.com/kumargu/openestates)",
            },
        )
        try:
            with urlopen(request, timeout=60) as response:
                payload = json.load(response)
            if not isinstance(payload.get("elements"), list):
                raise ValueError("invalid station response")
            return payload
        except (HTTPError, URLError, TimeoutError, ValueError) as error:
            failures.append("{}: {}".format(endpoint, error))
    raise RuntimeError("all Overpass endpoints failed: {}".format("; ".join(failures)))


def is_namma_metro(tags: Dict[str, Any]) -> bool:
    values = " ".join(
        str(tags.get(key) or "")
        for key in ("network", "operator", "brand", "description")
    ).lower()
    return any(
        marker in values
        for marker in ("namma metro", "bangalore metro", "bengaluru metro", "bmrcl")
    )


def station_status(tags: Dict[str, Any], observed_date: str) -> str:
    if any(tags.get(key) for key in ("construction", "proposed", "disused")):
        return "non_operational"
    start_date = optional_string(tags.get("start_date"))
    if start_date:
        match = re.match(r"^(\d{4})-(\d{2})-(\d{2})", start_date)
        if match:
            starts = date(*map(int, match.groups()))
            observed = datetime.strptime(observed_date, "%Y-%m-%d").date()
            if starts > observed:
                return "non_operational"
    return "operational"


def parse_acres(value: Any):
    text = optional_string(value)
    if not text:
        return None
    match = re.search(r"([0-9]+(?:\.[0-9]+)?)\s*acres?", text, re.I)
    return float(match.group(1)) if match else None


def parse_price_inr(value: Any):
    text = optional_string(value)
    if not text:
        return None
    match = re.search(r"([0-9]+(?:\.[0-9]+)?)", text.replace(",", ""))
    if not match:
        return None
    amount = float(match.group(1))
    normalized = text.lower()
    if "crore" in normalized or re.search(r"\bcr\b", normalized):
        return amount * 10_000_000
    if "lakh" in normalized or re.search(r"\blac\b", normalized):
        return amount * 100_000
    return amount


def parse_bhk_options(value: Any) -> List[str]:
    text = optional_string(value) or ""
    seen = set()
    options = []
    for match in re.findall(r"\d+(?:\.\d+)?", text):
        normalized = match.rstrip("0").rstrip(".") if "." in match else match
        if normalized not in seen:
            seen.add(normalized)
            options.append(normalized)
    return options


def normalized_name(value: Any) -> str:
    return re.sub(r"[^a-z0-9]+", " ", str(value or "").lower()).strip()


def normalized_slug(value: Any) -> str:
    return re.sub(r"[^a-z0-9]+", "-", str(value or "").lower()).strip("-")


def required_string(value: Any, field: str) -> str:
    result = optional_string(value)
    if not result:
        raise ValueError("Prestige inventory is missing {}".format(field))
    return result


def optional_string(value: Any):
    if value is None:
        return None
    text = str(value).strip()
    return text or None


def optional_float(value: Any):
    if value is None or value == "":
        return None
    try:
        return float(value)
    except (TypeError, ValueError):
        return None


def optional_int(value: Any):
    number = optional_float(value)
    return int(number) if number is not None else None


def partition_values(request: Dict[str, Any]) -> Dict[str, str]:
    partition = request.get("partition", {})
    return {str(key): str(value) for key, value in partition.get("parts", [])}


def normalized_planned_at(request: Dict[str, Any]) -> str:
    return str(request.get("planned_at") or "").strip()
