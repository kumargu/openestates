"""
ChatGPT Browser Client (Playwright)

Opens your actual system Firefox with your existing login session.
No separate login needed — uses your real Firefox profile.

Usage:
  python3 pipeline/chatgpt_client.py --test    # Test connection

Set CHATGPT_CONVERSATION_ID in .env to target a specific conversation.

NOTE: Firefox must be CLOSED before running, since Firefox locks its profile.
"""

from __future__ import annotations

import os
import re
import shutil
import sys
import time
from pathlib import Path
from typing import Optional

from playwright.sync_api import sync_playwright


PROJECT_ROOT = Path(__file__).resolve().parent.parent
PROFILE_COPY_DIR = Path(__file__).resolve().parent / ".firefox_profile_copy"


def _html_to_markdown(html: str) -> str:
    """Convert ChatGPT's rendered HTML back to markdown.

    ChatGPT renders markdown as HTML in the DOM. When we extract via innerHTML,
    we get tags like <h1>, <h2>, <p>, <ul>, <li>, <pre><code>, <strong>, <em>.
    This converts them back to markdown so day plans are properly formatted.

    Uses only stdlib — no external dependencies.
    """
    import html as html_module

    text = html

    # Code blocks: <pre><code class="language-xxx">...</code></pre>
    # Also handle ChatGPT's wrapper: <div class="..."><pre><code>...</code></pre></div>
    text = re.sub(
        r'<pre><code(?:\s+class="[^"]*language-(\w+)[^"]*")?[^>]*>(.*?)</code></pre>',
        lambda m: f'\n```{m.group(1) or ""}\n{html_module.unescape(m.group(2)).strip()}\n```\n',
        text, flags=re.DOTALL
    )

    # Inline code: <code>...</code>
    text = re.sub(r'<code>(.*?)</code>', r'`\1`', text)

    # Headers
    for i in range(6, 0, -1):
        text = re.sub(
            rf'<h{i}[^>]*>(.*?)</h{i}>',
            lambda m, level=i: f'\n{"#" * level} {m.group(1).strip()}\n',
            text, flags=re.DOTALL
        )

    # Bold and italic
    text = re.sub(r'<strong>(.*?)</strong>', r'**\1**', text, flags=re.DOTALL)
    text = re.sub(r'<em>(.*?)</em>', r'*\1*', text, flags=re.DOTALL)

    # List items: <li> may contain <p> tags inside — strip them first
    def _clean_li(m):
        content = m.group(1)
        # Remove <p>...</p> wrappers inside list items (ChatGPT nests these)
        content = re.sub(r'<p[^>]*>(.*?)</p>', r'\1', content, flags=re.DOTALL)
        # Collapse internal whitespace
        content = ' '.join(content.split())
        return f'- {content}\n'

    text = re.sub(r'<li[^>]*>(.*?)</li>', _clean_li, text, flags=re.DOTALL)

    # Remove list wrappers
    text = re.sub(r'</?[uo]l[^>]*>', '\n', text)

    # Paragraphs
    text = re.sub(r'<p[^>]*>(.*?)</p>', lambda m: f'\n{m.group(1).strip()}\n', text, flags=re.DOTALL)

    # Line breaks
    text = re.sub(r'<br\s*/?>', '\n', text)

    # Horizontal rules
    text = re.sub(r'<hr\s*/?>', '\n---\n', text)

    # Links: <a href="...">text</a>
    text = re.sub(r'<a[^>]+href="([^"]*)"[^>]*>(.*?)</a>', r'[\2](\1)', text, flags=re.DOTALL)

    # Tables: basic conversion
    text = re.sub(r'<th[^>]*>(.*?)</th>', r'| \1 ', text, flags=re.DOTALL)
    text = re.sub(r'<td[^>]*>(.*?)</td>', r'| \1 ', text, flags=re.DOTALL)
    text = re.sub(r'<tr[^>]*>(.*?)</tr>', lambda m: m.group(1).strip() + ' |\n', text, flags=re.DOTALL)

    # Strip any remaining HTML tags
    text = re.sub(r'<[^>]+>', '', text)

    # Unescape HTML entities
    text = html_module.unescape(text)

    # Post-processing cleanup:
    # 1. Fix orphaned bullets: "- \n  actual content" → "- actual content"
    text = re.sub(r'^- \n\s*(\S)', r'- \1', text, flags=re.MULTILINE)

    # 2. Fix bare language labels that should be code fence markers
    #    "Plain text\n..." or "JSON\n{" → "```\n..."
    text = re.sub(
        r'\n(Plain text|JSON|TypeScript|Rust|Bash|Markdown|Python)\n',
        lambda m: f'\n```{m.group(1).lower() if m.group(1) not in ("Plain text",) else ""}\n',
        text
    )

    # 3. Clean up excessive blank lines (max 2)
    text = re.sub(r'\n{3,}', '\n\n', text)

    return text.strip()

# Auto-detect Firefox profile
FIREFOX_PROFILES_DIR = Path.home() / "Library" / "Application Support" / "Firefox" / "Profiles"


def _load_dotenv():
    """Load .env from project root."""
    env_file = PROJECT_ROOT / ".env"
    if not env_file.exists():
        return
    for line in env_file.read_text().splitlines():
        line = line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, _, value = line.partition("=")
        key, value = key.strip(), value.strip()
        if key and not os.environ.get(key):
            os.environ[key] = value


_load_dotenv()


def find_firefox_profile() -> Path:
    """Find the most recently used Firefox profile."""
    if not FIREFOX_PROFILES_DIR.exists():
        raise RuntimeError(f"Firefox profiles not found at {FIREFOX_PROFILES_DIR}")

    profiles = list(FIREFOX_PROFILES_DIR.iterdir())
    if not profiles:
        raise RuntimeError("No Firefox profiles found")

    # Find profile with most recently modified cookies
    best = None
    best_mtime = 0
    for p in profiles:
        cookies = p / "cookies.sqlite"
        if cookies.exists():
            mtime = cookies.stat().st_mtime
            if mtime > best_mtime:
                best_mtime = mtime
                best = p

    if best is None:
        raise RuntimeError("No Firefox profile with cookies found")

    return best


def copy_firefox_profile(source_profile: Path) -> Path:
    """Copy Firefox profile for Playwright use.

    Firefox locks its profile so we can't use it directly while Firefox
    might be running. We copy the essential files (cookies, storage).
    """
    PROFILE_COPY_DIR.mkdir(parents=True, exist_ok=True)

    # Copy only essential files for session persistence
    essential_files = [
        "cookies.sqlite",
        "cookies.sqlite-wal",
        "cookies.sqlite-shm",
        "webappsstore.sqlite",
        "webappsstore.sqlite-wal",
        "webappsstore.sqlite-shm",
    ]

    for fname in essential_files:
        src = source_profile / fname
        if src.exists():
            shutil.copy2(str(src), str(PROFILE_COPY_DIR / fname))

    # Also copy localStorage / IndexedDB for session tokens
    storage_dir = source_profile / "storage" / "default"
    if storage_dir.exists():
        dest_storage = PROFILE_COPY_DIR / "storage" / "default"
        # Only copy chatgpt-related storage
        for item in storage_dir.iterdir():
            if "openai" in item.name.lower() or "chatgpt" in item.name.lower():
                dest_item = dest_storage / item.name
                if dest_item.exists():
                    shutil.rmtree(str(dest_item))
                shutil.copytree(str(item), str(dest_item))

    return PROFILE_COPY_DIR


class ChatGPTClient:
    """Browser-based ChatGPT client using Playwright + Firefox."""

    def __init__(
        self,
        conversation_id: Optional[str] = None,
        headless: bool = True,
    ):
        self.conversation_id = conversation_id or os.environ.get("CHATGPT_CONVERSATION_ID")
        self.headless = headless
        self._playwright = None
        self._browser = None
        self._context = None
        self._page = None

    def _ensure_browser(self):
        """Launch Firefox with copied profile."""
        if self._page is not None:
            return

        # Find and copy the Firefox profile
        profile = find_firefox_profile()
        profile_copy = copy_firefox_profile(profile)

        self._playwright = sync_playwright().start()

        # Launch Firefox with anti-detection
        self._browser = self._playwright.firefox.launch(
            headless=self.headless,
            firefox_user_prefs={
                "dom.webdriver.enabled": False,
            },
        )
        self._context = self._browser.new_context(
            viewport={"width": 1280, "height": 900},
        )
        # Remove webdriver detection flag
        self._context.add_init_script(
            "Object.defineProperty(navigator, 'webdriver', {get: () => undefined})"
        )

        # Import cookies from Firefox profile
        self._import_firefox_cookies(profile_copy)

        self._page = self._context.new_page()

    def _import_firefox_cookies(self, profile_dir: Path):
        """Extract cookies from Firefox SQLite and add to Playwright context."""
        import sqlite3

        cookies_db = profile_dir / "cookies.sqlite"
        if not cookies_db.exists():
            print("  WARNING: No cookies.sqlite found")
            return

        conn = sqlite3.connect(str(cookies_db))
        try:
            cursor = conn.execute("""
                SELECT name, value, host, path, expiry, isSecure, isHttpOnly, sameSite
                FROM moz_cookies
                WHERE host LIKE '%openai%' OR host LIKE '%chatgpt%'
            """)

            cookies = []
            for name, value, host, path, expiry, is_secure, is_http_only, same_site in cursor:
                # Convert Firefox domain format to Playwright format
                domain = host.lstrip(".")

                # Map sameSite values
                same_site_map = {0: "None", 1: "Lax", 2: "Strict"}
                ss = same_site_map.get(same_site, "Lax")

                # Firefox stores expiry in ms, Playwright wants seconds
                raw_exp = int(expiry) if expiry else 0
                if raw_exp > 1e12:  # milliseconds — convert to seconds
                    exp = raw_exp // 1000
                elif raw_exp > 0:
                    exp = raw_exp
                else:
                    exp = -1

                cookie = {
                    "name": name,
                    "value": value,
                    "domain": domain,
                    "path": path or "/",
                    "secure": bool(is_secure),
                    "httpOnly": bool(is_http_only),
                    "expires": exp,
                    "sameSite": ss,
                }

                cookies.append(cookie)

            if cookies:
                self._context.add_cookies(cookies)
                print(f"  Imported {len(cookies)} cookies from Firefox")
            else:
                print("  WARNING: No ChatGPT cookies found in Firefox profile")
        finally:
            conn.close()

    def _navigate_to_conversation(self):
        """Navigate to the ChatGPT conversation."""
        self._ensure_browser()
        page = self._page

        if self.conversation_id:
            url = f"https://chatgpt.com/c/{self.conversation_id}"
        else:
            url = "https://chatgpt.com"

        current = page.url
        if self.conversation_id and self.conversation_id in current:
            return

        page.goto(url, wait_until="networkidle", timeout=30000)

        # Check if login is needed
        if "auth" in page.url or "login" in page.url:
            raise RuntimeError(
                "Not logged into ChatGPT in Firefox. "
                "Open Firefox, log into chatgpt.com, then close Firefox and retry."
            )

        # Wait for chat interface
        page.wait_for_selector('[id="prompt-textarea"]', timeout=15000)

    def send_message(self, message: str) -> str:
        """Type a message into ChatGPT and wait for the response."""
        self._navigate_to_conversation()
        page = self._page

        # Find the contenteditable element inside the prompt textarea
        editor = page.locator('#prompt-textarea [contenteditable="true"]')
        if editor.count() == 0:
            # Fallback: the textarea itself might be contenteditable
            editor = page.locator('#prompt-textarea')

        editor.wait_for(state="visible", timeout=10000)
        editor.click()
        time.sleep(0.3)

        # For short messages, type directly (triggers React properly)
        # For long messages, use clipboard paste
        if len(message) < 500:
            page.keyboard.type(message, delay=10)
        else:
            # Clipboard paste for long prompts
            self._page.evaluate("text => navigator.clipboard.writeText(text)", message)
            page.keyboard.press("Meta+a")
            time.sleep(0.1)
            page.keyboard.press("Meta+v")
        time.sleep(1)

        # Click send via JavaScript (avoids overlay/interception issues)
        page.evaluate("""() => {
            const btn = document.querySelector('[data-testid="send-button"]')
                     || document.querySelector('#composer-submit-button');
            if (btn) btn.click();
        }""")
        time.sleep(0.5)
        # Fallback: press Enter
        page.keyboard.press("Enter")

        time.sleep(2)

        response = self._wait_for_response(timeout=180)
        return response

    def _wait_for_response(self, timeout: int = 180) -> str:
        """Wait for ChatGPT to finish responding and extract text."""
        page = self._page
        start = time.time()

        # Wait for streaming to start
        try:
            page.wait_for_selector(
                '[data-testid="stop-button"], [aria-label="Stop generating"]',
                timeout=15000
            )
        except Exception:
            pass

        # Wait for streaming to finish
        while time.time() - start < timeout:
            stop_btn = page.locator('[data-testid="stop-button"], [aria-label="Stop generating"]')
            if stop_btn.count() == 0:
                break
            time.sleep(1)

        time.sleep(1)

        # Extract last assistant message as HTML, then convert to markdown
        html = page.evaluate("""() => {
            const msgs = document.querySelectorAll('[data-message-author-role="assistant"]');
            if (msgs.length === 0) return '';
            const last = msgs[msgs.length - 1];
            const content = last.querySelector('.markdown');
            if (content) return content.innerHTML;
            return last.innerText;
        }""")

        if not html:
            return ""

        return _html_to_markdown(html)

    def start_new_conversation(self) -> str:
        """Start a new conversation."""
        self._ensure_browser()
        self._page.goto("https://chatgpt.com", wait_until="networkidle", timeout=30000)
        self._page.wait_for_selector('[id="prompt-textarea"]', timeout=15000)

        old_id = self.conversation_id
        self.conversation_id = None

        handoff = (
            f"This is a continuation of our OpenEstates product planning conversation. "
            f"Previous conversation: {old_id}. "
            f"Continue as the product visionary for OpenEstates — "
            f"a transparency-first property discovery platform. "
            f"Ready for the next day plan."
        )
        self.send_message(handoff)

        time.sleep(1)
        url = self._page.url
        match = re.search(r'/c/([a-f0-9-]+)', url)
        if match:
            self.conversation_id = match.group(1)

        return self.conversation_id or "new"

    def get_conversation_length(self) -> int:
        """Count messages in the current conversation."""
        try:
            self._navigate_to_conversation()
            count = self._page.evaluate("""() => {
                return document.querySelectorAll(
                    '[data-message-author-role="user"], [data-message-author-role="assistant"]'
                ).length;
            }""")
            return count or 0
        except Exception:
            return 0

    def close(self):
        """Close the browser."""
        if self._context:
            self._context.close()
        if self._browser:
            self._browser.close()
        if self._playwright:
            self._playwright.stop()
        self._page = None
        self._context = None
        self._browser = None
        self._playwright = None

    def __del__(self):
        try:
            self.close()
        except Exception:
            pass


def test_connection():
    """Quick test: open ChatGPT and verify login."""
    print("  Testing ChatGPT browser connection...")
    print(f"  (Make sure Firefox is CLOSED first)\n")

    client = ChatGPTClient(headless=False)
    try:
        print(f"  Conversation: {client.conversation_id}")
        print("  Sending test message...")
        response = client.send_message(
            "Reply with just the word 'connected' — this is a test."
        )
        print(f"  Response: {response[:200]}")
        print("\n  Connection OK")
    finally:
        client.close()


if __name__ == "__main__":
    if "--test" in sys.argv:
        test_connection()
    else:
        print("Usage:")
        print("  python3 pipeline/chatgpt_client.py --test  # Test connection")
        print()
        print("Make sure Firefox is CLOSED before running (Firefox locks its profile).")
        print("Your existing Firefox login cookies are copied automatically.")
