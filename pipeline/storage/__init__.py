"""
Storage abstraction for OpenEstates data lake.

Organizes data using an S3-like key scheme:
    {entity_type}/{city}/{area_or_id}/{filename}

Examples:
    properties/bangalore/whitefield/prop_001/data.json
    societies/bangalore/whitefield/prestige-shantiniketan/data.json
    intelligence/bangalore/whitefield/reddit_summary.json
    manifests/properties_bangalore.json
"""

from pipeline.storage.client import StorageClient, LocalFSBackend
from pipeline.storage.keys import KeyBuilder
from pipeline.storage.manifest import ManifestIndex

__all__ = ["StorageClient", "LocalFSBackend", "KeyBuilder", "ManifestIndex"]
