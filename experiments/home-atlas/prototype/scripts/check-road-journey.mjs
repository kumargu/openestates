import assert from 'node:assert/strict';
import nearby from '../web/atlas-document.js';
import {createRoadPath} from '../web/road-path.js';
import {geoDistance,roadAnchor} from '../web/atlas-core.js';
import {installArrivalWalk} from '../web/arrival-walk.js';
const road=nearby.places.find(p=>p.id==='road-ecc'),path=createRoadPath(road);
assert(path.total>1000&&path.total<2000);
for(let m=1;m<path.total;m+=5){assert(geoDistance(path.at(m),roadAnchor(path.at(m),road))<.5);assert(geoDistance(path.at(m),path.at(m-1))<1.1);assert(Number.isFinite(path.heading(m)));}
assert(geoDistance(path.at(-10),path.points[0])<.01);assert(geoDistance(path.at(path.total+10),path.points.at(-1))<.01);
assert(Math.abs(path.nearest(path.at(400))-400)<4);
const separated=createRoadPath({segments:[{path:[{lat:0,lng:0},{lat:0,lng:.01}]},{path:[{lat:1,lng:1},{lat:1,lng:1.001}]}]});assert(separated.total<1200);
const mapped=createRoadPath({segments:[{path:[{lat:2,lng:1},{lat:1,lng:1}]}]});
assert.equal(mapped.points[0].lat,2,'Default route direction preserves source order');
assert.equal(createRoadPath({segments:[{path:[{lat:2,lng:1},{lat:1,lng:1}]}]},{direction:'reverse'}).points[0].lat,1,'Direction reversal is explicit');
assert.throws(()=>createRoadPath(road,{direction:'north-ish'}),/as-mapped or reverse/);
class Element{constructor(){this.textContent='';this.hidden=false;this.attrs={};this.classList={add(){},remove(){}};}setAttribute(k,v){this.attrs[k]=v;}append(){}focus(){}addEventListener(){}}
const els=new Map(),el=s=>{if(!els.has(s))els.set(s,new Element());return els.get(s);};
globalThis.document={querySelector:el,createElement:()=>new Element(),body:new Element(),addEventListener(){}};
globalThis.matchMedia=()=>({matches:false,addEventListener(){}});globalThis.innerWidth=1200;
const frames=new Map();let frameId=0,hold,applied=0,moved=0,restored=0;
globalThis.requestAnimationFrame=f=>{frames.set(++frameId,f);return frameId;};globalThis.cancelAnimationFrame=id=>frames.delete(id);
globalThis.setTimeout=f=>{hold=f;return 1;};
const host={home:nearby.places[0],road,ready:()=>true,elevation:()=>921,prepare(){},stop(){},restore(){restored++;},toast(){},move:async()=>{moved++;return true;},apply(){applied++;}};
let launch;el('.story').append=b=>{launch=b;};const journey=installArrivalWalk(host);
const pending=launch.onclick();await Promise.resolve();assert(hold);journey.pause();hold();await pending;assert.equal(moved,1,'Pausing during road hold prevents later descent');assert.equal(frames.size,0);
el('#roadSpeed').oninput({target:{value:'2'}});assert.equal(el('#roadSpeedValue').textContent,'2×');
el('#roadPlay').onclick();assert.equal(frames.size,1);const f=[...frames.values()][0];frames.clear();f(performance.now()+100);assert(applied>0);assert(Number(el('#roadReadout').textContent.split(' / ')[0])>=2,'Speed control advances the route');journey.pause();assert.equal(frames.size,0,'Manual pause cancels continuous flight');
const streetPoint=path.at(2);class Panorama{setPano(){}setPov(v){this.pov=v}setVisible(v){this.visible=v}getPosition(){return {lat:()=>streetPoint.lat,lng:()=>streetPoint.lng}}getPov(){return this.pov}}
globalThis.google={maps:{importLibrary:async()=>({StreetViewService:class{async getPanorama(){return {data:{location:{pano:'road-pano',latLng:{lat:()=>streetPoint.lat,lng:()=>streetPoint.lng}},imageDate:'2025'}}}},StreetViewPanorama:Panorama})}};
await el('#roadGround').onclick();assert.equal(el('#roadReturn').hidden,false,'Street mode always exposes Back above');assert.equal(el('#roadAbove').attrs['aria-pressed'],'false');
await el('#roadReturn').onclick();assert.equal(el('#roadReturn').hidden,true);assert.equal(el('#roadAbove').attrs['aria-pressed'],'true','Back above restores aerial mode');
el('#roadExit').onclick();assert.equal(journey.active(),false);assert.equal(restored,1);
console.log('Road journey: OSM alignment, speed, cancellation, persistent Street exit and aerial restoration passed.');
