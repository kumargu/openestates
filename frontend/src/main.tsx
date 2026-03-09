import "./index.css";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { BrowserRouter, Routes, Route, Link, useLocation } from "react-router-dom";
import { HomePage } from "./pages/HomePage.tsx";
import { ResultsPage } from "./pages/ResultsPage.tsx";
import { PropertyPage } from "./pages/PropertyPage.tsx";
import { ShortlistPage } from "./pages/ShortlistPage.tsx";

function Nav() {
  const location = useLocation();
  const isHome = location.pathname === "/";

  return (
    <nav
      style={{
        position: isHome ? "fixed" : "sticky",
        top: 0,
        left: 0,
        right: 0,
        display: "flex",
        gap: "2rem",
        padding: "1rem clamp(1.5rem, 4vw, 4rem)",
        fontSize: "0.9rem",
        alignItems: "center",
        zIndex: 100,
        backgroundColor: isHome ? "transparent" : "rgba(253,249,247,0.9)",
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
        }}
      >
        OpenEstates
      </Link>
      <div style={{ flex: 1 }} />
      <Link
        to="/results"
        style={{
          textDecoration: "none",
          color: "#555",
          transition: "color 0.2s",
        }}
      >
        Properties
      </Link>
      <Link
        to="/shortlist"
        style={{
          textDecoration: "none",
          color: "#555",
          transition: "color 0.2s",
        }}
      >
        Shortlist
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
        <Route path="/results" element={<ResultsPage />} />
        <Route path="/property/:id" element={<PropertyPage />} />
        <Route path="/shortlist" element={<ShortlistPage />} />
      </Routes>
    </BrowserRouter>
  );
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>
);
