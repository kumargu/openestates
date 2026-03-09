import { useEffect, useState, useRef } from "react";
import { useNavigate } from "react-router-dom";
import type { AreaListItem } from "../lib/types.ts";
import { getAreas } from "../lib/api.ts";

function useOnScreen(ref: React.RefObject<HTMLElement | null>) {
  const [visible, setVisible] = useState(false);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const obs = new IntersectionObserver(
      ([entry]) => { if (entry.isIntersecting) { setVisible(true); obs.disconnect(); } },
      { threshold: 0.15 }
    );
    obs.observe(el);
    return () => obs.disconnect();
  }, [ref]);
  return visible;
}

const ROTATING_WORDS = [
  "transparent homes",
  "honest pricing",
  "trusted societies",
  "clear tradeoffs",
  "real insights",
];

function RotatingText() {
  const [index, setIndex] = useState(0);
  const [fading, setFading] = useState(false);

  useEffect(() => {
    const interval = setInterval(() => {
      setFading(true);
      setTimeout(() => {
        setIndex((i) => (i + 1) % ROTATING_WORDS.length);
        setFading(false);
      }, 400);
    }, 3000);
    return () => clearInterval(interval);
  }, []);

  return (
    <span
      style={{
        display: "inline-block",
        transition: "opacity 0.4s ease, transform 0.4s ease",
        opacity: fading ? 0 : 1,
        transform: fading ? "translateY(8px)" : "translateY(0)",
        color: "#c96b4f",
      }}
    >
      {ROTATING_WORDS[index]}
    </span>
  );
}

export function HomePage() {
  const [areas, setAreas] = useState<AreaListItem[]>([]);
  const [query, setQuery] = useState("");
  const navigate = useNavigate();
  const areasRef = useRef<HTMLElement | null>(null);
  const featuresRef = useRef<HTMLElement | null>(null);
  const areasVisible = useOnScreen(areasRef);
  const featuresVisible = useOnScreen(featuresRef);

  useEffect(() => {
    getAreas()
      .then(setAreas)
      .catch(() => {});
  }, []);

  const handleSearch = (e: React.FormEvent) => {
    e.preventDefault();
    const q = query.trim();
    navigate(q ? `/results?q=${encodeURIComponent(q)}` : "/results");
  };

  return (
    <div>
      {/* Hero */}
      <section
        style={{
          minHeight: "100vh",
          display: "flex",
          flexDirection: "column",
          alignItems: "center",
          justifyContent: "center",
          padding: "0 clamp(1.5rem, 4vw, 4rem)",
          position: "relative",
          overflow: "hidden",
        }}
      >
        {/* Subtle gradient background */}
        <div
          style={{
            position: "absolute",
            inset: 0,
            background:
              "radial-gradient(ellipse 80% 60% at 50% 40%, rgba(201,107,79,0.06) 0%, transparent 70%), " +
              "radial-gradient(ellipse 60% 50% at 80% 20%, rgba(100,140,200,0.04) 0%, transparent 60%)",
            zIndex: -1,
          }}
        />

        <div className="fade-up" style={{ textAlign: "center", maxWidth: "720px" }}>
          <h1
            style={{
              fontSize: "clamp(2.5rem, 2rem + 3vw, 4.5rem)",
              fontWeight: 700,
              lineHeight: 1.1,
              letterSpacing: "-0.03em",
              margin: "0 0 1.5rem",
              color: "#1a1a1a",
            }}
          >
            Discover{" "}
            <RotatingText />
          </h1>
        </div>

        <p
          className="fade-up fade-up-delay-1"
          style={{
            fontSize: "clamp(1.05rem, 0.95rem + 0.5vw, 1.35rem)",
            color: "#666",
            maxWidth: "520px",
            textAlign: "center",
            margin: "0 0 3rem",
            lineHeight: 1.7,
          }}
        >
          Property discovery that explains why, not just what.
          Every listing comes with context you can trust.
        </p>

        {/* Search bar */}
        <p
          className="fade-up fade-up-delay-1"
          style={{
            fontSize: "0.82rem",
            color: "#999",
            textTransform: "uppercase",
            letterSpacing: "0.08em",
            margin: "0 0 0.75rem",
          }}
        >
          Describe what you're looking for
        </p>
        <form
          onSubmit={handleSearch}
          className="fade-up fade-up-delay-2 search-container"
          style={{
            width: "100%",
            maxWidth: "600px",
            display: "flex",
            alignItems: "center",
            gap: "0.75rem",
            padding: "1rem 1.5rem",
            borderRadius: "16px",
            backgroundColor: "#fff",
            border: "1px solid rgba(0,0,0,0.08)",
            boxShadow: "0 4px 24px rgba(0,0,0,0.06)",
          }}
        >
          <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="#999" strokeWidth="2" strokeLinecap="round">
            <circle cx="11" cy="11" r="8" />
            <line x1="21" y1="21" x2="16.65" y2="16.65" />
          </svg>
          <input
            className="search-input"
            type="text"
            placeholder="Try: 3BHK near metro in Whitefield under 1.5Cr"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
          <button
            type="submit"
            style={{
              border: "none",
              background: "#1a1a1a",
              color: "#fff",
              padding: "0.6rem 1.5rem",
              borderRadius: "10px",
              fontSize: "0.9rem",
              cursor: "pointer",
              whiteSpace: "nowrap",
              fontFamily: "inherit",
              transition: "background 0.2s ease",
            }}
            onMouseEnter={(e) => (e.currentTarget.style.background = "#333")}
            onMouseLeave={(e) => (e.currentTarget.style.background = "#1a1a1a")}
          >
            Search
          </button>
        </form>

        {/* Secondary action */}
        <div
          className="fade-up fade-up-delay-3"
          style={{ marginTop: "1.25rem", display: "flex", gap: "0.75rem", alignItems: "center" }}
        >
          <button
            onClick={() => navigate("/results")}
            style={{
              border: "1px solid rgba(0,0,0,0.12)",
              background: "transparent",
              color: "#555",
              padding: "0.55rem 1.25rem",
              borderRadius: "10px",
              fontSize: "0.85rem",
              cursor: "pointer",
              fontFamily: "inherit",
              transition: "all 0.2s ease",
            }}
            onMouseEnter={(e) => { e.currentTarget.style.background = "#1a1a1a"; e.currentTarget.style.color = "#fff"; }}
            onMouseLeave={(e) => { e.currentTarget.style.background = "transparent"; e.currentTarget.style.color = "#555"; }}
          >
            or browse all 20 listings
          </button>
        </div>

        {/* Scroll hint */}
        <div
          className="fade-up fade-up-delay-4"
          style={{
            position: "absolute",
            bottom: "2.5rem",
            display: "flex",
            flexDirection: "column",
            alignItems: "center",
            gap: "0.5rem",
          }}
        >
          <span style={{ fontSize: "0.75rem", color: "#bbb", letterSpacing: "0.1em", textTransform: "uppercase" }}>
            Explore
          </span>
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="#bbb" strokeWidth="2" strokeLinecap="round">
            <polyline points="6 9 12 15 18 9" />
          </svg>
        </div>
      </section>

      {/* Features section */}
      <section
        ref={featuresRef}
        style={{
          padding: "clamp(4rem, 8vw, 8rem) clamp(1.5rem, 4vw, 4rem)",
          backgroundColor: "var(--color-bg-soft)",
        }}
      >
        <div
          style={{
            maxWidth: "960px",
            margin: "0 auto",
            opacity: featuresVisible ? 1 : 0,
            transform: featuresVisible ? "translateY(0)" : "translateY(24px)",
            transition: "opacity 0.8s cubic-bezier(0.16, 1, 0.3, 1), transform 0.8s cubic-bezier(0.16, 1, 0.3, 1)",
          }}
        >
          <h2
            style={{
              fontSize: "clamp(1.5rem, 1.2rem + 1.5vw, 2.5rem)",
              fontWeight: 600,
              letterSpacing: "-0.02em",
              textAlign: "center",
              margin: "0 0 1rem",
            }}
          >
            Built on transparency
          </h2>
          <p
            style={{
              textAlign: "center",
              color: "#888",
              maxWidth: "480px",
              margin: "0 auto 3rem",
              fontSize: "1.05rem",
            }}
          >
            Every property shows the full picture, so you can compare with confidence.
          </p>
          <div
            className={featuresVisible ? "stagger-children" : ""}
            style={{
              display: "grid",
              gridTemplateColumns: "repeat(auto-fit, minmax(240px, 1fr))",
              gap: "1.5rem",
            }}
          >
            <FeatureCard
              title="Price context"
              description="See how every listing compares to the area median. No guessing if it's a fair price."
            />
            <FeatureCard
              title="Society insights"
              description="Maintenance quality, builder reputation, resident sentiment — all in one view."
            />
            <FeatureCard
              title="Area signals"
              description="Metro access, traffic, waterlogging, noise — real externalities that matter."
            />
          </div>

          {/* Transparency widget preview */}
          <div
            style={{
              marginTop: "3rem",
              padding: "2rem",
              borderRadius: "12px",
              backgroundColor: "#fff",
              border: "1px solid rgba(0,0,0,0.06)",
              maxWidth: "480px",
              marginLeft: "auto",
              marginRight: "auto",
            }}
          >
            <p style={{ fontSize: "0.7rem", textTransform: "uppercase", letterSpacing: "0.1em", color: "#999", margin: "0 0 1rem" }}>
              Example transparency widget
            </p>
            <div style={{ display: "flex", justifyContent: "space-between", alignItems: "baseline", marginBottom: "0.75rem" }}>
              <span style={{ fontSize: "0.9rem", color: "#444" }}>Listed price</span>
              <span style={{ fontSize: "1.1rem", fontWeight: 700 }}>8,200 /sqft</span>
            </div>
            <div style={{ display: "flex", justifyContent: "space-between", alignItems: "baseline", marginBottom: "0.75rem" }}>
              <span style={{ fontSize: "0.9rem", color: "#444" }}>Whitefield median</span>
              <span style={{ fontSize: "1.1rem", fontWeight: 600, color: "#888" }}>9,100 /sqft</span>
            </div>
            <div style={{ height: "6px", backgroundColor: "#eee", borderRadius: "3px", marginBottom: "0.5rem" }}>
              <div style={{ height: "6px", width: "90%", backgroundColor: "#2a7a2a", borderRadius: "3px" }} />
            </div>
            <p style={{ fontSize: "0.8rem", color: "#2a7a2a", margin: 0, fontWeight: 500 }}>
              10% below area median — good value signal
            </p>
          </div>
        </div>
      </section>

      {/* Areas section */}
      <section
        ref={areasRef}
        style={{
          padding: "clamp(4rem, 8vw, 8rem) clamp(1.5rem, 4vw, 4rem)",
        }}
      >
        <div
          style={{
            maxWidth: "960px",
            margin: "0 auto",
            opacity: areasVisible ? 1 : 0,
            transform: areasVisible ? "translateY(0)" : "translateY(24px)",
            transition: "opacity 0.8s cubic-bezier(0.16, 1, 0.3, 1), transform 0.8s cubic-bezier(0.16, 1, 0.3, 1)",
          }}
        >
          <h2
            style={{
              fontSize: "clamp(1.5rem, 1.2rem + 1.5vw, 2.5rem)",
              fontWeight: 600,
              letterSpacing: "-0.02em",
              margin: "0 0 0.75rem",
            }}
          >
            Explore Bengaluru
          </h2>
          <p style={{ color: "#888", marginBottom: "2.5rem", fontSize: "1.05rem" }}>
            Five micro-markets, each with a distinct character.
          </p>

          {areas.length > 0 && (
            <div
              className={areasVisible ? "stagger-children" : ""}
              style={{
                display: "grid",
                gridTemplateColumns: "repeat(auto-fill, minmax(260px, 1fr))",
                gap: "1.25rem",
              }}
            >
              {areas.map((area) => (
                <AreaCardLanding key={area.id} area={area} />
              ))}
            </div>
          )}
        </div>
      </section>

      {/* Footer CTA */}
      <section
        style={{
          padding: "clamp(4rem, 8vw, 7rem) clamp(1.5rem, 4vw, 4rem)",
          textAlign: "center",
          backgroundColor: "#1a1a1a",
          color: "#fff",
        }}
      >
        <h2
          style={{
            fontSize: "clamp(1.5rem, 1.2rem + 1.5vw, 2.5rem)",
            fontWeight: 600,
            letterSpacing: "-0.02em",
            margin: "0 0 1rem",
          }}
        >
          Ready to see the difference?
        </h2>
        <p style={{ color: "#999", marginBottom: "2rem", fontSize: "1.05rem" }}>
          Browse properties with full transparency. No hidden information.
        </p>
        <button
          onClick={() => navigate("/results")}
          style={{
            border: "1px solid rgba(255,255,255,0.2)",
            background: "transparent",
            color: "#fff",
            padding: "0.85rem 2.5rem",
            borderRadius: "12px",
            fontSize: "1rem",
            cursor: "pointer",
            fontFamily: "inherit",
            transition: "all 0.3s ease",
          }}
          onMouseEnter={(e) => {
            e.currentTarget.style.background = "#fff";
            e.currentTarget.style.color = "#1a1a1a";
          }}
          onMouseLeave={(e) => {
            e.currentTarget.style.background = "transparent";
            e.currentTarget.style.color = "#fff";
          }}
        >
          Browse properties
        </button>
      </section>
    </div>
  );
}

function FeatureCard({ title, description }: { title: string; description: string }) {
  return (
    <div
      className="feature-card"
      style={{
        padding: "2rem",
        borderRadius: "12px",
        backgroundColor: "#fff",
        border: "1px solid rgba(0,0,0,0.06)",
      }}
    >
      <h3
        style={{
          margin: "0 0 0.5rem",
          fontSize: "1.1rem",
          fontWeight: 600,
          letterSpacing: "-0.01em",
        }}
      >
        {title}
      </h3>
      <p style={{ margin: 0, color: "#777", fontSize: "0.95rem", lineHeight: 1.6 }}>
        {description}
      </p>
    </div>
  );
}

// Per-area concrete signals for the homepage cards
const AREA_SIGNALS: Record<string, { signals: string[]; vibe: string }> = {
  Whitefield: {
    signals: ["Metro now live", "IT corridor", "Varthur lake risk"],
    vibe: "Tech hub with new metro access",
  },
  "Sarjapur Road": {
    signals: ["No metro yet", "Heavy traffic", "Rapid growth"],
    vibe: "Fast-growing IT corridor, congestion trade-off",
  },
  Bellandur: {
    signals: ["Premium zone", "Lake pollution", "ORR traffic"],
    vibe: "Premium but lake concerns persist",
  },
  "HSR Layout": {
    signals: ["Startup hub", "Walkable", "Stable prices"],
    vibe: "Walkable neighbourhood, strong livability",
  },
  "Electronic City": {
    signals: ["Value pricing", "Elevated expressway", "Growing infra"],
    vibe: "Affordable tech zone with improving connectivity",
  },
};

function AreaCardLanding({ area }: { area: AreaListItem }) {
  const trendColor = area.trend_direction === "up" ? "var(--color-positive)" : "var(--color-text-muted)";
  const trendIcon = area.trend_direction === "up" ? "\u2197" : "\u2192";
  const extra = AREA_SIGNALS[area.name];

  return (
    <div
      className="area-card"
      style={{
        padding: "1.75rem",
        borderRadius: "12px",
        backgroundColor: "#fff",
        border: "1px solid rgba(0,0,0,0.06)",
      }}
    >
      <h3 style={{ margin: "0 0 0.35rem", fontSize: "1.15rem", fontWeight: 600, letterSpacing: "-0.01em" }}>
        {area.name}
      </h3>
      {extra && (
        <p style={{ margin: "0 0 0.75rem", fontSize: "0.82rem", color: "var(--color-text-muted)", lineHeight: 1.4 }}>
          {extra.vibe}
        </p>
      )}
      <div style={{ display: "flex", alignItems: "baseline", gap: "0.5rem", marginBottom: "0.75rem" }}>
        <span style={{ fontSize: "1.3rem", fontWeight: 700 }}>
          {area.median_price_per_sqft.toLocaleString("en-IN")}
        </span>
        <span style={{ color: "#999", fontSize: "0.85rem" }}>/sqft</span>
        <span style={{ color: trendColor, fontSize: "0.85rem", fontWeight: 500 }}>
          {trendIcon} {area.trend_direction}
        </span>
      </div>
      {extra && (
        <div style={{ display: "flex", gap: "0.35rem", flexWrap: "wrap" }}>
          {extra.signals.map((s) => (
            <span key={s} className="tag tag-neutral" style={{ fontSize: "0.72rem" }}>
              {s}
            </span>
          ))}
        </div>
      )}
      {!extra && area.primary_signal && (
        <span className="tag tag-neutral">
          {area.primary_signal.replace(/_/g, " ")}
        </span>
      )}
    </div>
  );
}
