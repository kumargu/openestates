/** Renderer-independent path sampling and cinematic camera primitives. */
import {geoDistance} from '../atlas-core.js';
export const clamp=(v,a=0,b=1)=>Math.max(a,Math.min(b,v));
export const mix=(a,b,t)=>a+(b-a)*t;
export const ease=t=>t*t*(3-2*t);
export function cameraBlend(a,b,t){return {center:{lat:mix(a.center.lat,b.center.lat,t),lng:mix(a.center.lng,b.center.lng,t),altitude:mix(a.center.altitude,b.center.altitude,t)},heading:a.heading+((b.heading-a.heading+540)%360-180)*t,range:mix(a.range,b.range,t),tilt:mix(a.tilt,b.tilt,t)}}
export function makeRoute(points){
 if(points.length<2)throw new Error('A tour needs a mapped road with at least two vertices');
 const path=points[0].lat>points.at(-1).lat?[...points].reverse():[...points];
 const distances=[0];for(let i=1;i<path.length;i++)distances.push(distances.at(-1)+geoDistance(path[i-1],path[i]));
 return {path,distances,length:distances.at(-1)};
}
export function routePoint(route,metres){
 const d=clamp(metres,0,route.length);let i=1;while(i<route.path.length-1&&route.distances[i]<d)i++;
 const t=(d-route.distances[i-1])/(route.distances[i]-route.distances[i-1]||1),a=route.path[i-1],b=route.path[i];
 return {lat:mix(a.lat,b.lat,t),lng:mix(a.lng,b.lng,t)};
}
export function routeHeading(route,d){
 const a=routePoint(route,d-25),b=routePoint(route,d+55);
 return (Math.atan2((b.lng-a.lng)*Math.cos(a.lat*Math.PI/180),b.lat-a.lat)*180/Math.PI+360)%360;
}
export function nearestDistance(route,point){
 let nearest=0,best=Infinity;
 for(let d=0;d<=route.length;d+=3){const delta=geoDistance(routePoint(route,d),point);if(delta<best){best=delta;nearest=d}}
 return nearest;
}
export function roadPose(route,d,elevation,scale=1,rangeM=285){const p=routePoint(route,d);return {center:{...p,altitude:elevation(p)+14},heading:routeHeading(route,d),tilt:66,range:rangeM*scale}}
export function makeTimeline(route,stops){
 const scenes=[{kind:'overview',stop:0,duration:6500}];let previous=0;
 stops.slice(1).forEach((stop,index)=>{
  const distance=Math.max(previous,nearestDistance(route,stop.center));
  if(index===0)scenes.push({kind:'arrival',stop:index+1,to:distance,duration:6000});
  else scenes.push({kind:'road',stop:index+1,from:previous,to:distance,duration:Math.max(6000,(distance-previous)/20*1000)});
  scenes.push({kind:'focus',stop:index+1,at:distance,duration:11000});previous=distance;
 });
 scenes.push({kind:'return',stop:0,duration:6500});let elapsed=0;
 return scenes.map(s=>{const item={...s,start:elapsed,end:elapsed+s.duration};elapsed=item.end;return item});
}
export function sceneAt(scenes,elapsed){const scene=scenes.find(s=>elapsed<s.end)||scenes.at(-1);return {scene,t:clamp((elapsed-scene.start)/scene.duration)}}

export function scenePose(s,t,{route,altitude,scale,overview,focusPose,stops,scenes,roadRangeM=285}){
 if(s.kind==='overview')return overview();
 if(s.kind==='arrival')return cameraBlend(overview(),roadPose(route,s.to,altitude,scale(),roadRangeM),ease(t));
 if(s.kind==='road')return roadPose(route,s.from+(s.to-s.from)*ease(t),altitude,scale(),roadRangeM);
 if(s.kind==='return')return cameraBlend(roadPose(route,scenes.at(-2).at,altitude,scale(),roadRangeM),overview(),ease(t));
 const road=roadPose(route,s.at,altitude,scale(),roadRangeM),a=focusPose(stops[s.stop],275),b=focusPose(stops[s.stop],315);
 if(t<.28)return cameraBlend(road,a,ease(t/.28));
 if(t>.73)return cameraBlend(b,road,ease((t-.73)/.27));
 return cameraBlend(a,b,ease((t-.28)/.45));
}
