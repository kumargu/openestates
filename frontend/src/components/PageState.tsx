import { useNavigate } from "react-router-dom";

type Variant = "loading" | "error" | "empty" | "not_found" | "backend_unavailable";

interface PageStateProps {
  variant: Variant;
  message?: string;
  context?: "results" | "property" | "generic";
  onRetry?: () => void;
}

const contextMessages: Record<string, Record<Variant, { title: string; subtitle: string; actions?: { label: string; path: string }[] }>> = {
  results: {
    loading: { title: "Finding properties...", subtitle: "Searching across Bengaluru micro-markets." },
    error: {
      title: "Explore is temporarily unavailable",
      subtitle: "We couldn't load homes right now. Please try again shortly.",
      actions: [{ label: "Explore", path: "/" }],
    },
    backend_unavailable: {
      title: "Explore is temporarily unavailable",
      subtitle: "We couldn't load homes right now. Please try again shortly.",
      actions: [{ label: "Explore", path: "/" }],
    },
    empty: { title: "No properties match", subtitle: "Try adjusting your search.", actions: [{ label: "Explore", path: "/" }] },
    not_found: { title: "Not found", subtitle: "This page doesn't exist.", actions: [{ label: "Return to homepage", path: "/" }] },
  },
  property: {
    loading: { title: "Loading property details...", subtitle: "Loading the home and its market context." },
    error: {
      title: "Property details unavailable",
      subtitle: "This property could not be loaded right now.",
      actions: [{ label: "Explore", path: "/" }],
    },
    backend_unavailable: {
      title: "Property details unavailable",
      subtitle: "This property could not be loaded right now.",
      actions: [{ label: "Explore", path: "/" }],
    },
    empty: { title: "No details available", subtitle: "This property doesn't have detailed information yet." },
    not_found: {
      title: "Property not found",
      subtitle: "This listing may no longer be available or the link may be incorrect.",
      actions: [{ label: "Explore", path: "/" }],
    },
  },
  generic: {
    loading: { title: "Loading...", subtitle: "This should only take a moment." },
    error: { title: "Something went wrong", subtitle: "We couldn't load this page. Please try again.", actions: [{ label: "Return to homepage", path: "/" }] },
    backend_unavailable: { title: "Data temporarily unavailable", subtitle: "We're reconnecting. Please try again shortly.", actions: [{ label: "Return to homepage", path: "/" }] },
    empty: { title: "Nothing here yet", subtitle: "No data available for this view." },
    not_found: { title: "Not found", subtitle: "The page or item you're looking for doesn't exist.", actions: [{ label: "Return to homepage", path: "/" }] },
  },
};

export function PageState({
  variant,
  message,
  context = "generic",
  onRetry,
}: PageStateProps) {
  const navigate = useNavigate();
  const msgs = contextMessages[context] || contextMessages.generic;
  const { title, subtitle, actions } = msgs[variant];

  return (
    <div className="page-state" role={variant === "error" ? "alert" : undefined}>
      <h2>{title}</h2>
      <p>{message || subtitle}</p>
      {(onRetry || (actions && actions.length > 0)) && (
        <div className="page-state__actions">
          {onRetry && (
            <button
              type="button"
              className="page-state__action page-state__action--primary"
              onClick={onRetry}
            >
              Retry
            </button>
          )}
          {actions?.slice(0, onRetry ? 1 : undefined).map((action) => (
            <button
              key={action.label}
              type="button"
              className={`page-state__action${onRetry ? "" : " page-state__action--primary"}`}
              onClick={() => navigate(action.path)}
            >
              {action.label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
