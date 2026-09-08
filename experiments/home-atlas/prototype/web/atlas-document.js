import sourceData from './nearby-data.js';

// Stable hand-off seam for the prototype. OpenEstates can later replace this
// module with an API adapter while the atlas math and scene recipes stay intact.
export const ATLAS_SCHEMA_VERSION='1.0';

export function featureSource(feature){
  if(feature.osmId)return {provider:'openstreetmap',sourceId:feature.osmId,url:`https://www.openstreetmap.org/way/${feature.osmId}`};
  if(feature.placeId)return {provider:'google-places',sourceId:feature.placeId,url:feature.url};
  return {provider:'curated',sourceId:null,url:feature.url};
}

// Coordinate provenance and geometry provenance are independent; a Google place
// point must never be presented as the source of an OSM boundary (or vice versa).
export function featureEvidence(feature){
  return {
    location:{provider:feature.placeId||feature.id==='home'?'google-places':'openstreetmap',sourceId:feature.placeId||feature.osmId||null},
    geometry:feature.osmId?{provider:'openstreetmap',sourceId:feature.osmId,url:`https://www.openstreetmap.org/way/${feature.osmId}`} : null,
    distance:{metres:feature.distanceM,method:'straight-line',target:feature.kind==='road'?'representative road vertex':feature.kind==='lake'?'mapped extent centre':'place point'},
    cameraElevation:{metres:feature.elevation,method:feature.cameraElevationSource?'nearest terrain sample':'Google elevation sample',use:'camera framing only'},
    caveat:feature.note
  };
}

// OSM source snapshot: atlas-north extract, 2026-09-08. These subway ways
// have no service=yard/siding/crossover tag. The raw snapshot remains intact.
// Passenger context must not render depot tracks as station connections.
const passengerAlignmentIds=new Set([
  '458951602','458951605','494676178','549187816','549187817','549187818',
  '607901215','607901216','607901217','607901219','607901220','1190024586',
  '1190024589','1190025013','1240358636','1240358637'
]);

const atlasDocument={
  schemaVersion:ATLAS_SCHEMA_VERSION,
  homeId:'home',
  generatedAt:sourceData.contextFetched,
  places:sourceData.places.map(feature=>({...feature,evidence:featureEvidence(feature)})),
  contextGeometry:{metroSegments:(sourceData.metroSegments||[]).filter(segment=>passengerAlignmentIds.has(segment.id))}
};

// Compatibility alias while the renderer still reads the prototype shape.
atlasDocument.metroSegments=atlasDocument.contextGeometry.metroSegments;

export default Object.freeze(atlasDocument);
