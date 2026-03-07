"""
OpenFang REST client — thin wrapper around the /v1/chat/completions endpoint.

Uses only stdlib (urllib). No external dependencies.
"""

import json
import urllib.request
import urllib.error


class OpenFangClient:
    """Communicates with the OpenFang daemon via its OpenAI-compatible API."""

    def __init__(self, base_url: str = "http://localhost:4200"):
        self.base_url = base_url.rstrip("/")

    def is_available(self) -> bool:
        """Check if OpenFang is reachable."""
        try:
            req = urllib.request.Request(f"{self.base_url}/health", method="GET")
            with urllib.request.urlopen(req, timeout=3):
                return True
        except Exception:
            return False

    def chat_completion(
        self,
        system_prompt: str,
        user_message: str,
        model: str = "default",
    ) -> str:
        """
        Send a one-shot chat completion request.
        Returns the raw content string from the response.
        Raises ConnectionError if OpenFang is unreachable.
        Raises ValueError if response is malformed.
        """
        payload = {
            "model": model,
            "messages": [
                {"role": "system", "content": system_prompt},
                {"role": "user", "content": user_message},
            ],
            "response_format": {"type": "json_object"},
        }

        data = json.dumps(payload).encode("utf-8")
        req = urllib.request.Request(
            f"{self.base_url}/v1/chat/completions",
            data=data,
            headers={"Content-Type": "application/json"},
            method="POST",
        )

        try:
            with urllib.request.urlopen(req, timeout=30) as resp:
                body = json.loads(resp.read().decode("utf-8"))
        except (urllib.error.URLError, ConnectionRefusedError, OSError) as e:
            raise ConnectionError(f"OpenFang unavailable at {self.base_url}: {e}")

        try:
            return body["choices"][0]["message"]["content"]
        except (KeyError, IndexError) as e:
            raise ValueError(f"Unexpected OpenFang response shape: {e}")
