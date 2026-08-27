from __future__ import annotations

import tempfile
from pathlib import Path
import unittest

import generate


class RequestTests(unittest.TestCase):
    def test_text_only_request(self) -> None:
        request = generate.build_request("gemini-3-pro-image", "hello", None)
        self.assertEqual(request["input"], "hello")
        self.assertNotIn("generation_config", request)
        self.assertEqual(request["response_format"]["aspect_ratio"], "16:9")

    def test_reference_is_redacted_from_public_plan(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            reference = Path(directory) / "reference.png"
            reference.write_bytes(b"not-a-real-png-but-sufficient-for-plan-testing")
            request = generate.build_request(
                "gemini-3.1-flash-image", "hello", reference
            )
            public = generate.public_request(request, reference)

        self.assertEqual(public["input"][1]["path"], str(reference))
        self.assertNotIn("data", public["input"][1])
        self.assertEqual(request["generation_config"]["thinking_level"], "high")

    def test_safe_error_redacts_environment_key(self) -> None:
        previous = generate.os.environ.get("GEMINI_API_KEY")
        generate.os.environ["GEMINI_API_KEY"] = "example-secret"
        try:
            message = generate.safe_error(RuntimeError("bad example-secret"))
        finally:
            if previous is None:
                generate.os.environ.pop("GEMINI_API_KEY", None)
            else:
                generate.os.environ["GEMINI_API_KEY"] = previous
        self.assertNotIn("example-secret", message)
        self.assertIn("[REDACTED]", message)


if __name__ == "__main__":
    unittest.main()

