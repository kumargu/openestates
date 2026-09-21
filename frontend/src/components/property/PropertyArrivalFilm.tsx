import type { ArrivalSearchSociety, PropertyMapContext } from "../../lib/types.ts";
import { hasArrivalMap } from "../../lib/arrivalMapProjection.ts";
import { PropertyArrivalMap } from "./PropertyArrivalMap.tsx";
import "../../styles/property-arrival.css";

type Props = {
  propertyId: string;
  mapContext?: PropertyMapContext | null;
  searchContextSocieties?: ArrivalSearchSociety[];
};

export function PropertyArrivalFilm({
  propertyId,
  mapContext,
  searchContextSocieties,
}: Props) {
  if (!hasArrivalMap(mapContext) || !mapContext) return null;

  return (
    <section
      id="remote-arrival"
      className="property-arrival"
      aria-labelledby="property-arrival-title"
    >
      <header className="property-story-heading">
        <span>Arrival</span>
        <h2 id="property-arrival-title">The way in.</h2>
      </header>
      <PropertyArrivalMap
        key={propertyId}
        context={mapContext}
        searchContextSocieties={searchContextSocieties}
      />
    </section>
  );
}
