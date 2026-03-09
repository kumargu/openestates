import { useEffect, useState, useMemo } from "react";
import { useSearchParams, useNavigate } from "react-router-dom";
import type { PropertyCard as PropertyCardType } from "../lib/types.ts";
import { getProperties } from "../lib/api.ts";
import { PageState } from "../components/PageState.tsx";
import { PropertyCard } from "../components/PropertyCard.tsx";
import { SAMPLE_PROPERTIES } from "../lib/sample-data.ts";
import { parseSearch, filterProperties, computeMatch, formatSearchSummary } from "../lib/search.ts";

export function ResultsPage() {
  const [properties, setProperties] = useState<PropertyCardType[]>([]);
  const [status, setStatus] = useState<"loading" | "error" | "ok">("loading");
  const [searchParams] = useSearchParams();
  const navigate = useNavigate();
  const query = searchParams.get("q") || "";

  useEffect(() => {
    getProperties()
      .then((data) => {
        setProperties(data);
        setStatus("ok");
      })
      .catch(() => setStatus("error"));
  }, []);

  const intent = useMemo(() => parseSearch(query), [query]);

  const showSample = status === "error";
  const allItems = showSample ? SAMPLE_PROPERTIES : properties;

  // Apply filters from search
  const filtered = useMemo(() => {
    if (!query) return allItems;
    const result = filterProperties(allItems, intent);
    return result.length > 0 ? result : allItems; // fallback to all if no matches
  }, [allItems, intent, query]);

  // Compute matches
  const matchResults = useMemo(() => {
    return filtered.map((p) => ({
      property: p,
      match: computeMatch(p, intent),
    }));
  }, [filtered, intent]);

  if (status === "loading") return <PageState variant="loading" context="results" />;

  const summary = formatSearchSummary(intent);
  const hasSearchChips = intent.area || intent.bhk || intent.budgetMax || intent.preferences.length > 0;

  return (
    <div className="page-container">
      <div className="page-header">
        <h1>Properties</h1>

        {/* Intentional product-owned fallback banner when backend is unavailable */}
        {showSample && (
          <div style={{
            margin: "1rem 0",
            padding: "1.25rem 1.5rem",
            borderRadius: "var(--radius-md)",
            backgroundColor: "var(--color-bg-card)",
            border: "1px solid var(--color-border)",
            textAlign: "center",
          }}>
            <h2 style={{
              fontSize: "1.15rem",
              fontWeight: 600,
              color: "var(--color-text)",
              margin: "0 0 0.5rem",
            }}>
              Results temporarily unavailable
            </h2>
            <p style={{
              fontSize: "0.9rem",
              color: "var(--color-text-secondary)",
              margin: "0 0 1rem",
              lineHeight: 1.6,
            }}>
              We couldn't load live property data right now, but you can still continue exploring Bengaluru areas.
            </p>
            <div style={{ display: "flex", gap: "0.75rem", justifyContent: "center", flexWrap: "wrap" }}>
              <button
                className="btn btn-outline"
                onClick={() => navigate("/")}
                style={{ fontSize: "0.85rem" }}
              >
                Browse areas
              </button>
              <button
                className="btn btn-outline"
                onClick={() => navigate("/")}
                style={{ fontSize: "0.85rem" }}
              >
                Return home
              </button>
            </div>
          </div>
        )}

        {/* Search interpretation */}
        {query && (
          <div style={{ marginTop: "0.5rem" }}>
            <p style={{
              color: "var(--color-text-secondary)",
              fontSize: "0.85rem",
              margin: "0 0 0.5rem",
              fontStyle: "italic",
            }}>
              &ldquo;{query}&rdquo;
            </p>
            {hasSearchChips && (
              <div style={{ display: "flex", gap: "0.35rem", flexWrap: "wrap", marginBottom: "0.5rem" }}>
                {intent.area && <span className="tag tag-neutral">{intent.area}</span>}
                {intent.bhk && <span className="tag tag-neutral">{intent.bhk} BHK</span>}
                {intent.budgetMax && (
                  <span className="tag tag-neutral">
                    under {intent.budgetMax >= 10_000_000
                      ? `${(intent.budgetMax / 10_000_000).toFixed(1)} Cr`
                      : `${(intent.budgetMax / 100_000).toFixed(0)}L`}
                  </span>
                )}
                {intent.preferences.map((pref) => (
                  <span key={pref} className="tag tag-neutral">{pref}</span>
                ))}
              </div>
            )}
            <p style={{ color: "var(--color-text-muted)", fontSize: "0.8rem", margin: 0 }}>
              {summary}. Showing {filtered.length} {filtered.length === 1 ? "property" : "properties"}.
            </p>
          </div>
        )}

        {!query && !showSample && (
          <p>{filtered.length} listings with full transparency reports</p>
        )}
      </div>

      <div
        style={{
          display: "grid",
          gridTemplateColumns: "repeat(auto-fill, minmax(280px, 1fr))",
          gap: "1.25rem",
          opacity: showSample ? 0.8 : 1,
        }}
      >
        {matchResults.map(({ property, match }) => (
          <PropertyCard key={property.id} property={property} match={match} />
        ))}
      </div>
    </div>
  );
}
