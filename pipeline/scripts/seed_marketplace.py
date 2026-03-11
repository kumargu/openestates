"""
Seed dummy marketplace data for UI development.

Creates 10 sellers with 2-4 active listings each, spread across Bangalore areas.
Seeds 15 properties with realistic bid histories.
Tags those properties as seller_listed in seed/properties.json.

Usage:
    python3 pipeline/scripts/seed_marketplace.py
    python3 pipeline/scripts/seed_marketplace.py --dry-run
"""

import json
import sys
from pathlib import Path

PROJECT_ROOT = Path(__file__).resolve().parent.parent.parent
MARKETPLACE_DIR = PROJECT_ROOT / "data" / "marketplace"
SEED_PROPERTIES = PROJECT_ROOT / "data" / "seed" / "properties.json"


SELLERS = [
    {"name": "Rajesh Kumar",     "phone": "+91-98450-11001", "rera": None},
    {"name": "Priya Nair",       "phone": "+91-98450-22002", "rera": "PRM/KA/RERA/1251/446/PR/210401/003905"},
    {"name": "Suresh Bhat",      "phone": "+91-98450-33003", "rera": None},
    {"name": "Anjali Sharma",    "phone": "+91-98450-44004", "rera": "PRM/KA/RERA/1251/446/AG/201011/001234"},
    {"name": "Vikram Reddy",     "phone": "+91-98450-55005", "rera": None},
    {"name": "Deepa Menon",      "phone": "+91-98450-66006", "rera": None},
    {"name": "Arun Joshi",       "phone": "+91-98450-77007", "rera": "PRM/KA/RERA/1251/302/PR/210601/004100"},
    {"name": "Kavitha Rao",      "phone": "+91-98450-88008", "rera": None},
    {"name": "Mohan Pillai",     "phone": "+91-98450-99009", "rera": None},
    {"name": "Sneha Kulkarni",   "phone": "+91-98450-10010", "rera": None},
]

# 2-4 listings per seller. Multiple sellers can list the same property.
SELLER_LISTINGS = [
    # Rajesh — Whitefield specialist
    [
        "discovered-prestige-lakeside-habitat-3bhk",
        "discovered-prestige-lakeside-habitat-2bhk",
        "discovered-prestige-park-grove-3bhk",
    ],
    # Priya — Marathahalli + Whitefield (RERA agent)
    [
        "discovered-prestige-somerville-3bhk",
        "discovered-prestige-somerville-2bhk",
        "discovered-vaswani-starlight-3bhk",
    ],
    # Suresh — HSR Layout
    [
        "discovered-prestige-raintree-park-3bhk",
        "discovered-prestige-raintree-park-2bhk",
        "discovered-casagrand-flamingo-2bhk",
    ],
    # Anjali — Hebbal (RERA agent)
    [
        "discovered-sumadhura-capitol-residences-3bhk",
        "discovered-century-ethos-3bhk",
        "discovered-lt-elara-celestia-3bhk",
    ],
    # Vikram — Koramangala
    [
        "discovered-prestige-pine-wood-2bhk",
        "discovered-k-raheja-vivarea-2bhk",
    ],
    # Deepa — Sarjapur Road
    [
        "discovered-vaswani-starlight-4bhk",
        "discovered-sarang-by-sumadhura-phase-2-4bhk",
        "discovered-brigade-cornerstone-utopia-3bhk",
    ],
    # Arun — premium segment (RERA agent)
    [
        "discovered-prestige-lakeside-habitat-4bhk",
        "discovered-total-environment-pursuit-of-a-radical-rhapsody-3bhk",
        "discovered-sobha-infinia-3bhk",
    ],
    # Kavitha — Thanisandra + Hebbal
    [
        "discovered-prestige-somerville-4bhk",
        "discovered-sobha-dream-gardens-2bhk",
        "discovered-sumadhura-epitome-1-3bhk",
    ],
    # Mohan — also lists raintree (two sellers, same property)
    [
        "discovered-prestige-raintree-park-3bhk",
        "discovered-century-mirai-3bhk",
    ],
    # Sneha — Varthur + Bellandur
    [
        "discovered-prestige-lavender-fields-3bhk",
        "discovered-prestige-lavender-fields-2bhk",
        "discovered-brigade-altair-apartments-2bhk",
    ],
]

# Seller asking prices — slight variation from discovery price for realism
ASKING_PRICE_OVERRIDES = {
    "discovered-prestige-lakeside-habitat-2bhk":                    13000000,
    "discovered-prestige-lakeside-habitat-3bhk":                    14500000,
    "discovered-prestige-lakeside-habitat-4bhk":                    14800000,
    "discovered-prestige-park-grove-3bhk":                         22500000,
    "discovered-prestige-somerville-2bhk":                         16800000,
    "discovered-prestige-somerville-3bhk":                         23800000,
    "discovered-prestige-somerville-4bhk":                         38200000,
    "discovered-vaswani-starlight-3bhk":                           25900000,
    "discovered-vaswani-starlight-4bhk":                           37200000,
    "discovered-prestige-raintree-park-2bhk":                       7800000,
    "discovered-prestige-raintree-park-3bhk":                      11200000,
    "discovered-casagrand-flamingo-2bhk":                          14500000,
    "discovered-sumadhura-capitol-residences-3bhk":                26500000,
    "discovered-century-ethos-3bhk":                               35000000,
    "discovered-lt-elara-celestia-3bhk":                           26500000,
    "discovered-prestige-pine-wood-2bhk":                          17800000,
    "discovered-k-raheja-vivarea-2bhk":                            36500000,
    "discovered-sarang-by-sumadhura-phase-2-4bhk":                 30800000,
    "discovered-brigade-cornerstone-utopia-3bhk":                  15300000,
    "discovered-total-environment-pursuit-of-a-radical-rhapsody-3bhk": 43000000,
    "discovered-sobha-infinia-3bhk":                               41500000,
    "discovered-sobha-dream-gardens-2bhk":                         14200000,
    "discovered-sumadhura-epitome-1-3bhk":                         15200000,
    "discovered-century-mirai-3bhk":                               31200000,
    "discovered-prestige-lavender-fields-2bhk":                    25200000,
    "discovered-prestige-lavender-fields-3bhk":                    36000000,
    "discovered-brigade-altair-apartments-2bhk":                    6400000,
}

# 15 properties with seeded bids — realistic spread across price points
SAMPLE_BIDS = {
    # Whitefield — high activity
    "discovered-prestige-lakeside-habitat-3bhk": [
        {"name": "Amit Sinha",     "phone": "+91-97400-11111", "amount": 14000000, "ts": "2026-03-01T09:15:00Z"},
        {"name": "Rekha Iyer",     "phone": "+91-97400-22222", "amount": 13750000, "ts": "2026-03-04T11:30:00Z"},
        {"name": "Nikhil Gupta",   "phone": "+91-97400-33333", "amount": 14300000, "ts": "2026-03-07T14:00:00Z"},
        {"name": "Pooja Verma",    "phone": "+91-97400-44444", "amount": 14250000, "ts": "2026-03-09T08:45:00Z"},
        {"name": "Siddharth Roy",  "phone": "+91-97400-55555", "amount": 14100000, "ts": "2026-03-10T17:00:00Z"},
    ],
    "discovered-prestige-lakeside-habitat-2bhk": [
        {"name": "Nandita Shah",   "phone": "+91-97401-11111", "amount": 12500000, "ts": "2026-03-05T10:00:00Z"},
        {"name": "Rajiv Menon",    "phone": "+91-97401-22222", "amount": 12800000, "ts": "2026-03-08T14:30:00Z"},
        {"name": "Asha Krishnan",  "phone": "+91-97401-33333", "amount": 12700000, "ts": "2026-03-10T09:00:00Z"},
    ],
    "discovered-prestige-park-grove-3bhk": [
        {"name": "Rohit Khanna",   "phone": "+91-97402-11111", "amount": 21800000, "ts": "2026-03-06T11:00:00Z"},
        {"name": "Tanya Bose",     "phone": "+91-97402-22222", "amount": 22000000, "ts": "2026-03-09T15:00:00Z"},
    ],
    # Marathahalli — moderate activity
    "discovered-prestige-somerville-3bhk": [
        {"name": "Arjun Mehta",    "phone": "+91-97403-11111", "amount": 23000000, "ts": "2026-03-03T10:00:00Z"},
        {"name": "Sunita Dhar",    "phone": "+91-97403-22222", "amount": 23500000, "ts": "2026-03-07T16:20:00Z"},
        {"name": "Pranav Iyer",    "phone": "+91-97403-33333", "amount": 23200000, "ts": "2026-03-10T11:00:00Z"},
    ],
    "discovered-prestige-somerville-2bhk": [
        {"name": "Divya Anand",    "phone": "+91-97404-11111", "amount": 16200000, "ts": "2026-03-08T09:30:00Z"},
        {"name": "Kiran Bhat",     "phone": "+91-97404-22222", "amount": 16500000, "ts": "2026-03-10T13:00:00Z"},
    ],
    "discovered-vaswani-starlight-3bhk": [
        {"name": "Karthik Raj",    "phone": "+91-97405-11111", "amount": 25000000, "ts": "2026-03-02T12:00:00Z"},
        {"name": "Meera Pillai",   "phone": "+91-97405-22222", "amount": 25600000, "ts": "2026-03-06T09:00:00Z"},
        {"name": "Dev Sharma",     "phone": "+91-97405-33333", "amount": 25200000, "ts": "2026-03-09T11:15:00Z"},
        {"name": "Preethi Rao",    "phone": "+91-97405-44444", "amount": 25800000, "ts": "2026-03-11T10:00:00Z"},
    ],
    # HSR Layout — brisk
    "discovered-prestige-raintree-park-3bhk": [
        {"name": "Lakshmi Bose",   "phone": "+91-97406-11111", "amount": 10800000, "ts": "2026-03-01T10:30:00Z"},
        {"name": "Vinay Chandra",  "phone": "+91-97406-22222", "amount": 11000000, "ts": "2026-03-04T15:45:00Z"},
        {"name": "Anita Kapoor",   "phone": "+91-97406-33333", "amount": 11100000, "ts": "2026-03-07T10:00:00Z"},
        {"name": "Rohit Malhotra", "phone": "+91-97406-44444", "amount": 10900000, "ts": "2026-03-09T09:30:00Z"},
        {"name": "Sujatha Nair",   "phone": "+91-97406-55555", "amount": 11050000, "ts": "2026-03-11T14:20:00Z"},
    ],
    "discovered-casagrand-flamingo-2bhk": [
        {"name": "Gaurav Jain",    "phone": "+91-97407-11111", "amount": 14000000, "ts": "2026-03-07T11:00:00Z"},
        {"name": "Sheela Reddy",   "phone": "+91-97407-22222", "amount": 14200000, "ts": "2026-03-10T16:00:00Z"},
    ],
    # Hebbal — good demand
    "discovered-sumadhura-capitol-residences-3bhk": [
        {"name": "Rahul Nanda",    "phone": "+91-97408-11111", "amount": 26000000, "ts": "2026-03-05T13:00:00Z"},
        {"name": "Supriya Kaul",   "phone": "+91-97408-22222", "amount": 25800000, "ts": "2026-03-09T10:00:00Z"},
        {"name": "Venkat Rao",     "phone": "+91-97408-33333", "amount": 26200000, "ts": "2026-03-11T09:00:00Z"},
    ],
    "discovered-century-ethos-3bhk": [
        {"name": "Harish Naik",    "phone": "+91-97409-11111", "amount": 34000000, "ts": "2026-03-06T14:00:00Z"},
        {"name": "Bhavna Desai",   "phone": "+91-97409-22222", "amount": 34500000, "ts": "2026-03-10T11:30:00Z"},
    ],
    # Koramangala — single serious bid
    "discovered-k-raheja-vivarea-2bhk": [
        {"name": "Sameer Dutta",   "phone": "+91-97410-11111", "amount": 35500000, "ts": "2026-03-08T12:00:00Z"},
    ],
    # Sarjapur Road
    "discovered-brigade-cornerstone-utopia-3bhk": [
        {"name": "Lavanya Kumar",  "phone": "+91-97411-11111", "amount": 14800000, "ts": "2026-03-07T10:00:00Z"},
        {"name": "Sathish Gowda",  "phone": "+91-97411-22222", "amount": 15100000, "ts": "2026-03-09T14:00:00Z"},
        {"name": "Rupa Nagaraj",   "phone": "+91-97411-33333", "amount": 14950000, "ts": "2026-03-11T08:30:00Z"},
    ],
    # Premium segment
    "discovered-total-environment-pursuit-of-a-radical-rhapsody-3bhk": [
        {"name": "Vivek Patel",    "phone": "+91-97412-11111", "amount": 42000000, "ts": "2026-03-04T15:00:00Z"},
        {"name": "Nisha Agarwal",  "phone": "+91-97412-22222", "amount": 42500000, "ts": "2026-03-09T11:00:00Z"},
    ],
    # Varthur
    "discovered-prestige-lavender-fields-3bhk": [
        {"name": "Deepak Shetty",  "phone": "+91-97413-11111", "amount": 35200000, "ts": "2026-03-08T09:00:00Z"},
        {"name": "Usha Pillai",    "phone": "+91-97413-22222", "amount": 35500000, "ts": "2026-03-10T15:00:00Z"},
    ],
    # Bellandur — budget segment, competitive
    "discovered-brigade-altair-apartments-2bhk": [
        {"name": "Manoj Iyer",     "phone": "+91-97414-11111", "amount":  6200000, "ts": "2026-03-06T10:00:00Z"},
        {"name": "Chitra Balan",   "phone": "+91-97414-22222", "amount":  6100000, "ts": "2026-03-08T13:00:00Z"},
        {"name": "Ajay Nambiar",   "phone": "+91-97414-33333", "amount":  6300000, "ts": "2026-03-10T17:30:00Z"},
        {"name": "Suma Reddy",     "phone": "+91-97414-44444", "amount":  6250000, "ts": "2026-03-11T11:00:00Z"},
    ],
}


def new_id(prefix: str, index: int) -> str:
    return f"{prefix}-{1700000000 + index * 1000}"


def save_json(path: Path, data: dict, dry_run: bool):
    if dry_run:
        print(f"  [dry-run] would write: {path.relative_to(PROJECT_ROOT)}")
        return
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(".tmp")
    tmp.write_text(json.dumps(data, indent=2, ensure_ascii=False))
    tmp.rename(path)
    print(f"  wrote: {path.relative_to(PROJECT_ROOT)}")


def load_properties() -> list[dict]:
    return json.loads(SEED_PROPERTIES.read_text())


def seed_sellers(properties: list[dict], dry_run: bool) -> set[str]:
    prop_ids = {p["id"] for p in properties}
    all_listing_ids: set[str] = set()

    for i, (seller_def, listing_ids) in enumerate(zip(SELLERS, SELLER_LISTINGS)):
        valid_ids = [pid for pid in listing_ids if pid in prop_ids]
        missing = [pid for pid in listing_ids if pid not in prop_ids]
        if missing:
            print(f"  WARN: property IDs not found (skipping): {missing}")

        seller_id = new_id("seller", i)
        tier = "rera_verified" if seller_def["rera"] else "identity_verified"

        seller: dict = {
            "id": seller_id,
            "name": seller_def["name"],
            "phone": seller_def["phone"],
            "verificationTier": tier,
            "listingIds": valid_ids,
            "registeredAt": "2026-02-15T10:00:00Z",
        }
        if seller_def["rera"]:
            seller["reraNumber"] = seller_def["rera"]

        save_json(MARKETPLACE_DIR / "sellers" / f"{seller_id}.json", seller, dry_run)
        all_listing_ids.update(valid_ids)

    return all_listing_ids


def tag_properties_as_seller_listed(
    properties: list[dict],
    listing_ids: set[str],
    dry_run: bool,
) -> None:
    changed = 0
    for prop in properties:
        if prop["id"] in listing_ids:
            prop["source"] = "seller_listed"
            prop["listing_status"] = "active"
            if prop["id"] in ASKING_PRICE_OVERRIDES:
                prop["price"] = ASKING_PRICE_OVERRIDES[prop["id"]]
            changed += 1

    if dry_run:
        print(f"  [dry-run] would update {changed} properties in seed/properties.json")
        return

    tmp = SEED_PROPERTIES.with_suffix(".tmp")
    tmp.write_text(json.dumps(properties, indent=2, ensure_ascii=False))
    tmp.rename(SEED_PROPERTIES)
    print(f"  updated {changed} properties → source=seller_listed")


def seed_bids(dry_run: bool) -> None:
    bids_dir = MARKETPLACE_DIR / "bids"
    stats_dir = MARKETPLACE_DIR / "bid_stats"

    for property_id, bid_list in SAMPLE_BIDS.items():
        bids = [
            {
                "id": f"bid-{property_id[:22]}-{j}",
                "propertyId": property_id,
                "bidderName": b["name"],
                "bidderPhone": b["phone"],
                "amount": b["amount"],
                "createdAt": b["ts"],
                "status": "pending",
            }
            for j, b in enumerate(bid_list)
        ]

        if dry_run:
            print(f"  [dry-run] would write {len(bids)} bids for {property_id}")
            continue

        bids_dir.mkdir(parents=True, exist_ok=True)
        bid_path = bids_dir / f"{property_id}.jsonl"
        with open(bid_path, "w") as f:
            for bid in bids:
                f.write(json.dumps(bid) + "\n")
        print(f"  wrote {len(bids)} bids → {bid_path.relative_to(PROJECT_ROOT)}")

        amounts = [b["amount"] for b in bids]
        stats = {
            "propertyId": property_id,
            "count": len(amounts),
            "average": sum(amounts) // len(amounts),
            "p100": max(amounts),
            "lastBidAt": max(b["ts"] for b in bid_list),
        }
        save_json(stats_dir / f"{property_id}.json", stats, dry_run)


def main(dry_run: bool = False) -> None:
    print(f"Seeding marketplace data (dry_run={dry_run})\n")

    properties = load_properties()
    print(f"Loaded {len(properties)} properties from seed")

    print("\n--- sellers ---")
    all_listing_ids = seed_sellers(properties, dry_run)

    print("\n--- property tags ---")
    tag_properties_as_seller_listed(properties, all_listing_ids, dry_run)

    print("\n--- bids ---")
    seed_bids(dry_run)

    print(f"\nDone: {len(SELLERS)} sellers, {len(all_listing_ids)} listed properties, {len(SAMPLE_BIDS)} with bid history")


if __name__ == "__main__":
    dry_run = "--dry-run" in sys.argv
    main(dry_run=dry_run)
