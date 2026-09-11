/**
 * Portable, renderer-free neighborhood atlas primitives.
 *
 * The functions in this module deliberately know nothing about Google Maps,
 * HTML, playback, or OpenEstates state. They can move to TypeScript unchanged
 * apart from replacing these JSDoc contracts with native types.
 */

/** @typedef {{lat:number,lng:number}} AtlasPoint */
/** @typedef {{center:AtlasPoint & {altitude:number},heading:number,tilt:number,range:number}} AtlasCamera */
/**
 * @typedef {Object} NearbyFeature
 * @property {string} id
 * @property {string} name
 * @property {'home'|'society'|'school'|'hospital'|'road'|'metro'|'lake'} kind
 * @property {number} lat
 * @property {number} lng
 * @property {number} distanceM
 * @property {number=} elevation
 * @property {AtlasPoint[]=} boundary
 * @property {{id:string,path:AtlasPoint[]}[]=} segments
 */

export const CATEGORY_LABELS=Object.freeze({
  home:'Home',society:'Societies',school:'Schools',hospital:'Hospitals',
  road:'Roads',metro:'Metro',lake:'Lakes'
});

export const CATEGORY_COLORS=Object.freeze({
  road:'#f4c176',metro:'#d5a7ff',lake:'#7dd8f4',default:'#8ee2ed'
});

export function categoryName(kind){return CATEGORY_LABELS[kind]||'Nearby'}
export function kindColor(kind){return CATEGORY_COLORS[kind]||CATEGORY_COLORS.default}
export function distanceLabel(feature){return feature.distanceM<1000?feature.distanceM+' m':(feature.distanceM/1000).toFixed(1)+' km'}
export function placesInCategory(places,kind){return places.filter(p=>p.kind===kind).slice().sort((a,b)=>a.distanceM-b.distanceM)}

export function geoDistance(a,b){
  const lat=(b.lat-a.lat)*111320;
  const lng=(b.lng-a.lng)*111320*Math.cos((a.lat+b.lat)*Math.PI/360);
  return Math.hypot(lat,lng);
}

export function featurePoints(feature){
  return feature.boundary||feature.segments?.flatMap(segment=>segment.path)||[{lat:feature.lat,lng:feature.lng}];
}

export function footprintCircle(feature,radiusM=130){
  return Array.from({length:49},(_,i)=>{
    const angle=i/48*Math.PI*2;
    return {
      lat:feature.lat+Math.sin(angle)*radiusM/111320,
      lng:feature.lng+Math.cos(angle)*radiusM/(111320*Math.cos(feature.lat*Math.PI/180))
    };
  });
}

function responsiveScale(viewportWidth,mobile=1.35){return viewportWidth<700?mobile:1}
function altitudeOf(feature,defaultElevation){return (feature.elevation??defaultElevation)+20}

export function placeCamera({defaultElevation,viewportWidth},feature,{heading=225,range=550,tilt=60}={}){
  return {center:{lat:feature.lat,lng:feature.lng,altitude:altitudeOf(feature,defaultElevation)},heading,range:range*responsiveScale(viewportWidth),tilt};
}

export function pairCamera({home,defaultElevation,viewportWidth},feature){
  return {
    center:{
      lat:(home.lat+feature.lat)/2,
      lng:(home.lng+feature.lng)/2,
      altitude:((home.elevation??defaultElevation)+(feature.elevation??defaultElevation))/2+20
    },
    heading:0,tilt:26,
    range:Math.max(950,feature.distanceM*2.15)*responsiveScale(viewportWidth)
  };
}

export function groupCamera({home,defaultElevation,viewportWidth,maxContextDistanceM=4200},features){
  const localPoints=[home,...features.flatMap(featurePoints)].filter(point=>geoDistance(home,point)<maxContextDistanceM);
  const points=localPoints.length?localPoints:[home];
  const lat=(Math.min(...points.map(p=>p.lat))+Math.max(...points.map(p=>p.lat)))/2;
  const lng=(Math.min(...points.map(p=>p.lng))+Math.max(...points.map(p=>p.lng)))/2;
  const radius=Math.max(0,...points.map(p=>geoDistance({lat,lng},p)));
  const averageElevation=features.length
    ?features.reduce((total,p)=>total+(p.elevation??defaultElevation),0)/features.length
    :(home.elevation??defaultElevation);
  return {
    center:{lat,lng,altitude:averageElevation+20},heading:0,tilt:25,
    range:Math.max(1000,radius*3.8)*responsiveScale(viewportWidth,1.3)
  };
}

export function featureCamera(context,feature){
  if(['road','lake'].includes(feature.kind)){
    const camera=groupCamera(context,[feature]);
    return {...camera,tilt:feature.kind==='lake'?30:50,heading:feature.kind==='road'?25:210};
  }
  return placeCamera(context,feature);
}

// Local tangent-plane projection, intended for neighborhood-scale scenes only.
// Pick the nearest actual segment, never join disconnected OSM ways.
export function roadAnchor(home,feature){
  const scale=111320*Math.cos(home.lat*Math.PI/180);
  let best=null;
  for(const segment of feature.segments||[]){
    for(let i=1;i<segment.path.length;i++){
      const a=segment.path[i-1],b=segment.path[i];
      const ax=(a.lng-home.lng)*scale,ay=(a.lat-home.lat)*111320;
      const dx=(b.lng-a.lng)*scale,dy=(b.lat-a.lat)*111320;
      const lengthSquared=dx*dx+dy*dy;
      if(lengthSquared===0)continue;
      const t=Math.max(0,Math.min(1,-(ax*dx+ay*dy)/lengthSquared));
      const distanceM=Math.hypot(ax+t*dx,ay+t*dy);
      if(!best||distanceM<best.distanceM)best={
        lat:a.lat+(b.lat-a.lat)*t,lng:a.lng+(b.lng-a.lng)*t,
        heading:(Math.atan2(dx,dy)*180/Math.PI+360)%360,
        distanceM,segmentId:segment.id
      };
    }
  }
  return best;
}

export function roadCamera(context,feature){
  const anchor=roadAnchor(context.home,feature);
  if(!anchor)return featureCamera(context,feature);
  return placeCamera(context,{...feature,lat:anchor.lat,lng:anchor.lng},{heading:anchor.heading,range:430,tilt:58});
}

export function visibleFeatures(places,homeId,selectedId,overview=false){
  const home=places.find(p=>p.id===homeId),selected=places.find(p=>p.id===selectedId)||home;
  if(!home)throw new Error('Atlas home is missing');
  if(selected.id===home.id)return [home];
  return [home,...(overview?placesInCategory(places,selected.kind):[selected])];
}
