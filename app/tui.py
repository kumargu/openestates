"""
OpenEstates Prototype — Chat-First Terminal Interface (Day 3)

Layout:
  ┌────────────────────────────────┬──────────────────┐
  │  Chat transcript               │  Buyer Context   │
  │                                │  + Signals       │
  │                                │  + Last diff     │
  ├────────────────────────────────┴──────────────────┤
  │  > input                                          │
  └───────────────────────────────────────────────────┘

Commands: /help  /generate  /buyer <id>  /context  /extract [N]  /clear  /quit
"""

import json
import os
import random
import sys
from typing import Dict, List, Optional

from textual.app import App, ComposeResult
from textual.binding import Binding
from textual.containers import Horizontal
from textual.widgets import Footer, Header, Input, RichLog, Static

# Project root for imports and data paths
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
PROJECT_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DATA_DIR = os.path.join(PROJECT_ROOT, "data")


def _format_price(amount: int) -> str:
    if amount >= 10_000_000:
        return f"{amount / 10_000_000:.1f}Cr"
    return f"{amount / 100_000:.0f}L"


class OpenEstatesApp(App):
    """Chat-first TUI for the OpenEstates prototype."""

    TITLE = "OpenEstates"

    CSS = """
    #main {
        height: 1fr;
    }
    #chat {
        width: 60%;
        border: solid $primary;
        padding: 0 1;
    }
    #context-panel {
        width: 40%;
        border: solid $accent;
        padding: 1;
        overflow-y: auto;
    }
    #cmd-input {
        dock: bottom;
        height: 3;
        margin: 0 1;
    }
    """

    BINDINGS = [
        Binding("ctrl+c", "quit", "Quit", show=False),
    ]

    def __init__(self):
        super().__init__()
        # Session state
        self.active_buyer_id: Optional[str] = None
        self.active_buyer_profile: Optional[dict] = None
        self.conversation_turns: List[str] = []
        self.last_diffs: List[dict] = []
        self.market_data: Optional[dict] = None

        # Services (initialized on mount)
        self.graph_store = None
        self.context_graph = None
        self.extractor = None
        self.narrator = None
        self.openfang_available = False

    def compose(self) -> ComposeResult:
        yield Header(show_clock=True)
        with Horizontal(id="main"):
            yield RichLog(id="chat", wrap=True, markup=True)
            yield Static("No buyer selected.\n\nUse /generate then /buyer <id>", id="context-panel")
        yield Input(placeholder="Type a message or /command...", id="cmd-input")
        yield Footer()

    def on_mount(self) -> None:
        from agents.openfang_client import OpenFangClient
        from agents.signal_extractor import OpenFangExtractor, StubExtractor
        from agents.change_narrator import OpenFangNarrator, StubNarrator
        from graph.graph_store import GraphStore

        self.graph_store = GraphStore(DATA_DIR)

        # Try OpenFang, fall back to stub
        client = OpenFangClient()
        self.openfang_available = client.is_available()

        if self.openfang_available:
            self.extractor = OpenFangExtractor(client)
            self.narrator = OpenFangNarrator(client)
            self.sub_title = "OpenFang: connected"
        else:
            self.extractor = StubExtractor()
            self.narrator = StubNarrator()
            self.sub_title = "OpenFang: offline (stub mode)"

        # Try loading existing market
        market_path = os.path.join(DATA_DIR, "synthetic_market.json")
        if os.path.exists(market_path):
            with open(market_path) as f:
                self.market_data = json.load(f)

        chat = self.query_one("#chat", RichLog)
        chat.write("[bold]OpenEstates Prototype[/bold]")
        if not self.openfang_available:
            chat.write("[yellow]OpenFang offline — using stub extractor[/yellow]")
        chat.write("Type [bold]/help[/bold] for commands.\n")

    # --- Input handling ---

    def on_input_submitted(self, event: Input.Submitted) -> None:
        text = event.value.strip()
        if not text:
            return
        event.input.value = ""

        chat = self.query_one("#chat", RichLog)

        if text.startswith("/"):
            self._handle_command(text, chat)
        else:
            self._handle_chat(text, chat)

    def _handle_command(self, text: str, chat: RichLog) -> None:
        parts = text.split(maxsplit=1)
        cmd = parts[0].lower()
        arg = parts[1].strip() if len(parts) > 1 else ""

        commands = {
            "/help":     self._cmd_help,
            "/generate": self._cmd_generate,
            "/buyer":    self._cmd_buyer,
            "/context":  self._cmd_context,
            "/extract":  self._cmd_extract,
            "/clear":    self._cmd_clear,
            "/quit":     lambda c, a: self.exit(),
        }

        handler = commands.get(cmd)
        if handler:
            handler(chat, arg)
        else:
            chat.write(f"[red]Unknown command: {cmd}[/red]  Type /help")

    def _handle_chat(self, text: str, chat: RichLog) -> None:
        if not self.active_buyer_id:
            chat.write("[yellow]No buyer selected. Use /buyer <id> first.[/yellow]")
            return
        self.conversation_turns.append(text)
        chat.write(f"[bold]>[/bold] {text}")

    # --- Commands ---

    def _cmd_help(self, chat: RichLog, arg: str) -> None:
        chat.write(
            "\n[bold]Commands:[/bold]\n"
            "  /generate          Generate synthetic market\n"
            "  /buyer <id>        Select a buyer (random if no id)\n"
            "  /context           Show current context graph\n"
            "  /extract [N]       Extract signals from last N turns (default 5)\n"
            "  /clear             Clear chat and conversation buffer\n"
            "  /quit              Exit\n"
        )

    def _cmd_generate(self, chat: RichLog, arg: str) -> None:
        chat.write("[dim]Generating synthetic market...[/dim]")

        from simulation.market_generator import MarketGenerator
        gen = MarketGenerator(seed=42)
        result = gen.generate(num_buyers=150, num_sellers=200, num_properties=200)

        os.makedirs(DATA_DIR, exist_ok=True)
        with open(os.path.join(DATA_DIR, "synthetic_market.json"), "w") as f:
            json.dump(result["market"], f, indent=2)
        with open(os.path.join(DATA_DIR, "synthetic_market_truth.json"), "w") as f:
            json.dump(result["truth"], f, indent=2)

        self.market_data = result["market"]
        counts = result["market"]["meta"]["counts"]
        chat.write(
            f"[green]Market generated[/green]  "
            f"Properties: {counts['properties']}  "
            f"Buyers: {counts['buyers']}  "
            f"Sellers: {counts['sellers']}"
        )

    def _cmd_buyer(self, chat: RichLog, arg: str) -> None:
        if not self.market_data:
            chat.write("[red]No market data. Run /generate first.[/red]")
            return

        buyers = self.market_data["buyers"]
        buyer_map = {b["id"]: b for b in buyers}

        if arg:
            buyer = buyer_map.get(arg)
            if not buyer:
                chat.write(f"[red]Buyer '{arg}' not found.[/red]")
                return
        else:
            buyer = random.choice(buyers)

        self.active_buyer_id = buyer["id"]
        self.active_buyer_profile = buyer
        self.conversation_turns = []
        self.last_diffs = []
        self.context_graph = self.graph_store.load(buyer["id"])

        areas = ", ".join(buyer["preferred_areas"])
        bhk = ", ".join(str(b) for b in buyer["preferred_bhk"])
        chat.write(
            f"\n[green]Loaded: {buyer['id']}[/green]\n"
            f"  Budget: {_format_price(buyer['budget_min'])} – {_format_price(buyer['budget_max'])}\n"
            f"  Areas: {areas}\n"
            f"  BHK: [{bhk}]  Timeline: {buyer['timeline_months']}mo\n"
        )
        self._refresh_context_panel()

    def _cmd_context(self, chat: RichLog, arg: str) -> None:
        if not self.context_graph:
            chat.write("[yellow]No buyer selected.[/yellow]")
            return

        snapshot = self.context_graph.snapshot()
        signals = snapshot.get("signals", {})
        if not signals:
            chat.write("[dim]Context graph is empty. Chat then /extract to build it.[/dim]")
        else:
            chat.write(f"\n[bold]Context for {self.active_buyer_id}[/bold]")
            for key, sig in signals.items():
                chat.write(
                    f"  {key:28s} {sig['value']:12s} "
                    f"w:{sig['weight']:.2f}  c:{sig['confidence']:.2f}  "
                    f"x{sig['repetition_count']}"
                )
            chat.write("")

    def _cmd_extract(self, chat: RichLog, arg: str) -> None:
        if not self.active_buyer_id:
            chat.write("[yellow]No buyer selected.[/yellow]")
            return
        if not self.conversation_turns:
            chat.write("[yellow]No conversation turns to extract from. Type some messages first.[/yellow]")
            return

        n = int(arg) if arg.isdigit() else 5
        turns = self.conversation_turns[-n:]
        context_before = self.context_graph.snapshot()

        chat.write(f"[dim]Extracting signals from {len(turns)} turn(s)...[/dim]")

        try:
            updates = self.extractor.extract(
                buyer_id=self.active_buyer_id,
                buyer_profile=self.active_buyer_profile,
                context_snapshot=context_before,
                conversation_turns=turns,
            )
        except (ConnectionError, ValueError) as e:
            chat.write(f"[red]Extraction failed: {e}[/red]")
            chat.write("[yellow]Falling back to stub extractor.[/yellow]")
            from agents.signal_extractor import StubExtractor
            stub = StubExtractor()
            updates = stub.extract(
                buyer_id=self.active_buyer_id,
                buyer_profile=self.active_buyer_profile,
                context_snapshot=context_before,
                conversation_turns=turns,
            )

        if not updates:
            chat.write("[dim]No signals extracted.[/dim]")
            return

        # Apply updates
        self.last_diffs = self.context_graph.apply_updates(updates)
        self.graph_store.save(self.context_graph)
        context_after = self.context_graph.snapshot()

        # Print extracted signals
        chat.write(f"\n[green]Signals extracted ({len(updates)}):[/green]")
        for u in updates:
            chat.write(
                f"  {u.signal_key:28s} {u.signal_value:12s} "
                f"w:{u.weight:.1f} c:{u.confidence:.1f} [{u.action}]"
            )

        # Print diff
        chat.write("\n[bold]Context diff:[/bold]")
        for d in self.last_diffs:
            before_w = d["before"]["weight"] if d["before"] else None
            after_w = d["after"]["weight"] if d["after"] else None
            if d["action"] == "remove":
                chat.write(f"  [red]- {d['key']}[/red]")
            elif before_w is None:
                chat.write(f"  [green]+ {d['key']}  new w:{after_w:.2f}[/green]")
            else:
                chat.write(f"  [cyan]~ {d['key']}  w:{before_w:.2f} -> {after_w:.2f}[/cyan]")

        # Narrate
        try:
            narrative = self.narrator.narrate(context_before, context_after, updates)
            chat.write(f"\n[bold cyan]What changed:[/bold cyan] {narrative}\n")
        except Exception as e:
            chat.write(f"[dim]Narrative unavailable: {e}[/dim]\n")

        self._refresh_context_panel()

    def _cmd_clear(self, chat: RichLog, arg: str) -> None:
        chat.clear()
        self.conversation_turns = []
        self.last_diffs = []
        chat.write("[dim]Chat and conversation buffer cleared.[/dim]")

    # --- Context panel ---

    def _refresh_context_panel(self) -> None:
        panel = self.query_one("#context-panel", Static)

        if not self.active_buyer_id or not self.active_buyer_profile:
            panel.update("No buyer selected.\n\nUse /generate then /buyer <id>")
            return

        b = self.active_buyer_profile
        areas = ", ".join(b["preferred_areas"])
        bhk = ", ".join(str(x) for x in b["preferred_bhk"])

        lines = [
            f"[bold]{b['id']}[/bold]",
            f"Budget: {_format_price(b['budget_min'])} – {_format_price(b['budget_max'])}",
            f"Areas:  {areas}",
            f"BHK:    [{bhk}]",
            f"Timeline: {b['timeline_months']}mo",
            "",
        ]

        # Current signals
        snapshot = self.context_graph.snapshot() if self.context_graph else {}
        signals = snapshot.get("signals", {})
        lines.append(f"[bold]Signals ({len(signals)})[/bold]")
        if signals:
            for key, sig in signals.items():
                lines.append(f"  {key:20s} {sig['value']:8s} w:{sig['weight']:.2f}")
        else:
            lines.append("  [dim]empty[/dim]")

        # Last diff
        if self.last_diffs:
            lines.append("")
            lines.append("[bold]Last diff[/bold]")
            for d in self.last_diffs:
                before_w = d["before"]["weight"] if d["before"] else None
                after_w = d["after"]["weight"] if d["after"] else None
                if d["action"] == "remove":
                    lines.append(f"  [red]- {d['key']}[/red]")
                elif before_w is None:
                    lines.append(f"  [green]+ {d['key']} new {after_w:.2f}[/green]")
                else:
                    lines.append(f"  [cyan]~ {d['key']} {before_w:.2f}->{after_w:.2f}[/cyan]")

        panel.update("\n".join(lines))
