import "./index.css";
import "./styles/evidence.css";
import "./styles/property-scene.css";
import { StrictMode, useState, useCallback, useEffect, useRef, lazy, Suspense } from "react";
import { createRoot } from "react-dom/client";
import { BrowserRouter, Routes, Route, Link, useLocation } from "react-router-dom";
import { HelmetProvider, Helmet } from "react-helmet-async";
import { ErrorBoundary } from "./components/ErrorBoundary.tsx";
import { OfflineToast } from "./components/OfflineToast.tsx";

const HomePage = lazy(() => import("./pages/HomePage.tsx").then(m => ({ default: m.HomePage })));
const ResultsPageA = lazy(() => import("./pages/ResultsPageA.tsx").then(m => ({ default: m.ResultsPageA })));
const PropertyPage = lazy(() => import("./pages/PropertyPage.tsx").then(m => ({ default: m.PropertyPage })));
const HomePlanPage = lazy(() => import("./pages/HomePlanPage.tsx").then(m => ({ default: m.HomePlanPage })));
const SocietySearchPage = lazy(() => import("./pages/SocietySearchPage.tsx").then(m => ({ default: m.SocietySearchPage })));
const SocietyDetailPage = lazy(() => import("./pages/SocietyDetailPage.tsx").then(m => ({ default: m.SocietyDetailPage })));
const NotFoundPage = lazy(() => import("./pages/NotFoundPage.tsx").then(m => ({ default: m.NotFoundPage })));

/** Scroll to top and move focus to main content on route change */
export function FocusOnNavigate() {
  const { pathname } = useLocation();
  useEffect(() => {
    window.scrollTo(0, 0);
    const main = document.getElementById("main-content");
    if (main) main.focus();
  }, [pathname]);
  return null;
}

export function NavLink({ to, label, active }: { to: string; label: string; active: boolean }) {
  return (
    <Link
      to={to}
      style={{
        textDecoration: "none",
        color: active ? "#c96b4f" : "#555",
        fontWeight: active ? 500 : 400,
        fontSize: "0.88rem",
        padding: "0.35rem 0.75rem",
        borderRadius: "8px",
        backgroundColor: active ? "rgba(201,107,79,0.06)" : "transparent",
        transition: "all 0.2s ease",
      }}
      onMouseEnter={(e) => {
        if (!active) {
          e.currentTarget.style.color = "#1a1a1a";
          e.currentTarget.style.backgroundColor = "rgba(0,0,0,0.03)";
        }
      }}
      onMouseLeave={(e) => {
        if (!active) {
          e.currentTarget.style.color = "#555";
          e.currentTarget.style.backgroundColor = "transparent";
        }
      }}
    >
      {label}
    </Link>
  );
}

const NAV_ITEMS = [
  { to: "/results", label: "Properties", matchFn: (p: string) => p === "/results" || p.startsWith("/property/") },
  { to: "/societies", label: "Societies", matchFn: (p: string) => p === "/societies" || p.startsWith("/society/") },
];

export function Nav() {
  const location = useLocation();
  const isHome = location.pathname === "/";
  const path = location.pathname;
  const isHomePlan = /^\/property\/[^/]+\/plan$/.test(path);
  const [drawerOpen, setDrawerOpen] = useState(false);
  const hamburgerRef = useRef<HTMLButtonElement>(null);
  const closeButtonRef = useRef<HTMLButtonElement>(null);

  const closeDrawer = useCallback(() => {
    setDrawerOpen(false);
    // Return focus to the hamburger button
    hamburgerRef.current?.focus();
  }, []);

  // When drawer opens: focus close button + lock body scroll
  useEffect(() => {
    if (drawerOpen) {
      closeButtonRef.current?.focus();
      document.body.style.overflow = "hidden";
    } else {
      document.body.style.overflow = "";
    }
    return () => { document.body.style.overflow = ""; };
  }, [drawerOpen]);

  if (isHomePlan) return null;

  return (
    <>
      <nav
        style={{
          position: isHome ? "fixed" : "sticky",
          top: 0,
          left: 0,
          right: 0,
          display: "flex",
          gap: "0.25rem",
          padding: "0.75rem clamp(1.5rem, 4vw, 4rem)",
          alignItems: "center",
          zIndex: 100,
          backgroundColor: isHome ? "transparent" : "rgba(253,249,247,0.92)",
          backdropFilter: isHome ? "none" : "blur(16px)",
          borderBottom: isHome ? "none" : "1px solid rgba(201,107,79,0.08)",
          transition: "background-color 0.3s ease",
        }}
      >
        <Link
          to="/"
          style={{
            fontWeight: 600,
            textDecoration: "none",
            color: "#1a1a1a",
            fontSize: "1.05rem",
            letterSpacing: "-0.02em",
            marginRight: "1rem",
          }}
        >
          OpenEstates
        </Link>
        <div style={{ flex: 1 }} />
        {/* Desktop nav links */}
        <div className="nav-links">
          {NAV_ITEMS.map((item) => (
            <NavLink key={item.to} to={item.to} label={item.label} active={item.matchFn(path)} />
          ))}
        </div>
        {/* Hamburger button — visible only on mobile via CSS */}
        <button
          ref={hamburgerRef}
          className="nav-hamburger"
          onClick={() => setDrawerOpen(true)}
          aria-label="Open navigation menu"
        >
          <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round">
            <line x1="3" y1="6" x2="21" y2="6" />
            <line x1="3" y1="12" x2="21" y2="12" />
            <line x1="3" y1="18" x2="21" y2="18" />
          </svg>
        </button>
      </nav>

      {/* Mobile drawer backdrop */}
      <div
        className={`nav-drawer-backdrop ${drawerOpen ? "nav-drawer-backdrop--open" : ""}`}
        onClick={closeDrawer}
      />

      {/* Mobile slide-in drawer */}
      <div
        className={`nav-drawer ${drawerOpen ? "nav-drawer--open" : ""}`}
        role={drawerOpen ? "dialog" : undefined}
        aria-modal={drawerOpen ? "true" : undefined}
        aria-hidden={!drawerOpen}
        onKeyDown={(e) => { if (e.key === "Escape") closeDrawer(); }}
      >
        <button ref={closeButtonRef} className="nav-drawer-close" onClick={closeDrawer} aria-label="Close menu">
          <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round">
            <line x1="18" y1="6" x2="6" y2="18" />
            <line x1="6" y1="6" x2="18" y2="18" />
          </svg>
        </button>
        {NAV_ITEMS.map((item) => (
          <Link
            key={item.to}
            to={item.to}
            className={`nav-drawer-link ${item.matchFn(path) ? "nav-drawer-link--active" : ""}`}
            onClick={closeDrawer}
          >
            {item.label}
          </Link>
        ))}
      </div>
    </>
  );
}

export function SiteFooter() {
  const location = useLocation();
  if (location.pathname === "/" || /^\/property\/[^/]+\/plan$/.test(location.pathname)) return null;

  return (
    <footer className="site-footer">
      <div className="site-footer-links">
        <Link to="/results">Properties</Link>
        <Link to="/societies">Societies</Link>
      </div>
      <div className="site-footer-wordmark">OpenEstates</div>
      <p className="site-footer-tagline">Transparent property discovery</p>
    </footer>
  );
}

export function App() {
  return (
    <HelmetProvider>
      <Helmet>
        <title>OpenEstates — Transparent Property Discovery</title>
        <meta name="description" content="Property discovery that explains why, not just what. Every listing comes with context, transparency scores, and tradeoffs you can trust." />
        <meta property="og:title" content="OpenEstates — Transparent Property Discovery" />
        <meta property="og:description" content="Property discovery that explains why, not just what. Every listing comes with context, transparency scores, and tradeoffs you can trust." />
        <meta property="og:type" content="website" />
        <meta property="og:site_name" content="OpenEstates" />
      </Helmet>
      <BrowserRouter>
        <a href="#main-content" className="skip-link">Skip to main content</a>
        <FocusOnNavigate />
        <Nav />
        <main id="main-content" tabIndex={-1}>
          <ErrorBoundary>
            <Suspense fallback={
              <div style={{ display: "flex", alignItems: "center", justifyContent: "center", minHeight: "60vh" }}>
                <div style={{
                  width: "32px",
                  height: "32px",
                  border: "3px solid rgba(201,107,79,0.15)",
                  borderTopColor: "#c96b4f",
                  borderRadius: "50%",
                  animation: "spin 0.7s linear infinite",
                }} />
                <style>{`@keyframes spin { to { transform: rotate(360deg); } }`}</style>
              </div>
            }>
              <Routes>
                <Route path="/" element={<HomePage />} />
                <Route path="/results" element={<ResultsPageA />} />
                <Route path="/property/:id" element={<PropertyPage />} />
                <Route path="/property/:id/plan" element={<HomePlanPage />} />
                <Route path="/societies" element={<SocietySearchPage />} />
                <Route path="/society/:slug" element={<SocietyDetailPage />} />
                <Route path="*" element={<NotFoundPage />} />
              </Routes>
            </Suspense>
          </ErrorBoundary>
        </main>
        <SiteFooter />
        <OfflineToast />
      </BrowserRouter>
    </HelmetProvider>
  );
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>
);
