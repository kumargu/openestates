import "../../src/index.css";
import "../../src/styles/property-scene.css";
import "./preview.css";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { BrowserRouter, Route, Routes } from "react-router-dom";
import { Preview } from "./Preview.tsx";

createRoot(document.getElementById("root")!).render(<StrictMode><BrowserRouter><Routes>
  <Route path="/" element={<Preview />} />
  <Route path="/property/:id" element={<Preview />} />
  <Route path="/saved" element={<Preview />} />
  <Route path="*" element={<Preview />} />
</Routes></BrowserRouter></StrictMode>);
