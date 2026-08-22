import tempfile
import unittest
from pathlib import Path

from pipeline.sources.external_images import local_society_photo_dir


class LocalSocietyPhotoDirectoryTest(unittest.TestCase):
    def test_falls_back_to_canonical_rera_cache_directory(self):
        with tempfile.TemporaryDirectory() as directory:
            project_root = Path(directory)
            canonical_dir = (
                project_root
                / "data"
                / "cache"
                / "media_ingest"
                / "societies"
                / "rera-example"
            )
            canonical_dir.mkdir(parents=True)
            (canonical_dir / "1.jpg").write_bytes(b"cached image")

            resolved = local_society_photo_dir(
                project_root,
                "society:rera-example",
                "Example Heights",
            )

            self.assertEqual(resolved, canonical_dir)


if __name__ == "__main__":
    unittest.main()
