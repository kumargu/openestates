import "./index.css";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { BrowserRouter, Routes, Route, Link, useLocation } from "react-router-dom";
import { HomePage } from "./pages/HomePage.tsx";
import { ResultsPageA } from "./pages/ResultsPageA.tsx";
import { PropertyPage } from "./pages/PropertyPage.tsx";
import { ShortlistPage } from "./pages/ShortlistPage.tsx";
import { SocietySearchPage } from "./pages/SocietySearchPage.tsx";
import { SellerRegisterPage, SellerDashboardPage, SellerListingFormPage } from "./pages/SellerPage.tsx";

function NavLink({ to, label, active }: { to: string; label: string; active: boolean }) {
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

function Nav() {
  const location = useLocation();
  const isHome = location.pathname === "/";
  const path = location.pathname;

  return (
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
      <NavLink to="/results" label="Properties" active={path === "/results" || path.startsWith("/property/")} />
      <NavLink to="/shortlist" label="Shortlist" active={path === "/shortlist"} />
      <Link
        to="/sell"
        style={{
          textDecoration: "none",
          fontSize: "0.88rem",
          fontWeight: 500,
          padding: "0.35rem 0.9rem",
          borderRadius: "8px",
          backgroundColor: path.startsWith("/sell") ? "#c96b4f" : "transparent",
          color: path.startsWith("/sell") ? "#fff" : "#c96b4f",
          border: "1px solid rgba(201,107,79,0.35)",
          transition: "all 0.2s ease",
        }}
      >
        Sell
      </Link>
    </nav>
  );
}

function App() {
  return (
    <BrowserRouter>
      <Nav />
      <Routes>
        <Route path="/" element={<HomePage />} />
        <Route path="/results" element={<ResultsPageA />} />
        <Route path="/property/:id" element={<PropertyPage />} />
        <Route path="/societies" element={<SocietySearchPage />} />
        <Route path="/shortlist" element={<ShortlistPage />} />
        <Route path="/sell" element={<SellerRegisterPage />} />
        <Route path="/sell/dashboard" element={<SellerDashboardPage />} />
        <Route path="/sell/list" element={<SellerListingFormPage />} />
      </Routes>
    </BrowserRouter>
  );
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>
);
