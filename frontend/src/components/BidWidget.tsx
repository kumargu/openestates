import { useState, useEffect } from "react";
import type { Bid, BidStats } from "../lib/types.ts";
import { getBidStats, getPropertyBids, placeBid } from "../lib/api.ts";

function formatPrice(price: number): string {
  if (price >= 10_000_000) return `₹${(price / 10_000_000).toFixed(2)} Cr`;
  if (price >= 100_000) return `₹${(price / 100_000).toFixed(1)} L`;
  return `₹${price.toLocaleString("en-IN")}`;
}

function timeAgo(iso: string): string {
  const diff = Date.now() - new Date(iso).getTime();
  const h = Math.floor(diff / 3_600_000);
  const d = Math.floor(h / 24);
  if (d > 0) return `${d}d ago`;
  if (h > 0) return `${h}h ago`;
  return "just now";
}

function BidStatBar({ label, value, sub }: { label: string; value: string; sub?: string }) {
  return (
    <div style={{ textAlign: "center" }}>
      <div style={{ fontSize: "1rem", fontWeight: 700, color: "var(--color-text)" }}>{value}</div>
      {sub && <div style={{ fontSize: "0.7rem", color: "var(--color-text-muted)", marginTop: "0.1rem" }}>{sub}</div>}
      <div style={{ fontSize: "0.68rem", textTransform: "uppercase", letterSpacing: "0.07em", color: "var(--color-text-muted)", marginTop: "0.2rem" }}>{label}</div>
    </div>
  );
}

type BidWidgetProps = {
  propertyId: string;
  askingPrice: number;
};

type FormState = "idle" | "submitting" | "success" | "error";

export function BidWidget({ propertyId, askingPrice }: BidWidgetProps) {
  const [stats, setStats] = useState<BidStats | null>(null);
  const [bids, setBids] = useState<Bid[]>([]);
  const [showForm, setShowForm] = useState(false);
  const [showBids, setShowBids] = useState(false);

  const [name, setName] = useState("");
  const [phone, setPhone] = useState("");
  const [amount, setAmount] = useState("");
  const [formState, setFormState] = useState<FormState>("idle");
  const [formError, setFormError] = useState("");

  useEffect(() => {
    getBidStats(propertyId).then(setStats).catch(() => {});
  }, [propertyId]);

  const loadBids = () => {
    getPropertyBids(propertyId)
      .then((r) => setBids(r.bids))
      .catch(() => {});
    setShowBids(true);
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    const amountNum = parseInt(amount.replace(/[^0-9]/g, ""), 10);
    if (!name.trim() || !phone.trim() || !amountNum) {
      setFormError("Please fill all fields.");
      return;
    }
    if (amountNum < 100_000) {
      setFormError("Minimum bid is ₹1 L.");
      return;
    }
    setFormError("");
    setFormState("submitting");
    try {
      const result = await placeBid(propertyId, {
        bidderName: name.trim(),
        bidderPhone: phone.trim(),
        amount: amountNum,
      });
      setStats(result.bidStats);
      setFormState("success");
      setShowForm(false);
      // Append new bid to visible list
      setBids((prev) => [
        ...prev,
        { id: result.bid.id, amount: result.bid.amount, createdAt: result.bid.createdAt, status: "pending" },
      ]);
      if (!showBids) setShowBids(true);
    } catch {
      setFormState("error");
      setFormError("Failed to place bid. Please try again.");
    }
  };

  const hasBids = stats && stats.count > 0;

  return (
    <div className="section-card" style={{ marginTop: "1rem" }}>
      {/* Header */}
      <div style={{
        fontSize: "0.62rem",
        fontWeight: 600,
        textTransform: "uppercase",
        letterSpacing: "0.08em",
        color: "var(--color-text-muted)",
        marginBottom: "0.75rem",
        display: "flex",
        alignItems: "center",
        gap: "0.4rem",
      }}>
        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round">
          <line x1="12" y1="1" x2="12" y2="23" />
          <path d="M17 5H9.5a3.5 3.5 0 0 0 0 7h5a3.5 3.5 0 0 1 0 7H6" />
        </svg>
        Bids &amp; Market Price
      </div>

      {/* Asking price */}
      <div style={{ marginBottom: "0.75rem" }}>
        <span style={{ fontSize: "0.78rem", color: "var(--color-text-muted)" }}>Asking price</span>
        <div style={{ fontSize: "1.35rem", fontWeight: 700, letterSpacing: "-0.02em" }}>
          {formatPrice(askingPrice)}
        </div>
      </div>

      {/* Bid stats — 3-column grid */}
      {hasBids ? (
        <div style={{
          display: "grid",
          gridTemplateColumns: "1fr 1fr 1fr",
          gap: "0.5rem",
          padding: "0.75rem",
          borderRadius: "var(--radius-sm)",
          backgroundColor: "var(--color-bg-elevated)",
          border: "1px solid var(--color-border)",
          marginBottom: "0.75rem",
        }}>
          <BidStatBar label="Bids" value={String(stats.count)} />
          <BidStatBar
            label="Avg bid"
            value={formatPrice(stats.average)}
            sub={stats.average < askingPrice
              ? `${Math.round((1 - stats.average / askingPrice) * 100)}% below ask`
              : undefined}
          />
          <BidStatBar label="Highest" value={formatPrice(stats.p100)} />
        </div>
      ) : (
        <div style={{
          padding: "0.75rem",
          borderRadius: "var(--radius-sm)",
          backgroundColor: "var(--color-bg-elevated)",
          border: "1px solid var(--color-border)",
          marginBottom: "0.75rem",
          textAlign: "center",
          fontSize: "0.82rem",
          color: "var(--color-text-muted)",
        }}>
          No bids yet — be the first to set the price
        </div>
      )}

      {/* Last bid timestamp */}
      {stats?.lastBidAt && (
        <div style={{ fontSize: "0.72rem", color: "var(--color-text-muted)", marginBottom: "0.75rem" }}>
          Last bid {timeAgo(stats.lastBidAt)}
        </div>
      )}

      {/* Place bid CTA */}
      {formState === "success" ? (
        <div style={{
          padding: "0.75rem",
          borderRadius: "var(--radius-sm)",
          backgroundColor: "var(--color-positive-bg)",
          border: "1px solid var(--color-positive-border)",
          fontSize: "0.85rem",
          color: "var(--color-positive)",
          fontWeight: 600,
          marginBottom: "0.5rem",
        }}>
          ✓ Bid placed — seller will be notified
        </div>
      ) : showForm ? (
        <form onSubmit={handleSubmit} style={{ display: "grid", gap: "0.5rem", marginBottom: "0.5rem" }}>
          <input
            className="input"
            placeholder="Your name"
            value={name}
            onChange={(e) => setName(e.target.value)}
            required
          />
          <input
            className="input"
            placeholder="Phone number"
            value={phone}
            onChange={(e) => setPhone(e.target.value)}
            required
          />
          <input
            className="input"
            placeholder={`Bid amount (e.g. ${Math.round(askingPrice * 0.95 / 100000) * 100000})`}
            value={amount}
            onChange={(e) => setAmount(e.target.value)}
            required
          />
          {formError && (
            <div style={{ fontSize: "0.75rem", color: "var(--color-negative)" }}>{formError}</div>
          )}
          <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: "0.5rem" }}>
            <button
              type="button"
              className="btn btn-outline"
              onClick={() => { setShowForm(false); setFormError(""); }}
              style={{ fontSize: "0.82rem" }}
            >
              Cancel
            </button>
            <button
              type="submit"
              className="btn btn-primary"
              disabled={formState === "submitting"}
              style={{ fontSize: "0.82rem" }}
            >
              {formState === "submitting" ? "Placing…" : "Submit bid"}
            </button>
          </div>
        </form>
      ) : (
        <button
          onClick={() => { setShowForm(true); setFormState("idle"); }}
          className="btn btn-primary"
          style={{ width: "100%", justifyContent: "center", fontSize: "0.9rem", marginBottom: "0.5rem" }}
        >
          Place a bid
        </button>
      )}

      {/* Public bid list toggle */}
      {hasBids && (
        <button
          onClick={showBids ? () => setShowBids(false) : loadBids}
          style={{
            background: "none",
            border: "none",
            cursor: "pointer",
            fontSize: "0.75rem",
            color: "var(--color-text-muted)",
            padding: 0,
            textDecoration: "underline",
            textUnderlineOffset: "2px",
          }}
        >
          {showBids ? "Hide bids" : `View all ${stats.count} bid${stats.count > 1 ? "s" : ""}`}
        </button>
      )}

      {/* Bid list */}
      {showBids && bids.length > 0 && (
        <div style={{ marginTop: "0.75rem", display: "grid", gap: "0.35rem" }}>
          {[...bids]
            .filter((b) => b.status !== "rejected")
            .sort((a, b) => b.amount - a.amount)
            .map((bid) => (
              <div
                key={bid.id}
                style={{
                  display: "flex",
                  justifyContent: "space-between",
                  alignItems: "center",
                  padding: "0.45rem 0.65rem",
                  borderRadius: "var(--radius-xs, 4px)",
                  backgroundColor: "var(--color-bg-elevated)",
                  border: bid.status === "accepted"
                    ? "1px solid var(--color-positive-border)"
                    : "1px solid var(--color-border)",
                }}
              >
                <span style={{ fontSize: "0.82rem", fontWeight: 600 }}>
                  {formatPrice(bid.amount)}
                </span>
                <div style={{ display: "flex", alignItems: "center", gap: "0.5rem" }}>
                  {bid.status === "accepted" && (
                    <span style={{ fontSize: "0.68rem", color: "var(--color-positive)", fontWeight: 600 }}>
                      ACCEPTED
                    </span>
                  )}
                  <span style={{ fontSize: "0.72rem", color: "var(--color-text-muted)" }}>
                    {timeAgo(bid.createdAt)}
                  </span>
                </div>
              </div>
            ))}
        </div>
      )}
    </div>
  );
}
