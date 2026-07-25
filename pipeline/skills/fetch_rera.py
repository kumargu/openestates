"""
fetch_rera — scrape Karnataka RERA portal for real project data.

Replaces the old guessed RERA verifier.
This skill fetches actual government-sourced data with confidence=1.0.

Karnataka RERA portal flow:
  1. GET /viewAllProjects — 6MB HTML with 9,469 projects in JS arrays
  2. POST /projectViewDetails — search by project name, returns table with numeric IDs
  3. POST /projectDetails — detail page (needs session cookie), returns multi-tab HTML

No external dependencies — stdlib only.
"""

import http.cookiejar
import html as html_module
import json
import logging
import os
import re
import time
from dataclasses import dataclass, field
from datetime import datetime, timezone, timedelta
from pathlib import Path
from typing import Dict, List, Optional, Tuple
from urllib.request import Request, build_opener, HTTPCookieProcessor
from urllib.parse import urlencode

from pipeline.skills.base import BaseSkill, SkillResult, SkillCost, SourcedFact, FactSource

logger = logging.getLogger(__name__)

RERA_BASE = "https://rera.karnataka.gov.in"
LISTING_URL = f"{RERA_BASE}/viewAllProjects?language=en"
SEARCH_URL = f"{RERA_BASE}/projectViewDetails"
DETAIL_URL = f"{RERA_BASE}/projectDetails"

LISTING_CACHE_PATH = Path("data/cache/skills/rera_listing.json")
LISTING_CACHE_TTL_DAYS = 7

DETAIL_CACHE_DIR = Path("data/cache/skills/rera_details")
DETAIL_CACHE_TTL_DAYS = 30

# Rate limiting: 1 second between detail page fetches
_last_detail_fetch_time = 0.0


def _parse_cache_timestamp(value: str) -> datetime:
    """Parse an ISO-ish UTC timestamp on Python 3.6.

    Python 3.6 does not have datetime.fromisoformat(), but the cache only needs
    enough precision to decide whether to refresh. Treat stored offsets as UTC.
    """
    value = (value or "2000-01-01T00:00:00").replace("Z", "+00:00")
    value = re.sub(r'([+-]\d{2}:\d{2})$', '', value)
    for fmt in ("%Y-%m-%dT%H:%M:%S.%f", "%Y-%m-%dT%H:%M:%S", "%Y-%m-%d"):
        try:
            return datetime.strptime(value, fmt).replace(tzinfo=timezone.utc)
        except ValueError:
            continue
    return datetime(2000, 1, 1, tzinfo=timezone.utc)


# ---------------------------------------------------------------------------
# Data classes
# ---------------------------------------------------------------------------

@dataclass
class ReraListingEntry:
    ack_number: str
    registration_number: str
    project_name: str
    promoter_name: str


@dataclass
class ReraSearchResult:
    ack_number: str
    registration_number: str
    project_name: str
    promoter_name: str
    status: str
    district: str
    taluk: str
    project_type: str
    approved_on: str
    completion_date: str
    original_completion_date: str
    numeric_id: str  # DOM element ID used for detail fetch
    certificate_url: Optional[str] = None


@dataclass
class ReraDocumentArtifact:
    artifact_id: str
    document_kind: str
    label: str
    source_url: Optional[str] = None
    configuration_type: Optional[str] = None
    bedroom_count: Optional[float] = None
    confidence: float = 0.7


@dataclass
class ReraUnitConfiguration:
    configuration_type: str
    bedroom_count: Optional[float] = None
    floor_plan_asset_id: Optional[str] = None
    tower_label: Optional[str] = None
    confidence: float = 0.75


@dataclass
class ReraProjectDetail:
    # From search result
    ack_number: str = ""
    registration_number: str = ""
    project_name: str = ""
    promoter_name: str = ""
    status: str = ""
    approved_on: str = ""

    # From detail page - Project info
    project_type: str = ""
    project_sub_type: str = ""
    project_address: str = ""
    district: str = ""
    taluk: str = ""
    latitude: Optional[str] = None
    longitude: Optional[str] = None
    start_date: str = ""
    completion_date: str = ""
    original_completion_date: str = ""

    # Units and area
    total_units: Optional[int] = None
    open_parking: Optional[int] = None
    covered_parking: Optional[int] = None
    total_land_area_sqm: Optional[float] = None
    total_carpet_area_sqm: Optional[float] = None
    total_builtup_area_sqm: Optional[float] = None
    open_area_pct: Optional[float] = None
    far_sanctioned: Optional[float] = None
    num_towers: Optional[int] = None
    max_floor_count: Optional[int] = None

    # Cost
    land_cost_inr: Optional[float] = None
    construction_cost_inr: Optional[float] = None
    total_project_cost_inr: Optional[float] = None
    has_borrowing: Optional[bool] = None
    has_mortgage: Optional[bool] = None

    # Escrow
    escrow_bank: Optional[str] = None
    escrow_account: Optional[str] = None
    escrow_ifsc: Optional[str] = None

    # Land
    land_litigation: Optional[bool] = None
    survey_numbers: List[str] = field(default_factory=list)

    # Builder track record
    builder_other_rera_projects: int = 0
    builder_revocations: int = 0
    builder_states: List[str] = field(default_factory=list)

    # Complaints (across all promoter's projects)
    complaints_count: int = 0
    complaints_resolved: int = 0

    # Deep RERA schedules / artifacts
    parking_total_car_count: Optional[int] = None
    parking_basement_count: Optional[int] = None
    parking_surface_count: Optional[int] = None
    parking_visitor_count: Optional[int] = None
    parking_accessible_count: Optional[int] = None
    parking_ev_ready_count: Optional[int] = None
    parking_two_wheeler_count: Optional[int] = None
    parking_offered_for_sale_count: Optional[int] = None
    stp_count: Optional[int] = None
    stp_capacity_kld: Optional[float] = None
    borewell_proposed_count: Optional[int] = None
    borewell_existing_count: Optional[int] = None
    borewell_depth_ft: Optional[float] = None
    borewell_yield_lph: Optional[float] = None
    document_artifacts: List[ReraDocumentArtifact] = field(default_factory=list)
    configurations: List[ReraUnitConfiguration] = field(default_factory=list)

    # Certificate
    certificate_url: Optional[str] = None

    # Numeric ID for linking
    numeric_id: str = ""


# ---------------------------------------------------------------------------
# Session management
# ---------------------------------------------------------------------------

class ReraSession:
    """Manages session cookies for RERA portal scraping."""

    _USER_AGENT = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36"

    def __init__(self):
        self.cj = http.cookiejar.CookieJar()
        self.opener = build_opener(HTTPCookieProcessor(self.cj))
        self._session_initialized = False

    def _ensure_session(self):
        """Hit the listing page to get a JSESSIONID cookie."""
        if self._session_initialized:
            return
        req = Request(LISTING_URL, headers={"User-Agent": self._USER_AGENT})
        try:
            self.opener.open(req, timeout=60).read()
            self._session_initialized = True
            logger.info("RERA session initialized (JSESSIONID acquired)")
        except Exception as e:
            logger.error("Failed to initialize RERA session: %s", e)
            raise

    def get(self, url: str, timeout: int = 60) -> str:
        """GET request with session cookies."""
        self._ensure_session()
        req = Request(url, headers={"User-Agent": self._USER_AGENT})
        with self.opener.open(req, timeout=timeout) as resp:
            return resp.read().decode("utf-8", errors="replace")

    def post(self, url: str, data: dict, ajax: bool = False, timeout: int = 60) -> str:
        """POST request with session cookies."""
        self._ensure_session()
        encoded = urlencode(data).encode()
        headers = {
            "User-Agent": self._USER_AGENT,
            "Content-Type": "application/x-www-form-urlencoded",
            "Referer": f"{RERA_BASE}/viewAllProjects?language=en",
        }
        if ajax:
            headers["X-Requested-With"] = "XMLHttpRequest"
            headers["Accept"] = "text/html, */*; q=0.01"
        req = Request(url, data=encoded, headers=headers, method="POST")
        with self.opener.open(req, timeout=timeout) as resp:
            return resp.read().decode("utf-8", errors="replace")


# ---------------------------------------------------------------------------
# HTML parsing helpers
# ---------------------------------------------------------------------------

def _clean_html(html_text: str) -> str:
    """Strip HTML tags and normalize whitespace."""
    text = re.sub(r'<[^>]+>', ' ', html_text)
    text = re.sub(r'\s+', ' ', text)
    return html_module.unescape(text.strip())


def _extract_number(text: str, pattern: str) -> Optional[float]:
    """Extract a number following a label pattern from text.

    Tolerates parenthetical content between the label and the colon, e.g.:
      "Cost of Land (INR) (C1) ( As Certified by CA in Form 1 ) : 148670485"
      "Total Area Of Land (Sq Mtr) (A1+A2) : 3946"

    The key insight: look for a colon followed by the number, since RERA
    always uses "label (annotations) : value" format.
    """
    # Strategy: find the label, then look for ": <number>" within 300 chars
    label_match = re.search(pattern, text, re.IGNORECASE)
    if not label_match:
        return None

    # Search for ": number" after the label (within a reasonable window)
    remainder = text[label_match.end():label_match.end() + 300]
    num_match = re.search(r':\s*([\d,]+(?:\.\d+)?)', remainder)
    if num_match:
        try:
            return float(num_match.group(1).replace(',', ''))
        except ValueError:
            pass

    # Fallback: simple pattern (label directly followed by number)
    simple = re.search(pattern + r'\s*:?\s*([\d,]+(?:\.\d+)?)', text, re.IGNORECASE)
    if simple:
        try:
            return float(simple.group(1).replace(',', ''))
        except ValueError:
            pass
    return None


def _extract_int(text: str, pattern: str) -> Optional[int]:
    """Extract an integer following a label pattern."""
    val = _extract_number(text, pattern)
    return int(val) if val is not None else None


def _extract_ints(text: str, pattern: str) -> List[int]:
    """Extract all integer values following a label-like pattern."""
    values: List[int] = []
    for match in re.finditer(pattern + r'\s*:?\s*([\d,]+)', text, re.IGNORECASE):
        try:
            values.append(int(match.group(1).replace(",", "")))
        except ValueError:
            continue
    return values


def _extract_text(text: str, pattern: str) -> Optional[str]:
    """Extract text value following a label pattern.

    Handles RERA-style labels with parenthetical annotations:
      "Project Type : Residential/Group Housing"
      "Registration Start Date : 01-01-2023"
      "Total Number of Inventories/Flats/Villas : 33"

    Uses a "next label" detection pattern that requires a sequence of
    Capitalized Words (2+ words) followed by a colon, to avoid splitting
    on words like "Housing" in "Residential/Group Housing".
    """
    # Find the label first
    label_match = re.search(pattern, text, re.IGNORECASE)
    if not label_match:
        return None

    # Look for ": value" after the label (within reasonable window)
    remainder = text[label_match.end():label_match.end() + 500]
    colon_match = re.search(r':\s*(.+?)(?:\s{2,}|$)', remainder)
    if colon_match:
        val = colon_match.group(1).strip()
        val = _truncate_at_next_label(val)
        if val and val.lower() not in ('', 'null', 'none', 'n/a', '-'):
            return val

    # Fallback: direct pattern match
    direct = re.search(pattern + r'\s*:\s*(.+?)(?:\s{2,}|$)', text, re.IGNORECASE)
    if direct:
        val = direct.group(1).strip()
        val = _truncate_at_next_label(val)
        if val and val.lower() not in ('', 'null', 'none', 'n/a', '-'):
            return val
    return None


def _truncate_at_next_label(val: str) -> str:
    """Truncate a value string at the next RERA-style label.

    RERA labels follow a "Key Phrase :" pattern. We look for known label
    keywords that signal the start of a new field, then cut before them.
    """
    # Known RERA field prefixes that signal a new label
    _RERA_LABELS = [
        "Project Name", "Project Type", "Project Sub Type", "Project Status",
        "Project Start Date", "Project Description", "Project Address",
        "Proposed Completion Date", "Proposed Project Completion Date", "Registration Start Date",
        "Total Number", "Number of Towers", "No. of Open", "No. of Covered",
        "No of Open", "No of Covered",
        "Total Area", "Total Carpet Area", "Total Built", "Total Coverd",
        "Total Open Area",
        "Cost of Land", "Cost of Layout", "Total Project Cost",
        "Total Construction Cost",
        "FAR Sanctioned", "Certificate",
        "District", "Taluk", "Latitude", "Longitude",
        "Bank Name", "Account No", "IFSC Code", "Branch",
        "Survey Number", "Land Type", "Land Litigation",
        "Promoter Name", "Registration Number", "PAN",
        "Is there any Borrowing", "Is there Any Mortgage",
        "Pin Code", "State", "Branch",
        "North Schedule", "South Schedule", "East Schedule", "West Schedule",
        "Plan Details", "Approving Authority", "Approved Plan",
        "Have you applied",
    ]

    best_pos = len(val)
    for label in _RERA_LABELS:
        idx = val.find(label)
        if idx > 0:  # Must be after position 0 (not at start of value)
            # Verify it's preceded by whitespace (word boundary)
            if val[idx - 1] in (' ', '\t', '\n'):
                best_pos = min(best_pos, idx)

    return val[:best_pos].strip()


def _parse_rera_date(date_str: str) -> Optional[datetime]:
    """Parse RERA date formats: DD-MM-YYYY or DD/MM/YYYY."""
    if not date_str:
        return None
    for fmt in ("%d-%m-%Y", "%d/%m/%Y", "%Y-%m-%d"):
        try:
            return datetime.strptime(date_str.strip(), fmt)
        except ValueError:
            continue
    return None


def _iso_rera_date(date_str: str) -> Optional[str]:
    parsed = _parse_rera_date(date_str)
    return parsed.strftime("%Y-%m-%d") if parsed else None


def _slug(value: str) -> str:
    return re.sub(r"[^a-z0-9]+", "-", str(value or "").lower()).strip("-")


def _canonical_configuration(text: str) -> Tuple[Optional[str], Optional[float]]:
    normalized = re.sub(r"\s+", " ", text or "").strip().lower()
    bhk_match = re.search(r"\b([1-6](?:\.5)?)\s*(?:bhk|b h k|bed)\b", normalized)
    if bhk_match:
        bedrooms = float(bhk_match.group(1))
        label = ("%gBHK" % bedrooms).replace(".0", "")
        return label, bedrooms
    if any(term in normalized for term in ("penthouse", "duplex")):
        return "penthouse", None
    if any(term in normalized for term in ("villa", "row house", "rowhouse")):
        return "villa", None
    return None, None


def _extract_coordinate(text: str, label: str, minimum: float, maximum: float) -> Optional[str]:
    idx = text.lower().find(label.lower())
    if idx < 0:
        return None
    vicinity = text[idx:idx + 180]

    dms = re.search(
        r"(\d{1,3})\D+(\d{1,2})\D+(\d{1,2}(?:\.\d+)?)\D*([NSEW])?",
        vicinity,
        re.IGNORECASE,
    )
    if dms and (re.search(r"[°'\"]", vicinity) or dms.group(4)):
        degrees = float(dms.group(1))
        minutes = float(dms.group(2))
        seconds = float(dms.group(3))
        value = degrees + minutes / 60.0 + seconds / 3600.0
        direction = (dms.group(4) or "").upper()
        if direction in ("S", "W"):
            value = -value
        if minimum <= value <= maximum:
            return "{:.6f}".format(value)

    decimal = re.search(r"([+-]?\d{1,3}\.\d{2,})", vicinity)
    if decimal:
        value = float(decimal.group(1))
        if minimum <= value <= maximum:
            return decimal.group(1)
    return None


def _count_keywords(text: str, keywords: Tuple[str, ...]) -> Optional[int]:
    total = 0
    for keyword in keywords:
        total += len(re.findall(r"\b{}\b".format(re.escape(keyword)), text, re.IGNORECASE))
    return total or None


# ---------------------------------------------------------------------------
# Listing scraper (cached to disk for 7 days)
# ---------------------------------------------------------------------------

def scrape_rera_listing(force: bool = False) -> List[ReraListingEntry]:
    """Scrape full RERA project listing. Cached to disk for 7 days.

    The listing page contains ~9,469 projects as JavaScript .push() calls
    across four parallel arrays.
    """
    # Check cache
    if not force and LISTING_CACHE_PATH.exists():
        try:
            cache_data = json.loads(LISTING_CACHE_PATH.read_text())
            cached_at = _parse_cache_timestamp(cache_data.get("cached_at", "2000-01-01"))
            if datetime.now(timezone.utc) - cached_at < timedelta(days=LISTING_CACHE_TTL_DAYS):
                entries = [ReraListingEntry(**e) for e in cache_data["entries"]]
                logger.info("Using cached RERA listing: %d entries", len(entries))
                return entries
        except (json.JSONDecodeError, KeyError, TypeError) as e:
            logger.warning("Failed to read listing cache, will re-fetch: %s", e)

    # Fetch fresh listing page (can be ~6MB)
    logger.info("Fetching RERA listing page (this may take a moment)...")
    session = ReraSession()
    body = session.get(LISTING_URL, timeout=120)

    # Parse the four JS arrays
    lists = {}
    for suffix in ["", "2", "3", "4"]:
        name = f"applicationNameList{suffix}"
        pattern = rf"{name}\s*\.push\('([^']*)'\)"
        lists[name] = re.findall(pattern, body)

    ack_list = lists["applicationNameList"]
    reg_list = lists["applicationNameList2"]
    name_list = lists["applicationNameList3"]
    promoter_list = lists["applicationNameList4"]

    count = min(len(ack_list), len(reg_list), len(name_list), len(promoter_list))
    if count == 0:
        logger.error(
            "RERA listing parse failed: got 0 entries. Array sizes: %s",
            {k: len(v) for k, v in lists.items()},
        )
        return []

    entries = []
    for i in range(count):
        entries.append(ReraListingEntry(
            ack_number=ack_list[i],
            registration_number=reg_list[i],
            project_name=html_module.unescape(name_list[i]),
            promoter_name=html_module.unescape(promoter_list[i]),
        ))

    # Persist to disk cache
    LISTING_CACHE_PATH.parent.mkdir(parents=True, exist_ok=True)
    LISTING_CACHE_PATH.write_text(json.dumps({
        "cached_at": datetime.now(timezone.utc).isoformat(),
        "count": len(entries),
        "entries": [
            {
                "ack_number": e.ack_number,
                "registration_number": e.registration_number,
                "project_name": e.project_name,
                "promoter_name": e.promoter_name,
            }
            for e in entries
        ],
    }, indent=2))

    logger.info("Scraped RERA listing: %d entries", len(entries))
    return entries


# ---------------------------------------------------------------------------
# Search
# ---------------------------------------------------------------------------

def search_rera_project(session: ReraSession, project_name: str) -> Optional[ReraSearchResult]:
    """Search RERA portal for a project and return the first matching result.

    Posts to /projectViewDetails with the project name and parses the result
    table to extract the numeric ID needed for the detail page.
    """
    body = session.post(SEARCH_URL, {
        "project": project_name,
        "promoter": "",
        "registrationNo": "",
        "district": "",
        "taluk": "",
        "applicationNo": "",
    })

    # Extract all table rows (skip header)
    rows = re.findall(r'<tr[^>]*>(.*?)</tr>', body, re.DOTALL)
    if not rows:
        logger.warning("RERA search returned no table rows for '%s'", project_name)
        return None

    # Find the row with the best match — look for the onclick handler with numeric id
    for row in rows:
        id_match = re.search(
            r'<a\s+id="(\d+)"[^>]*onclick="return showFileApplicationPreview',
            row,
        )
        if not id_match:
            continue

        # Parse table cells from this row
        tds = re.findall(r'<td[^>]*>(.*?)</td>', row, re.DOTALL)
        if len(tds) < 13:
            continue

        # Extract certificate URL if present
        cert_match = re.search(r'href="(/certificate\?CER_NO=[^"]+)"', row)
        cert_url = f"{RERA_BASE}{cert_match.group(1)}" if cert_match else None

        return ReraSearchResult(
            ack_number=_clean_html(tds[1]) if len(tds) > 1 else "",
            registration_number=_clean_html(tds[2]) if len(tds) > 2 else "",
            promoter_name=_clean_html(tds[4]) if len(tds) > 4 else "",
            project_name=_clean_html(tds[5]) if len(tds) > 5 else project_name,
            status=_clean_html(tds[6]) if len(tds) > 6 else "",
            district=_clean_html(tds[7]) if len(tds) > 7 else "",
            taluk=_clean_html(tds[8]) if len(tds) > 8 else "",
            project_type=_clean_html(tds[9]) if len(tds) > 9 else "",
            approved_on=_clean_html(tds[10]) if len(tds) > 10 else "",
            completion_date=_clean_html(tds[11]) if len(tds) > 11 else "",
            original_completion_date=_clean_html(tds[12]) if len(tds) > 12 else "",
            numeric_id=id_match.group(1),
            certificate_url=cert_url,
        )

    # Fallback: maybe the id is outside the row tags — search the whole body
    id_match = re.search(
        r'<a\s+id="(\d+)"[^>]*onclick="return showFileApplicationPreview',
        body,
    )
    if not id_match:
        logger.warning("Could not find numeric ID for '%s'", project_name)
        return None

    # Parse all tds from body as a flat list (single-result fallback)
    tds = re.findall(r'<td[^>]*>(.*?)</td>', body, re.DOTALL)
    if len(tds) < 13:
        logger.warning(
            "RERA search returned insufficient data for '%s' (%d cells)",
            project_name, len(tds),
        )
        return None

    cert_match = re.search(r'href="(/certificate\?CER_NO=[^"]+)"', body)
    cert_url = f"{RERA_BASE}{cert_match.group(1)}" if cert_match else None

    return ReraSearchResult(
        ack_number=_clean_html(tds[1]),
        registration_number=_clean_html(tds[2]),
        promoter_name=_clean_html(tds[4]) if len(tds) > 4 else "",
        project_name=_clean_html(tds[5]) if len(tds) > 5 else project_name,
        status=_clean_html(tds[6]) if len(tds) > 6 else "",
        district=_clean_html(tds[7]) if len(tds) > 7 else "",
        taluk=_clean_html(tds[8]) if len(tds) > 8 else "",
        project_type=_clean_html(tds[9]) if len(tds) > 9 else "",
        approved_on=_clean_html(tds[10]) if len(tds) > 10 else "",
        completion_date=_clean_html(tds[11]) if len(tds) > 11 else "",
        original_completion_date=_clean_html(tds[12]) if len(tds) > 12 else "",
        numeric_id=id_match.group(1),
        certificate_url=cert_url,
    )


# ---------------------------------------------------------------------------
# Detail page
# ---------------------------------------------------------------------------

def _rate_limit_detail():
    """Enforce 1-second delay between detail page fetches."""
    global _last_detail_fetch_time
    now = time.time()
    elapsed = now - _last_detail_fetch_time
    if elapsed < 1.0:
        time.sleep(1.0 - elapsed)
    _last_detail_fetch_time = time.time()


def _get_detail_cache_path(numeric_id: str) -> Path:
    """Return path to cached detail HTML for a project."""
    return DETAIL_CACHE_DIR / f"{numeric_id}.html"


def fetch_rera_detail(session: ReraSession, numeric_id: str) -> str:
    """Fetch the full project detail HTML page, with disk caching.

    Detail pages are cached for 30 days since RERA data changes slowly.
    """
    cache_path = _get_detail_cache_path(numeric_id)

    # Check disk cache
    if cache_path.exists():
        try:
            stat = cache_path.stat()
            age_days = (time.time() - stat.st_mtime) / 86400
            if age_days < DETAIL_CACHE_TTL_DAYS:
                logger.debug("Using cached detail for ID %s (%.0f days old)", numeric_id, age_days)
                return cache_path.read_text(encoding="utf-8", errors="replace")
        except OSError:
            pass

    _rate_limit_detail()

    html = session.post(DETAIL_URL, {"action": numeric_id}, ajax=True, timeout=120)

    # Cache to disk
    cache_path.parent.mkdir(parents=True, exist_ok=True)
    try:
        cache_path.write_text(html, encoding="utf-8")
    except OSError as e:
        logger.warning("Failed to cache detail HTML for ID %s: %s", numeric_id, e)

    return html


# ---------------------------------------------------------------------------
# Detail page parsing
# ---------------------------------------------------------------------------

def _get_tab_text(detail_html: str, tab_id: str) -> str:
    """Extract cleaned text from a specific tab section of the detail page.

    Tabs are identified by id="home", id="menu1", id="menu2", etc.
    We take the HTML between this tab's id and the next tab's id, then strip tags.
    """
    idx = detail_html.find(f'id="{tab_id}"')
    if idx < 0:
        return ""

    # Find end: next tab boundary or end of document
    tab_ids = ["home", "menu1", "menu2", "menu3", "menu4", "menu5", "menu6", "menu7"]
    end = len(detail_html)
    for other_tab in tab_ids:
        if other_tab == tab_id:
            continue
        other_idx = detail_html.find(f'id="{other_tab}"', idx + len(tab_id) + 10)
        if other_idx > 0:
            end = min(end, other_idx)

    section = detail_html[idx:end]
    return _clean_html(section)


def _project_details_tab_text(detail_html: str) -> str:
    """Return the tab text that carries RERA project-detail labels."""
    candidates = [_get_tab_text(detail_html, tab_id) for tab_id in ("menu2", "menu1")]
    return max(candidates, key=_project_detail_score, default="")


def _project_detail_score(text: str) -> int:
    labels = (
        "Project Address",
        "Project Type",
        "Project Status",
        "Project Start Date",
        "Proposed Completion Date",
        "Total Project Cost",
        "Total Number of Inventories",
    )
    return sum(1 for label in labels if label in text)


def _check_yes_no(text: str, label: str) -> Optional[bool]:
    """Check if a label in text is followed by Yes or No."""
    idx = text.lower().find(label.lower())
    if idx < 0:
        return None
    vicinity = text[idx:idx + len(label) + 80].upper()
    if "YES" in vicinity:
        return True
    if "NO" in vicinity:
        return False
    return None


def _extract_document_artifacts(detail_html: str, project_name: str) -> List[ReraDocumentArtifact]:
    artifacts: List[ReraDocumentArtifact] = []
    seen = set()
    for match in re.finditer(r"<a\b([^>]*)>(.*?)</a>", detail_html, re.IGNORECASE | re.DOTALL):
        attrs = match.group(1)
        label = _clean_html(match.group(2))
        href_match = re.search(r'href\s*=\s*["\']([^"\']+)["\']', attrs, re.IGNORECASE)
        href = href_match.group(1) if href_match else None
        surrounding = _clean_html(detail_html[max(0, match.start() - 240):match.end() + 240])
        link_text = "{} {}".format(label, href or "").strip()
        combined = "{} {}".format(link_text, surrounding).strip()
        kind = _document_kind(link_text, href)
        if not kind:
            continue
        source_url = f"{RERA_BASE}{href}" if href and href.startswith("/") else href
        artifact_id = "{}:{}:{}".format(
            _slug(project_name),
            kind,
            _slug(label or source_url or str(len(artifacts) + 1)),
        )
        if artifact_id in seen:
            continue
        seen.add(artifact_id)
        config_type, bedrooms = _canonical_configuration(combined)
        artifacts.append(
            ReraDocumentArtifact(
                artifact_id=artifact_id,
                document_kind=kind,
                label=label or kind.replace("_", " ").title(),
                source_url=source_url,
                configuration_type=config_type if kind == "floor_plan" else None,
                bedroom_count=bedrooms if kind == "floor_plan" else None,
                confidence=0.85 if source_url else 0.65,
            )
        )

    if not any(artifact.document_kind == "site_plan" for artifact in artifacts):
        if re.search(r"\b(site plan|layout plan|master plan)\b", detail_html, re.IGNORECASE):
            artifacts.append(
                ReraDocumentArtifact(
                    artifact_id="{}:site_plan:detected".format(_slug(project_name)),
                    document_kind="site_plan",
                    label="Site plan detected",
                    confidence=0.55,
                )
            )
    return artifacts


def _document_kind(text: str, href: Optional[str]) -> Optional[str]:
    combined = "{} {}".format(text or "", href or "").lower()
    if any(term in combined for term in ("floor plan", "floorplan", "unit plan", "flat plan")):
        return "floor_plan"
    if any(term in combined for term in ("site plan", "siteplan", "layout plan", "master plan")):
        return "site_plan"
    if any(term in combined for term in ("sanction plan", "approved plan", "approval plan")):
        return "sanction_plan"
    return None


def _extract_configurations(
    project_text: str, artifacts: List[ReraDocumentArtifact]
) -> List[ReraUnitConfiguration]:
    by_label: Dict[str, ReraUnitConfiguration] = {}
    for artifact in artifacts:
        if artifact.document_kind != "floor_plan" or not artifact.configuration_type:
            continue
        by_label[artifact.configuration_type] = ReraUnitConfiguration(
            configuration_type=artifact.configuration_type,
            bedroom_count=artifact.bedroom_count,
            floor_plan_asset_id=artifact.artifact_id,
            confidence=artifact.confidence,
        )

    for match in re.finditer(r"\b([1-6](?:\.5)?)\s*(?:BHK|B H K|BED)\b", project_text, re.IGNORECASE):
        bedrooms = float(match.group(1))
        label = ("%gBHK" % bedrooms).replace(".0", "")
        by_label.setdefault(
            label,
            ReraUnitConfiguration(
                configuration_type=label,
                bedroom_count=bedrooms,
                confidence=0.65,
            ),
        )
    return sorted(by_label.values(), key=lambda item: (item.bedroom_count or 99, item.configuration_type))


def _extract_open_area_pct(project_details: str, total_land_area_sqm: Optional[float]) -> Optional[float]:
    explicit = _extract_number(project_details, r"Open Area.*?%") or _extract_number(
        project_details, r"Percentage of Open Area"
    )
    if explicit is not None and 0.0 < explicit <= 100.0:
        return explicit
    open_area = _extract_number(project_details, r"Total Open Area")
    if open_area and total_land_area_sqm and total_land_area_sqm > 0:
        pct = open_area / total_land_area_sqm * 100.0
        if 0.0 < pct <= 100.0:
            return round(pct, 2)
    return None


def _extract_infra_counts(text: str, detail: ReraProjectDetail) -> None:
    detail.stp_count = (
        _extract_int(text, r"No\.?\s*of STP")
        or _extract_int(text, r"Number of STP")
        or _applicable_work_present(text, ("STP", "Sewage Treatment Plant"))
        or _count_keywords(text, ("STP", "Sewage Treatment Plant"))
    )
    detail.stp_capacity_kld = (
        _extract_number(text, r"STP.*?Capacity.*?\(?KLD\)?")
        or _extract_number(text, r"Sewage Treatment Plant.*?Capacity")
    )
    detail.borewell_proposed_count = (
        _extract_int(text, r"Proposed.*?Borewell")
        or _extract_int(text, r"No\.?\s*of Proposed Borewell")
    )
    detail.borewell_existing_count = (
        _extract_int(text, r"Existing.*?Borewell")
        or _extract_int(text, r"No\.?\s*of Existing Borewell")
    )
    detail.borewell_depth_ft = _extract_number(text, r"Borewell.*?Depth.*?(?:Feet|Ft)")
    detail.borewell_yield_lph = _extract_number(text, r"Borewell.*?Yield.*?(?:LPH|Ltrs)")


def _applicable_work_present(text: str, labels: Tuple[str, ...]) -> Optional[int]:
    """Return 1 when a project-schedule work row is marked applicable."""
    for label in labels:
        idx = text.lower().find(label.lower())
        if idx < 0:
            continue
        window = text[max(0, idx - 80):idx + len(label) + 120]
        if re.search(r"\bYes\b", window, re.IGNORECASE):
            return 1
    return None


def _fill_detail_wide_schedule_fields(detail_html: str, detail: ReraProjectDetail) -> None:
    """Fill fields that Karnataka RERA often keeps outside the project-detail tab."""
    text = _clean_html(detail_html)
    if not text:
        return

    if detail.total_units is None:
        detail.total_units = (
            _extract_int(text, r"Total No of Units")
            or _extract_int(text, r"No of Inventory")
            or _extract_int(text, r"Total Number of Inventories")
        )

    if detail.num_towers is None:
        detail.num_towers = _extract_int(text, r"Number of Towers")

    if detail.max_floor_count is None:
        floor_counts = _extract_ints(text, r"No\.?\s*of Floors")
        if floor_counts:
            detail.max_floor_count = max(floor_counts)

    if detail.parking_total_car_count is None:
        tower_parking_counts = _extract_ints(text, r"Total No\.?\s*of Parking")
        if tower_parking_counts:
            detail.parking_total_car_count = sum(tower_parking_counts)

    if detail.parking_offered_for_sale_count is None:
        detail.parking_offered_for_sale_count = _extract_int(text, r"No of Parking for Sale")

    _extract_infra_counts(text, detail)


def parse_rera_detail(detail_html: str, search_result: ReraSearchResult) -> ReraProjectDetail:
    """Parse the multi-tab detail HTML into structured data."""
    detail = ReraProjectDetail(
        ack_number=search_result.ack_number,
        registration_number=search_result.registration_number,
        project_name=search_result.project_name,
        promoter_name=search_result.promoter_name,
        status=search_result.status,
        approved_on=search_result.approved_on,
        original_completion_date=search_result.original_completion_date,
        completion_date=search_result.completion_date,
        district=search_result.district,
        taluk=search_result.taluk,
        certificate_url=search_result.certificate_url,
        numeric_id=search_result.numeric_id,
    )

    # --- Project Details tab — RERA has used both menu1 and menu2 for this section.
    project_details = _project_details_tab_text(detail_html)
    if project_details:
        detail.project_type = _extract_text(project_details, "Project Type") or search_result.project_type
        detail.project_sub_type = _extract_text(project_details, "Project Sub Type") or ""
        # Start date: try label extraction, fall back to finding the first date
        # in the "At the time of Registration" row
        start = _extract_text(project_details, "Registration Start Date") or _extract_text(project_details, "Project Start Date")
        if start and re.match(r'\d{2}[-/]\d{2}[-/]\d{4}', start):
            detail.start_date = start
        else:
            # Try to grab first date after "At the time of Registration"
            reg_idx = project_details.find("At the time of Registration")
            if reg_idx >= 0:
                date_match = re.search(r'(\d{2}-\d{2}-\d{4})', project_details[reg_idx:reg_idx + 100])
                if date_match:
                    detail.start_date = date_match.group(1)

        # Completion date: try to extract from detail, but only use if it
        # looks like a valid date (DD-MM-YYYY or DD/MM/YYYY). The detail page
        # often has "Proposed Completion Date" as a table header without a colon,
        # so _extract_text may grab the wrong value.
        comp = _extract_text(project_details, "Proposed Completion Date")
        if comp and re.match(r'\d{2}[-/]\d{2}[-/]\d{4}', comp):
            detail.completion_date = comp

        detail.project_address = _extract_text(project_details, "Project Address") or ""
        detail.latitude = _extract_coordinate(project_details, "Latitude", 6.0, 38.0)
        detail.longitude = _extract_coordinate(project_details, "Longitude", 68.0, 98.0)

        detail.total_units = (
            _extract_int(project_details, r"Total Number of Inventories")
            or _extract_int(project_details, r"Total Number of Flats")
            or _extract_int(project_details, r"Total.*?Inventories/Flats/Villas")
        )
        detail.open_parking = _extract_int(project_details, r"No\.?\s*of Open Parking")
        detail.covered_parking = _extract_int(project_details, r"No\.?\s*of Covered Parking")
        detail.parking_surface_count = detail.open_parking
        detail.parking_total_car_count = _extract_int(project_details, r"Total.*?Car Parking")

        # Use specific patterns that match the "(Sq Mtr) (A1+A2)" variant
        detail.total_land_area_sqm = _extract_number(project_details, r"Total Area [Oo]f Land \(Sq Mtr\)") or _extract_number(project_details, r"Total Area [Oo]f Land")
        detail.total_carpet_area_sqm = _extract_number(project_details, r"Total Carpet Area.*?\(Sq Mtr\)") or _extract_number(project_details, r"Total Carpet Area")
        detail.total_builtup_area_sqm = _extract_number(project_details, r"Total Built[\s-]?[Uu]p Area.*?\(Sq Mtr\)") or _extract_number(project_details, r"Total Built[\s-]?[Uu]p Area")
        detail.open_area_pct = _extract_open_area_pct(project_details, detail.total_land_area_sqm)

        detail.land_cost_inr = _extract_number(project_details, r"Cost of Land")
        detail.construction_cost_inr = _extract_number(project_details, r"Cost of Layout Development")
        detail.total_project_cost_inr = _extract_number(project_details, r"Total Project Cost")

        detail.far_sanctioned = _extract_number(project_details, r"FAR Sanctioned")
        detail.num_towers = _extract_int(project_details, r"Number of Towers")
        detail.max_floor_count = (
            _extract_int(project_details, r"Maximum.*?Floor")
            or _extract_int(project_details, r"No\.?\s*of Floors")
            or _extract_int(project_details, r"Number of Floors")
        )
        _extract_infra_counts(project_details, detail)

    # --- menu3 (Cost Details) ---
    menu3 = _get_tab_text(detail_html, "menu3")
    if menu3:
        if not detail.total_project_cost_inr:
            detail.total_project_cost_inr = _extract_number(menu3, r"Total Project Cost")
        if not detail.construction_cost_inr:
            detail.construction_cost_inr = _extract_number(menu3, r"Total Construction Cost")

        detail.has_borrowing = _check_yes_no(menu3, "Borrowing")
        detail.has_mortgage = _check_yes_no(menu3, "Mortgage")
        if detail.parking_total_car_count is None:
            detail.parking_total_car_count = _extract_int(menu3, r"Total.*?Car Parking")
        detail.parking_basement_count = _extract_int(menu3, r"Basement.*?Parking")
        detail.parking_visitor_count = _extract_int(menu3, r"Visitor.*?Parking")
        detail.parking_accessible_count = _extract_int(menu3, r"(?:Accessible|Differently Abled).*?Parking")
        detail.parking_ev_ready_count = _extract_int(menu3, r"(?:EV|Electric Vehicle).*?Parking")
        detail.parking_two_wheeler_count = _extract_int(menu3, r"Two Wheeler.*?Parking")
        _extract_infra_counts(menu3, detail)

    _fill_detail_wide_schedule_fields(detail_html, detail)

    # --- menu4 (Bank/Escrow) ---
    menu4 = _get_tab_text(detail_html, "menu4")
    if menu4:
        detail.escrow_bank = _extract_text(menu4, "Bank Name")
        detail.escrow_account = _extract_text(menu4, r"Account No")
        detail.escrow_ifsc = _extract_text(menu4, "IFSC Code")

    # --- menu1 (Land Details) ---
    menu1 = _get_tab_text(detail_html, "menu1")
    if menu1:
        detail.land_litigation = _check_yes_no(menu1, "Litigation")
        # If no clear Yes/No but "NO" appears near litigation, treat as False
        if detail.land_litigation is None and "Litigation" in menu1:
            vicinity = menu1[menu1.find("Litigation"):menu1.find("Litigation") + 100]
            if "NO" in vicinity.upper():
                detail.land_litigation = False

        # Extract actual survey numbers (numeric patterns like "128/7", "45/2A")
        # from land details, filtering out labels like "Type" or "No."
        raw_surveys = re.findall(r'(?:^|\s)(\d+(?:/\d+[A-Z]?)+)(?:\s|$)', menu1)
        detail.survey_numbers = list(set(raw_surveys))

    # --- home tab (Promoter + other RERA registrations) ---
    home = _get_tab_text(detail_html, "home")
    if home:
        # Count distinct RERA registration numbers from other states
        other_rera = re.findall(
            r'(?:PRM|ACK)/(?:KA|MH|GJ|TN|HR|KL|RJ|TS|AP|MP|UP|GA)/RERA/[\w/]+',
            home,
        )
        detail.builder_other_rera_projects = len(set(other_rera))

        # Check for revocations
        revoked_count = len(re.findall(r'revoked', home, re.IGNORECASE))
        detail.builder_revocations = revoked_count

        # Extract states where builder has RERA registrations
        states = set()
        known_states = [
            "Maharashtra", "Gujarat", "Tamil Nadu", "Haryana", "Kerala",
            "Rajasthan", "Telangana", "Andhra Pradesh", "Madhya Pradesh",
            "Uttar Pradesh", "Goa", "Karnataka",
        ]
        for state in known_states:
            if state.lower() in home.lower():
                states.add(state)
        detail.builder_states = sorted(states)

    # --- Complaints ---
    comp_idx = detail_html.lower().find("complaint details")
    if comp_idx >= 0:
        comp_section = _clean_html(detail_html[comp_idx:comp_idx + 20000])
        # Count complaint numbers — patterns like CMP/123/2024 or numbered entries
        complaint_nos = re.findall(r'(?:CMP|COMP)[/\-]\d+[/\-]\d+', comp_section, re.IGNORECASE)
        if not complaint_nos:
            # Try alternate pattern: just look for table-row-like entries with dates
            complaint_nos = re.findall(r'\d{2}[/-]\d{2}[/-]\d{4}', comp_section)
            # Rough: each date likely represents a complaint entry
            # Divide by expected date fields per complaint (at least 1)
        detail.complaints_count = len(set(complaint_nos))

        resolved = len(re.findall(r'DISPOSED', comp_section, re.IGNORECASE))
        if detail.complaints_count > 0:
            detail.complaints_resolved = min(resolved, detail.complaints_count)
        else:
            detail.complaints_resolved = 0

    detail.document_artifacts = _extract_document_artifacts(detail_html, detail.project_name)
    detail.configurations = _extract_configurations(
        "{} {} {}".format(project_details, menu3, _clean_html(detail_html)),
        detail.document_artifacts,
    )

    return detail


# ---------------------------------------------------------------------------
# Delay calculation
# ---------------------------------------------------------------------------

def _compute_delay_months(original_date_str: str, current_date_str: str) -> Optional[int]:
    """Compute delay in months between original and current completion dates."""
    orig = _parse_rera_date(original_date_str)
    curr = _parse_rera_date(current_date_str)
    if not orig or not curr:
        return None
    if curr <= orig:
        return 0
    delta = curr - orig
    return max(0, int(delta.days / 30.44))


# ---------------------------------------------------------------------------
# Facts conversion
# ---------------------------------------------------------------------------

def rera_detail_to_facts(detail: ReraProjectDetail) -> List[SourcedFact]:
    """Convert parsed RERA data into self-describing SourcedFacts.

    Every fact has confidence=1.0 because the source is a government portal.
    """
    source = FactSource(
        source_type="Rera",
        url=f"{RERA_BASE}/projectViewDetails",
        skill_id="fetch_rera",
    )

    facts: List[SourcedFact] = []

    def add_fact(
        key: str,
        value: dict,
        display_template: str,
        answers_prefs: Optional[List[str]] = None,
        scoring_hint: Optional[dict] = None,
    ):
        facts.append(SourcedFact(
            key=key,
            value=value,
            confidence=1.0,
            source=source,
            display_template=display_template,
            answers_preferences=answers_prefs,
            scoring_hint=scoring_hint,
        ))

    def add_numeric_fact(
        key: str,
        value: Optional[float],
        display_template: str,
        answers_prefs: Optional[List[str]] = None,
        scoring_hint: Optional[dict] = None,
    ):
        if value is None:
            return
        if not isinstance(value, (int, float)) or not float(value) == float(value):
            return
        add_fact(
            key,
            {"type": "Numeric", "data": value},
            display_template,
            answers_prefs,
            scoring_hint,
        )

    def add_text_fact(
        key: str,
        value: Optional[str],
        display_template: str,
        answers_prefs: Optional[List[str]] = None,
        scoring_hint: Optional[dict] = None,
    ):
        if value:
            add_fact(
                key,
                {"type": "Text", "data": value},
                display_template,
                answers_prefs,
                scoring_hint,
            )

    # --- Core registration ---
    add_fact(
        "rera_registered",
        {"type": "Bool", "data": True},
        "RERA Registered: Yes",
        ["rera verified", "legally verified", "safe investment", "verified project"],
        {"direction": "TextMatch", "weight": 3.0},
    )

    add_fact(
        "rera_number",
        {"type": "Text", "data": detail.registration_number},
        "RERA No: {value}",
    )

    add_fact(
        "rera_ack_number",
        {"type": "Text", "data": detail.ack_number},
        "RERA ACK: {value}",
    )

    if detail.status:
        add_fact(
            "rera_status",
            {"type": "Text", "data": detail.status},
            "RERA Status: {value}",
            ["rera verified", "safe investment"],
        )

    if detail.promoter_name:
        add_fact(
            "rera_promoter_name",
            {"type": "Text", "data": detail.promoter_name},
            "RERA Promoter: {value}",
        )

    if detail.approved_on:
        add_fact(
            "rera_approved_on",
            {"type": "Text", "data": detail.approved_on},
            "RERA Approved: {value}",
        )

    # --- Completion timeline ---
    if detail.completion_date:
        completion_iso = _iso_rera_date(detail.completion_date) or detail.completion_date
        add_text_fact(
            "rera_completion_date",
            completion_iso,
            "Expected Completion: {value}",
            ["possession date", "completion", "ready to move", "when ready"],
        )
        add_text_fact(
            "project_revised_completion_date",
            completion_iso,
            "Revised completion: {value}",
            ["completion date", "possession date"],
        )

    if detail.original_completion_date:
        original_iso = _iso_rera_date(detail.original_completion_date) or detail.original_completion_date
        add_text_fact(
            "rera_original_completion_date",
            original_iso,
            "Original Completion Date: {value}",
        )
        add_text_fact(
            "project_original_completion_date",
            original_iso,
            "Original completion: {value}",
        )

    # Calculate and emit delay
    if detail.original_completion_date and detail.completion_date:
        delay = _compute_delay_months(detail.original_completion_date, detail.completion_date)
        if delay is not None and delay > 0:
            add_fact(
                "rera_delay_months",
                {"type": "Numeric", "data": delay},
                "Project delayed by {value} months",
                ["delayed", "on time", "possession delay", "risk"],
                {"direction": "LowerIsBetter", "weight": 2.5, "thresholds": [0.0, 12.0]},
            )

    if detail.start_date:
        start_iso = _iso_rera_date(detail.start_date) or detail.start_date
        add_text_fact(
            "rera_start_date",
            start_iso,
            "Registration Start Date: {value}",
        )
        add_text_fact(
            "project_start_date",
            start_iso,
            "Project start: {value}",
            ["project start", "new launch", "upcoming project"],
        )

    # --- Project type and address ---
    if detail.project_type:
        add_fact(
            "rera_project_type",
            {"type": "Text", "data": detail.project_type},
            "Project Type: {value}",
            ["apartment", "villa", "plotted", "project type"],
        )

    if detail.project_sub_type:
        add_fact(
            "rera_project_sub_type",
            {"type": "Text", "data": detail.project_sub_type},
            "Sub Type: {value}",
        )

    if detail.project_address:
        add_fact(
            "rera_project_address",
            {"type": "Text", "data": detail.project_address},
            "Address: {value}",
        )

    # --- Units and area ---
    if detail.total_units is not None:
        add_numeric_fact("rera_total_units", detail.total_units, "{value} total units", ["project size", "how many units", "number of flats"])
        add_numeric_fact("project_unit_count", detail.total_units, "{value} total units", ["project size", "unit count", "number of flats"])

    if detail.num_towers is not None:
        add_numeric_fact("rera_num_towers", detail.num_towers, "{value} towers")
        add_numeric_fact("project_tower_count", detail.num_towers, "{value} towers", ["tower count", "number of towers"])

    if detail.max_floor_count is not None:
        add_numeric_fact(
            "project_max_floor_count",
            detail.max_floor_count,
            "{value} max floors",
            ["tower height", "floor count", "high rise"],
        )

    if detail.open_parking is not None:
        add_numeric_fact("rera_open_parking", detail.open_parking, "{value} open parking spots", ["parking"])
        add_numeric_fact("parking_surface_count", detail.open_parking, "{value} surface parking spots", ["parking", "surface parking"])

    if detail.covered_parking is not None:
        add_numeric_fact("rera_covered_parking", detail.covered_parking, "{value} covered parking spots", ["parking", "covered parking"])
        add_numeric_fact("parking_covered_count", detail.covered_parking, "{value} covered parking spots", ["parking", "covered parking"])

    for key, value, label, prefs in [
        ("parking_total_car_count", detail.parking_total_car_count, "{value} sanctioned car parks", ["parking", "car parking"]),
        ("parking_basement_count", detail.parking_basement_count, "{value} basement parking spots", ["parking", "basement parking"]),
        ("parking_visitor_count", detail.parking_visitor_count, "{value} visitor parking spots", ["parking", "visitor parking"]),
        ("parking_accessible_count", detail.parking_accessible_count, "{value} accessible parking spots", ["parking", "accessible parking"]),
        ("parking_ev_ready_count", detail.parking_ev_ready_count, "{value} EV-ready parking spots", ["parking", "ev parking"]),
        ("parking_two_wheeler_count", detail.parking_two_wheeler_count, "{value} two-wheeler parking spots", ["parking", "two wheeler parking"]),
        ("parking_offered_for_sale_count", detail.parking_offered_for_sale_count, "{value} parking spots offered for sale", ["parking", "parking for sale"]),
    ]:
        add_numeric_fact(key, value, label, prefs)

    if detail.total_land_area_sqm is not None:
        add_numeric_fact("rera_total_land_area_sqm", detail.total_land_area_sqm, "Total Land Area: {value} sq m")
        add_numeric_fact("project_land_area_sqm", detail.total_land_area_sqm, "Project land area: {value} sq m", ["land area", "large campus", "acres"])
        add_numeric_fact("project_land_area_acres", round(detail.total_land_area_sqm / 4046.8564224, 2), "Project land area: {value} acres", ["land area", "large campus", "acres"])
        if detail.total_units and detail.total_units > 0:
            add_numeric_fact("project_units_per_acre", round(detail.total_units / (detail.total_land_area_sqm / 4046.8564224), 2), "{value} units per acre", ["low density", "density", "spacious"])

    if detail.open_area_pct is not None:
        add_numeric_fact("project_open_area_pct", detail.open_area_pct, "{value}% open area", ["open area", "open space", "greenery"])

    if detail.total_carpet_area_sqm is not None:
        add_fact(
            "rera_total_carpet_area_sqm",
            {"type": "Numeric", "data": detail.total_carpet_area_sqm},
            "Total Carpet Area: {value} sq m",
        )

    if detail.total_builtup_area_sqm is not None:
        add_fact(
            "rera_total_builtup_area_sqm",
            {"type": "Numeric", "data": detail.total_builtup_area_sqm},
            "Total Built-up Area: {value} sq m",
        )

    if detail.far_sanctioned is not None:
        add_fact(
            "rera_far_sanctioned",
            {"type": "Numeric", "data": detail.far_sanctioned},
            "FAR Sanctioned: {value}",
        )

    # RERA cost fields are intentionally not promoted. Builder-entered project
    # cost values are unreliable; price facts must come from listing or
    # transaction evidence.

    # --- Financial safety ---
    if detail.has_borrowing is not None:
        add_fact(
            "rera_has_borrowing",
            {"type": "Bool", "data": detail.has_borrowing},
            "Has Borrowing: {value}",
            ["financially safe", "debt"],
        )

    if detail.has_mortgage is not None:
        add_fact(
            "rera_has_mortgage",
            {"type": "Bool", "data": detail.has_mortgage},
            "Has Mortgage: {value}",
            ["financially safe", "mortgage"],
        )

    # --- Escrow ---
    if detail.escrow_bank:
        add_fact(
            "rera_escrow_bank",
            {"type": "Text", "data": detail.escrow_bank},
            "Escrow Bank: {value}",
            ["financially safe", "escrow"],
        )

    if detail.escrow_account:
        add_fact(
            "rera_escrow_account",
            {"type": "Text", "data": detail.escrow_account},
            "Escrow Account: {value}",
        )

    if detail.escrow_ifsc:
        add_fact(
            "rera_escrow_ifsc",
            {"type": "Text", "data": detail.escrow_ifsc},
            "Escrow IFSC: {value}",
        )

    # --- Land ---
    if detail.land_litigation is not None:
        lit_text = "Yes" if detail.land_litigation else "No"
        add_fact(
            "rera_land_litigation",
            {"type": "Bool", "data": detail.land_litigation},
            f"Land Litigation: {lit_text}",
            ["legal issues", "litigation", "safe", "clear title"],
            {"direction": "LowerIsBetter", "weight": 3.0},
        )

    if detail.survey_numbers:
        add_fact(
            "rera_survey_numbers",
            {"type": "Text", "data": ", ".join(detail.survey_numbers)},
            "Survey Numbers: {value}",
        )

    # --- Complaints ---
    add_fact(
        "rera_complaints_count",
        {"type": "Numeric", "data": detail.complaints_count},
        "{value} complaints filed",
        ["complaints", "legal issues", "problems", "safe"],
        {"direction": "LowerIsBetter", "weight": 2.0, "thresholds": [0.0, 3.0]},
    )

    if detail.complaints_count > 0 and detail.complaints_resolved > 0:
        pct = round(detail.complaints_resolved / detail.complaints_count * 100, 1)
        pct = max(0.0, min(100.0, pct))
        add_fact(
            "rera_complaints_resolved_pct",
            {"type": "Numeric", "data": pct},
            "{value}% complaints resolved",
            ["complaints resolved"],
        )

    # --- Builder track record ---
    if detail.builder_other_rera_projects > 0:
        add_fact(
            "rera_builder_projects_count",
            {"type": "Numeric", "data": detail.builder_other_rera_projects},
            "Builder has {value} RERA projects",
            ["trusted builder", "experienced builder", "track record"],
            {"direction": "HigherIsBetter", "weight": 1.5},
        )

    add_fact(
        "rera_builder_revocations",
        {"type": "Numeric", "data": detail.builder_revocations},
        "{value} revocations",
        ["safe builder", "trusted builder"],
        {"direction": "LowerIsBetter", "weight": 3.0},
    )

    if detail.builder_states:
        add_fact(
            "rera_builder_states",
            {"type": "Text", "data": ", ".join(detail.builder_states)},
            "Builder active in: {value}",
            ["multi-state builder", "national builder"],
        )

    # --- Water/infrastructure ---
    for key, value, label, prefs in [
        ("stp_count", detail.stp_count, "{value} STP units", ["stp", "sewage treatment"]),
        ("stp_capacity_kld", detail.stp_capacity_kld, "{value} KLD STP capacity", ["stp capacity", "sewage treatment"]),
        ("borewell_proposed_count", detail.borewell_proposed_count, "{value} proposed borewells", ["borewell", "water infra"]),
        ("borewell_existing_count", detail.borewell_existing_count, "{value} existing borewells", ["borewell", "water infra"]),
        ("borewell_depth_ft", detail.borewell_depth_ft, "{value} ft borewell depth", ["borewell depth", "water infra"]),
        ("borewell_yield_lph", detail.borewell_yield_lph, "{value} LPH borewell yield", ["borewell yield", "water infra"]),
    ]:
        add_numeric_fact(key, value, label, prefs)

    # --- Plan media and configurations ---
    site_plan_count = sum(1 for artifact in detail.document_artifacts if artifact.document_kind == "site_plan")
    floor_plan_count = sum(1 for artifact in detail.document_artifacts if artifact.document_kind == "floor_plan")
    sanction_plan_count = sum(1 for artifact in detail.document_artifacts if artifact.document_kind == "sanction_plan")
    if site_plan_count > 0:
        add_numeric_fact("site_plan_asset_count", site_plan_count, "{value} site plan assets", ["site plan", "project layout"])
    if floor_plan_count > 0:
        add_numeric_fact("floor_plan_asset_count", floor_plan_count, "{value} floor plan assets", ["floor plan", "unit layout"])
    if sanction_plan_count > 0:
        add_numeric_fact("sanction_plan_asset_count", sanction_plan_count, "{value} sanction plan assets", ["sanction plan", "approved plan"])

    if detail.document_artifacts:
        manifest = [
            {
                "artifact_id": artifact.artifact_id,
                "kind": artifact.document_kind,
                "label": artifact.label,
                "source_url": artifact.source_url,
                "configuration_type": artifact.configuration_type,
                "bedroom_count": artifact.bedroom_count,
                "confidence": artifact.confidence,
            }
            for artifact in detail.document_artifacts
        ]
        add_fact(
            "rera_plan_artifact_manifest",
            {"type": "Text", "data": json.dumps(manifest, sort_keys=True, separators=(",", ":"))},
            "RERA plan artifacts available",
            ["site plan", "floor plan", "sanction plan"],
        )

    if detail.configurations:
        labels = [config.configuration_type for config in detail.configurations]
        add_fact(
            "available_configurations",
            {"type": "Tags", "data": labels},
            "Available configurations: {value}",
            ["1bhk", "2bhk", "3bhk", "4bhk", "floor plan", "configuration"],
            {"direction": "TextMatch", "weight": 2.0},
        )
        add_numeric_fact(
            "configuration_count",
            len(labels),
            "{value} configurations found",
            ["configuration", "floor plan"],
        )
        for bhk in (1, 2, 3, 4):
            has_bhk = any(config.bedroom_count == float(bhk) for config in detail.configurations)
            if has_bhk:
                add_fact(
                    "has_{}bhk".format(bhk),
                    {"type": "Bool", "data": True},
                    "{}BHK available: {{value}}".format(bhk),
                    ["{}bhk".format(bhk), "{} bhk".format(bhk), "floor plan"],
                    {"direction": "TextMatch", "weight": 1.5},
                )

    # --- Location ---
    if detail.latitude and detail.longitude:
        add_fact(
            "rera_lat_lng",
            {"type": "Text", "data": f"{detail.latitude},{detail.longitude}"},
            "Location: {value}",
        )

    # --- Portal URL ---
    add_fact(
        "rera_portal_url",
        {"type": "Text", "data": f"{RERA_BASE}/projectViewDetails"},
        "View on RERA Portal",
    )

    return facts


# ---------------------------------------------------------------------------
# Skill class
# ---------------------------------------------------------------------------

class FetchReraSkill(BaseSkill):
    """Scrape Karnataka RERA portal for real project registration data.

    Replaces the old guessed RERA verifier. This skill fetches actual
    government-sourced data.
    """

    skill_id = "fetch_rera"
    description = "Scrape Karnataka RERA portal for real project registration data"
    version = "3.0"  # v3.0 promotes project/config/media facts and drops cost facts.
    output_keys = [
        "rera_registered", "rera_number", "rera_ack_number", "rera_status",
        "rera_promoter_name", "rera_approved_on", "rera_completion_date",
        "rera_original_completion_date", "rera_delay_months", "rera_start_date",
        "rera_project_type", "rera_project_sub_type", "rera_project_address",
        "rera_total_units", "rera_num_towers", "rera_open_parking", "rera_covered_parking",
        "rera_total_land_area_sqm", "project_land_area_sqm",
        "project_land_area_acres", "project_open_area_pct",
        "project_unit_count", "project_tower_count", "project_max_floor_count",
        "project_units_per_acre", "available_configurations", "configuration_count",
        "has_1bhk", "has_2bhk", "has_3bhk", "has_4bhk",
        "parking_total_car_count", "parking_covered_count", "parking_surface_count",
        "parking_basement_count", "parking_visitor_count", "parking_accessible_count",
        "parking_ev_ready_count", "parking_two_wheeler_count", "parking_offered_for_sale_count",
        "stp_count", "stp_capacity_kld", "borewell_proposed_count",
        "borewell_existing_count", "borewell_depth_ft", "borewell_yield_lph",
        "site_plan_asset_count", "floor_plan_asset_count", "sanction_plan_asset_count",
        "rera_plan_artifact_manifest", "rera_total_carpet_area_sqm",
        "rera_total_builtup_area_sqm", "rera_far_sanctioned",
    ]

    def __init__(self, **kwargs):
        super().__init__(**kwargs)
        self._session: Optional[ReraSession] = None

    @property
    def session(self) -> ReraSession:
        if self._session is None:
            self._session = ReraSession()
        return self._session

    def execute(self, input_data: dict) -> SkillResult:
        project_name = input_data.get("project_name", "")
        if not project_name:
            logger.error("fetch_rera requires project_name")
            return SkillResult(confidence=0.0)

        api_calls = 0

        try:
            # Step 1: Search
            search_result = search_rera_project(self.session, project_name)
            api_calls += 2  # session init + search POST

            if not search_result:
                # Project not found on RERA portal
                not_found_source = FactSource(
                    source_type="Rera",
                    url=SEARCH_URL,
                    skill_id=self.skill_id,
                    triggered_by=input_data.get("triggered_by"),
                )
                return SkillResult(
                    facts=[SourcedFact(
                        key="rera_registered",
                        value={"type": "Bool", "data": False},
                        confidence=0.8,  # Not 1.0: might be a name mismatch
                        source=not_found_source,
                        display_template="RERA Registration: Not Found",
                        answers_preferences=["rera verified"],
                        scoring_hint={"direction": "TextMatch", "weight": 3.0},
                    )],
                    confidence=0.8,
                    cost=SkillCost(api_calls=api_calls),
                )

            # Step 2: Fetch detail page
            detail_html = fetch_rera_detail(self.session, search_result.numeric_id)
            api_calls += 1

            # Step 3: Parse detail
            detail = parse_rera_detail(detail_html, search_result)

            # Step 4: Convert to facts
            facts = rera_detail_to_facts(detail)

            logger.info(
                "RERA scrape for '%s': %d facts, reg=%s, status=%s, configs=%d, plans=%d",
                project_name, len(facts),
                detail.registration_number, detail.status,
                len(detail.configurations), len(detail.document_artifacts),
            )

            return SkillResult(
                facts=facts,
                confidence=1.0,
                cost=SkillCost(api_calls=api_calls),
            )

        except Exception as e:
            logger.error("RERA scrape failed for '%s': %s", project_name, e, exc_info=True)
            return SkillResult(
                confidence=0.0,
                cost=SkillCost(api_calls=api_calls),
            )

    def estimated_cost(self) -> SkillCost:
        return SkillCost(api_calls=3, estimated_usd=0.0)  # Free: government website


# ---------------------------------------------------------------------------
# CLI entry point
# ---------------------------------------------------------------------------

if __name__ == "__main__":
    import sys

    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s %(levelname)s %(name)s: %(message)s",
    )

    # Test 1: Listing (cached after first run)
    print("=" * 60)
    print("RERA LISTING")
    print("=" * 60)
    entries = scrape_rera_listing()
    print(f"Total projects: {len(entries)}")

    sobha = [e for e in entries if "SOBHA" in e.project_name.upper() or "SOBHA" in e.promoter_name.upper()]
    print(f"Sobha projects: {len(sobha)}")
    for s in sobha[:5]:
        print(f"  {s.project_name} | {s.promoter_name} | {s.registration_number}")

    # Test 2: Full scrape for a specific project
    test_project = sys.argv[1] if len(sys.argv) > 1 else "SOBHA INSIGNIA"
    print()
    print("=" * 60)
    print(f"DETAIL: {test_project}")
    print("=" * 60)

    skill = FetchReraSkill()
    result = skill.run({"project_name": test_project, "triggered_by": "manual_test"}, force=True)

    print(f"\nFacts: {len(result.facts)}")
    print(f"Confidence: {result.confidence}")
    print(f"API calls: {result.cost.api_calls}")
    print(f"Cached: {result.cached}")
    print()

    for fact in result.facts:
        template = fact.display_template or "{value}"
        val = fact.value.get("data", "")
        display = template.replace("{value}", str(val))
        print(f"  [{fact.key}] {display}")
        if fact.answers_preferences:
            print(f"    answers: {fact.answers_preferences}")
        if fact.scoring_hint:
            print(f"    scoring: {fact.scoring_hint}")
