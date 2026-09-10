import { createRoot } from "react-dom/client";
import { HomeExplorer } from "../react/HomeExplorer.tsx";
import { waterford } from "../fixtures/waterford.ts";
import { configuration } from "../fixtures/configurations.ts";
import "../react/home-explorer.css";
// Demo-only fixture selection. The reusable explorer only receives a HomePlan.
const name = new URLSearchParams(location.search).get("plan");
const fixture =
  name === "studio"
    ? configuration(0)
    : name === "1br"
      ? configuration(1)
      : name === "2br"
        ? configuration(2, 0.41)
        : name === "4br"
          ? configuration(4)
          : waterford;
createRoot(document.getElementById("root")!).render(
  <HomeExplorer plan={fixture} />,
);
