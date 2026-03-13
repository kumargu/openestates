import { useEffect, useState, useRef } from "react";
import { useNavigate } from "react-router-dom";

function useOnScreen(ref: React.RefObject<HTMLElement | null>) {
  const [visible, setVisible] = useState(false);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const obs = new IntersectionObserver(
      ([entry]) => {
        if (entry.isIntersecting) {
          setVisible(true);
          obs.disconnect();
        }
      },
      { threshold: 0.15 }
    );
    obs.observe(el);
    return () => obs.disconnect();
  }, [ref]);
  return visible;
}

const ROTATING_WORDS = [
  "transparent pricing",
  "verified documents",
  "genuine buyers",
  "honest rankings",
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

const VALUE_PROPS = [
  {
    icon: (
      <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="#c96b4f" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
        <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z" />
        <path d="M9 12l2 2 4-4" />
      </svg>
    ),
    title: "Transparency that sells",
    description:
      "Buyers see your documents, verification status, and honest pricing. Complete profiles rank higher in search results.",
  },
  {
    icon: (
      <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="#c96b4f" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
        <path d="M17 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2" />
        <circle cx="9" cy="7" r="4" />
        <path d="M23 21v-2a4 4 0 0 0-3-3.87" />
        <path d="M16 3.13a4 4 0 0 1 0 7.75" />
      </svg>
    ),
    title: "Reach serious buyers",
    description:
      "Every buyer who clicks 'I'm Interested' is actively searching. No brokers, no spam calls.",
  },
  {
    icon: (
      <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="#c96b4f" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
        <rect x="3" y="3" width="18" height="18" rx="2" ry="2" />
        <line x1="9" y1="3" x2="9" y2="21" />
        <line x1="3" y1="9" x2="21" y2="9" />
      </svg>
    ),
    title: "Your listing, your control",
    description:
      "7-step registration at your pace. Skip steps, come back later. Publish when ready.",
  },
];

const HOW_IT_WORKS = [
  {
    step: 1,
    title: "Register in 5 minutes",
    description: "Basic info + property description",
  },
  {
    step: 2,
    title: "Add details at your pace",
    description: "Pricing, documents, photos (all optional, improve ranking)",
  },
  {
    step: 3,
    title: "Get discovered",
    description: "Property appears in buyer searches with trust indicators",
  },
  {
    step: 4,
    title: "Track interest",
    description: "See who's interested on your seller dashboard",
  },
];

export function SellerLandingPage() {
  const navigate = useNavigate();
  const valueRef = useRef<HTMLElement | null>(null);
  const howRef = useRef<HTMLElement | null>(null);
  const trustRef = useRef<HTMLElement | null>(null);
  const bottomRef = useRef<HTMLElement | null>(null);

  const valueVisible = useOnScreen(valueRef);
  const howVisible = useOnScreen(howRef);
  const trustVisible = useOnScreen(trustRef);
  const bottomVisible = useOnScreen(bottomRef);

  return (
    <div>
      {/* Hero Section */}
      <section
        style={{
          minHeight: "85vh",
          display: "flex",
          flexDirection: "column",
          alignItems: "center",
          justifyContent: "center",
          padding: "0 clamp(1.5rem, 4vw, 4rem)",
          position: "relative",
          overflow: "hidden",
        }}
      >
        <div
          style={{
            position: "absolute",
            inset: 0,
            background:
              "radial-gradient(ellipse 80% 60% at 50% 40%, rgba(201,107,79,0.08) 0%, transparent 70%), " +
              "radial-gradient(ellipse 60% 50% at 20% 70%, rgba(100,140,200,0.04) 0%, transparent 60%)",
            zIndex: -1,
          }}
        />

        <div className="fade-up" style={{ textAlign: "center", maxWidth: "720px" }}>
          <h1
            style={{
              fontSize: "clamp(2.2rem, 1.8rem + 2.5vw, 4rem)",
              fontWeight: 700,
              lineHeight: 1.1,
              letterSpacing: "-0.03em",
              margin: "0 0 1.5rem",
              color: "#1a1a1a",
            }}
          >
            List your property where buyers trust the data
          </h1>
        </div>

        <p
          className="fade-up fade-up-delay-1"
          style={{
            fontSize: "clamp(1.05rem, 0.95rem + 0.5vw, 1.35rem)",
            color: "#666",
            maxWidth: "520px",
            textAlign: "center",
            margin: "0 0 1rem",
            lineHeight: 1.7,
          }}
        >
          Sell with <RotatingText />
        </p>

        <p
          className="fade-up fade-up-delay-2"
          style={{
            fontSize: "0.9rem",
            color: "#999",
            margin: "0 0 2.5rem",
            textAlign: "center",
          }}
        >
          Free to list. No hidden charges.
        </p>

        <button
          className="fade-up fade-up-delay-2"
          onClick={() => navigate("/register")}
          style={{
            border: "none",
            background: "#c96b4f",
            color: "#fff",
            padding: "0.85rem 2.5rem",
            borderRadius: "12px",
            fontSize: "1.05rem",
            fontWeight: 600,
            cursor: "pointer",
            fontFamily: "inherit",
            transition: "background 0.2s ease, transform 0.15s ease",
            boxShadow: "0 4px 16px rgba(201,107,79,0.25)",
          }}
          onMouseEnter={(e) => {
            e.currentTarget.style.background = "#b55d44";
            e.currentTarget.style.transform = "translateY(-1px)";
          }}
          onMouseLeave={(e) => {
            e.currentTarget.style.background = "#c96b4f";
            e.currentTarget.style.transform = "translateY(0)";
          }}
        >
          List Your Property
        </button>
      </section>

      {/* Value Propositions */}
      <section
        ref={valueRef}
        style={{
          padding: "clamp(3rem, 6vw, 5rem) clamp(1.5rem, 4vw, 4rem)",
          backgroundColor: "#fff",
          borderTop: "1px solid rgba(0,0,0,0.04)",
        }}
      >
        <div
          style={{
            maxWidth: "960px",
            margin: "0 auto",
            opacity: valueVisible ? 1 : 0,
            transform: valueVisible ? "translateY(0)" : "translateY(24px)",
            transition:
              "opacity 0.8s cubic-bezier(0.16, 1, 0.3, 1), transform 0.8s cubic-bezier(0.16, 1, 0.3, 1)",
          }}
        >
          <h2
            style={{
              fontSize: "clamp(1.3rem, 1rem + 1.2vw, 2rem)",
              fontWeight: 600,
              letterSpacing: "-0.02em",
              margin: "0 0 0.5rem",
              textAlign: "center",
            }}
          >
            Why list on OpenEstates?
          </h2>
          <p
            style={{
              color: "#888",
              marginBottom: "2.5rem",
              fontSize: "0.95rem",
              textAlign: "center",
            }}
          >
            Transparency is not just our promise to buyers — it works for sellers too
          </p>

          <div
            style={{
              display: "grid",
              gridTemplateColumns: "repeat(auto-fit, minmax(260px, 1fr))",
              gap: "1.5rem",
            }}
          >
            {VALUE_PROPS.map((vp, i) => (
              <div
                key={vp.title}
                style={{
                  padding: "2rem 1.5rem",
                  borderRadius: "14px",
                  backgroundColor: "#fdf9f7",
                  border: "1px solid rgba(201,107,79,0.08)",
                  opacity: valueVisible ? 1 : 0,
                  transform: valueVisible ? "translateY(0)" : "translateY(16px)",
                  transition: `opacity 0.6s cubic-bezier(0.16, 1, 0.3, 1) ${i * 0.15}s, transform 0.6s cubic-bezier(0.16, 1, 0.3, 1) ${i * 0.15}s`,
                }}
              >
                <div style={{ marginBottom: "1rem" }}>{vp.icon}</div>
                <h3
                  style={{
                    fontSize: "1.05rem",
                    fontWeight: 600,
                    margin: "0 0 0.5rem",
                    color: "#1a1a1a",
                  }}
                >
                  {vp.title}
                </h3>
                <p
                  style={{
                    fontSize: "0.88rem",
                    color: "#777",
                    lineHeight: 1.6,
                    margin: 0,
                  }}
                >
                  {vp.description}
                </p>
              </div>
            ))}
          </div>
        </div>
      </section>

      {/* How It Works */}
      <section
        ref={howRef}
        style={{
          padding: "clamp(3rem, 6vw, 5rem) clamp(1.5rem, 4vw, 4rem)",
          backgroundColor: "var(--color-bg-soft)",
        }}
      >
        <div
          style={{
            maxWidth: "720px",
            margin: "0 auto",
            opacity: howVisible ? 1 : 0,
            transform: howVisible ? "translateY(0)" : "translateY(24px)",
            transition:
              "opacity 0.8s cubic-bezier(0.16, 1, 0.3, 1), transform 0.8s cubic-bezier(0.16, 1, 0.3, 1)",
          }}
        >
          <h2
            style={{
              fontSize: "clamp(1.3rem, 1rem + 1.2vw, 2rem)",
              fontWeight: 600,
              letterSpacing: "-0.02em",
              margin: "0 0 0.5rem",
              textAlign: "center",
            }}
          >
            How it works
          </h2>
          <p
            style={{
              color: "#888",
              marginBottom: "2.5rem",
              fontSize: "0.95rem",
              textAlign: "center",
            }}
          >
            From registration to your first interested buyer
          </p>

          <div style={{ display: "flex", flexDirection: "column", gap: "1.25rem" }}>
            {HOW_IT_WORKS.map((step, i) => (
              <div
                key={step.step}
                style={{
                  display: "flex",
                  alignItems: "flex-start",
                  gap: "1.25rem",
                  padding: "1.25rem 1.5rem",
                  borderRadius: "12px",
                  backgroundColor: "#fff",
                  border: "1px solid rgba(0,0,0,0.06)",
                  opacity: howVisible ? 1 : 0,
                  transform: howVisible ? "translateX(0)" : "translateX(-16px)",
                  transition: `opacity 0.5s cubic-bezier(0.16, 1, 0.3, 1) ${i * 0.12}s, transform 0.5s cubic-bezier(0.16, 1, 0.3, 1) ${i * 0.12}s`,
                }}
              >
                <div
                  style={{
                    width: "36px",
                    height: "36px",
                    borderRadius: "50%",
                    backgroundColor: "rgba(201,107,79,0.1)",
                    color: "#c96b4f",
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "center",
                    fontWeight: 700,
                    fontSize: "0.95rem",
                    flexShrink: 0,
                  }}
                >
                  {step.step}
                </div>
                <div>
                  <h3
                    style={{
                      fontSize: "0.98rem",
                      fontWeight: 600,
                      margin: "0 0 0.25rem",
                      color: "#1a1a1a",
                    }}
                  >
                    {step.title}
                  </h3>
                  <p style={{ fontSize: "0.85rem", color: "#888", margin: 0, lineHeight: 1.5 }}>
                    {step.description}
                  </p>
                </div>
              </div>
            ))}
          </div>
        </div>
      </section>

      {/* Trust Bar */}
      <section
        ref={trustRef}
        style={{
          padding: "2.5rem clamp(1.5rem, 4vw, 4rem)",
          backgroundColor: "#fff",
          borderTop: "1px solid rgba(0,0,0,0.04)",
          borderBottom: "1px solid rgba(0,0,0,0.04)",
        }}
      >
        <div
          style={{
            maxWidth: "720px",
            margin: "0 auto",
            display: "flex",
            justifyContent: "center",
            gap: "clamp(1.5rem, 4vw, 3rem)",
            flexWrap: "wrap",
            opacity: trustVisible ? 1 : 0,
            transform: trustVisible ? "translateY(0)" : "translateY(16px)",
            transition:
              "opacity 0.6s cubic-bezier(0.16, 1, 0.3, 1), transform 0.6s cubic-bezier(0.16, 1, 0.3, 1)",
          }}
        >
          {[
            { value: "10", label: "sellers listed" },
            { value: "150+", label: "active buyers" },
            { value: "45", label: "interests expressed" },
          ].map((stat) => (
            <div key={stat.label} style={{ textAlign: "center" }}>
              <div
                style={{
                  fontSize: "1.8rem",
                  fontWeight: 700,
                  color: "#c96b4f",
                  letterSpacing: "-0.02em",
                }}
              >
                {stat.value}
              </div>
              <div
                style={{
                  fontSize: "0.78rem",
                  color: "#999",
                  textTransform: "uppercase",
                  letterSpacing: "0.06em",
                  marginTop: "0.25rem",
                }}
              >
                {stat.label}
              </div>
            </div>
          ))}
        </div>
      </section>

      {/* Bottom CTA */}
      <section
        ref={bottomRef}
        style={{
          padding: "clamp(3rem, 6vw, 5rem) clamp(1.5rem, 4vw, 4rem)",
          textAlign: "center",
          backgroundColor: "var(--color-bg-soft)",
        }}
      >
        <div
          style={{
            maxWidth: "520px",
            margin: "0 auto",
            opacity: bottomVisible ? 1 : 0,
            transform: bottomVisible ? "translateY(0)" : "translateY(24px)",
            transition:
              "opacity 0.8s cubic-bezier(0.16, 1, 0.3, 1), transform 0.8s cubic-bezier(0.16, 1, 0.3, 1)",
          }}
        >
          <h2
            style={{
              fontSize: "clamp(1.5rem, 1.2rem + 1.5vw, 2.5rem)",
              fontWeight: 700,
              letterSpacing: "-0.03em",
              margin: "0 0 1rem",
              color: "#1a1a1a",
            }}
          >
            Ready to list?
          </h2>
          <p
            style={{
              fontSize: "1rem",
              color: "#777",
              marginBottom: "2rem",
              lineHeight: 1.6,
            }}
          >
            Join sellers who believe in transparent property transactions. Your listing goes live in
            minutes.
          </p>
          <button
            onClick={() => navigate("/register")}
            style={{
              border: "none",
              background: "#1a1a1a",
              color: "#fff",
              padding: "0.85rem 2.5rem",
              borderRadius: "12px",
              fontSize: "1.05rem",
              fontWeight: 600,
              cursor: "pointer",
              fontFamily: "inherit",
              transition: "background 0.2s ease, transform 0.15s ease",
            }}
            onMouseEnter={(e) => {
              e.currentTarget.style.background = "#333";
              e.currentTarget.style.transform = "translateY(-1px)";
            }}
            onMouseLeave={(e) => {
              e.currentTarget.style.background = "#1a1a1a";
              e.currentTarget.style.transform = "translateY(0)";
            }}
          >
            Start listing now
          </button>
        </div>
      </section>
    </div>
  );
}
