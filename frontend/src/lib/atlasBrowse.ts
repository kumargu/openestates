import { distanceMetres } from './atlas/geometry.ts';
import { buildNumberedPlaces, placeMatchesProofFocus, type NumberedPlace } from './nearbyPlateProjection.ts';
import type { MapPlacePin, ProofFocus } from './types.ts';

/** Sort by the displayed evidence distance, deriving it from coordinates only when absent. */
export function nearbyPlacesByDistance(places: MapPlacePin[], home: {latitude: number; longitude: number} | null) {
  return buildNumberedPlaces(places).map(place => ({...place,
    distance_km: typeof place.distance_km === 'number' && Number.isFinite(place.distance_km) && place.distance_km >= 0
      ? place.distance_km
      : home ? distanceMetres({lat: home.latitude, lng: home.longitude},
        {lat: place.latitude, lng: place.longitude}) / 1000 : undefined,
  })).sort((a, b) => (a.distance_km ?? Infinity) - (b.distance_km ?? Infinity)
    || (a.feature_id ?? a.id).localeCompare(b.feature_id ?? b.id))
    .map((place, index) => ({...place, number: index + 1}));
}

/** The list stays complete; selection and search proof are additive to the browsing window. */
export function nearbyBrowseWindow(places: NumberedPlace[], start: number, count: number,
  selectedId: string | null, proof?: ProofFocus | null) {
  const first = Math.max(0, Math.min(start, Math.max(0, places.length - count)));
  return places.filter((place, index) => (index >= first && index < first + count)
    || (place.feature_id ?? place.name) === selectedId || placeMatchesProofFocus(place, proof));
}
