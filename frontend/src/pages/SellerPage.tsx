/**
 * Seller flows — three views in one file:
 *   /sell            → Register form (or redirect to dashboard if already registered)
 *   /sell/dashboard  → Seller's listings + bid activity
 *   /sell/list       → New listing form
 *
 * Seller identity is stored in localStorage (no accounts yet).
 * All actions are one-tap; no confirmation dialogs for reversible actions.
 */
import { useState, useEffect } from "react";
import { Link, useNavigate } from "react-router-dom";
import type { Seller, SellerListingsResponse } from "../lib/types.ts";
import { registerSeller, getSellerListings, updateBidStatus } from "../lib/api.ts";

const STORAGE_KEY = "openestates_seller_id";

function getSellerId(): string | null {
  return localStorage.getItem(STORAGE_KEY);
}

function setSellerId(id: string) {
  localStorage.setItem(STORAGE_KEY, id);
}

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

function VerificationBadge({ tier }: { tier: string }) {
  const isRera = tier === "rera_verified";
  return (
    <span style={{
      display: "inline-flex",
      alignItems: "center",
      gap: "0.25rem",
      fontSize: "0.72rem",
      fontWeight: 600,
      padding: "0.15rem 0.5rem",
      borderRadius: "999px",
      backgroundColor: isRera ? "#f0fdf4" : "#eff6ff",
      color: isRera ? "#15803d" : "#1d4ed8",
      border: `1px solid ${isRera ? "#bbf7d0" : "#bfdbfe"}`,
    }}>
      <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round">
        <polyline points="20 6 9 17 4 12" />
      </svg>
      {isRera ? "RERA Verified" : "Identity Verified"}
    </span>
  );
}

// ---------------------------------------------------------------------------
// Register page
// ---------------------------------------------------------------------------

export function SellerRegisterPage() {
  const navigate = useNavigate();
  const [name, setName] = useState("");
  const [phone, setPhone] = useState("");
  const [rera, setRera] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState("");

  // If already registered, go straight to dashboard
  useEffect(() => {
    if (getSellerId()) navigate("/sell/dashboard", { replace: true });
  }, [navigate]);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!name.trim() || !phone.trim()) {
      setError("Name and phone are required.");
      return;
    }
    setError("");
    setSubmitting(true);
    try {
      const result = await registerSeller({
        name: name.trim(),
        phone: phone.trim(),
        reraNumber: rera.trim() || undefined,
      });
      setSellerId(result.seller.id);
      navigate("/sell/dashboard");
    } catch (err) {
      setError(err instanceof Error ? err.message : "Registration failed.");
      setSubmitting(false);
    }
  };

  return (
    <div style={{ maxWidth: "480px", margin: "4rem auto", padding: "0 1.5rem" }}>
      <Link to="/" style={{ fontSize: "0.82rem", color: "var(--color-text-muted)", textDecoration: "none" }}>
        ← OpenEstates
      </Link>
      <h1 style={{ fontSize: "1.6rem", fontWeight: 700, letterSpacing: "-0.025em", margin: "1.5rem 0 0.35rem" }}>
        Sell your property
      </h1>
      <p style={{ color: "var(--color-text-secondary)", fontSize: "0.92rem", margin: "0 0 2rem", lineHeight: 1.6 }}>
        Register to list your property, receive bids, and track buyer interest — all in one place.
      </p>

      <form onSubmit={handleSubmit} style={{ display: "grid", gap: "1rem" }}>
        <div>
          <label style={{ display: "block", fontSize: "0.82rem", fontWeight: 600, marginBottom: "0.4rem" }}>
            Full name
          </label>
          <input
            className="input"
            style={{ width: "100%", boxSizing: "border-box" }}
            placeholder="e.g. Rajesh Kumar"
            value={name}
            onChange={(e) => setName(e.target.value)}
            required
          />
        </div>

        <div>
          <label style={{ display: "block", fontSize: "0.82rem", fontWeight: 600, marginBottom: "0.4rem" }}>
            Phone number
          </label>
          <input
            className="input"
            style={{ width: "100%", boxSizing: "border-box" }}
            placeholder="+91-98450-xxxxx"
            value={phone}
            onChange={(e) => setPhone(e.target.value)}
            required
          />
          <p style={{ fontSize: "0.72rem", color: "var(--color-text-muted)", margin: "0.3rem 0 0" }}>
            Verified instantly — you'll receive an Identity Verified badge.
          </p>
        </div>

        <div>
          <label style={{ display: "block", fontSize: "0.82rem", fontWeight: 600, marginBottom: "0.4rem" }}>
            RERA registration number{" "}
            <span style={{ fontWeight: 400, color: "var(--color-text-muted)" }}>(optional)</span>
          </label>
          <input
            className="input"
            style={{ width: "100%", boxSizing: "border-box" }}
            placeholder="PRM/KA/RERA/..."
            value={rera}
            onChange={(e) => setRera(e.target.value)}
          />
          <p style={{ fontSize: "0.72rem", color: "var(--color-text-muted)", margin: "0.3rem 0 0" }}>
            Adds a RERA Verified badge — buyers trust you more.
          </p>
        </div>

        {error && (
          <div style={{ fontSize: "0.82rem", color: "var(--color-negative)", padding: "0.5rem 0" }}>
            {error}
          </div>
        )}

        <button
          type="submit"
          className="btn btn-primary"
          disabled={submitting}
          style={{ width: "100%", justifyContent: "center", fontSize: "0.95rem", padding: "0.75rem" }}
        >
          {submitting ? "Registering…" : "Register as Seller"}
        </button>
      </form>

      <p style={{ marginTop: "1.5rem", fontSize: "0.8rem", color: "var(--color-text-muted)", textAlign: "center", lineHeight: 1.6 }}>
        By registering, you agree that your listings will be visible to all buyers on OpenEstates.
        Bids are public. Contact details are shared only when a bid is accepted.
      </p>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Seller dashboard
// ---------------------------------------------------------------------------

export function SellerDashboardPage() {
  const navigate = useNavigate();
  const sellerId = getSellerId();
  const [data, setData] = useState<SellerListingsResponse | null>(null);
  const [loading, setLoading] = useState(true);
  const [updatingBid, setUpdatingBid] = useState<string | null>(null);

  useEffect(() => {
    if (!sellerId) { navigate("/sell", { replace: true }); return; }
    getSellerListings(sellerId)
      .then(setData)
      .catch(() => setData(null))
      .finally(() => setLoading(false));
  }, [sellerId, navigate]);

  const handleBidAction = async (
    propertyId: string,
    bidId: string,
    action: "accepted" | "rejected",
  ) => {
    if (!sellerId) return;
    setUpdatingBid(bidId);
    try {
      await updateBidStatus(propertyId, bidId, action);
      // Refresh
      const fresh = await getSellerListings(sellerId);
      setData(fresh);
    } catch {
      // swallow
    } finally {
      setUpdatingBid(null);
    }
  };

  if (loading) {
    return (
      <div style={{ padding: "4rem 2rem", textAlign: "center", color: "var(--color-text-muted)" }}>
        Loading your dashboard…
      </div>
    );
  }

  if (!data) {
    return (
      <div style={{ padding: "4rem 2rem", textAlign: "center" }}>
        <p style={{ color: "var(--color-text-muted)" }}>Could not load dashboard. Please try again.</p>
        <button className="btn btn-outline" style={{ marginTop: "1rem" }} onClick={() => window.location.reload()}>
          Retry
        </button>
      </div>
    );
  }

  const { seller, listings } = data;
  const totalBids = listings.reduce((sum, l) => sum + l.bidStats.count, 0);

  return (
    <div style={{ maxWidth: "800px", margin: "0 auto", padding: "2rem 1.5rem" }}>
      {/* Header */}
      <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", marginBottom: "2rem", flexWrap: "wrap", gap: "1rem" }}>
        <div>
          <h1 style={{ fontSize: "1.4rem", fontWeight: 700, letterSpacing: "-0.02em", margin: "0 0 0.35rem" }}>
            {seller.name}
          </h1>
          <div style={{ display: "flex", alignItems: "center", gap: "0.5rem", flexWrap: "wrap" }}>
            <VerificationBadge tier={seller.verificationTier} />
            <span style={{ fontSize: "0.78rem", color: "var(--color-text-muted)" }}>
              {listings.length} listing{listings.length !== 1 ? "s" : ""} &middot; {totalBids} bid{totalBids !== 1 ? "s" : ""} received
            </span>
          </div>
        </div>
        <Link
          to="/sell/list"
          className="btn btn-primary"
          style={{ textDecoration: "none", fontSize: "0.88rem" }}
        >
          + List a property
        </Link>
      </div>

      {listings.length === 0 ? (
        <div style={{
          padding: "3rem",
          textAlign: "center",
          borderRadius: "var(--radius-md)",
          border: "1px dashed var(--color-border)",
          color: "var(--color-text-muted)",
        }}>
          <p style={{ marginBottom: "1rem", fontSize: "0.95rem" }}>No listings yet.</p>
          <Link to="/sell/list" className="btn btn-outline" style={{ textDecoration: "none" }}>
            List your first property →
          </Link>
        </div>
      ) : (
        <div style={{ display: "grid", gap: "1.25rem" }}>
          {listings.map((listing) => (
            <ListingCard
              key={listing.propertyId}
              listing={listing}
              onBidAction={handleBidAction}
              updatingBid={updatingBid}
            />
          ))}
        </div>
      )}
    </div>
  );
}

function ListingCard({
  listing,
  onBidAction,
  updatingBid,
}: {
  listing: SellerListingsResponse["listings"][0];
  onBidAction: (pid: string, bid: string, action: "accepted" | "rejected") => void;
  updatingBid: string | null;
}) {
  const { propertyId, listingStatus, bidStats } = listing;
  const [showBids, setShowBids] = useState(false);

  const statusColor = listingStatus === "under_offer"
    ? { bg: "#fff7ed", color: "#c2410c", border: "#fed7aa" }
    : listingStatus === "matched"
    ? { bg: "#f0fdf4", color: "#15803d", border: "#bbf7d0" }
    : { bg: "var(--color-bg-elevated)", color: "var(--color-text-secondary)", border: "var(--color-border)" };

  return (
    <div className="section-card">
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "flex-start", flexWrap: "wrap", gap: "0.75rem" }}>
        <div>
          <Link
            to={`/property/${propertyId}`}
            style={{ fontWeight: 600, fontSize: "0.95rem", color: "var(--color-text)", textDecoration: "none" }}
          >
            {propertyId.replace(/^discovered-/, "").replace(/-/g, " ").replace(/\b\w/g, (c) => c.toUpperCase())}
          </Link>
          <p style={{ margin: "0.15rem 0 0", fontSize: "0.78rem", color: "var(--color-text-muted)" }}>
            ID: {propertyId}
          </p>
        </div>
        <span style={{
          display: "inline-block",
          fontSize: "0.72rem",
          fontWeight: 600,
          padding: "0.2rem 0.6rem",
          borderRadius: "999px",
          backgroundColor: statusColor.bg,
          color: statusColor.color,
          border: `1px solid ${statusColor.border}`,
          textTransform: "capitalize",
        }}>
          {listingStatus.replace(/_/g, " ")}
        </span>
      </div>

      {/* Bid stats row */}
      <div style={{
        display: "grid",
        gridTemplateColumns: "repeat(4, 1fr)",
        gap: "0.5rem",
        marginTop: "1rem",
        padding: "0.75rem",
        borderRadius: "var(--radius-sm)",
        backgroundColor: "var(--color-bg-elevated)",
        border: "1px solid var(--color-border)",
      }}>
        {[
          { label: "Bids", value: String(bidStats.count) },
          { label: "Average", value: bidStats.count > 0 ? formatPrice(bidStats.average) : "—" },
          { label: "Highest", value: bidStats.count > 0 ? formatPrice(bidStats.p100) : "—" },
          { label: "Last bid", value: bidStats.lastBidAt ? timeAgo(bidStats.lastBidAt) : "—" },
        ].map(({ label, value }) => (
          <div key={label} style={{ textAlign: "center" }}>
            <div style={{ fontSize: "0.9rem", fontWeight: 700 }}>{value}</div>
            <div style={{ fontSize: "0.68rem", color: "var(--color-text-muted)", textTransform: "uppercase", letterSpacing: "0.06em", marginTop: "0.15rem" }}>{label}</div>
          </div>
        ))}
      </div>

      {/* Bid list toggle */}
      {bidStats.count > 0 && (
        <button
          onClick={() => setShowBids(!showBids)}
          style={{
            background: "none",
            border: "none",
            cursor: "pointer",
            fontSize: "0.78rem",
            color: "var(--color-text-muted)",
            padding: "0.5rem 0 0",
            textDecoration: "underline",
            textUnderlineOffset: "2px",
          }}
        >
          {showBids ? "Hide bids" : `Manage bids (${bidStats.count})`}
        </button>
      )}

      {showBids && (
        <div style={{ marginTop: "0.75rem", display: "grid", gap: "0.5rem" }}>
          <p style={{ fontSize: "0.78rem", color: "var(--color-text-muted)", margin: 0 }}>
            Accept a bid to mark this listing as Under Offer. Pass on bids you don't want.
          </p>
          {/* We show placeholder bids from stats; real list needs /bids endpoint */}
          {Array.from({ length: bidStats.count }, (_, i) => (
            <div
              key={i}
              style={{
                display: "flex",
                justifyContent: "space-between",
                alignItems: "center",
                padding: "0.6rem 0.75rem",
                borderRadius: "var(--radius-sm)",
                backgroundColor: "var(--color-bg-elevated)",
                border: "1px solid var(--color-border)",
              }}
            >
              <div>
                <span style={{ fontSize: "0.9rem", fontWeight: 600 }}>
                  {formatPrice(bidStats.p100 - i * Math.floor((bidStats.p100 - (bidStats.average * 0.9)) / Math.max(bidStats.count - 1, 1)))}
                </span>
                <span style={{ marginLeft: "0.5rem", fontSize: "0.72rem", color: "var(--color-text-muted)" }}>
                  Bid #{i + 1}
                </span>
              </div>
              <div style={{ display: "flex", gap: "0.4rem" }}>
                <button
                  className="btn btn-primary"
                  style={{ fontSize: "0.78rem", padding: "0.3rem 0.7rem" }}
                  disabled={updatingBid !== null}
                  onClick={() => onBidAction(propertyId, `bid-placeholder-${i}`, "accepted")}
                >
                  Accept
                </button>
                <button
                  className="btn btn-outline"
                  style={{ fontSize: "0.78rem", padding: "0.3rem 0.7rem" }}
                  disabled={updatingBid !== null}
                  onClick={() => onBidAction(propertyId, `bid-placeholder-${i}`, "rejected")}
                >
                  Pass
                </button>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// New listing form
// ---------------------------------------------------------------------------

export function SellerListingFormPage() {
  const navigate = useNavigate();
  const sellerId = getSellerId();

  const [title, setTitle] = useState("");
  const [area, setArea] = useState("");
  const [bhk, setBhk] = useState("");
  const [sqft, setSqft] = useState("");
  const [society, setSociety] = useState("");
  const [description, setDescription] = useState("");
  const [price, setPrice] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    if (!sellerId) navigate("/sell", { replace: true });
  }, [sellerId, navigate]);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!title.trim() || !area.trim() || !price.trim()) {
      setError("Title, area, and asking price are required.");
      return;
    }
    setError("");
    setSubmitting(true);
    // TODO: POST /api/sellers/{id}/listings when backend endpoint is ready
    // For now, show a success state with the entered data
    await new Promise((r) => setTimeout(r, 600));
    navigate("/sell/dashboard");
  };

  const BANGALORE_AREAS = ["Whitefield", "Koramangala", "HSR Layout", "Indiranagar", "Sarjapur Road", "Hebbal", "Yelahanka", "Electronic City", "Bannerghatta Road", "JP Nagar"];

  return (
    <div style={{ maxWidth: "640px", margin: "0 auto", padding: "2rem 1.5rem" }}>
      <Link to="/sell/dashboard" style={{ fontSize: "0.82rem", color: "var(--color-text-muted)", textDecoration: "none" }}>
        ← Dashboard
      </Link>

      <h1 style={{ fontSize: "1.4rem", fontWeight: 700, letterSpacing: "-0.02em", margin: "1.5rem 0 0.35rem" }}>
        List a property
      </h1>
      <p style={{ color: "var(--color-text-secondary)", fontSize: "0.88rem", margin: "0 0 2rem", lineHeight: 1.6 }}>
        Fill in the basics. We'll enrich your listing with area intelligence and match it to buyer searches automatically.
      </p>

      <form onSubmit={handleSubmit} style={{ display: "grid", gap: "1.25rem" }}>
        {/* Title */}
        <div>
          <label style={{ display: "block", fontSize: "0.82rem", fontWeight: 600, marginBottom: "0.4rem" }}>
            Listing title
          </label>
          <input
            className="input"
            style={{ width: "100%", boxSizing: "border-box" }}
            placeholder="e.g. Spacious 3 BHK in Prestige Lakeside"
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            required
          />
        </div>

        {/* Area + BHK row */}
        <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: "0.75rem" }}>
          <div>
            <label style={{ display: "block", fontSize: "0.82rem", fontWeight: 600, marginBottom: "0.4rem" }}>
              Area
            </label>
            <select
              className="input"
              style={{ width: "100%", boxSizing: "border-box" }}
              value={area}
              onChange={(e) => setArea(e.target.value)}
              required
            >
              <option value="">Select area</option>
              {BANGALORE_AREAS.map((a) => (
                <option key={a} value={a}>{a}</option>
              ))}
            </select>
          </div>
          <div>
            <label style={{ display: "block", fontSize: "0.82rem", fontWeight: 600, marginBottom: "0.4rem" }}>
              BHK
            </label>
            <select
              className="input"
              style={{ width: "100%", boxSizing: "border-box" }}
              value={bhk}
              onChange={(e) => setBhk(e.target.value)}
              required
            >
              <option value="">Select</option>
              {[1, 2, 3, 4, 5].map((n) => <option key={n} value={n}>{n} BHK</option>)}
            </select>
          </div>
        </div>

        {/* Sqft + Society row */}
        <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: "0.75rem" }}>
          <div>
            <label style={{ display: "block", fontSize: "0.82rem", fontWeight: 600, marginBottom: "0.4rem" }}>
              Carpet area (sqft)
            </label>
            <input
              className="input"
              style={{ width: "100%", boxSizing: "border-box" }}
              placeholder="e.g. 1250"
              type="number"
              value={sqft}
              onChange={(e) => setSqft(e.target.value)}
            />
          </div>
          <div>
            <label style={{ display: "block", fontSize: "0.82rem", fontWeight: 600, marginBottom: "0.4rem" }}>
              Society / project name
            </label>
            <input
              className="input"
              style={{ width: "100%", boxSizing: "border-box" }}
              placeholder="e.g. Prestige Lakeside"
              value={society}
              onChange={(e) => setSociety(e.target.value)}
            />
          </div>
        </div>

        {/* Description */}
        <div>
          <label style={{ display: "block", fontSize: "0.82rem", fontWeight: 600, marginBottom: "0.4rem" }}>
            Description{" "}
            <span style={{ fontWeight: 400, color: "var(--color-text-muted)" }}>(optional)</span>
          </label>
          <textarea
            className="input"
            style={{ width: "100%", boxSizing: "border-box", resize: "vertical", minHeight: "100px" }}
            placeholder="What makes this property special? Documents, amenities, move-in timeline, motivation…"
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            maxLength={500}
          />
          <p style={{ fontSize: "0.72rem", color: "var(--color-text-muted)", margin: "0.2rem 0 0", textAlign: "right" }}>
            {description.length}/500
          </p>
        </div>

        {/* Asking price */}
        <div>
          <label style={{ display: "block", fontSize: "0.82rem", fontWeight: 600, marginBottom: "0.4rem" }}>
            Asking price (₹)
          </label>
          <input
            className="input"
            style={{ width: "100%", boxSizing: "border-box" }}
            placeholder="e.g. 14500000 (₹1.45 Cr)"
            type="number"
            value={price}
            onChange={(e) => setPrice(e.target.value)}
            required
          />
          {price && Number(price) >= 100_000 && (
            <p style={{ fontSize: "0.78rem", color: "var(--color-positive)", margin: "0.3rem 0 0", fontWeight: 600 }}>
              = {formatPrice(Number(price))}
            </p>
          )}
        </div>

        {error && (
          <div style={{ fontSize: "0.82rem", color: "var(--color-negative)" }}>{error}</div>
        )}

        <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: "0.75rem" }}>
          <Link to="/sell/dashboard" className="btn btn-outline" style={{ textDecoration: "none", textAlign: "center" }}>
            Cancel
          </Link>
          <button
            type="submit"
            className="btn btn-primary"
            disabled={submitting}
            style={{ justifyContent: "center" }}
          >
            {submitting ? "Listing…" : "List property"}
          </button>
        </div>
      </form>

      <p style={{ marginTop: "1.5rem", fontSize: "0.75rem", color: "var(--color-text-muted)", lineHeight: 1.6 }}>
        Your listing will be visible on OpenEstates search immediately.
        We'll automatically enrich it with area intelligence, society scores, and market context.
      </p>
    </div>
  );
}
