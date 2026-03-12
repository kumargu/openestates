/**
 * Society Detail Page
 *
 * Full detail view for a single society — hero image, dimension scores,
 * signals/cautions, resident quote, photos grid, and link to properties.
 */
import { useEffect, useState } from "react";
import { useParams, Link, useNavigate } from "react-router-dom";
import { Helmet } from "react-helmet-async";
import type { SocietySearchResult } from "../lib/types.ts";
import { getSociety } from "../lib/api.ts";
import { PageState } from "../components/PageState.tsx";
import { ShareButtons } from "../components/ShareButtons.tsx";

// ---------------------------------------------------------------------------
// Helpers (reused from SocietySearchPage patterns)
// ---------------------------------------------------------------------------

function dimensionLabel(key: string): string {
  const map: Record<string, string> = {
    family_friendly: "Family Friendly",
    maintenance_quality: "Maintenance Quality",
    school_access: "School Access",
    calm_environment: "Calm Environment",
    builder_trust: "Builder Trust",
    value: "Value for Money",
  };
  return map[key] || key.replace(/_/g, " ").replace(/\b\w/g, (c) => c.toUpperCase());
}

function scoreColor(score: number): string {
  if (score >= 80) return "var(--color-positive)";
  if (score >= 65) return "var(--color-accent)";
  if (score >= 50) return "var(--color-warning)";
  return "var(--color-negative)";
}

function confidenceDisplay(c: string): { label: string; color: string } {
  if (c === "high") return { label: "High", color: "var(--color-positive)" };
  if (c === "moderate") return { label: "Moderate", color: "var(--color-warning)" };
  return { label: "Low", color: "var(--color-text-muted)" };
}

// ---------------------------------------------------------------------------
// Sub-components
// ---------------------------------------------------------------------------

function DimensionBar({ label, score }: { label: string; score: number }) {
  return (
    <div style={{ marginBottom: "0.6rem" }}>
      <div style={{ display: "flex", justifyContent: "space-between", fontSize: "0.85rem", marginBottom: "0.2rem" }}>
        <span style={{ color: "var(--color-text-secondary)" }}>{label}</span>
        <span style={{ fontWeight: 600, color: scoreColor(score) }}>{score}</span>
      </div>
      <div style={{
        height: "7px",
        borderRadius: "3.5px",
        background: "var(--color-bg-soft)",
        overflow: "hidden",
      }}>
        <div
          style={{
            width: `${score}%`,
            height: "100%",
            borderRadius: "3.5px",
            background: scoreColor(score),
            transition: "width 0.4s ease",
          }}
        />
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Main page
// ---------------------------------------------------------------------------

function buildSocietyJsonLd(s: SocietySearchResult) {
  const jsonLd: Record<string, unknown> = {
    "@context": "https://schema.org",
    "@type": "ApartmentComplex",
    name: s.name,
    description: s.summary || `${s.name} by ${s.builder}`,
    address: {
      "@type": "PostalAddress",
      addressLocality: s.name,
      addressRegion: "Bengaluru",
    },
  };
  if (s.photos[0]) {
    jsonLd.image = s.photos[0];
  }
  if (s.total_units) {
    jsonLd.numberOfAvailableAccommodationUnits = s.total_units;
  }
  if (s.year_built) {
    jsonLd.yearBuilt = s.year_built;
  }
  return jsonLd;
}

export function SocietyDetailPage() {
  const { slug } = useParams<{ slug: string }>();
  const navigate = useNavigate();
  const [data, setData] = useState<SocietySearchResult | null>(null);
  const [status, setStatus] = useState<"loading" | "error" | "not_found" | "ok">("loading");

  useEffect(() => {
    if (!slug) return;
    getSociety(slug)
      .then((d) => {
        setData(d);
        setStatus("ok");
      })
      .catch((err: Error) => {
        setStatus(err.message.includes("404") ? "not_found" : "error");
      });
  }, [slug]);

  if (status === "loading") return (
    <div className="page-container-wide">
      {/* Hero placeholder */}
      <div className="skeleton-bar" style={{ width: "100%", height: "300px", borderRadius: "var(--radius-md)", marginBottom: "1.5rem" }} />
      {/* Title */}
      <div className="skeleton-bar" style={{ width: "55%", height: "28px", marginBottom: "0.5rem" }} />
      {/* Subtitle */}
      <div className="skeleton-bar" style={{ width: "45%", height: "16px", marginBottom: "1rem" }} />
      {/* Price */}
      <div className="skeleton-bar" style={{ width: "20%", height: "22px", marginBottom: "2rem" }} />
      {/* Two-column layout */}
      <div className="property-layout">
        <div className="property-main">
          <div className="skeleton-detail-section skeleton-bar" />
          <div className="skeleton-detail-section skeleton-bar" style={{ height: "200px" }} />
          <div className="skeleton-detail-section skeleton-bar" style={{ height: "140px" }} />
        </div>
        <div className="property-sidebar">
          <div className="skeleton-detail-section skeleton-bar" style={{ height: "120px" }} />
          <div className="skeleton-detail-section skeleton-bar" style={{ height: "140px" }} />
        </div>
      </div>
    </div>
  );
  if (status === "not_found") return <PageState variant="not_found" context="generic" message={`Society "${slug}" was not found.`} />;
  if (status === "error") return <PageState variant="error" context="generic" />;
  if (!data) return null;

  const s = data;
  const heroSrc = s.photos[0] || null;
  const dims = Object.entries(s.dimension_scores);
  const conf = confidenceDisplay(s.confidence);

  const pageTitle = `${s.name} — Society Detail | OpenEstates`;
  const pageDescription = `${s.name} by ${s.builder}. ${s.year_built ? `Built ${s.year_built}. ` : ""}${s.total_units ? `${s.total_units} units. ` : ""}${s.price_range || ""} Overall score: ${s.overall_score}/100.`;

  return (
    <div className="page-container-wide">
      <Helmet>
        <title>{pageTitle}</title>
        <meta name="description" content={pageDescription} />
        <meta property="og:title" content={pageTitle} />
        <meta property="og:description" content={pageDescription} />
        <meta property="og:type" content="website" />
        <meta property="og:site_name" content="OpenEstates" />
        {heroSrc && <meta property="og:image" content={heroSrc} />}
        <script type="application/ld+json">{JSON.stringify(buildSocietyJsonLd(s))}</script>
      </Helmet>

      {/* Back button with context-aware navigation */}
      <button
        onClick={() => {
          if (window.history.length > 1) {
            navigate(-1);
          } else {
            navigate("/societies");
          }
        }}
        className="back-link"
        style={{ background: "none", border: "none", cursor: "pointer", padding: 0 }}
      >
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round">
          <polyline points="15 18 9 12 15 6" />
        </svg>
        Back to societies
      </button>

      {/* Hero image */}
      {heroSrc && (
        <div className="society-hero-wrapper">
          <img
            src={heroSrc}
            alt={s.name}
            className="society-hero-image"
            loading="lazy"
          />

          {/* Score badge overlay */}
          <div
            style={{
              position: "absolute",
              top: "1rem",
              left: "1rem",
              display: "flex",
              alignItems: "center",
              gap: "0.5rem",
            }}
          >
            <span className="society-hero-badge-rank">
              #{s.rank}
            </span>
            <span
              className="society-hero-badge"
              style={{
                fontSize: "0.88rem",
                color: scoreColor(s.overall_score),
              }}
            >
              {s.overall_score}/100
            </span>
          </div>

          {/* Confidence badge */}
          <span
            className="society-hero-badge-confidence"
            style={{
              position: "absolute",
              top: "1rem",
              right: "1rem",
              color: conf.color,
            }}
          >
            Confidence: {conf.label}
          </span>

          {/* Best-for ribbon */}
          <span
            className="society-hero-badge-bestfor"
            style={{
              position: "absolute",
              bottom: "1rem",
              left: "1rem",
            }}
          >
            {s.best_for_label}
          </span>
        </div>
      )}

      {/* Society header */}
      <div style={{ marginBottom: "2rem" }}>
        <h1 style={{
          fontSize: "clamp(1.5rem, 1.2rem + 1vw, 2rem)",
          fontWeight: 700,
          letterSpacing: "-0.025em",
          margin: "0 0 0.35rem",
        }}>
          {s.name}
        </h1>
        <p style={{ color: "var(--color-text-secondary)", margin: "0 0 0.75rem", fontSize: "0.95rem" }}>
          {s.builder}
          {s.year_built ? ` \u00B7 Est. ${s.year_built}` : ""}
          {s.total_units ? ` \u00B7 ${s.total_units.toLocaleString()} units` : ""}
          {s.unit_types ? ` \u00B7 ${s.unit_types}` : ""}
        </p>
        {s.price_range && (
          <span style={{
            fontSize: "1.2rem",
            fontWeight: 700,
            letterSpacing: "-0.02em",
            color: "var(--color-text)",
          }}>
            {s.price_range}
          </span>
        )}
      </div>

      {/* Two-column layout */}
      <div className="property-layout">
        {/* Main column */}
        <div className="property-main">

          {/* Life-fit reason */}
          <div className="section-card">
            <div className="section-card-header">
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="var(--color-text-muted)" strokeWidth="2" strokeLinecap="round">
                <path d="M20.84 4.61a5.5 5.5 0 0 0-7.78 0L12 5.67l-1.06-1.06a5.5 5.5 0 0 0-7.78 7.78l1.06 1.06L12 21.23l7.78-7.78 1.06-1.06a5.5 5.5 0 0 0 0-7.78z" />
              </svg>
              <h2>Why this society</h2>
            </div>
            <p style={{
              margin: 0,
              fontSize: "1.05rem",
              lineHeight: 1.6,
              color: "var(--color-text)",
              fontWeight: 500,
            }}>
              {s.life_fit_reason}
            </p>
          </div>

          {/* All dimension score bars */}
          <div className="section-card">
            <div className="section-card-header">
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="var(--color-text-muted)" strokeWidth="2" strokeLinecap="round">
                <path d="M22 11.08V12a10 10 0 1 1-5.93-9.14" />
                <polyline points="22 4 12 14.01 9 11.01" />
              </svg>
              <h2>Dimension scores</h2>
            </div>
            {dims.map(([key, val]) => (
              <DimensionBar key={key} label={dimensionLabel(key)} score={val} />
            ))}
          </div>

          {/* Signals & Cautions */}
          {(s.signals.length > 0 || s.cautions.length > 0) && (
            <div className="section-card">
              <div className="section-card-header">
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="var(--color-text-muted)" strokeWidth="2" strokeLinecap="round">
                  <path d="M12 22c5.523 0 10-4.477 10-10S17.523 2 12 2 2 6.477 2 12s4.477 10 10 10z" />
                  <path d="M12 16v-4" />
                  <path d="M12 8h.01" />
                </svg>
                <h2>Signals and cautions</h2>
              </div>
              <div className="detail-grid" style={{ gap: "1rem" }}>
                {s.signals.length > 0 && (
                  <div style={{
                    padding: "1rem",
                    borderRadius: "var(--radius-sm)",
                    backgroundColor: "var(--color-positive-bg)",
                    border: "1px solid var(--color-positive-border)",
                  }}>
                    <h3 style={{
                      margin: "0 0 0.5rem",
                      fontSize: "0.7rem",
                      fontWeight: 600,
                      textTransform: "uppercase",
                      letterSpacing: "0.08em",
                      color: "var(--color-positive)",
                    }}>
                      Signals
                    </h3>
                    <div style={{ display: "flex", gap: "0.35rem", flexWrap: "wrap" }}>
                      {s.signals.map((sig) => (
                        <span key={sig} className="tag" style={{
                          background: "rgba(237, 247, 237, 0.8)",
                          color: "var(--color-positive)",
                          fontSize: "0.78rem",
                          padding: "0.25rem 0.65rem",
                          borderRadius: "100px",
                        }}>{sig}</span>
                      ))}
                    </div>
                  </div>
                )}
                {s.cautions.length > 0 && (
                  <div style={{
                    padding: "1rem",
                    borderRadius: "var(--radius-sm)",
                    backgroundColor: "var(--color-negative-bg)",
                    border: "1px solid var(--color-negative-border)",
                  }}>
                    <h3 style={{
                      margin: "0 0 0.5rem",
                      fontSize: "0.7rem",
                      fontWeight: 600,
                      textTransform: "uppercase",
                      letterSpacing: "0.08em",
                      color: "var(--color-negative)",
                    }}>
                      Cautions
                    </h3>
                    <div style={{ display: "flex", gap: "0.35rem", flexWrap: "wrap" }}>
                      {s.cautions.map((cau) => (
                        <span key={cau} className="tag" style={{
                          background: "rgba(254, 242, 237, 0.8)",
                          color: "var(--color-negative, #d63031)",
                          fontSize: "0.78rem",
                          padding: "0.25rem 0.65rem",
                          borderRadius: "100px",
                        }}>{cau}</span>
                      ))}
                    </div>
                  </div>
                )}
              </div>
            </div>
          )}

          {/* Resident quote */}
          {s.resident_quote && (
            <div className="section-card">
              <div className="section-card-header">
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="var(--color-text-muted)" strokeWidth="2" strokeLinecap="round">
                  <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" />
                </svg>
                <h2>Resident voice</h2>
              </div>
              <blockquote
                style={{
                  margin: 0,
                  padding: "1rem 1.25rem",
                  background: "var(--color-bg-soft)",
                  borderLeft: "3px solid var(--color-accent)",
                  borderRadius: "0 6px 6px 0",
                  fontSize: "0.92rem",
                  lineHeight: 1.6,
                  color: "var(--color-text-secondary)",
                  fontStyle: "italic",
                }}
              >
                &ldquo;{s.resident_quote}&rdquo;
                <span style={{ display: "block", marginTop: "0.5rem", fontSize: "0.75rem", color: "var(--color-text-muted)", fontStyle: "normal" }}>
                  From Reddit discussions
                </span>
              </blockquote>
            </div>
          )}

          {/* Summary */}
          {s.summary && (
            <div className="section-card">
              <div className="section-card-header">
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="var(--color-text-muted)" strokeWidth="2" strokeLinecap="round">
                  <rect x="3" y="3" width="7" height="7" />
                  <rect x="14" y="3" width="7" height="7" />
                  <rect x="3" y="14" width="7" height="7" />
                  <rect x="14" y="14" width="7" height="7" />
                </svg>
                <h2>Summary</h2>
              </div>
              <p style={{
                margin: 0,
                fontSize: "0.92rem",
                lineHeight: 1.6,
                color: "var(--color-text-secondary)",
              }}>
                {s.summary}
              </p>
            </div>
          )}

          {/* Why this ranks */}
          {s.why_above_next && (
            <div className="section-card">
              <div className="section-card-header">
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="var(--color-text-muted)" strokeWidth="2" strokeLinecap="round">
                  <polyline points="23 6 13.5 15.5 8.5 10.5 1 18" />
                  <polyline points="17 6 23 6 23 12" />
                </svg>
                <h2>Why this ranks #{s.rank}</h2>
              </div>
              <p style={{
                margin: 0,
                fontSize: "0.92rem",
                lineHeight: 1.6,
                color: "var(--color-text-secondary)",
              }}>
                {s.why_above_next}
              </p>
            </div>
          )}

          {/* Photos grid */}
          {s.photos.length > 1 && (
            <div className="section-card">
              <div className="section-card-header">
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="var(--color-text-muted)" strokeWidth="2" strokeLinecap="round">
                  <rect x="3" y="3" width="18" height="18" rx="2" ry="2" />
                  <circle cx="8.5" cy="8.5" r="1.5" />
                  <polyline points="21 15 16 10 5 21" />
                </svg>
                <h2>Photos</h2>
              </div>
              <div className="society-photos-grid">
                {s.photos.slice(1).map((p, i) => (
                  <div key={i} style={{ height: "160px", borderRadius: "var(--radius-sm)", overflow: "hidden" }}>
                    <img
                      src={p}
                      alt={`${s.name} photo ${i + 2}`}
                      style={{ width: "100%", height: "100%", objectFit: "cover" }}
                      loading="lazy"
                    />
                  </div>
                ))}
              </div>
            </div>
          )}

        </div>{/* end property-main */}

        {/* Sidebar */}
        <div className="property-sidebar">
          {/* Score card */}
          <div className="section-card" style={{ marginBottom: "1rem" }}>
            <div style={{ marginBottom: "0.75rem" }}>
              <div style={{ fontSize: "1.5rem", fontWeight: 700, letterSpacing: "-0.02em", color: scoreColor(s.overall_score) }}>
                {s.overall_score}/100
              </div>
              <div style={{ fontSize: "0.82rem", color: "var(--color-text-muted)" }}>
                Overall score &middot; {conf.label} confidence
              </div>
            </div>
            {s.price_range && (
              <div style={{
                padding: "0.6rem 0.85rem",
                borderRadius: "var(--radius-sm)",
                backgroundColor: "var(--color-bg-elevated)",
                border: "1px solid var(--color-border)",
                fontSize: "0.85rem",
                fontWeight: 500,
                marginBottom: "0.75rem",
              }}>
                {s.price_range}
              </div>
            )}
            <Link
              to={`/results?q=${encodeURIComponent(s.name)}`}
              className="btn btn-primary"
              style={{ width: "100%", justifyContent: "center", textDecoration: "none", display: "flex" }}
            >
              Properties in this society
            </Link>
            <ShareButtons title={s.name} path={`/society/${slug}`} />
          </div>

          {/* Quick facts */}
          <div className="section-card">
            <div className="section-card-header" style={{ marginBottom: "0.5rem" }}>
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="var(--color-text-muted)" strokeWidth="2" strokeLinecap="round">
                <rect x="3" y="3" width="7" height="7" />
                <rect x="14" y="3" width="7" height="7" />
                <rect x="3" y="14" width="7" height="7" />
                <rect x="14" y="14" width="7" height="7" />
              </svg>
              <h2 style={{ fontSize: "0.85rem" }}>Quick facts</h2>
            </div>
            <div style={{ display: "grid", gap: "0.5rem" }}>
              <FactRow label="Builder" value={s.builder} />
              {s.year_built && <FactRow label="Year built" value={String(s.year_built)} />}
              {s.total_units && <FactRow label="Total units" value={s.total_units.toLocaleString()} />}
              {s.unit_types && <FactRow label="Unit types" value={s.unit_types} />}
            </div>
          </div>

          {/* Evidence */}
          <div className="section-card" style={{ marginTop: "1rem" }}>
            <div className="section-card-header" style={{ marginBottom: "0.5rem" }}>
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="var(--color-text-muted)" strokeWidth="2" strokeLinecap="round">
                <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
                <polyline points="14 2 14 8 20 8" />
              </svg>
              <h2 style={{ fontSize: "0.85rem" }}>Evidence sources</h2>
            </div>
            <div style={{ display: "grid", gap: "0.4rem", fontSize: "0.82rem", color: "var(--color-text-secondary)" }}>
              <div>{s.evidence.reddit_threads} Reddit threads analyzed</div>
              <div>{s.evidence.society_threads} society-specific threads</div>
              <div>{s.evidence.area_threads} area threads</div>
              {s.evidence.has_seed_data && <div>Seed data available</div>}
              <div style={{ fontSize: "0.75rem", color: "var(--color-text-muted)", marginTop: "0.25rem" }}>
                Reddit confidence: {s.evidence.reddit_confidence}
              </div>
            </div>
          </div>
        </div>{/* end property-sidebar */}
      </div>{/* end property-layout */}
    </div>
  );
}

function FactRow({ label, value }: { label: string; value: string }) {
  return (
    <div style={{
      display: "flex",
      justifyContent: "space-between",
      alignItems: "center",
      padding: "0.5rem 0.75rem",
      borderRadius: "var(--radius-sm)",
      backgroundColor: "var(--color-bg-elevated)",
      border: "1px solid var(--color-border)",
    }}>
      <span style={{ fontSize: "0.78rem", color: "var(--color-text-muted)" }}>{label}</span>
      <span style={{ fontSize: "0.82rem", fontWeight: 500, color: "var(--color-text)" }}>{value}</span>
    </div>
  );
}
