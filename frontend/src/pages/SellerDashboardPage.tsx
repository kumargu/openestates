import { useEffect, useState } from "react";
import { useParams, Link } from "react-router-dom";
import { Helmet } from "react-helmet-async";
import type { SellerDashboard, PropertyInterestSummary, InterestTimelineEntry } from "../lib/types.ts";
import { getSellerDashboard } from "../lib/api.ts";
import { PageState } from "../components/PageState.tsx";

function formatPrice(price: number): string {
  if (price >= 10_000_000) return `${(price / 10_000_000).toFixed(1)} Cr`;
  if (price >= 100_000) return `${(price / 100_000).toFixed(1)} L`;
  return price.toLocaleString("en-IN");
}

/** Human-readable labels for completeness step keys. */
const STEP_LABELS: Record<string, string> = {
  basic_info: "Basic information",
  property_prompt: "Property description",
  details: "Property details",
  pricing: "Pricing information",
  documents: "Documents uploaded",
  photos: "Photos uploaded",
  society_info: "Society information",
};

/** Actionable recommendations for each incomplete step. */
const STEP_RECOMMENDATIONS: Record<string, string> = {
  basic_info: "Add your name and contact details so buyers can reach you.",
  property_prompt: "Write a short description to help buyers understand your property.",
  details: "Add property details (BHK, area, floor) to appear in filtered searches.",
  pricing: "Set your asking price — listings with pricing get 3x more interest.",
  documents: "Upload documents (sale deed, RERA) to increase your transparency score.",
  photos: "Add photos to rank higher in search results — listings with photos get 5x more views.",
  society_info: "Add society details to help buyers compare neighborhoods.",
};

function stepLabel(key: string): string {
  return STEP_LABELS[key] ?? key;
}

function stepRecommendation(key: string): string {
  return STEP_RECOMMENDATIONS[key] ?? "";
}

/** Inline SVG sparkline for interest timeline data. */
function Sparkline({ data }: { data: InterestTimelineEntry[] }) {
  if (data.length === 0) return null;

  const counts = data.map((d) => d.count);
  const maxVal = Math.max(...counts, 1);
  const width = 120;
  const height = 20;
  const padding = 1;

  // Single data point: render a dot instead of a polyline
  if (data.length === 1) {
    const cx = width / 2;
    const cy = height - padding - (counts[0] / maxVal) * (height - 2 * padding);
    return (
      <svg
        width={width}
        height={height}
        viewBox={`0 0 ${width} ${height}`}
        style={{ display: "block", flexShrink: 0 }}
      >
        <circle cx={cx} cy={cy} r={2.5} fill="#c96b4f" />
      </svg>
    );
  }

  const points = counts
    .map((c, i) => {
      const x = padding + (i / (counts.length - 1)) * (width - 2 * padding);
      const y = height - padding - (c / maxVal) * (height - 2 * padding);
      return `${x.toFixed(1)},${y.toFixed(1)}`;
    })
    .join(" ");

  return (
    <svg
      width={width}
      height={height}
      viewBox={`0 0 ${width} ${height}`}
      style={{ display: "block", flexShrink: 0 }}
    >
      <polyline
        points={points}
        fill="none"
        stroke="#c96b4f"
        strokeWidth="1.5"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

function CompletenessRing({ pct }: { pct: number }) {
  const radius = 40;
  const stroke = 6;
  const circumference = 2 * Math.PI * radius;
  const offset = circumference - (pct / 100) * circumference;
  const color =
    pct >= 80 ? "var(--color-positive, #22c55e)" :
    pct >= 50 ? "var(--color-warning, #f59e0b)" :
    "var(--color-negative, #ef4444)";

  return (
    <svg width="100" height="100" viewBox="0 0 100 100" style={{ display: "block" }}>
      <circle
        cx="50" cy="50" r={radius}
        fill="none" stroke="rgba(0,0,0,0.06)" strokeWidth={stroke}
      />
      <circle
        cx="50" cy="50" r={radius}
        fill="none" stroke={color} strokeWidth={stroke}
        strokeLinecap="round"
        strokeDasharray={circumference}
        strokeDashoffset={offset}
        transform="rotate(-90 50 50)"
        style={{ transition: "stroke-dashoffset 0.6s ease" }}
      />
      <text
        x="50" y="50" textAnchor="middle" dominantBaseline="central"
        style={{ fontSize: "1.25rem", fontWeight: 700, fill: "#1a1a1a" }}
      >
        {pct}%
      </text>
    </svg>
  );
}

function ListingCard({ summary }: { summary: PropertyInterestSummary & { area?: string; price?: number; timeline?: InterestTimelineEntry[] } }) {
  return (
    <Link
      to={`/property/${summary.property_id}`}
      style={{
        display: "block",
        textDecoration: "none",
        color: "inherit",
        border: "1px solid rgba(0,0,0,0.08)",
        borderRadius: "12px",
        padding: "1.25rem",
        background: "#fff",
        transition: "box-shadow 0.2s ease, border-color 0.2s ease",
      }}
      onMouseEnter={(e) => {
        e.currentTarget.style.boxShadow = "0 2px 12px rgba(0,0,0,0.06)";
        e.currentTarget.style.borderColor = "rgba(201,107,79,0.25)";
      }}
      onMouseLeave={(e) => {
        e.currentTarget.style.boxShadow = "none";
        e.currentTarget.style.borderColor = "rgba(0,0,0,0.08)";
      }}
    >
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "flex-start", gap: "1rem" }}>
        <div style={{ flex: 1, minWidth: 0 }}>
          <h3 style={{ margin: 0, fontSize: "1rem", fontWeight: 600, lineHeight: 1.4 }}>
            {summary.property_title}
          </h3>
          {summary.area && (
            <p style={{ margin: "0.25rem 0 0", fontSize: "0.85rem", color: "#888" }}>
              {summary.area}
              {summary.price ? ` \u00b7 ${formatPrice(summary.price)}` : ""}
            </p>
          )}
        </div>
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: "0.35rem",
            background: summary.interest_count > 0 ? "rgba(201,107,79,0.08)" : "rgba(0,0,0,0.04)",
            color: summary.interest_count > 0 ? "#c96b4f" : "#999",
            padding: "0.35rem 0.75rem",
            borderRadius: "20px",
            fontSize: "0.82rem",
            fontWeight: 600,
            whiteSpace: "nowrap",
            flexShrink: 0,
          }}
        >
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round">
            <path d="M20.84 4.61a5.5 5.5 0 0 0-7.78 0L12 5.67l-1.06-1.06a5.5 5.5 0 0 0-7.78 7.78l1.06 1.06L12 21.23l7.78-7.78 1.06-1.06a5.5 5.5 0 0 0 0-7.78z" />
          </svg>
          {summary.interest_count}
        </div>
      </div>
      {summary.last_interest_at && (
        <p style={{ margin: "0.5rem 0 0", fontSize: "0.78rem", color: "#aaa" }}>
          Last interest: {new Date(summary.last_interest_at).toLocaleDateString("en-IN", { day: "numeric", month: "short", year: "numeric" })}
        </p>
      )}
      {summary.timeline && summary.timeline.length > 0 && summary.interest_count > 0 && (
        <div style={{ marginTop: "0.5rem", display: "flex", alignItems: "center", gap: "0.5rem" }}>
          <Sparkline data={summary.timeline} />
          <span style={{ fontSize: "0.72rem", color: "#aaa" }}>30-day trend</span>
        </div>
      )}
    </Link>
  );
}

export function SellerDashboardPage() {
  const { id } = useParams<{ id: string }>();
  const [data, setData] = useState<SellerDashboard | null>(null);
  const [status, setStatus] = useState<"loading" | "loaded" | "error" | "not_found">("loading");

  useEffect(() => {
    if (!id) return;
    let cancelled = false;

    queueMicrotask(() => {
      if (cancelled) return;
      setStatus("loading");
      getSellerDashboard(id)
        .then((d) => {
          if (cancelled) return;
          setData(d);
          setStatus("loaded");
        })
        .catch((err) => {
          if (cancelled) return;
          setStatus(err?.message?.includes("404") ? "not_found" : "error");
        });
    });

    return () => {
      cancelled = true;
    };
  }, [id]);

  if (status === "loading") return <PageState variant="loading" context="generic" message="Loading seller dashboard..." />;
  if (status === "not_found") return <PageState variant="not_found" context="generic" message="This seller profile was not found." />;
  if (status === "error" || !data) return <PageState variant="error" context="generic" />;

  const { seller, interest_summary, completeness_guide } = data;
  const totalInterest = interest_summary.reduce((sum, s) => sum + s.interest_count, 0);

  // Enrich interest summaries with area/price from seller.properties
  const enrichedSummaries = interest_summary.map((s) => {
    const prop = seller.properties.find((p) => p.id === s.property_id);
    return { ...s, area: prop?.area, price: prop?.price };
  });

  return (
    <>
      <Helmet>
        <title>{seller.name} - Seller Dashboard | OpenEstates</title>
      </Helmet>

      <div style={{
        maxWidth: "800px",
        margin: "0 auto",
        padding: "2rem clamp(1rem, 4vw, 2rem)",
      }}>

        {/* --- Profile Header --- */}
        <section style={{
          display: "flex",
          alignItems: "center",
          gap: "1.5rem",
          marginBottom: "2.5rem",
          flexWrap: "wrap",
        }}>
          <CompletenessRing pct={completeness_guide.pct} />
          <div style={{ flex: 1, minWidth: "200px" }}>
            <div style={{ display: "flex", alignItems: "center", gap: "0.5rem", flexWrap: "wrap" }}>
              <h1 style={{ margin: 0, fontSize: "1.5rem", fontWeight: 700, color: "#1a1a1a" }}>
                {seller.name}
              </h1>
              {seller.verified && (
                <span style={{
                  display: "inline-flex",
                  alignItems: "center",
                  gap: "0.25rem",
                  background: "rgba(34,197,94,0.1)",
                  color: "#16a34a",
                  fontSize: "0.75rem",
                  fontWeight: 600,
                  padding: "0.2rem 0.6rem",
                  borderRadius: "20px",
                }}>
                  <svg width="12" height="12" viewBox="0 0 24 24" fill="currentColor">
                    <path d="M9 16.17L4.83 12l-1.42 1.41L9 19 21 7l-1.41-1.41L9 16.17z" />
                  </svg>
                  Verified
                </span>
              )}
            </div>
            <p style={{ margin: "0.35rem 0 0", fontSize: "0.9rem", color: "#888" }}>
              {seller.properties.length} listing{seller.properties.length !== 1 ? "s" : ""}
              {" \u00b7 "}
              {totalInterest} total interest{totalInterest !== 1 ? "s" : ""}
            </p>
            {seller.email && (
              <p style={{ margin: "0.25rem 0 0", fontSize: "0.85rem", color: "#666" }}>
                {seller.email}
              </p>
            )}
            {seller.phone && (
              <p style={{ margin: "0.15rem 0 0", fontSize: "0.85rem", color: "#666" }}>
                {seller.phone}
              </p>
            )}
          </div>
        </section>

        {/* --- Listings --- */}
        <section style={{ marginBottom: "2.5rem" }}>
          <h2 style={{ fontSize: "1.15rem", fontWeight: 600, color: "#1a1a1a", marginBottom: "1rem" }}>
            Your Listings
          </h2>
          {enrichedSummaries.length === 0 ? (
            <p style={{ color: "#999", fontSize: "0.9rem" }}>No listings yet.</p>
          ) : (
            <div style={{ display: "flex", flexDirection: "column", gap: "0.75rem" }}>
              {enrichedSummaries.map((s) => (
                <ListingCard key={s.property_id} summary={s} />
              ))}
            </div>
          )}
        </section>

        {/* --- Completeness Guide --- */}
        <section style={{ marginBottom: "2rem" }}>
          <h2 style={{ fontSize: "1.15rem", fontWeight: 600, color: "#1a1a1a", marginBottom: "0.5rem" }}>
            Complete Your Profile
          </h2>
          {completeness_guide.pct < completeness_guide.min_pct && (
            <p style={{
              fontSize: "0.85rem",
              color: "#c96b4f",
              background: "rgba(201,107,79,0.06)",
              padding: "0.6rem 1rem",
              borderRadius: "8px",
              marginBottom: "1rem",
              lineHeight: 1.5,
            }}>
              Your profile is below the {completeness_guide.min_pct}% minimum. Complete more steps to increase visibility to buyers.
            </p>
          )}
          <p style={{ fontSize: "0.85rem", color: "#888", marginBottom: "1rem" }}>
            {completeness_guide.completed_steps.length} of 7 steps completed
          </p>

          <div style={{ display: "flex", flexDirection: "column", gap: "0.5rem" }}>
            {/* Completed steps */}
            {completeness_guide.completed_steps.map((step) => (
              <div
                key={step}
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: "0.75rem",
                  padding: "0.6rem 1rem",
                  borderRadius: "8px",
                  background: "rgba(34,197,94,0.04)",
                  border: "1px solid rgba(34,197,94,0.12)",
                }}
              >
                <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="#16a34a" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
                  <polyline points="20 6 9 17 4 12" />
                </svg>
                <span style={{ fontSize: "0.9rem", color: "#333" }}>{stepLabel(step)}</span>
              </div>
            ))}

            {/* Next steps with recommendations */}
            {completeness_guide.next_steps.map((step) => (
              <div
                key={step}
                style={{
                  display: "flex",
                  alignItems: "flex-start",
                  gap: "0.75rem",
                  padding: "0.6rem 1rem",
                  borderRadius: "8px",
                  background: "rgba(0,0,0,0.02)",
                  border: "1px solid rgba(0,0,0,0.06)",
                }}
              >
                <div style={{
                  width: "18px",
                  height: "18px",
                  borderRadius: "50%",
                  border: "2px solid #ccc",
                  flexShrink: 0,
                  marginTop: "1px",
                }} />
                <div>
                  <span style={{ fontSize: "0.9rem", color: "#666" }}>{stepLabel(step)}</span>
                  {stepRecommendation(step) && (
                    <p style={{ margin: "0.2rem 0 0", fontSize: "0.78rem", color: "#aaa", lineHeight: 1.4 }}>
                      {stepRecommendation(step)}
                    </p>
                  )}
                </div>
              </div>
            ))}
          </div>
        </section>

      </div>
    </>
  );
}
