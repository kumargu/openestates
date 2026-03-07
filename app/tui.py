"""
OpenEstates — Conversational Terminal Interface

Simple single-window chat. The buyer talks, the system listens,
extracts signals, and helps find properties.

Slash commands exist for power users but the primary flow is conversation.

Run: .venv/bin/python app/main.py
"""

import json
import os
import random
import sys
from typing import Dict, List, Optional

from rich.console import Console
from rich.panel import Panel
from rich.table import Table
from rich.text import Text

# Project root for imports and data paths
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
PROJECT_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DATA_DIR = os.path.join(PROJECT_ROOT, "data")

console = Console()


def _format_price(amount: int) -> str:
    if amount >= 10_000_000:
        return f"{amount / 10_000_000:.1f}Cr"
    return f"{amount / 100_000:.0f}L"


class OpenEstatesChat:
    """Conversational interface for the OpenEstates prototype."""

    def __init__(self):
        self.active_buyer_id: Optional[str] = None
        self.active_buyer_profile: Optional[dict] = None
        self.conversation_turns: List[str] = []
        self.market_data: Optional[dict] = None
        self.truth_data: Optional[dict] = None

        self.graph_store = None
        self.context_graph = None
        self.extractor = None
        self.narrator = None

    def setup(self):
        """Initialize services and load data."""
        from agents.openfang_client import OpenFangClient
        from agents.signal_extractor import OpenFangExtractor, StubExtractor
        from agents.change_narrator import OpenFangNarrator, StubNarrator
        from graph.graph_store import GraphStore

        self.graph_store = GraphStore(DATA_DIR)

        # Initialize truth model weights file if not present
        from simulation.truth_model import initialize_weights_file
        initialize_weights_file()

        client = OpenFangClient()
        if client.is_available():
            self.extractor = OpenFangExtractor(client)
            self.narrator = OpenFangNarrator(client)
        else:
            self.extractor = StubExtractor()
            self.narrator = StubNarrator()

        # Load existing market if available
        market_path = os.path.join(DATA_DIR, "synthetic_market.json")
        truth_path = os.path.join(DATA_DIR, "synthetic_market_truth.json")
        if os.path.exists(market_path):
            with open(market_path) as f:
                self.market_data = json.load(f)
        if os.path.exists(truth_path):
            with open(truth_path) as f:
                self.truth_data = json.load(f)

    def _ensure_market(self):
        """Generate market if not loaded."""
        if self.market_data:
            return
        console.print("[dim]Setting up market...[/dim]")
        from simulation.market_generator import MarketGenerator
        gen = MarketGenerator(seed=42)
        result = gen.generate(num_buyers=150, num_sellers=200, num_properties=200)
        os.makedirs(DATA_DIR, exist_ok=True)
        with open(os.path.join(DATA_DIR, "synthetic_market.json"), "w") as f:
            json.dump(result["market"], f, indent=2)
        with open(os.path.join(DATA_DIR, "synthetic_market_truth.json"), "w") as f:
            json.dump(result["truth"], f, indent=2)
        self.market_data = result["market"]
        self.truth_data = result["truth"]
        counts = result["market"]["meta"]["counts"]
        console.print(
            f"  [green]Market ready[/green] — "
            f"{counts['properties']} properties, "
            f"{counts['buyers']} buyers, "
            f"{counts['sellers']} sellers"
        )

    def _ensure_buyer(self):
        """Pick a random buyer if none selected."""
        if self.active_buyer_id:
            return
        self._ensure_market()
        buyer = random.choice(self.market_data["buyers"])
        self._set_buyer(buyer)

    def _set_buyer(self, buyer: dict):
        self.active_buyer_id = buyer["id"]
        self.active_buyer_profile = buyer
        self.conversation_turns = []
        self.context_graph = self.graph_store.load(buyer["id"])

        areas = ", ".join(buyer["preferred_areas"])
        bhk = "/".join(str(b) for b in buyer["preferred_bhk"])
        console.print(
            f"\n  [bold]You are {buyer['id']}[/bold]\n"
            f"  Budget: {_format_price(buyer['budget_min'])} – {_format_price(buyer['budget_max'])}  |  "
            f"Areas: {areas}  |  {bhk}BHK  |  {buyer['timeline_months']}mo timeline\n"
        )

    def _extract_and_respond(self):
        """Extract signals from recent conversation and show what was learned."""
        if not self.conversation_turns or not self.context_graph:
            return

        turns = self.conversation_turns[-5:]
        context_before = self.context_graph.snapshot()

        try:
            updates = self.extractor.extract(
                buyer_id=self.active_buyer_id,
                buyer_profile=self.active_buyer_profile,
                context_snapshot=context_before,
                conversation_turns=turns,
            )
        except (ConnectionError, ValueError):
            from agents.signal_extractor import StubExtractor
            updates = StubExtractor().extract(
                buyer_id=self.active_buyer_id,
                buyer_profile=self.active_buyer_profile,
                context_snapshot=context_before,
                conversation_turns=turns,
            )

        if not updates:
            return

        diffs = self.context_graph.apply_updates(updates)
        self.graph_store.save(self.context_graph)

        # Log
        self.graph_store.log_event("conversation_ingested", {
            "buyer_id": self.active_buyer_id,
            "turns_count": len(turns),
            "updates_count": len(updates),
            "signals": [u.signal_key for u in updates],
        })

        # Show what we learned — brief and inline
        console.print()
        for d in diffs:
            after = d["after"]
            before = d["before"]
            if d["action"] == "remove":
                console.print(f"  [red]- {d['key']}[/red]")
            elif before is None:
                console.print(f"  [green]+ {d['key']}: {after['value']}[/green]")
            else:
                console.print(
                    f"  [cyan]~ {d['key']}: {before['value']} -> {after['value']} "
                    f"(confidence {after['confidence']:.1f})[/cyan]"
                )

        # Narrate briefly
        context_after = self.context_graph.snapshot()
        try:
            narrative = self.narrator.narrate(context_before, context_after, updates)
            console.print(f"\n  [italic]{narrative}[/italic]")
        except Exception:
            pass

    def _show_matches(self, top_k: int = 5):
        """Show current best property matches based on context."""
        if not self.market_data or not self.active_buyer_profile:
            console.print("  [yellow]No buyer or market data yet.[/yellow]")
            return

        # For now, use a simple filter-based approach from observable data
        # (truth model is for eval only, matching engine is Day 5)
        buyer = self.active_buyer_profile
        scored = []
        for prop in self.market_data["properties"]:
            score = 0.0
            # Budget fit
            if buyer["budget_min"] <= prop["price"] <= buyer["budget_max"]:
                score += 0.3
            elif prop["price"] <= buyer["budget_max"] * 1.1:
                score += 0.15
            # Area fit
            if prop["area"] in buyer["preferred_areas"]:
                score += 0.25
            # BHK fit
            if prop["bhk"] in buyer["preferred_bhk"]:
                score += 0.2
            # Metro (higher weight = more important)
            metro_score = max(0, 1 - prop["metro_distance_minutes"] / 45)
            score += 0.1 * buyer["metro_preference_weight"] * metro_score
            # Doc safety
            score += 0.1 * buyer["document_safety_weight"] * prop["document_completeness_score"]
            # Low noise bonus
            score += 0.05 * (1 - prop["noise_score"])

            scored.append((prop, score))

        scored.sort(key=lambda x: x[1], reverse=True)
        top = scored[:top_k]

        if not top:
            console.print("  [yellow]No matches found.[/yellow]")
            return

        table = Table(show_header=True, header_style="bold", box=None, padding=(0, 2))
        table.add_column("#", style="dim", width=3)
        table.add_column("Property")
        table.add_column("Area")
        table.add_column("BHK", justify="center")
        table.add_column("Price", justify="right")
        table.add_column("Match", justify="right")

        for i, (prop, score) in enumerate(top, 1):
            match_pct = f"{score * 100:.0f}%"
            table.add_row(
                str(i),
                prop["id"],
                prop["area"],
                str(prop["bhk"]),
                _format_price(prop["price"]),
                match_pct,
            )

        console.print()
        console.print(table)
        console.print()

    # --- Command handlers ---

    def _cmd_help(self, arg: str):
        console.print(
            "\n  [bold]Just type naturally[/bold] — tell me what you're looking for.\n"
            "  I'll extract what matters and find matching properties.\n\n"
            "  [bold]Commands:[/bold]\n"
            "  /find              Show best matches for you\n"
            "  /context           Show what I know about your preferences\n"
            "  /buyer [id]        Switch buyer (random if no id)\n"
            "  /generate          Regenerate market data\n"
            "  /truth [id]        [debug] Show ground truth matches\n"
            "  /truth weights     [debug] Show truth model weight rationale\n"
            "  /research reddit [sub] [limit]  Analyze Reddit posts\n"
            "  /research reddit slow           Slow crawl (new+hot)\n"
            "  /research taxonomy              Show accumulated knowledge\n"
            "  /research compact               Compact + merge stale data\n"
            "  /clear             Clear conversation history\n"
            "  /quit              Exit\n"
        )

    def _cmd_find(self, arg: str):
        self._ensure_buyer()
        # Extract first if there are unprocessed turns
        if self.conversation_turns:
            self._extract_and_respond()
        self._show_matches()

    def _cmd_context(self, arg: str):
        if not self.context_graph:
            console.print("  [dim]No context yet. Start chatting![/dim]")
            return

        snapshot = self.context_graph.snapshot()
        signals = snapshot.get("signals", {})
        if not signals:
            console.print("  [dim]No signals learned yet. Tell me what you're looking for.[/dim]")
            return

        console.print(f"\n  [bold]What I know about {self.active_buyer_id}:[/bold]")
        for key, sig in signals.items():
            console.print(
                f"    {key}: [bold]{sig['value']}[/bold]  "
                f"(weight: {sig['weight']:.1f}, confidence: {sig['confidence']:.1f}, "
                f"mentioned {sig['repetition_count']}x)"
            )
        console.print()

    def _cmd_buyer(self, arg: str):
        self._ensure_market()
        buyers = self.market_data["buyers"]
        if arg:
            buyer_map = {b["id"]: b for b in buyers}
            buyer = buyer_map.get(arg)
            if not buyer:
                console.print(f"  [red]Buyer '{arg}' not found.[/red]")
                return
        else:
            buyer = random.choice(buyers)
        self._set_buyer(buyer)

    def _cmd_generate(self, arg: str):
        self.market_data = None
        self.truth_data = None
        self._ensure_market()

    def _cmd_truth(self, arg: str):
        from simulation.truth_model import rank_properties_for_buyer, show_weights

        # /truth weights — show current weight config
        if arg.strip().lower() == "weights":
            console.print()
            for line in show_weights():
                console.print(f"  {line}")
            console.print()
            return

        if not self.market_data or not self.truth_data:
            console.print("  [red]No market data. Run /generate first.[/red]")
            return

        buyer_id = arg if arg else self.active_buyer_id
        if not buyer_id:
            console.print("  [yellow]No buyer selected. Use /truth <buyer_id>[/yellow]")
            return

        try:
            top = rank_properties_for_buyer(buyer_id, self.market_data, self.truth_data, top_k=5)
        except ValueError as e:
            console.print(f"  [red]{e}[/red]")
            return

        console.print(f"\n  [magenta bold][TRUTH][/magenta bold] Ground truth top 5 for {buyer_id}:")
        for i, match in enumerate(top, 1):
            prop = next(p for p in self.market_data["properties"] if p["id"] == match["property_id"])
            bd = match["breakdown"]
            console.print(
                f"    {i}. {match['property_id']}  "
                f"[bold]{match['score']:.3f}[/bold]  "
                f"{prop['area']} {prop['bhk']}BHK {_format_price(prop['price'])}  "
                f"{prop['facing']}  {prop['possession_status']}"
            )
            console.print(
                f"       budget:{bd['budget']:.2f} area:{bd['area']:.2f} bhk:{bd['bhk']:.2f} "
                f"env:{bd['environment']:.2f} bld:{bd['builder_risk']:.2f} "
                f"vastu:{bd['vastu']:.2f} pos:{bd['possession']:.2f} "
                f"doc:{bd['doc_safety']:.2f}"
            )
        console.print()

        if self.graph_store:
            self.graph_store.log_event("truth_inspection", {
                "buyer_id": buyer_id,
                "top_matches": [{"property_id": m["property_id"], "score": m["score"]} for m in top],
            })

    def _cmd_research(self, arg: str):
        """
        /research reddit [subreddit] [limit]   — fetch one subreddit
        /research reddit slow                  — slow crawl (new + hot, from cache)
        /research taxonomy                     — show accumulated taxonomy
        """
        from research.reddit_sentiment_researcher import analyze_posts, save_report, update_taxonomy

        parts = arg.split() if arg else []
        if not parts:
            console.print(
                "  [yellow]Usage:[/yellow]\n"
                "    /research reddit [subreddit] [limit]  — fetch a subreddit\n"
                "    /research reddit slow                 — crawl new + hot\n"
                "    /research taxonomy                    — show accumulated knowledge\n"
            )
            return

        # --- /research taxonomy ---
        if parts[0].lower() == "taxonomy":
            self._show_taxonomy()
            return

        # --- /research compact ---
        if parts[0].lower() == "compact":
            from research.reddit_sentiment_researcher import compact_taxonomy
            console.print("  [dim]Compacting taxonomy...[/dim]")
            stats = compact_taxonomy(verbose=False)
            if stats.get("status") == "skipped":
                console.print(f"  [yellow]Skipped: {stats['reason']}[/yellow]")
            else:
                console.print(f"  [green]Compaction complete[/green]")
                console.print(f"    Terms pruned:      {stats.get('terms_pruned', 0)}")
                console.print(f"    Price obs trimmed: {stats.get('price_observations_trimmed', 0)}")
                console.print(f"    Reports merged:    {stats.get('reports_merged', 0)}")
                if stats.get("snapshot"):
                    console.print(f"    Snapshot saved:    {stats['snapshot'].split('/')[-1]}")
                if stats.get("merged_report"):
                    console.print(f"    Archive:           {stats['merged_report'].split('/')[-1]}")
            console.print()
            return

        if parts[0].lower() != "reddit":
            console.print("  [yellow]Unknown research type. Try: /research reddit[/yellow]")
            return

        # --- /research reddit slow ---
        if len(parts) > 1 and parts[1].lower() == "slow":
            from research.reddit_client import slow_crawl
            console.print("  [dim]Slow crawl: r/BangaloreRealEstates (new + hot)...[/dim]")
            try:
                posts = slow_crawl(
                    subreddits=["BangaloreRealEstates"],
                    limit_per_subreddit=25,
                    sorts=["new", "hot"],
                    cache_max_age=3600,
                )
            except Exception as e:
                console.print(f"  [red]Crawl failed: {e}[/red]")
                return
            subreddit_name = "BangaloreRealEstates"

        # --- /research reddit [subreddit] [limit] ---
        else:
            from research.reddit_client import fetch_posts
            subreddit = parts[1] if len(parts) > 1 else "BangaloreRealEstates"
            subreddit_name = subreddit.lstrip("r/") if subreddit.startswith("r/") else subreddit
            limit = int(parts[2]) if len(parts) > 2 and parts[2].isdigit() else 25
            console.print(f"  [dim]Fetching {limit} posts from r/{subreddit_name}...[/dim]")
            try:
                posts = fetch_posts(subreddit_name, limit=limit, use_cache=True, cache_max_age=3600)
            except Exception as e:
                console.print(f"  [red]Reddit fetch failed: {e}[/red]")
                return

        if not posts:
            console.print("  [yellow]No posts fetched.[/yellow]")
            return

        report = analyze_posts(posts, subreddit_name)
        report_path = save_report(report)
        taxonomy_path = update_taxonomy(report)

        console.print(f"  [green]Analyzed {len(posts)} posts from r/{subreddit_name}[/green]")

        if report.get("top_terms"):
            console.print(f"\n  [bold]Top terms discovered:[/bold]")
            console.print(f"    {', '.join(report['top_terms'][:10])}")

        if report.get("phrases"):
            console.print(f"\n  [bold]Recurring phrases:[/bold]")
            for p in report["phrases"][:5]:
                console.print(f"    \"{p}\"")

        if report.get("price_signals"):
            console.print(f"\n  [bold]Price signals by area:[/bold]")
            for area, data in sorted(report["price_signals"].items()):
                parts = []
                if data.get("median_price"):
                    parts.append(data["median_price"])
                    if data.get("price_range"):
                        parts.append(f"({data['price_range'][0]}–{data['price_range'][1]})")
                if data.get("median_sqft_rate"):
                    parts.append(data["median_sqft_rate"])
                console.print(f"    {area:22s}  {' '.join(parts)}")

        if report.get("decision_drivers"):
            console.print(f"\n  [bold]Decision drivers:[/bold]")
            for d in report["decision_drivers"][:5]:
                console.print(f"    {d}")

        console.print(f"\n  Report: {report_path}")
        console.print(f"  Taxonomy updated ({taxonomy_path.split('/')[-1]})\n")

        if self.graph_store:
            self.graph_store.log_event("research_run", {
                "subreddit": subreddit_name,
                "post_count": len(posts),
                "report_path": report_path,
            })

    def _show_taxonomy(self):
        """Print accumulated taxonomy state."""
        taxonomy_path = os.path.join(PROJECT_ROOT, "data", "reddit", "taxonomy.json")
        if not os.path.exists(taxonomy_path):
            console.print("  [dim]No taxonomy yet. Run /research reddit first.[/dim]")
            return

        with open(taxonomy_path) as f:
            t = json.load(f)

        console.print(f"\n  [bold]Accumulated Reddit Taxonomy[/bold]")
        console.print(f"  Runs: {t.get('run_count', 0)}  |  Subreddits: {', '.join(t.get('subreddits_crawled', []))}")
        console.print(f"  Unique terms tracked: {len(t.get('term_counts', {}))}")

        terms = list(t.get("term_counts", {}).items())[:15]
        if terms:
            console.print(f"\n  [bold]Top terms (cumulative frequency):[/bold]")
            for term, count in terms:
                bar = "█" * min(count, 20)
                console.print(f"    {term:18s} {bar} {count}")

        phrases = list(t.get("phrase_counts", {}).items())[:8]
        if phrases:
            console.print(f"\n  [bold]Top phrases:[/bold]")
            for phrase, count in phrases:
                console.print(f"    \"{phrase}\" ({count}x)")

        # Price signals by area
        area_prices = t.get("area_prices", {})
        area_sqft = t.get("area_sqft_rates", {})
        if area_prices or area_sqft:
            console.print(f"\n  [bold]Price signals by area (from Reddit):[/bold]")
            all_areas = sorted(set(area_prices) | set(area_sqft))
            for area in all_areas:
                parts = []
                prices = area_prices.get(area, [])
                rates = area_sqft.get(area, [])
                if prices:
                    med = sorted(prices)[len(prices)//2]
                    fmt = f"{med/1_00_00_000:.2f}Cr" if med >= 1_00_00_000 else f"{med/1_00_000:.0f}L"
                    parts.append(f"~{fmt} median ({len(prices)} mentions)")
                if rates:
                    med_r = sorted(rates)[len(rates)//2]
                    parts.append(f"~₹{med_r:,}/sqft ({len(rates)} mentions)")
                console.print(f"    {area:22s}  {' | '.join(parts)}")

        drivers = t.get("decision_drivers", [])
        if drivers:
            console.print(f"\n  [bold]Discovered decision drivers ({len(drivers)}):[/bold]")
            for d in drivers:
                console.print(f"    {d}")
        console.print()

    def _cmd_clear(self, arg: str):
        self.conversation_turns = []
        console.print("  [dim]Conversation cleared.[/dim]")

    # --- Main loop ---

    def run(self):
        """Main REPL loop."""
        self.setup()

        console.print(Panel.fit(
            "[bold]OpenEstates[/bold]\n"
            "Tell me what kind of property you're looking for.\n"
            "Type [bold]/help[/bold] for commands.",
            border_style="blue",
        ))
        console.print()

        # Auto-setup
        self._ensure_market()
        self._ensure_buyer()

        commands = {
            "/help": self._cmd_help,
            "/find": self._cmd_find,
            "/context": self._cmd_context,
            "/buyer": self._cmd_buyer,
            "/generate": self._cmd_generate,
            "/truth": self._cmd_truth,
            "/research": self._cmd_research,
            "/clear": self._cmd_clear,
        }

        while True:
            try:
                text = console.input("[bold blue]>[/bold blue] ").strip()
            except (EOFError, KeyboardInterrupt):
                console.print("\n  [dim]Goodbye![/dim]")
                break

            if not text:
                continue

            if text.lower() in ("/quit", "/exit", "/q"):
                console.print("  [dim]Goodbye![/dim]")
                break

            if text.startswith("/"):
                parts = text.split(maxsplit=1)
                cmd = parts[0].lower()
                arg = parts[1].strip() if len(parts) > 1 else ""
                handler = commands.get(cmd)
                if handler:
                    handler(arg)
                else:
                    console.print(f"  [red]Unknown command: {cmd}[/red]  Type /help")
                continue

            # Conversational input
            self._ensure_buyer()
            self.conversation_turns.append(text)

            # Auto-extract signals after each message
            self._extract_and_respond()

            # Show matches if we have enough context
            signals = {}
            if self.context_graph:
                snapshot = self.context_graph.snapshot()
                signals = snapshot.get("signals", {})
            if len(self.conversation_turns) >= 2 or len(signals) >= 2:
                console.print("  [dim]Here are your current best matches:[/dim]")
                self._show_matches(top_k=3)
