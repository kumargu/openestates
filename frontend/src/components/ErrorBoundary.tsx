import { Component } from "react";
import type { ReactNode, ErrorInfo } from "react";

interface Props {
  children: ReactNode;
}

interface State {
  hasError: boolean;
}

export class ErrorBoundary extends Component<Props, State> {
  constructor(props: Props) {
    super(props);
    this.state = { hasError: false };
  }

  static getDerivedStateFromError(): State {
    return { hasError: true };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("[ErrorBoundary] Uncaught render error:", error, info.componentStack);
  }

  render() {
    if (this.state.hasError) {
      return (
        <div
          style={{
            display: "flex",
            flexDirection: "column",
            alignItems: "center",
            justifyContent: "center",
            minHeight: "40vh",
            color: "#666",
            textAlign: "center",
            padding: "2rem",
          }}
        >
          <h2 style={{ fontSize: "1.5rem", marginBottom: "0.5rem", color: "#333" }}>
            Something went wrong
          </h2>
          <p style={{ fontSize: "1rem", maxWidth: "420px", lineHeight: 1.6 }}>
            An unexpected error occurred. Please try reloading the page.
          </p>
          <div style={{ display: "flex", gap: "0.75rem", marginTop: "1.5rem" }}>
            <button
              onClick={() => window.location.reload()}
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
              Retry
            </button>
            <a
              href="/"
              style={{
                display: "inline-flex",
                alignItems: "center",
                border: "1px solid rgba(0,0,0,0.12)",
                background: "transparent",
                color: "#555",
                padding: "0.65rem 1.75rem",
                borderRadius: "10px",
                fontSize: "0.9rem",
                cursor: "pointer",
                fontFamily: "inherit",
                textDecoration: "none",
                transition: "all 0.2s ease",
              }}
            >
              Return home
            </a>
          </div>
        </div>
      );
    }

    return this.props.children;
  }
}
