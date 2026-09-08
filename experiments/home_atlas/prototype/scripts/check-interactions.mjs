import fs from 'node:fs';
import vm from 'node:vm';
import assert from 'node:assert/strict';
import nearby from '../web/atlas-document.js';
import * as core from '../web/atlas-core.js';
import * as scenes from '../web/atlas-scenes.js';
import {resolveAtlasPolicy} from '../web/atlas-policy.js';
import {WATERFORD_ATLAS_CONFIG} from '../web/atlas-sites.js';

// Deterministic application-controller checks; this is not visual browser QA.
const ids=new Set([...fs.readFileSync(new URL('../web/index.html',import.meta.url),'utf8').matchAll(/id="([^"]+)"/g)].map(m=>m[1]));
class Element {
  constructor(){this.classes=new Set();this.classList={add:x=>this.classes.add(x),remove:x=>this.classes.delete(x),contains:x=>this.classes.has(x),toggle:()=>{}};this.dataset={};this.children=[];this.attrs={};}
  setAttribute(k,v){this.attrs[k]=v}
  addEventListener(){}
  append(...items){this.children.push(...items)}
  replaceChildren(...items){this.children=items}
  remove(){}
}
const elements=new Map(),frames=new Map();let clock=0,frameId=0;
const element=selector=>{if(selector.startsWith('#'))assert(ids.has(selector.slice(1)),selector);if(!elements.has(selector))elements.set(selector,new Element());return elements.get(selector)};
const context=vm.createContext({nearby,...core,...scenes,resolveAtlasPolicy,WATERFORD_ATLAS_CONFIG,
  calculateRoadCamera:core.roadCamera,selectPlacesInCategory:core.placesInCategory,
  calculatePlaceCamera:core.placeCamera,calculatePairCamera:core.pairCamera,
  calculateGroupCamera:core.groupCamera,calculateFeatureCamera:core.featureCamera,
  document:{querySelector:element,querySelectorAll:()=>[],createElement:()=>new Element(),body:new Element(),addEventListener(){}},
  window:{},matchMedia:()=>({matches:false}),innerWidth:1280,localStorage:{getItem:()=>null},
  performance:{now:()=>clock},setTimeout:()=>0,clearTimeout(){},
  requestAnimationFrame:f=>{frames.set(++frameId,f);return frameId},cancelAnimationFrame:id=>frames.delete(id),URLSearchParams,console
});
const source=fs.readFileSync(new URL('../web/app.js',import.meta.url),'utf8').replace(/^import .*;\n/gm,'').replace(/init\(\);\s*$/,'');
vm.runInContext(source,context);
const run=code=>vm.runInContext(code,context);
run(`map={center:{...HOME,altitude:elevation},heading:0,tilt:0,range:1000,append(){},stopCameraAnimation(){}};ready=true;lib3d={Polygon3DElement:class{remove(){}},Polyline3DElement:class{remove(){}},Marker3DInteractiveElement:class{addEventListener(){}remove(){}}};`);
async function advance(ms){clock+=ms;const pending=[...frames.values()];frames.clear();pending.forEach(f=>f(clock));await Promise.resolve();await Promise.resolve();}

run("choosePlace('metro-1',false)");assert.equal(element('#placeChoices').children.length,3);
run("groupOverview=true;renderOverlays();renderPlaceList('metro')");assert.equal(element('#pair').hidden,true);assert.equal(element('#placeName').hidden,true);
run("choosePlace('road-ecc',false)");assert.equal(element('#descendRoad').hidden,false);
const cancelled=run('moveTo(roadCamera(selectedPlace))');run('stop()');assert.equal(await cancelled,false);assert.equal(frames.size,0);

run("filmCategory='metro';filmPlayback=null;void neighborhoodFilm()");await advance(2000);
assert.equal(run('groupOverview'),true);assert.equal(run('selectedPlace.kind'),'metro');
await advance(700);run('stop()');assert.equal(frames.size,0);assert.equal(element('#filmCaption').hidden,false);assert.equal(element('#pause').textContent,'▶ Resume tour');
run('map.heading+=70;void neighborhoodFilm()');assert.equal(frames.size,1);await advance(1000);await advance(100);
assert.equal(run('playing'),true);assert.equal(frames.size,1);
run('goChapter(0)');await advance(2500);assert.equal(run('filmPlayback'),null);assert.equal(element('#filmCaption').hidden,true);
run("filmCategory='school';void neighborhoodFilm()");await advance(2000);await advance(100);run("choosePlace('metro-0',false)");await advance(10000);
assert.equal(run('selectedPlace.id'),'metro-0');assert.equal(run('playing'),false);assert.equal(frames.size,0);
assert.equal(nearby.metroSegments.length,16);assert(!nearby.metroSegments.some(s=>s.id==='452475467'));
console.log('PASS: group/point selection, cancellation, single-loop resume, category switching, caption state and depot-track exclusion.');
