import { useNavigate } from "react-router-dom";

type Variant = "loading" | "error" | "empty" | "not_found" | "backend_unavailable";

interface PageStateProps {
  variant: Variant;
  message?: string;
  context?: "results" | "property" | "generic";
}

const contextMessages: Record<string, Record<Variant, { title: string; subtitle: string; actions?: { label: string; path: string }[] }>> = {
  results: {
    loading: { title: "Finding homes...", subtitle: "Searching Bengaluru micro-markets." },
    error: {
      title: "Results temporarily unavailable",
      subtitle: "We couldn't load homes right now.",
      actions: [
        { label: "Return home", path: "/" },
      ],
    },
    backend_unavailable: {
      title: "Results temporarily unavailable",
      subtitle: "We couldn't load homes right now.",
      actions: [
        { label: "Return home", path: "/" },
      ],
    },
    empty: { title: "No homes match", subtitle: "Try a broader search.", actions: [{ label: "Browse homes", path: "/" }] },
    not_found: { title: "Not found", subtitle: "This page doesn't exist.", actions: [{ label: "Return to homepage", path: "/" }] },
  },
  property: {
    loading: { title: "Loading home...", subtitle: "Preparing property context." },
    error: {
      title: "Property details unavailable",
      subtitle: "This home could not be loaded right now.",
      actions: [
        { label: "Back to search", path: "/" },
      ],
    },
    backend_unavailable: {
      title: "Property details unavailable",
      subtitle: "This home could not be loaded right now.",
      actions: [
        { label: "Back to search", path: "/" },
      ],
    },
    empty: { title: "No details available", subtitle: "This home has limited detail right now." },
    not_found: {
      title: "Property not found",
      subtitle: "This home may no longer be available.",
      actions: [
        { label: "Browse homes", path: "/" },
        { label: "Return home", path: "/" },
      ],
    },
  },
  generic: {
    loading: { title: "Loading...", subtitle: "Preparing this view." },
    error: { title: "Something went wrong", subtitle: "We couldn't load this page.", actions: [{ label: "Return home", path: "/" }] },
    backend_unavailable: { title: "Data temporarily unavailable", subtitle: "We're reconnecting. Please try again shortly.", actions: [{ label: "Return to homepage", path: "/" }] },
    empty: { title: "Nothing here yet", subtitle: "This view is empty." },
    not_found: { title: "Not found", subtitle: "The page or item you're looking for doesn't exist.", actions: [{ label: "Return to homepage", path: "/" }] },
  },
};

export function PageState({ variant, message, context = "generic" }: PageStateProps) {
  const navigate = useNavigate();
  const msgs = contextMessages[context] || contextMessages.generic;
  const { title, subtitle, actions } = msgs[variant];

  return (
    <div style={{
      display: "flex",
      flexDirection: "column",
      alignItems: "center",
      justifyContent: "center",
      minHeight: "40vh",
      color: "#666",
      textAlign: "center",
      padding: "2rem",
    }}>
      <h2 style={{ fontSize: "1.5rem", marginBottom: "0.5rem", color: "#333" }}>
        {title}
      </h2>
      <p style={{ fontSize: "1rem", maxWidth: "420px", lineHeight: 1.6 }}>
        {message || subtitle}
      </p>
      {actions && actions.length > 0 && (
        <div style={{ display: "flex", gap: "0.75rem", marginTop: "1.5rem", flexWrap: "wrap", justifyContent: "center" }}>
          {actions.slice(0, 1).map((a) => (
            <button
              key={a.label}
              onClick={() => navigate(a.path)}
              style={{
                border: "1px solid rgba(0,0,0,0.12)",
                background: "#1a1a1a",
                color: "#fff",
                padding: "0.65rem 1.75rem",
                borderRadius: "10px",
                fontSize: "0.9rem",
                cursor: "pointer",
                fontFamily: "inherit",
                transition: "background 0.2s ease",
              }}
              onMouseEnter={(e) => (e.currentTarget.style.background = "#333")}
              onMouseLeave={(e) => (e.currentTarget.style.background = "#1a1a1a")}
            >
              {a.label}
            </button>
          ))}
          {actions.slice(1).map((a) => (
            <button
              key={a.label}
              onClick={() => navigate(a.path)}
              style={{
                border: "1px solid rgba(0,0,0,0.12)",
                background: "transparent",
                color: "#555",
                padding: "0.65rem 1.75rem",
                borderRadius: "10px",
                fontSize: "0.9rem",
                cursor: "pointer",
                fontFamily: "inherit",
                transition: "all 0.2s ease",
              }}
              onMouseEnter={(e) => { e.currentTarget.style.background = "#1a1a1a"; e.currentTarget.style.color = "#fff"; }}
              onMouseLeave={(e) => { e.currentTarget.style.background = "transparent"; e.currentTarget.style.color = "#555"; }}
            >
              {a.label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
