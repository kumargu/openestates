import { SoftComparableIcon, SoftNearbyIcon } from "../components/ui/SoftIcons.tsx";
import { labelVisual } from "./labelVisuals.ts";

export function LabelVisualIcon({ id, size = 18 }: { id: string; size?: number }) {
  const visual = labelVisual(id);
  if (visual.family === "nearby") return <SoftNearbyIcon kind={visual.icon} size={size} />;
  if (visual.family === "comparable") return <SoftComparableIcon id={visual.icon} size={size} />;
  return (
    <span className="label-visual-symbol" aria-hidden="true">
      {visual.symbol ?? "•"}
    </span>
  );
}
