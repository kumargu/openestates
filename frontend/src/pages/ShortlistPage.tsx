import { useEffect, useState, useCallback } from "react";
import { useNavigate, Link } from "react-router-dom";
import { Helmet } from "react-helmet-async";
import type { PropertyCard as PropertyCardType, PropertyDetailResponse, CompareThemes, ThemeLabel } from "../lib/types.ts";
import { getProperties, getProperty } from "../lib/api.ts";
import { PageState } from "../components/PageState.tsx";
import { PropertyCard } from "../components/PropertyCard.tsx";
import { getShortlistedIds, toggleShortlist } from "../lib/shortlist-store.ts";

function formatPrice(price: number): string {
  if (price >= 10_000_000) return `${(price / 10_000_000).toFixed(1)} Cr`;
  if (price >= 100_000) return `${(price / 100_000).toFixed(1)} L`;
  return price.toLocaleString("en-IN");
}

type DetailedProperty = {
  card: PropertyCardType;
  detail: PropertyDetailResponse;
  themes: CompareThemes;
};

export function ShortlistPage() {
  const [allProperties, setAllProperties] = useState<PropertyCardType[]>([]);
  const [savedIds, setSavedIds] = useState<string[]>(() => getShortlistedIds());
  const [loaded, setLoaded] = useState(false);
  const [detailedProps, setDetailedProps] = useState<DetailedProperty[]>([]);
  const [detailsLoaded, setDetailsLoaded] = useState(false);
  const navigate = useNavigate();

  const [loadError, setLoadError] = useState(false);

  useEffect(() => {
    getProperties()
      .then((data) => {
        setAllProperties(data);
        setLoaded(true);
      })
      .catch(() => {
        setLoadError(true);
        setLoaded(true);
      });
  }, []);

  // Load full details for saved properties (needed for theme comparison)
  useEffect(() => {
    if (!loaded || savedIds.length === 0) {
      setDetailedProps([]);
      setDetailsLoaded(true);
      return;
    }
    const savedCards = allProperties.filter((p) => savedIds.includes(p.id));
    if (savedCards.length === 0) {
      setDetailsLoaded(true);
      return;
    }

    Promise.all(savedCards.map((card) =>
      getProperty(card.id).then((detail) => {
        return { card, detail, themes: detail.themes } as DetailedProperty;
      }).catch(() => null)
    )).then((results) => {
      setDetailedProps(results.filter(Boolean) as DetailedProperty[]);
      setDetailsLoaded(true);
    });
  }, [loaded, savedIds, allProperties]);

  const refreshIds = useCallback(() => {
    setSavedIds(getShortlistedIds());
  }, []);

  const handleRemove = (id: string) => {
    toggleShortlist(id);
    refreshIds();
  };

  const savedProperties = allProperties.filter((p) => savedIds.includes(p.id));

  if (!loaded) {
    return (
      <div className="page-container" style={{ textAlign: "center", padding: "4rem 2rem", color: "var(--color-text-muted)" }}>
        Loading...
      </div>
    );
  }

  if (loadError) {
    return (
      <div className="page-container">
        <PageState variant="error" context="shortlist" />
      </div>
    );
  }

  // Empty state
  if (savedProperties.length === 0) {
    return (
      <div className="shortlist-empty-state">
        <h1 style={{ fontSize: "1.5rem", marginBottom: "1rem" }}>Compare saved homes</h1>
        <div data-testid="compare-empty-state" style={{
          padding: "2.5rem 2rem",
          borderRadius: "var(--radius-md)",
          backgroundColor: "var(--color-bg-card)",
          border: "1px solid var(--color-border)",
        }}>
          <h2 style={{ fontSize: "1.2rem", marginBottom: "0.75rem", color: "var(--color-text)" }}>
            Your shortlist is empty
          </h2>
          <p style={{ color: "var(--color-text-secondary)", lineHeight: 1.6, marginBottom: "1rem", fontSize: "0.95rem" }}>
            Save a few homes to compare value, commute, openness, and risk side by side.
          </p>
          <p style={{ color: "var(--color-text-muted)", fontSize: "0.82rem", margin: "0 0 1.5rem", lineHeight: 1.5 }}>
            Saved homes stay on this browser for now.
          </p>
          <div style={{ display: "flex", gap: "0.75rem", justifyContent: "center", flexWrap: "wrap" }}>
            {["Compare value", "Review tradeoffs", "Decide clearly"].map((item) => (
              <span key={item} className="tag tag-positive">{item}</span>
            ))}
          </div>
          <button className="btn btn-primary" onClick={() => navigate("/results")} style={{ marginTop: "1.5rem" }}>
            Browse properties
          </button>
        </div>
      </div>
    );
  }

  const bestFor = (() => {
    if (detailedProps.length < 2) return [];
    const themeOrder: ThemeLabel[] = ["strong", "good", "mixed", "weak"];
    const themeKeys: { key: keyof CompareThemes; display: string }[] = [
      { key: "value", display: "Best for value" },
      { key: "commute", display: "Best for commute" },
      { key: "greenery", display: "Best for greenery" },
      { key: "society", display: "Best for society quality" },
      { key: "resale", display: "Best for resale" },
    ];
    const results: { theme: string; propertyTitle: string; propertyId: string }[] = [];
    for (const { key, display } of themeKeys) {
      let best = detailedProps[0];
      for (const dp of detailedProps.slice(1)) {
        if (themeOrder.indexOf(dp.themes[key].label) < themeOrder.indexOf(best.themes[key].label)) {
          best = dp;
        }
      }
      if (best.themes[key].label === "strong" || best.themes[key].label === "good") {
        results.push({ theme: display, propertyTitle: best.card.title, propertyId: best.card.id });
      }
    }
    return results;
  })();

  return (
    <div className="page-container-wide" data-testid="shortlist-page">
      <Helmet>
        <title>Compare Saved Homes | OpenEstates</title>
        <meta name="description" content="Compare your shortlisted properties side by side — value, commute, society quality, and risk signals." />
        <meta property="og:title" content="Compare Saved Homes | OpenEstates" />
        <meta property="og:description" content="Compare your shortlisted properties side by side — value, commute, society quality, and risk signals." />
        <meta property="og:type" content="website" />
        <meta property="og:site_name" content="OpenEstates" />
      </Helmet>
      <div className="page-header">
        <h1>Compare saved homes</h1>
        <p>{savedProperties.length} saved {savedProperties.length === 1 ? "property" : "properties"} for comparison</p>
        <p style={{ fontSize: "0.82rem", color: "var(--color-text-muted)", margin: "0.25rem 0 0" }}>
          Saved homes stay on this browser for now.
        </p>
      </div>

      {/* === Layer C: Best For Summary === */}
      {bestFor.length > 0 && (
        <div className="section-card" data-testid="best-for-section" style={{ marginBottom: "1.5rem" }}>
          <div className="section-card-header">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="var(--color-text-muted)" strokeWidth="2" strokeLinecap="round">
              <polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2" />
            </svg>
            <h2>Best for</h2>
          </div>
          <div style={{ display: "flex", gap: "0.75rem", flexWrap: "wrap" }}>
            {bestFor.map((b) => (
              <Link
                key={b.theme}
                to={`/property/${b.propertyId}`}
                style={{
                  textDecoration: "none",
                  padding: "0.6rem 1rem",
                  borderRadius: "var(--radius-sm)",
                  backgroundColor: "var(--color-positive-bg)",
                  border: "1px solid var(--color-positive-border)",
                  display: "flex",
                  gap: "0.5rem",
                  alignItems: "center",
                  transition: "transform 0.2s ease",
                }}
              >
                <span style={{ fontSize: "0.75rem", fontWeight: 600, color: "var(--color-positive)", textTransform: "uppercase", letterSpacing: "0.04em" }}>
                  {b.theme}
                </span>
                <span style={{ fontSize: "0.8rem", color: "var(--color-text)" }}>
                  {b.propertyTitle.length > 30 ? b.propertyTitle.slice(0, 30) + "..." : b.propertyTitle}
                </span>
              </Link>
            ))}
          </div>
        </div>
      )}

      {/* === Layer A: Quick Compare === */}
      <div className="section-card mobile-scroll-x" data-testid="quick-compare-section" style={{ marginBottom: "1.5rem" }}>
        <div className="section-card-header">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="var(--color-text-muted)" strokeWidth="2" strokeLinecap="round">
            <line x1="18" y1="20" x2="18" y2="10" />
            <line x1="12" y1="20" x2="12" y2="4" />
            <line x1="6" y1="20" x2="6" y2="14" />
          </svg>
          <h2>Quick compare</h2>
        </div>
        <table style={{ width: "100%", borderCollapse: "collapse", fontSize: "0.85rem", minWidth: "500px" }}>
          <thead>
            <tr style={{ borderBottom: "1px solid var(--color-border)" }}>
              <th style={thStyle}></th>
              {savedProperties.map((p) => (
                <th key={p.id} style={{ ...thStyle, fontWeight: 600, maxWidth: "180px" }}>
                  <Link to={`/property/${p.id}`} style={{ color: "var(--color-text)", textDecoration: "none" }}>
                    {p.title.length > 28 ? p.title.slice(0, 28) + "..." : p.title}
                  </Link>
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            <CompareRow label="Price" values={savedProperties.map((p) => formatPrice(p.price))} />
            <CompareRow label="Price/sqft" values={savedProperties.map((p) => `\u20B9${p.price_per_sqft.toLocaleString("en-IN")}`)} highlight="lowest" numValues={savedProperties.map((p) => p.price_per_sqft)} />
            <CompareRow label="Config" values={savedProperties.map((p) => `${p.bhk} BHK`)} />
            <CompareRow label="Size" values={savedProperties.map((p) => `${p.sqft} sqft`)} />
            <CompareRow label="Area" values={savedProperties.map((p) => p.area)} />
            <CompareRow label="Society" values={savedProperties.map((p) => p.society_name)} />
            <tr style={{ borderBottom: "1px solid var(--color-border)" }}>
              <td style={{ ...thStyle, fontWeight: 600, color: "var(--color-text-secondary)" }}>Actions</td>
              {savedProperties.map((p) => (
                <td key={p.id} style={{ padding: "0.5rem 0.75rem" }}>
                  <button
                    onClick={() => handleRemove(p.id)}
                    style={{
                      background: "none",
                      border: "none",
                      color: "var(--color-negative)",
                      fontSize: "0.75rem",
                      cursor: "pointer",
                      padding: "0.2rem 0.4rem",
                      fontFamily: "inherit",
                    }}
                  >
                    Remove
                  </button>
                </td>
              ))}
            </tr>
          </tbody>
        </table>
      </div>

      {/* === Layer B: Decision Themes === */}
      {detailsLoaded && detailedProps.length >= 2 && (
        <div className="section-card mobile-scroll-x" data-testid="decision-themes-section" style={{ marginBottom: "1.5rem" }}>
          <div className="section-card-header">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="var(--color-text-muted)" strokeWidth="2" strokeLinecap="round">
              <circle cx="12" cy="12" r="10" />
              <path d="M12 16v-4" />
              <path d="M12 8h.01" />
            </svg>
            <h2>Decision themes</h2>
          </div>
          <p style={{ margin: "0 0 1.25rem", fontSize: "0.85rem", color: "var(--color-text-secondary)" }}>
            How each property compares across the themes that matter most.
          </p>
          <table style={{ width: "100%", borderCollapse: "collapse", fontSize: "0.85rem", minWidth: "500px" }}>
            <thead>
              <tr style={{ borderBottom: "1px solid var(--color-border)" }}>
                <th style={thStyle}>Theme</th>
                {detailedProps.map((dp) => (
                  <th key={dp.card.id} style={{ ...thStyle, fontWeight: 600, maxWidth: "200px" }}>
                    {dp.card.title.length > 28 ? dp.card.title.slice(0, 28) + "..." : dp.card.title}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              <ThemeRow theme="Value" props={detailedProps} themeKey="value" />
              <ThemeRow theme="Commute & Access" props={detailedProps} themeKey="commute" />
              <ThemeRow theme="Society Quality" props={detailedProps} themeKey="society" />
              <ThemeRow theme="Greenery & Open Space" props={detailedProps} themeKey="greenery" />
              <ThemeRow theme="Risk Signals" props={detailedProps} themeKey="risk" />
              <ThemeRow theme="Resale Strength" props={detailedProps} themeKey="resale" />
              <ThemeRow theme="Market Activity" props={detailedProps} themeKey="market" />
            </tbody>
          </table>
        </div>
      )}

      {/* Property cards grid */}
      <div style={{ marginTop: "1rem" }}>
        <h2 style={{
          fontSize: "0.7rem",
          fontWeight: 600,
          textTransform: "uppercase",
          letterSpacing: "0.08em",
          color: "var(--color-text-muted)",
          marginBottom: "1rem",
        }}>
          Saved properties
        </h2>
        <div className="shortlist-cards-grid">
          {savedProperties.map((p) => (
            <PropertyCard key={p.id} property={p} onShortlistChange={refreshIds} />
          ))}
        </div>
      </div>
    </div>
  );
}

const thStyle: React.CSSProperties = {
  textAlign: "left",
  padding: "0.6rem 0.75rem",
  fontSize: "0.75rem",
  color: "var(--color-text-muted)",
  textTransform: "uppercase",
  letterSpacing: "0.06em",
  fontWeight: 500,
};

function CompareRow({
  label,
  values,
  highlight,
  numValues,
}: {
  label: string;
  values: string[];
  highlight?: "lowest";
  numValues?: number[];
}) {
  const minIdx = highlight === "lowest" && numValues
    ? numValues.indexOf(Math.min(...numValues))
    : -1;

  return (
    <tr style={{ borderBottom: "1px solid var(--color-border)" }}>
      <td style={{ ...thStyle, fontWeight: 600, color: "var(--color-text-secondary)" }}>{label}</td>
      {values.map((v, i) => (
        <td key={i} style={{
          padding: "0.6rem 0.75rem",
          fontSize: "0.85rem",
          color: i === minIdx ? "var(--color-positive)" : "var(--color-text)",
          fontWeight: i === minIdx ? 600 : 400,
        }}>
          {v}
        </td>
      ))}
    </tr>
  );
}

function ThemeBadgeSmall({ level }: { level: ThemeLabel }) {
  const config: Record<ThemeLabel, { bg: string; color: string; border: string }> = {
    strong: { bg: "var(--color-positive-bg)", color: "var(--color-positive)", border: "var(--color-positive-border)" },
    good: { bg: "#f0f5f0", color: "#3a8a3a", border: "#c0d8c0" },
    mixed: { bg: "#fdf8f0", color: "var(--color-warning)", border: "#e8dcc0" },
    weak: { bg: "var(--color-negative-bg)", color: "var(--color-negative)", border: "var(--color-negative-border)" },
  };
  const c = config[level];
  return (
    <span style={{
      display: "inline-block",
      fontSize: "0.65rem",
      fontWeight: 600,
      padding: "0.1rem 0.4rem",
      borderRadius: "var(--radius-xl)",
      backgroundColor: c.bg,
      color: c.color,
      border: `1px solid ${c.border}`,
      textTransform: "capitalize",
      marginBottom: "0.25rem",
    }}>
      {level}
    </span>
  );
}

function ThemeRow({
  theme,
  props,
  themeKey,
}: {
  theme: string;
  props: DetailedProperty[];
  themeKey: keyof CompareThemes;
}) {
  // Find the best label among all properties for this theme
  const themeOrder: ThemeLabel[] = ["strong", "good", "mixed", "weak"];
  let bestIdx = 0;
  for (let i = 1; i < props.length; i++) {
    if (themeOrder.indexOf(props[i].themes[themeKey].label) < themeOrder.indexOf(props[bestIdx].themes[themeKey].label)) {
      bestIdx = i;
    }
  }

  return (
    <tr style={{ borderBottom: "1px solid var(--color-border)" }}>
      <td style={{ ...thStyle, fontWeight: 600, color: "var(--color-text-secondary)", verticalAlign: "top", paddingTop: "0.75rem" }}>
        {theme}
      </td>
      {props.map((dp, i) => {
        const result = dp.themes[themeKey];
        const isBest = i === bestIdx && props.length > 1;
        return (
          <td key={dp.card.id} style={{
            padding: "0.6rem 0.75rem",
            verticalAlign: "top",
            backgroundColor: isBest ? "rgba(42, 122, 42, 0.03)" : "transparent",
          }}>
            <div>
              <ThemeBadgeSmall level={result.label} />
              <p style={{
                margin: 0,
                fontSize: "0.8rem",
                color: "var(--color-text-secondary)",
                lineHeight: 1.4,
              }}>
                {result.summary}
              </p>
            </div>
          </td>
        );
      })}
    </tr>
  );
}
