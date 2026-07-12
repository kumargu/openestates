/**
 * Society Search Results Page
 *
 * Ranked society cards with dimension scores, resident quotes,
 * signal/caution chips, and area context — the transparency-first
 * society discovery surface.
 */
import { useEffect, useState, useCallback, useRef } from "react";
import { useSearchParams, Link } from "react-router-dom";
import { Helmet } from "react-helmet-async";
import type { SocietySearchResponse, SocietySearchResult } from "../lib/types.ts";
import { searchSocieties } from "../lib/api.ts";
import { PageState } from "../components/PageState.tsx";
const DEFAULT_QUERY = "family friendly whitefield";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function dimensionLabel(key: string): string {
  const map: Record<string, string> = {
    family_friendly: "Family",
    maintenance_quality: "Maintenance",
    school_access: "Schools",
    calm_environment: "Calm",
    builder_trust: "Builder Trust",
    value: "Value",
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

function WeightChips({ weights }: { weights: Record<string, number> }) {
  const labels: Record<string, string> = {
    family_friendly: "Family-friendly",
    maintenance_quality: "Well-maintained",
    school_access: "Good schools",
    calm_environment: "Calm & quiet",
    builder_trust: "Trusted builder",
    value: "Good value",
  };
  // Show top 3 weights
  const sorted = Object.entries(weights).sort(([, a], [, b]) => b - a).slice(0, 4);
  return (
    <div style={{ display: "flex", gap: "0.4rem", flexWrap: "wrap" }}>
      {sorted.map(([key, w]) => (
        <span key={key} className="tag tag-neutral" style={{ fontSize: "0.78rem", padding: "0.25rem 0.65rem" }}>
          {labels[key] || dimensionLabel(key)} ({Math.round(w * 100)}%)
        </span>
      ))}
    </div>
  );
}

function AreaContextBar({ ctx }: { ctx: NonNullable<SocietySearchResponse["area_context"]> }) {
  return (
    <div
      className="section-card"
      style={{ padding: "1.25rem 1.5rem", marginBottom: "1.75rem" }}
    >
      <div className="section-card-header" style={{ marginBottom: "0.75rem" }}>
        <h2 style={{ margin: 0, fontSize: "0.85rem", fontWeight: 600, textTransform: "uppercase", letterSpacing: "0.05em", color: "var(--color-text-secondary)" }}>
          Area Context: {ctx.name}
        </h2>
      </div>
      <div className="society-area-context-grid">
        {ctx.metro_access_summary && (
          <div>
            <span style={{ fontSize: "0.72rem", fontWeight: 600, textTransform: "uppercase", letterSpacing: "0.05em", color: "var(--color-text-muted)" }}>Metro access</span>
            <p style={{ margin: "0.2rem 0 0", fontSize: "0.85rem", lineHeight: 1.5, color: "var(--color-text-secondary)" }}>
              {ctx.metro_access_summary}
            </p>
          </div>
        )}
        {ctx.traffic_summary && (
          <div>
            <span style={{ fontSize: "0.72rem", fontWeight: 600, textTransform: "uppercase", letterSpacing: "0.05em", color: "var(--color-text-muted)" }}>Traffic reality</span>
            <p style={{ margin: "0.2rem 0 0", fontSize: "0.85rem", lineHeight: 1.5, color: "var(--color-text-secondary)" }}>
              {ctx.traffic_summary}
            </p>
          </div>
        )}
        {ctx.trend_summary && (
          <div>
            <span style={{ fontSize: "0.72rem", fontWeight: 600, textTransform: "uppercase", letterSpacing: "0.05em", color: "var(--color-text-muted)" }}>Price trend</span>
            <p style={{ margin: "0.2rem 0 0", fontSize: "0.85rem", lineHeight: 1.5, color: "var(--color-text-secondary)" }}>
              {ctx.trend_direction && <span style={{ fontWeight: 600 }}>{ctx.trend_direction} — </span>}
              {ctx.trend_summary}
            </p>
          </div>
        )}
      </div>
      {ctx.externality_tags && ctx.externality_tags.length > 0 && (
        <div style={{ marginTop: "0.75rem", display: "flex", gap: "0.35rem", flexWrap: "wrap" }}>
          {ctx.externality_tags.map((t) => (
            <span key={t} className="tag tag-neutral" style={{ fontSize: "0.72rem" }}>{t}</span>
          ))}
        </div>
      )}
    </div>
  );
}

function DimensionBar({ label, score }: { label: string; score: number }) {
  return (
    <div style={{ marginBottom: "0.45rem" }}>
      <div style={{ display: "flex", justifyContent: "space-between", fontSize: "0.78rem", marginBottom: "0.15rem" }}>
        <span style={{ color: "var(--color-text-secondary)" }}>{label}</span>
        <span style={{ fontWeight: 600, color: scoreColor(score) }}>{score}</span>
      </div>
      <div style={{
        height: "6px",
        borderRadius: "3px",
        background: "var(--color-bg-soft)",
        overflow: "hidden",
      }}>
        <div
          style={{
            width: `${score}%`,
            height: "100%",
            borderRadius: "3px",
            background: scoreColor(score),
            transition: "width 0.4s ease",
          }}
        />
      </div>
    </div>
  );
}

function SocietyCard({ result }: { result: SocietySearchResult }) {
  const [expanded, setExpanded] = useState(false);

  const heroSrc = result.photos[0] || null;
  const dims = Object.entries(result.dimension_scores);
  const conf = confidenceDisplay(result.confidence);

  return (
    <div
      className="card"
      style={{
        overflow: "hidden",
        cursor: "default",
      }}
    >
      {/* Hero image */}
      {heroSrc && (
        <div className="society-card-hero">
          <img
            src={heroSrc}
            alt={result.name}
            style={{ width: "100%", height: "100%", objectFit: "cover" }}
            loading="lazy"
          />

          {/* Rank badge */}
          <div
            style={{
              position: "absolute",
              top: "0.75rem",
              left: "0.75rem",
              display: "flex",
              alignItems: "center",
              gap: "0.5rem",
            }}
          >
            <span
              style={{
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
                width: "36px",
                height: "36px",
                borderRadius: "50%",
                background: "rgba(26, 26, 26, 0.85)",
                backdropFilter: "blur(8px)",
                color: "#fff",
                fontWeight: 700,
                fontSize: "0.95rem",
              }}
            >
              #{result.rank}
            </span>
            <span
              style={{
                padding: "0.3rem 0.75rem",
                borderRadius: "100px",
                background: "rgba(255, 255, 255, 0.92)",
                backdropFilter: "blur(8px)",
                fontSize: "0.8rem",
                fontWeight: 600,
                color: scoreColor(result.overall_score),
              }}
            >
              {result.overall_score}/100
            </span>
          </div>

          {/* Best-for ribbon */}
          <span
            style={{
              position: "absolute",
              bottom: "0.75rem",
              left: "0.75rem",
              padding: "0.25rem 0.75rem",
              borderRadius: "100px",
              background: "rgba(237, 247, 237, 0.95)",
              backdropFilter: "blur(8px)",
              color: "var(--color-positive)",
              fontSize: "0.75rem",
              fontWeight: 600,
            }}
          >
            {result.best_for_label}
          </span>

          {/* Confidence */}
          <span
            style={{
              position: "absolute",
              top: "0.75rem",
              right: "0.75rem",
              padding: "0.2rem 0.6rem",
              borderRadius: "100px",
              background: "rgba(255, 255, 255, 0.88)",
              backdropFilter: "blur(8px)",
              fontSize: "0.68rem",
              fontWeight: 500,
              color: conf.color,
            }}
          >
            Confidence: {conf.label}
          </span>
        </div>
      )}

      {/* Card body */}
      <div style={{ padding: "1.25rem 1.5rem 1rem" }}>
        {/* Header row */}
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "flex-start", marginBottom: "0.25rem" }}>
          <h3 style={{ margin: 0, fontSize: "1.15rem", fontWeight: 600, letterSpacing: "-0.01em" }}>
            {result.name}
          </h3>
          {result.price_range && (
            <span style={{ fontSize: "0.78rem", color: "var(--color-text-secondary)", whiteSpace: "nowrap", marginLeft: "0.75rem" }}>
              {result.price_range}
            </span>
          )}
        </div>

        {/* Meta row */}
        <p style={{ margin: "0 0 0.75rem", color: "var(--color-text-secondary)", fontSize: "0.82rem" }}>
          {result.builder}
          {result.year_built ? ` \u00B7 Est. ${result.year_built}` : ""}
          {result.total_units ? ` \u00B7 ${result.total_units.toLocaleString()} units` : ""}
          {result.unit_types ? ` \u00B7 ${result.unit_types}` : ""}
        </p>

        {/* Life-fit reason */}
        <p style={{
          margin: "0 0 1rem",
          fontSize: "0.88rem",
          lineHeight: 1.55,
          color: "var(--color-text)",
        }}>
          {result.life_fit_reason}
        </p>

        {/* Dimension score bars */}
        <div style={{ marginBottom: "1rem" }}>
          {dims.slice(0, expanded ? dims.length : 3).map(([key, val]) => (
            <DimensionBar key={key} label={dimensionLabel(key)} score={val} />
          ))}
        </div>

        {/* Signal & Caution chips */}
        <div style={{ display: "flex", gap: "0.35rem", flexWrap: "wrap", marginBottom: "0.75rem" }}>
          {result.signals.slice(0, expanded ? result.signals.length : 3).map((s) => (
            <span key={s} className="tag" style={{
              background: "rgba(237, 247, 237, 0.8)",
              color: "var(--color-positive)",
              fontSize: "0.75rem",
              padding: "0.2rem 0.6rem",
              borderRadius: "100px",
            }}>{s}</span>
          ))}
          {result.cautions.slice(0, expanded ? result.cautions.length : 2).map((c) => (
            <span key={c} className="tag" style={{
              background: "rgba(254, 242, 237, 0.8)",
              color: "var(--color-negative, #d63031)",
              fontSize: "0.75rem",
              padding: "0.2rem 0.6rem",
              borderRadius: "100px",
            }}>{c}</span>
          ))}
        </div>

        {/* Resident quote */}
        {result.resident_quote && (
          <blockquote
            style={{
              margin: "0 0 0.75rem",
              padding: "0.85rem 1rem",
              background: "var(--color-bg-soft)",
              borderLeft: "3px solid var(--color-accent)",
              borderRadius: "0 6px 6px 0",
              fontSize: "0.82rem",
              lineHeight: 1.55,
              color: "var(--color-text-secondary)",
              fontStyle: "italic",
            }}
          >
            &ldquo;{result.resident_quote}&rdquo;
            <span style={{ display: "block", marginTop: "0.35rem", fontSize: "0.72rem", color: "var(--color-text-muted)", fontStyle: "normal" }}>
              From Reddit discussions
            </span>
          </blockquote>
        )}

        {/* Expanded details */}
        {expanded && (
          <div style={{ animation: "fadeUp 0.3s ease forwards" }}>
            {/* More photos */}
            {result.photos.length > 1 && (
              <div style={{
                display: "grid",
                gridTemplateColumns: `repeat(${Math.min(result.photos.length - 1, 3)}, 1fr)`,
                gap: "0.5rem",
                marginBottom: "1rem",
              }}>
                {result.photos.slice(1, 4).map((p, i) => (
                  <div key={i} style={{ height: "120px", borderRadius: "6px", overflow: "hidden" }}>
                    <img
                      src={p}
                      alt={`${result.name} photo ${i + 2}`}
                      style={{ width: "100%", height: "100%", objectFit: "cover" }}
                      loading="lazy"
                    />
                  </div>
                ))}
              </div>
            )}

            {/* Summary */}
            {result.summary && (
              <p style={{
                fontSize: "0.82rem",
                lineHeight: 1.55,
                color: "var(--color-text-secondary)",
                marginBottom: "0.75rem",
              }}>
                {result.summary}
              </p>
            )}

            {/* Why this beat next */}
            {result.why_above_next && (
              <div
                style={{
                  padding: "0.85rem 1rem",
                  background: "var(--color-bg-elevated, #f8f9fa)",
                  borderRadius: "6px",
                  marginBottom: "0.75rem",
                  fontSize: "0.82rem",
                  lineHeight: 1.5,
                  color: "var(--color-text-secondary)",
                }}
              >
                <span style={{ fontWeight: 600, color: "var(--color-text)", fontSize: "0.72rem", textTransform: "uppercase", letterSpacing: "0.06em" }}>
                  Why this ranks #{result.rank}
                </span>
                <p style={{ margin: "0.3rem 0 0" }}>{result.why_above_next}</p>
              </div>
            )}
          </div>
        )}

        {/* Evidence footer */}
        <div
          style={{
            display: "flex",
            justifyContent: "space-between",
            alignItems: "center",
            paddingTop: "0.75rem",
            borderTop: "1px solid var(--color-border, #e5e5e5)",
            fontSize: "0.72rem",
            color: "var(--color-text-muted)",
          }}
        >
          <span>
            {result.evidence.reddit_threads} Reddit threads
            {result.evidence.has_seed_data ? " \u00B7 Seed data" : ""}
            {" \u00B7 "}Confidence: {conf.label.toLowerCase()}
          </span>
          <button
            onClick={() => setExpanded((prev) => !prev)}
            style={{
              background: "none",
              border: "none",
              color: "var(--color-accent)",
              cursor: "pointer",
              fontSize: "0.78rem",
              fontWeight: 500,
              padding: "0.25rem 0",
            }}
          >
            {expanded ? "Show less" : "More details"}
          </button>
        </div>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Main page
// ---------------------------------------------------------------------------

export function SocietySearchPage() {
  const [searchParams, setSearchParams] = useSearchParams();
  const queryParam = searchParams.get("q") || DEFAULT_QUERY;
  const [inputValue, setInputValue] = useState(queryParam);
  const [data, setData] = useState<SocietySearchResponse | null>(null);
  const [status, setStatus] = useState<"loading" | "ok" | "error">("loading");
  const [errorMsg, setErrorMsg] = useState("");
  const latestRequestId = useRef(0);

  const fetchResults = useCallback(async (q: string, requestId: number) => {
    try {
      const json = await searchSocieties(q);
      if (latestRequestId.current !== requestId) return;
      setData(json);
      setStatus("ok");
    } catch {
      if (latestRequestId.current !== requestId) return;
      setErrorMsg("Could not load society results. Please try again later.");
      setStatus("error");
    }
  }, []);

  useEffect(() => {
    const requestId = latestRequestId.current + 1;
    latestRequestId.current = requestId;

    queueMicrotask(() => {
      if (latestRequestId.current !== requestId) return;
      setData(null);
      setStatus("loading");
      setErrorMsg("");
      void fetchResults(queryParam, requestId);
    });

    return () => {
      if (latestRequestId.current === requestId) {
        latestRequestId.current += 1;
      }
    };
  }, [queryParam, fetchResults]);

  const handleSearch = (e: React.FormEvent) => {
    e.preventDefault();
    const trimmed = inputValue.trim();
    if (trimmed) {
      setStatus("loading");
      setErrorMsg("");
      setSearchParams({ q: trimmed });
    }
  };

  // Loading state
  if (status === "loading") {
    return (
      <div className="page-container" style={{ maxWidth: "800px" }}>
        <div style={{
          display: "flex",
          flexDirection: "column",
          alignItems: "center",
          justifyContent: "center",
          minHeight: "50vh",
          gap: "1rem",
        }}>
          <div style={{
            width: "40px",
            height: "40px",
            border: "3px solid var(--color-border, #e5e5e5)",
            borderTopColor: "var(--color-accent, #2d6a4f)",
            borderRadius: "50%",
            animation: "spin 0.8s linear infinite",
          }} />
          <p style={{ color: "var(--color-text-secondary)", fontSize: "0.9rem" }}>
            Ranking societies...
          </p>
          <style>{`@keyframes spin { to { transform: rotate(360deg); } }`}</style>
        </div>
      </div>
    );
  }

  // Error state
  if (status === "error") {
    return (
      <div className="page-container" style={{ maxWidth: "800px" }}>
        <PageState variant="error" context="society" message={errorMsg || undefined} />
      </div>
    );
  }

  if (!data) return null;

  const { query_interpreted, results, area_context, enrichment_status } = data;

  const societyHelmetTitle = queryParam
    ? `${queryParam} — Society Rankings | OpenEstates`
    : "Society Rankings — OpenEstates";
  const societyHelmetDescription = `${results.length} ${results.length === 1 ? "society" : "societies"} ranked for "${queryParam}"${query_interpreted.area ? ` in ${query_interpreted.area}` : ""}. Transparency-first society discovery on OpenEstates.`;

  return (
    <div className="page-container" style={{ maxWidth: "800px" }}>
      <Helmet>
        <title>{societyHelmetTitle}</title>
        <meta name="description" content={societyHelmetDescription} />
        <meta property="og:title" content={societyHelmetTitle} />
        <meta property="og:description" content={societyHelmetDescription} />
        <meta property="og:type" content="website" />
        <meta property="og:site_name" content="OpenEstates" />
      </Helmet>
      {/* Search bar */}
      <form onSubmit={handleSearch} style={{ marginBottom: "1.5rem" }}>
        <div className="society-search-bar">
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="var(--color-text-muted)" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <circle cx="11" cy="11" r="8" /><line x1="21" y1="21" x2="16.65" y2="16.65" />
          </svg>
          <input
            type="text"
            value={inputValue}
            onChange={(e) => setInputValue(e.target.value)}
            placeholder="e.g. family friendly whitefield, calm 3bhk near metro..."
            style={{
              flex: 1,
              border: "none",
              outline: "none",
              fontSize: "0.92rem",
              background: "transparent",
              color: "var(--color-text)",
            }}
          />
          <button
            type="submit"
            className="btn btn-primary"
            style={{ padding: "0.5rem 1.25rem", fontSize: "0.85rem", flexShrink: 0 }}
          >
            Search
          </button>
        </div>
      </form>

      {/* Interpreted intent */}
      <div style={{ marginBottom: "1rem" }}>
        <p style={{ fontSize: "0.82rem", color: "var(--color-text-muted)", margin: "0 0 0.5rem" }}>
          Understood as: <span style={{ color: "var(--color-text-secondary)", fontStyle: "italic" }}>&ldquo;{query_interpreted.intent}&rdquo;</span>
          {" in "}<span style={{ fontWeight: 600, color: "var(--color-text)" }}>{query_interpreted.area}</span>
        </p>
        <WeightChips weights={query_interpreted.weights_applied} />
      </div>

      {/* Area context bar */}
      {area_context && <AreaContextBar ctx={area_context} />}

      {/* Results header */}
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "baseline", marginBottom: "1rem" }}>
        <h1 style={{
          fontSize: "clamp(1.2rem, 1rem + 0.8vw, 1.6rem)",
          fontWeight: 600,
          letterSpacing: "-0.02em",
          margin: 0,
        }}>
          {results.length} {results.length === 1 ? "society" : "societies"} ranked
        </h1>
        <span style={{ fontSize: "0.72rem", color: "var(--color-text-muted)" }}>
          {enrichment_status.societies_discovered} discovered · {enrichment_status.reddit_enriched} Reddit-enriched
        </span>
      </div>

      {/* Society cards */}
      <div className="society-cards-grid">
        {results.map((r) => (
          <Link key={r.slug} to={`/society/${r.slug}`} style={{ textDecoration: "none", color: "inherit" }}>
            <SocietyCard result={r} />
          </Link>
        ))}
      </div>

      {/* Enrichment footer */}
      <div
        style={{
          marginTop: "2.5rem",
          padding: "1.25rem 1.5rem",
          background: "var(--color-bg-card, #fff)",
          border: "1px solid var(--color-border, #e5e5e5)",
          borderRadius: "8px",
          textAlign: "center",
          fontSize: "0.78rem",
          color: "var(--color-text-muted)",
          lineHeight: 1.6,
        }}
      >
        Rankings generated from {enrichment_status.reddit_enriched} Reddit-enriched societies,{" "}
        {enrichment_status.seed_matched} seed-matched, {enrichment_status.photos_available} with photos.
        <br />
        Scored at: {new Date(enrichment_status.scored_at).toLocaleDateString()}
      </div>
    </div>
  );
}
