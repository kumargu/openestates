import {geoDistance} from './atlas-core.js';
import {createRoadPath} from './road-path.js';
export function installArrivalWalk(host){
 const $=s=>document.querySelector(s),path=createRoadPath(host.road,{direction:host.roadDirection});
 const reduced=()=>matchMedia('(prefers-reduced-motion: reduce)').matches;
 let active=false,mode='above',playing=false,metres=0,frame=0,last=0,epoch=0,pano,heading=path.heading(0),loading=false,intro=false,speed=1;
 const root=document.createElement('section');root.id='arrivalExperience';root.hidden=true;root.setAttribute('aria-label','ECC Road journey');
 root.innerHTML=`<div id="arrivalStreet"></div><div class="roadIdentity"><strong>ECC Road</strong><span id="roadPosition">Beside Prestige Waterford</span></div><button id="roadReturn" hidden>← Back above</button><div class="roadDock"><button id="roadPlay">Pause</button><span id="roadReadout" role="status">Reading the road</span><label class="roadSpeed"><span>Speed <output id="roadSpeedValue">1×</output></span><input id="roadSpeed" type="range" min="0.5" max="2" value="1" step="0.25" aria-label="Road flight speed"></label><div class="roadModes" role="group" aria-label="Road perspective"><button id="roadAbove" aria-pressed="true">Above</button><button id="roadGround" aria-pressed="false">Street</button></div><button id="roadExit" aria-label="Exit road journey">×</button></div>`;
 document.body.append(root);
 const launch=document.createElement('button');launch.id='arriveWaterford';launch.textContent='ECC Road';$('.story').append(launch);
 const camera=()=>({center:{...path.at(metres),altitude:host.elevation()+8},heading,tilt:67,range:innerWidth<700?350:270});
 function sync(){
  $('#roadPlay').textContent=(playing||intro)?'Ⅱ Pause':metres>=path.total?'↻ Replay':'▶ Continue';$('#roadPlay').hidden=mode==='street';
  $('#roadReturn').hidden=mode!=='street';$('.roadSpeed').hidden=mode==='street';
  $('#roadAbove').setAttribute('aria-pressed',String(mode==='above'));$('#roadGround').setAttribute('aria-pressed',String(mode==='street'));$('#roadGround').disabled=loading;
  $('#roadReadout').textContent=loading?'Finding street imagery…':mode==='street'?'Use the road arrows to explore':`${Math.round(metres)} / ${Math.round(path.total)} m`;
 }
 function pause(){if(!active)return;intro=false;playing=false;cancelAnimationFrame(frame);host.stop();sync();}
 function tick(now){if(!active||!playing||mode!=='above')return;if(!host.ready()){pause();return;}const dt=Math.min(.08,(now-last)/1000);last=now;metres=Math.min(path.total,metres+dt*12*speed);const target=path.heading(metres);heading+=((target-heading+540)%360-180)*(1-Math.exp(-dt*3));host.apply(camera());sync();if(metres>=path.total){playing=false;sync();return;}frame=requestAnimationFrame(tick);}
 function play(){if(!active||mode!=='above')return;if(playing){pause();return;}host.stop();if(metres>=path.total)metres=0;playing=true;last=performance.now();sync();frame=requestAnimationFrame(tick);}
 async function begin(){
  if(!host.ready()){host.toast('The 3D view is not available yet.');return;}
  host.prepare();active=true;intro=true;const token=++epoch;mode='above';metres=0;heading=path.heading(0);root.hidden=false;document.body.classList.add('arrivalActive');sync();
  $('#roadReadout').textContent='Reading the road';
  const ok=await host.move({...camera(),tilt:48,range:720},reduced()?0:3200);if(!ok||token!==epoch||!active)return;
  await new Promise(r=>setTimeout(r,reduced()?0:2400));if(token!==epoch||!active)return;
  const down=await host.move(camera(),reduced()?0:3200);if(!down||token!==epoch||!active)return;
  intro=false;if(!reduced())play();else sync();
 }
 async function street(){
  if(loading||mode==='street')return;pause();loading=true;const token=++epoch;sync();
  try{
   const {StreetViewService,StreetViewPanorama}=await google.maps.importLibrary('streetView');
   const response=await new StreetViewService().getPanorama({location:path.at(metres),radius:45,preference:'nearest',source:'outdoor'});
   if(!active||token!==epoch)return;
   const position=response.data.location.latLng;if(geoDistance(path.at(metres),{lat:position.lat(),lng:position.lng()})>60)throw new Error('Imagery too far from road');
   pano??=new StreetViewPanorama($('#arrivalStreet'),{visible:false,fullscreenControl:false,addressControl:true,linksControl:true,clickToGo:true,motionTracking:false,motionTrackingControl:false,panControl:true,zoomControl:true});
   pano.setPano(response.data.location.pano);pano.setPov({heading,pitch:0});pano.setVisible(true);mode='street';document.body.classList.add('arrivalOnStreet');
   $('#roadPosition').textContent='Street View · '+(response.data.imageDate||'Recorded imagery');
  }catch{if(active&&token===epoch)host.toast('No street imagery here. Continue above and try further along.');}
  finally{if(token===epoch){loading=false;sync();}}
 }
 async function above(){
  if(mode!=='street')return;const p=pano?.getPosition();if(p)metres=path.nearest({lat:p.lat(),lng:p.lng()});heading=pano?.getPov()?.heading??path.heading(metres);
  pano?.setVisible(false);mode='above';document.body.classList.remove('arrivalOnStreet');$('#roadPosition').textContent='Beside Prestige Waterford';sync();await host.move(camera(),reduced()?0:1600);
 }
 function cleanup(){if(!active)return;pause();epoch++;loading=false;active=false;pano?.setVisible(false);root.hidden=true;document.body.classList.remove('arrivalActive','arrivalOnStreet');host.restore();launch.focus();}
 $('#roadSpeed').oninput=e=>{speed=Number(e.target.value);$('#roadSpeedValue').textContent=Number.isInteger(speed)?speed+'×':speed.toFixed(2).replace(/0$/,'')+'×';};
 $('#roadPlay').onclick=()=>{epoch++;if(intro)pause();else play();};$('#roadGround').onclick=street;$('#roadAbove').onclick=above;$('#roadReturn').onclick=above;$('#roadExit').onclick=cleanup;launch.onclick=begin;
 document.addEventListener('keydown',e=>{if(active&&e.key==='Escape')cleanup();});document.addEventListener('visibilitychange',()=>{if(document.hidden&&active){epoch++;pause();}});
 matchMedia('(prefers-reduced-motion: reduce)').addEventListener('change',()=>{epoch++;pause();});
 return {pause(){epoch++;pause();},active:()=>active};
}
