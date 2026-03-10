"""
Key generation for data lake storage.

Keys follow the pattern: {entity_type}/{city}/{area_or_id}/{filename}
City and area values are normalized to lowercase slugs.
"""

import re
from typing import Optional


def _slugify(value: str) -> str:
    """Convert a string to a URL-safe slug."""
    value = value.lower().strip()
    value = re.sub(r"[^\w\s-]", "", value)
    value = re.sub(r"[\s_]+", "-", value)
    value = re.sub(r"-+", "-", value)
    return value.strip("-")


class KeyBuilder:
    """Generates storage keys for different entity types."""

    @staticmethod
    def property_key(city: str, area: str, property_id: str, filename: str = "data.json") -> str:
        return f"properties/{_slugify(city)}/{_slugify(area)}/{_slugify(property_id)}/{filename}"

    @staticmethod
    def society_key(city: str, area: str, society_name: str, filename: str = "data.json") -> str:
        return f"societies/{_slugify(city)}/{_slugify(area)}/{_slugify(society_name)}/{filename}"

    @staticmethod
    def area_profile_key(city: str, area: str, filename: str = "data.json") -> str:
        return f"area_profiles/{_slugify(city)}/{_slugify(area)}/{filename}"

    @staticmethod
    def intelligence_key(city: str, area: str, filename: str) -> str:
        return f"intelligence/{_slugify(city)}/{_slugify(area)}/{filename}"

    @staticmethod
    def embedding_key(entity_type: str, entity_id: str) -> str:
        return f"embeddings/{_slugify(entity_type)}/{_slugify(entity_id)}.npy"

    @staticmethod
    def manifest_key(entity_type: str, city: Optional[str] = None) -> str:
        if city:
            return f"manifests/{_slugify(entity_type)}_{_slugify(city)}.json"
        return f"manifests/{_slugify(entity_type)}_all.json"

    @staticmethod
    def cache_key(source: str, url_hash: str) -> str:
        return f"cache/crawls/{_slugify(source)}/{url_hash}.json"

    @staticmethod
    def launch_key(city: str, area: str, launch_name: str, filename: str = "data.json") -> str:
        return f"launches/{_slugify(city)}/{_slugify(area)}/{_slugify(launch_name)}/{filename}"
