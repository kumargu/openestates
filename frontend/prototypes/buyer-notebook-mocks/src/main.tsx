import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App.tsx";
import { NotebookProvider } from "./store.tsx";
import "./styles.css";
import "./oe-ui.css";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <NotebookProvider>
      <App />
    </NotebookProvider>
  </StrictMode>,
);
