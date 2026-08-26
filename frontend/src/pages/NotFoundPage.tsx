/**
 * 404 Not Found catch-all page.
 */
import { Link } from "react-router-dom";
import { Helmet } from "react-helmet-async";
import { PUBLIC_BRAND_NAME } from "../lib/brand.ts";

export function NotFoundPage() {
  return (
    <div className="page-container" style={{ maxWidth: "600px" }}>
      <Helmet>
        <title>Page not found | {PUBLIC_BRAND_NAME}</title>
      </Helmet>
      <div
        style={{
          display: "flex",
          flexDirection: "column",
          alignItems: "center",
          justifyContent: "center",
          minHeight: "50vh",
          textAlign: "center",
          padding: "2rem",
        }}
      >
        <h1
          style={{
            fontSize: "3rem",
            fontWeight: 700,
            letterSpacing: "-0.03em",
            color: "var(--color-text-muted)",
            margin: "0 0 0.5rem",
          }}
        >
          404
        </h1>
        <h2
          style={{
            fontSize: "1.3rem",
            fontWeight: 600,
            margin: "0 0 0.75rem",
            color: "var(--color-text)",
          }}
        >
          Page not found
        </h2>
        <p
          style={{
            fontSize: "0.92rem",
            color: "var(--color-text-secondary)",
            maxWidth: "400px",
            lineHeight: 1.6,
            marginBottom: "1.75rem",
          }}
        >
          The page you are looking for does not exist or may have been moved.
        </p>
        <div style={{ display: "flex", gap: "0.75rem" }}>
          <Link to="/" className="btn btn-primary" style={{ textDecoration: "none" }}>
            Go home
          </Link>
          <Link to="/" className="btn btn-outline" style={{ textDecoration: "none" }}>
            Search homes
          </Link>
        </div>
      </div>
    </div>
  );
}
