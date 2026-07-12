import { useNavigate } from "react-router-dom";

type Variant = "loading" | "error" | "empty" | "not_found" | "backend_unavailable";

interface PageStateProps {
  variant: Variant;
  message?: string;
  context?: "results" | "property" | "society" | "generic";
}

const contextMessages: Record<string, Record<Variant, { title: string; subtitle: string; actions?: { label: string; path: string }[] }>> = {
  results: {
    loading: { title: "Finding properties...", subtitle: "Searching across Bengaluru micro-markets." },
    error: {
      title: "Results temporarily unavailable",
      subtitle: "We couldn't load live property data right now, but you can still continue exploring Bengaluru areas.",
      actions: [
        { label: "Browse areas", path: "/" },
        { label: "Return home", path: "/" },
      ],
    },
    backend_unavailable: {
      title: "Results temporarily unavailable",
      subtitle: "We couldn't load live property data right now, but you can still continue exploring Bengaluru areas.",
      actions: [
        { label: "Browse areas", path: "/" },
        { label: "Return home", path: "/" },
      ],
    },
    empty: { title: "No properties match", subtitle: "Try adjusting your search criteria or browse all areas.", actions: [{ label: "Browse properties", path: "/results" }] },
    not_found: { title: "Not found", subtitle: "This page doesn't exist.", actions: [{ label: "Return to homepage", path: "/" }] },
  },
  property: {
    loading: { title: "Loading property details...", subtitle: "Preparing the full transparency report." },
    error: {
      title: "Property details unavailable",
      subtitle: "This property page could not be loaded right now. You can go back to results or continue browsing other areas.",
      actions: [
        { label: "Back to results", path: "/results" },
        { label: "Browse areas", path: "/" },
      ],
    },
    backend_unavailable: {
      title: "Property details unavailable",
      subtitle: "This property page could not be loaded right now. You can go back to results or continue browsing other areas.",
      actions: [
        { label: "Back to results", path: "/results" },
        { label: "Browse areas", path: "/" },
      ],
    },
    empty: { title: "No details available", subtitle: "This property doesn't have detailed information yet." },
    not_found: {
      title: "Property not found",
      subtitle: "This listing may no longer be available or the link may be incorrect.",
      actions: [
        { label: "Browse properties", path: "/results" },
        { label: "Return home", path: "/" },
      ],
    },
  },
  society: {
    loading: { title: "Ranking societies...", subtitle: "Evaluating societies across multiple dimensions." },
    error: {
      title: "Society results unavailable",
      subtitle: "We couldn't load society rankings right now. Please try again later.",
      actions: [
        { label: "Retry", path: "/societies" },
        { label: "Return home", path: "/" },
      ],
    },
    backend_unavailable: {
      title: "Society results unavailable",
      subtitle: "We couldn't load society rankings right now. Please try again later.",
      actions: [
        { label: "Retry", path: "/societies" },
        { label: "Return home", path: "/" },
      ],
    },
    empty: { title: "No societies match", subtitle: "Try adjusting your search criteria or explore a different area.", actions: [{ label: "Browse societies", path: "/societies" }] },
    not_found: {
      title: "Society not found",
      subtitle: "This society may not be in our database yet.",
      actions: [
        { label: "Browse societies", path: "/societies" },
        { label: "Return home", path: "/" },
      ],
    },
  },
  generic: {
    loading: { title: "Loading...", subtitle: "Fetching data from OpenEstates." },
    error: { title: "Something went wrong", subtitle: "We couldn't load this page. Please try again.", actions: [{ label: "Return to homepage", path: "/" }] },
    backend_unavailable: { title: "Data temporarily unavailable", subtitle: "We're reconnecting. Please try again shortly.", actions: [{ label: "Return to homepage", path: "/" }] },
    empty: { title: "Nothing here yet", subtitle: "No data available for this view." },
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
