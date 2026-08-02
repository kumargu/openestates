import "./index.css";
import "./styles/evidence.css";
import "./styles/property-scene.css";
import "./styles/notebook.css";
import "./styles/rera-report.css";
import { StrictMode, useEffect, useRef, lazy, Suspense } from "react";
import { createRoot } from "react-dom/client";
import {
  BrowserRouter,
  Routes,
  Route,
  Navigate,
  useLocation,
  useParams,
  useSearchParams,
} from "react-router-dom";
import { HelmetProvider, Helmet } from "react-helmet-async";
import { ErrorBoundary } from "./components/ErrorBoundary.tsx";
import { OfflineToast } from "./components/OfflineToast.tsx";
import { NotebookToast } from "./components/notebook/NotebookToast.tsx";
import { WorkspaceFrame } from "./components/workspace/WorkspaceFrame.tsx";
import { readDiscoveryContext } from "./lib/navigationContext.ts";

const HomePage = lazy(() => import("./pages/HomePage.tsx").then(m => ({ default: m.HomePage })));
const PropertyPage = lazy(() => import("./pages/PropertyPage.tsx").then(m => ({ default: m.PropertyPage })));
const HomePlanPage = lazy(() => import("./pages/HomePlanPage.tsx").then(m => ({ default: m.HomePlanPage })));
const ReraReportPage = lazy(() => import("./pages/ReraReportPage.tsx").then(m => ({ default: m.ReraReportPage })));
const WorkspacePage = lazy(() => import("./pages/WorkspacePage.tsx").then(m => ({ default: m.WorkspacePage })));
const NotFoundPage = lazy(() => import("./pages/NotFoundPage.tsx").then(m => ({ default: m.NotFoundPage })));

/** Scroll to top and move focus to main content on route change */
export function FocusOnNavigate() {
  const { pathname, search } = useLocation();
  const previousPathname = useRef<string | null>(null);
  useEffect(() => {
    const routeChanged = previousPathname.current !== pathname;
    previousPathname.current = pathname;
    if (!routeChanged && pathname !== "/") return undefined;
    const discovery = readDiscoveryContext();
    const shouldRestoreDiscovery = discovery?.url === `${pathname}${search}` && discovery.scrollY > 0;
    const targetScrollY = shouldRestoreDiscovery ? discovery.scrollY : 0;
    window.scrollTo(0, targetScrollY);
    const settleScroll = shouldRestoreDiscovery
      ? window.setTimeout(() => window.scrollTo(0, targetScrollY), 350)
      : undefined;
    const main = document.getElementById("main-content");
    if (main) main.focus();
    return () => {
      if (settleScroll !== undefined) window.clearTimeout(settleScroll);
    };
  }, [pathname, search]);
  return null;
}

function ResultsRedirect() {
  const [params] = useSearchParams();
  const query = params.get("q")?.trim();
  return <Navigate to={query ? `/?q=${encodeURIComponent(query)}` : "/"} replace />;
}

function LegacyWorkspaceRedirect({ mode }: { mode: "notes" | "compare" }) {
  const { search } = useLocation();
  const target = mode === "compare" ? `/workspace/compare${search}` : `/workspace${search}`;
  return <Navigate to={target} replace />;
}

function LegacyPlanRedirect() {
  const { id } = useParams<{ id: string }>();
  return <Navigate to={id ? `/workspace/buy-vs-rent/${encodeURIComponent(id)}` : "/workspace/buy-vs-rent"} replace />;
}

export function App() {
  return (
    <HelmetProvider>
      <Helmet>
        <title>OpenEstates — Transparent Property Discovery</title>
        <meta name="description" content="Property discovery that explains why, not just what. Every listing comes with context, evidence, and tradeoffs you can verify." />
        <meta property="og:title" content="OpenEstates — Transparent Property Discovery" />
        <meta property="og:description" content="Property discovery that explains why, not just what. Every listing comes with context, evidence, and tradeoffs you can verify." />
        <meta property="og:type" content="website" />
        <meta property="og:site_name" content="OpenEstates" />
      </Helmet>
      <BrowserRouter>
        <a href="#main-content" className="skip-link">Skip to main content</a>
        <FocusOnNavigate />
        <WorkspaceFrame>
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
                  <Route path="/results" element={<ResultsRedirect />} />
                  <Route path="/property/:id" element={<PropertyPage />} />
                  <Route path="/property/:id/plan" element={<LegacyPlanRedirect />} />
                  <Route path="/property/:id/rera" element={<ReraReportPage />} />
                  <Route path="/workspace" element={<WorkspacePage />} />
                  <Route path="/workspace/compare" element={<WorkspacePage />} />
                  <Route path="/workspace/buy-vs-rent" element={<HomePlanPage />} />
                  <Route path="/workspace/buy-vs-rent/:id" element={<HomePlanPage />} />
                  <Route path="/notebook" element={<LegacyWorkspaceRedirect mode="notes" />} />
                  <Route path="/compare" element={<LegacyWorkspaceRedirect mode="compare" />} />
                  <Route path="*" element={<NotFoundPage />} />
                </Routes>
              </Suspense>
            </ErrorBoundary>
          </main>
        </WorkspaceFrame>
        <NotebookToast />
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
