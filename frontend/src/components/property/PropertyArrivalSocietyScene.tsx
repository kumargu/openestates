import {
  PropertyArrivalGoogle3DMap,
  type ArrivalGoogle3DMapProps,
} from "./PropertyArrivalGoogle3DMap.tsx";

type Props = Omit<ArrivalGoogle3DMapProps, "terrainCorridor" | "layerExperience">;

export function PropertyArrivalSocietyScene(props: Props) {
  return (
    <PropertyArrivalGoogle3DMap
      {...props}
      terrainCorridor={false}
    />
  );
}
