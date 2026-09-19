import assert from 'node:assert/strict';
import atlas,{featureSource} from '../web/atlas-document.js';
import {placesInCategory,geoDistance,footprintCircle,placeCamera,pairCamera,groupCamera,featureCamera} from '../web/atlas-core.js';
import {buildOriginalScenes,buildCategoryScenes,buildNeighborhoodScenes} from '../web/atlas-scenes.js';
import {roadAnchor,roadCamera,visibleFeatures} from '../web/atlas-core.js';

const ids=new Set();
for(const feature of atlas.places){
  assert(feature.id&&!ids.has(feature.id),'Feature IDs must be unique');
  ids.add(feature.id);
  assert(Number.isFinite(feature.lat)&&Number.isFinite(feature.lng),'Feature coordinates must be finite');
  assert(Number.isFinite(feature.distanceM)&&feature.distanceM>=0,'Feature distances must be non-negative');
  assert(featureSource(feature).provider,'Every feature must have provenance');
  if(feature.boundary){
    assert(feature.boundary.length>=4,'Boundaries need at least four points');
    assert.deepEqual(feature.boundary[0],feature.boundary.at(-1),'Boundaries must be closed');
  }
}
assert.equal(atlas.homeId,'home');
assert(ids.has(atlas.homeId));

for(const viewportWidth of [375,1280]){
  const home=atlas.places.find(feature=>feature.id===atlas.homeId);
  const context={home,defaultElevation:home.elevation,viewportWidth,maxContextDistanceM:4200};
  const cameras={
    placeCamera:(feature,heading,range,tilt)=>placeCamera(context,feature,{heading,range,tilt}),
    pairCamera:feature=>pairCamera(context,feature),
    groupCamera:features=>groupCamera(context,features),
    featureCamera:feature=>featureCamera(context,feature)
  };
  const helpers={places:atlas.places,...cameras,placesInCategory:kind=>placesInCategory(atlas.places,kind),categoryName:kind=>kind,distanceLabel:feature=>String(feature.distanceM)};
  const originalScenes=buildOriginalScenes(helpers);
  const neighborhoodScenes=buildNeighborhoodScenes({...helpers,originalScenes});
  const categories=['society','school','hospital','road','metro','lake'];
  const allScenes=[...originalScenes,...neighborhoodScenes,...categories.flatMap(kind=>buildCategoryScenes({...helpers,kind}))];
  for(const scene of allScenes){
    assert(ids.has(scene.id),'Scenes may reference only known features');
    assert(scene.duration>0,'Scene durations must be positive');
    const camera=scene.camera;
    assert([camera.center.lat,camera.center.lng,camera.center.altitude,camera.heading,camera.tilt,camera.range].every(Number.isFinite),'Camera values must be finite');
  }
  for(const kind of categories){
    const list=placesInCategory(atlas.places,kind);
    assert.deepEqual(list.map(feature=>feature.distanceM),list.map(feature=>feature.distanceM).slice().sort((a,b)=>a-b));
  }
  assert.equal(footprintCircle(home,100).length,49);
  assert(geoDistance(home,home)===0);
}

console.log(`Atlas ${atlas.schemaVersion}: ${atlas.places.length} features and ${atlas.metroSegments.length} metro segments validated.`);

const home=atlas.places.find(p=>p.id===atlas.homeId);
const ecc=atlas.places.find(p=>p.id==='road-ecc');
const anchor=roadAnchor(home,ecc);
assert(anchor&&anchor.distanceM<ecc.distanceM,'Road anchor projects onto the segment, not just its vertices');
assert(ecc.segments.some(s=>s.id===anchor.segmentId));
assert.equal(roadAnchor(home,{segments:[{id:'zero',path:[home,home]}]}),null);
for(const width of [375,1280]){
  const camera=roadCamera({home,defaultElevation:home.elevation,viewportWidth:width},ecc);
  assert(camera.range>=430&&camera.range<600);
  assert.equal(camera.heading,anchor.heading);
}
const metros=visibleFeatures(atlas.places,home.id,'metro-0',true);
assert.deepEqual(metros.map(p=>p.id),['home','metro-0','metro-1','metro-2']);
assert.deepEqual(visibleFeatures(atlas.places,home.id,'metro-1').map(p=>p.id),['home','metro-1']);
assert.equal(atlas.places.find(p=>p.id==='capitol').evidence.location.provider,'google-places');
assert.equal(atlas.places.find(p=>p.id==='capitol').evidence.geometry.provider,'openstreetmap');
assert.equal(ecc.evidence.distance.target,'representative road vertex');
console.log('Road descent, group/pair visibility, degenerate geometry and independent provenance checks passed.');
