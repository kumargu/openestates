"""
StorageClient: abstract base and LocalFSBackend implementation.

Provides get/put/list/exists methods using the key scheme.
LocalFSBackend stores everything under data/lake/.
"""

import json
import os
import time
from abc import ABC, abstractmethod
from pathlib import Path
from typing import Any, Dict, List, Optional

from pydantic import BaseModel


class ObjectMetadata(BaseModel):
    """Metadata attached to every stored object."""
    key: str
    content_type: str = "application/json"
    created_at: float = 0.0
    updated_at: float = 0.0
    version: int = 1
    size_bytes: int = 0


class StorageClient(ABC):
    """Abstract base for storage backends."""

    @abstractmethod
    def put(self, key: str, data: Any, content_type: str = "application/json", metadata: Optional[Dict[str, Any]] = None) -> ObjectMetadata:
        """Store data at key. Returns metadata."""
        ...

    @abstractmethod
    def get(self, key: str) -> Optional[Any]:
        """Retrieve data at key. Returns None if not found."""
        ...

    @abstractmethod
    def get_metadata(self, key: str) -> Optional[ObjectMetadata]:
        """Retrieve metadata for key. Returns None if not found."""
        ...

    @abstractmethod
    def exists(self, key: str) -> bool:
        """Check if key exists."""
        ...

    @abstractmethod
    def list_keys(self, prefix: str) -> List[str]:
        """List all keys under a prefix."""
        ...

    @abstractmethod
    def delete(self, key: str) -> bool:
        """Delete key. Returns True if existed."""
        ...


class LocalFSBackend(StorageClient):
    """
    Local filesystem storage backend.

    Stores data files and companion .meta.json files under a root directory.
    Default root: data/lake/
    """

    def __init__(self, root: Optional[str] = None):
        if root is None:
            # Default to data/lake/ relative to project root
            project_root = Path(__file__).resolve().parent.parent.parent
            root = str(project_root / "data" / "lake")
        self.root = Path(root)
        self.root.mkdir(parents=True, exist_ok=True)

    def _data_path(self, key: str) -> Path:
        return self.root / key

    def _meta_path(self, key: str) -> Path:
        data_path = self._data_path(key)
        return data_path.parent / f".{data_path.name}.meta.json"

    def put(self, key: str, data: Any, content_type: str = "application/json", metadata: Optional[Dict[str, Any]] = None) -> ObjectMetadata:
        data_path = self._data_path(key)
        meta_path = self._meta_path(key)

        data_path.parent.mkdir(parents=True, exist_ok=True)

        now = time.time()

        # Load existing metadata to bump version
        existing_meta = self.get_metadata(key)
        version = (existing_meta.version + 1) if existing_meta else 1
        created_at = existing_meta.created_at if existing_meta else now

        # Write data
        if content_type == "application/json":
            raw = json.dumps(data, indent=2, ensure_ascii=False)
            data_path.write_text(raw, encoding="utf-8")
        elif content_type == "application/octet-stream" or content_type.startswith("application/x-numpy"):
            # Binary data (e.g., numpy arrays)
            if isinstance(data, bytes):
                data_path.write_bytes(data)
            else:
                data_path.write_bytes(data)
        else:
            # Default: write as text
            data_path.write_text(str(data), encoding="utf-8")

        size_bytes = data_path.stat().st_size

        obj_meta = ObjectMetadata(
            key=key,
            content_type=content_type,
            created_at=created_at,
            updated_at=now,
            version=version,
            size_bytes=size_bytes,
        )

        # Write metadata
        meta_dict = obj_meta.model_dump()
        if metadata:
            meta_dict["custom"] = metadata
        meta_path.write_text(json.dumps(meta_dict, indent=2), encoding="utf-8")

        return obj_meta

    def get(self, key: str) -> Optional[Any]:
        data_path = self._data_path(key)
        if not data_path.exists():
            return None

        meta = self.get_metadata(key)
        content_type = meta.content_type if meta else "application/json"

        if content_type == "application/json":
            text = data_path.read_text(encoding="utf-8")
            return json.loads(text)
        elif content_type.startswith("application/x-numpy") or content_type == "application/octet-stream":
            return data_path.read_bytes()
        else:
            return data_path.read_text(encoding="utf-8")

    def get_metadata(self, key: str) -> Optional[ObjectMetadata]:
        meta_path = self._meta_path(key)
        if not meta_path.exists():
            return None
        text = meta_path.read_text(encoding="utf-8")
        meta_dict = json.loads(text)
        # Filter to only ObjectMetadata fields
        return ObjectMetadata(
            key=meta_dict.get("key", key),
            content_type=meta_dict.get("content_type", "application/json"),
            created_at=meta_dict.get("created_at", 0.0),
            updated_at=meta_dict.get("updated_at", 0.0),
            version=meta_dict.get("version", 1),
            size_bytes=meta_dict.get("size_bytes", 0),
        )

    def exists(self, key: str) -> bool:
        return self._data_path(key).exists()

    def list_keys(self, prefix: str) -> List[str]:
        prefix_path = self.root / prefix
        if not prefix_path.exists():
            # Try as a partial prefix — walk the parent
            parent = prefix_path.parent
            if not parent.exists():
                return []
            keys = []
            prefix_str = prefix
            for dirpath, _, filenames in os.walk(self.root):
                for fname in filenames:
                    if fname.startswith("."):
                        continue  # Skip metadata files
                    full = Path(dirpath) / fname
                    rel = str(full.relative_to(self.root))
                    if rel.startswith(prefix_str):
                        keys.append(rel)
            return sorted(keys)

        keys = []
        for dirpath, _, filenames in os.walk(prefix_path):
            for fname in filenames:
                if fname.startswith("."):
                    continue
                full = Path(dirpath) / fname
                keys.append(str(full.relative_to(self.root)))
        return sorted(keys)

    def delete(self, key: str) -> bool:
        data_path = self._data_path(key)
        meta_path = self._meta_path(key)
        existed = data_path.exists()
        if data_path.exists():
            data_path.unlink()
        if meta_path.exists():
            meta_path.unlink()
        return existed
