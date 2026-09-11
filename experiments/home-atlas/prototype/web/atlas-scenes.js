/**
 * Pure scene recipes. Renderers translate each returned camera into their own
 * animation system; captions and durations remain product-owned data.
 */

function featureById(places,id){
  const feature=places.find(place=>place.id===id);
  if(!feature)throw new Error('Atlas scene references missing feature: '+id);
  return feature;
}

export function buildOriginalScenes({places,placeCamera,pairCamera,distanceLabel}){
  const home=featureById(places,'home'),capitol=featureById(places,'capitol');
  const school=featureById(places,'school'),hospital=featureById(places,'hospital');
  return [
    {id:home.id,camera:placeCamera(home,205,1100,66),caption:'Waterford. Start with the whole picture.',duration:5000},
    {id:home.id,camera:placeCamera(home,275,460,61),caption:'Move around the society. Keep the real buildings in view.',duration:8500},
    {id:capitol.id,camera:pairCamera(capitol),caption:'Waterford + Sumadhura Capitol Residences · '+distanceLabel(capitol)+' apart',duration:8000},
    {id:capitol.id,camera:placeCamera(capitol,155,520,58),caption:'Sumadhura Capitol Residences · OSM marks this as a construction site.',duration:7500},
    {id:school.id,camera:pairCamera(school),caption:'Prajval Vidyanikethan School · '+distanceLabel(school)+' straight-line',duration:7000},
    {id:school.id,camera:placeCamera(school,230,500,52),caption:'A closer look at the school’s setting.',duration:6500},
    {id:hospital.id,camera:pairCamera(hospital),caption:'Amrik Hospital · '+distanceLabel(hospital)+' straight-line',duration:7500},
    {id:hospital.id,camera:placeCamera(hospital,290,480,55),caption:'See the surroundings. Check hospital services directly.',duration:6500},
    {id:home.id,camera:placeCamera(home,385,850,45),caption:'Back to Waterford. One neighborhood, a clearer picture.',duration:8000}
  ];
}

export function buildCategoryScenes({places,kind,placesInCategory,categoryName,distanceLabel,placeCamera,pairCamera,groupCamera,featureCamera,roadCamera=featureCamera}){
  const features=placesInCategory(kind),home=featureById(places,'home');
  return [
    {id:features[0]?.id||home.id,overview:true,camera:groupCamera(features),caption:categoryName(kind)+' around Waterford · '+features.length+' mapped places',duration:4500},
    ...features.flatMap(feature=>[
      {id:feature.id,camera:pairCamera(feature),caption:feature.name+' · '+distanceLabel(feature)+' straight-line from home',duration:5500},
      {id:feature.id,camera:featureCamera(feature),caption:feature.kind==='lake'?feature.name+' · OSM-mapped extent, not current water level':feature.kind==='metro'?feature.name+' · mapped metro alignment':feature.kind==='road'?feature.name+' · mapped alignment, not measured width':feature.name,duration:6500},
      ...(feature.kind==='road'?[{id:feature.id,camera:roadCamera(feature),caption:feature.name+' · closer aerial view; Street View shows ground conditions',duration:8000}]:[])
    ]),
    {id:home.id,camera:placeCamera(home,210,950,48),caption:'Back to Waterford.',duration:5500}
  ];
}

export function buildNeighborhoodScenes({places,originalScenes,placeCamera,pairCamera,featureCamera,roadCamera=featureCamera,distanceLabel}){
  const home=featureById(places,'home'),ecc=featureById(places,'road-ecc');
  const metro=featureById(places,'metro-0'),lake=featureById(places,'lake-866148184');
  return [
    ...originalScenes.slice(0,-1),
    {id:ecc.id,camera:featureCamera(ecc),caption:'ECC Road · the mapped road beside Waterford',duration:7500},
    {id:ecc.id,camera:roadCamera(ecc),caption:'ECC Road · a closer aerial view beside Waterford',duration:8000},
    {id:metro.id,camera:pairCamera(metro),caption:'Kadugodi Tree Park metro · '+distanceLabel(metro)+' straight-line',duration:7000},
    {id:metro.id,camera:placeCamera(metro,265,750,52),caption:'Follow the mapped metro alignment. Station access is a separate journey.',duration:6500},
    {id:lake.id,camera:featureCamera(lake),caption:'Pattandur Agrahara Lake · mapped as intermittent water',duration:8500},
    {id:home.id,camera:placeCamera(home,385,1000,48),caption:'Roads, metro, water—and your home in the middle.',duration:6500}
  ];
}
