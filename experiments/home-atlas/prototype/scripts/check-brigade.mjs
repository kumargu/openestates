import assert from 'node:assert/strict';
import fs from 'node:fs';
import {makeRoute,routePoint,makeTimeline,sceneAt,scenePose,roadPose,cameraBlend} from '../web/brigade/tour-core.js';
const data=JSON.parse(fs.readFileSync(new URL('../web/brigade/inventory.json',import.meta.url)));
const route=makeRoute(data.features.find(f=>f.id==='way/843807780').geometry);
assert.deepEqual(routePoint(route,0),route.path[0]);assert.deepEqual(routePoint(route,route.length),route.path.at(-1));
const stops=[{center:data.site.center},...['way/843807779','way/843807778','way/843854110','way/843854109','way/1297658656','way/843854056','way/843854052'].map(id=>data.features.find(f=>f.id===id))];
const scenes=makeTimeline(route,stops);
const options={route,stops,scenes,altitude:()=>900,scale:()=>1,overview:()=>({center:{...data.site.center,altitude:920},heading:345,range:2900,tilt:32}),focusPose:(stop,heading)=>({center:{...stop.center,altitude:920},heading,range:500,tilt:58})};
function near(a,b){for(const k of ['lat','lng','altitude'])assert.ok(Math.abs(a.center[k]-b.center[k])<1e-8,k);for(const k of ['range','tilt'])assert.ok(Math.abs(a[k]-b[k])<1e-8,k);assert.ok(Math.abs(((a.heading-b.heading+540)%360)-180)<1e-8,'heading')}
for(let i=1;i<scenes.length;i++){assert.equal(scenes[i].start,scenes[i-1].end);near(scenePose(scenes[i-1],1,options),scenePose(scenes[i],0,options))}
for(const s of scenes)for(let t=0;t<=1;t+=.02){const c=scenePose(s,t,options);assert.ok([...Object.values(c.center),c.heading,c.range,c.tilt].every(Number.isFinite));assert.ok(c.range>=285);}
assert.equal(sceneAt(scenes,1e9).t,1);
near(cameraBlend({...options.overview(),heading:359},{...options.overview(),heading:1},.5),{...options.overview(),heading:0});
console.log('Brigade: all scene boundaries are continuous; route endpoints, camera samples, heading wrap and timeline verified.');
