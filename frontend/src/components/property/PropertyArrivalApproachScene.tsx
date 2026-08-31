import type { MapLayerExperience } from "../../lib/types.ts";
import {
  PropertyArrivalGoogle3DMap,
  type ArrivalGoogle3DMapProps,
} from "./PropertyArrivalGoogle3DMap.tsx";

type Props = Omit<ArrivalGoogle3DMapProps, "terrainCorridor" | "layerExperience"> & {
  experience: MapLayerExperience;
};

export function PropertyArrivalApproachScene({ experience, ...props }: Props) {
  return (
    <PropertyArrivalGoogle3DMap
      {...props}
      terrainCorridor
      layerExperience={experience}
    />
  );
}
