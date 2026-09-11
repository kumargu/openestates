/** Presentation shell: reuses Atlas actions without owning its camera or data. */
export function installAtlasShell(){
 const $=s=>document.querySelector(s),body=document.body;
 body.classList.add('calmAtlas');
 const shell=document.createElement('div');shell.id='atlasShell';
 shell.innerHTML=`<div class="propertyIdentity"><strong>Prestige Waterford</strong><span>Whitefield, Bengaluru</span></div><nav class="atlasDock" aria-label="Explore Waterford"><button id="societyView">Society</button><span id="roadSlot"></span><button id="nearbyToggle" aria-expanded="false" aria-controls="nearbyDrawer">Nearby</button><span class="dockDivider"></span><span id="playSlot"></span><button id="endVisit" hidden aria-label="End guided visit">×</button></nav><div class="atlasUtilities"><button id="layersToggle" aria-expanded="false" aria-controls="layersDrawer">View</button><button id="moreToggle" aria-expanded="false" aria-controls="moreDrawer">Saved</button></div><section id="nearbyDrawer" class="atlasDrawer" aria-label="Nearby places" hidden><div class="drawerHeading"><strong>Explore nearby</strong><button data-close-drawer aria-label="Close nearby">×</button></div><div id="categorySlot"></div><div id="nearbyEmpty">Choose a category to see places around Waterford.</div><div id="placeSlot"></div></section><section id="layersDrawer" class="atlasDrawer utilityDrawer" aria-label="View settings" hidden><div class="drawerHeading"><strong>View</strong><button data-close-drawer aria-label="Close view settings">×</button></div><p class="drawerSection">Map layers</p><div id="layersSlot"></div><p class="drawerSection">Perspective</p><div id="cameraSlot"></div><p class="drawerSection">Display</p><div id="displaySlot"></div></section><section id="moreDrawer" class="atlasDrawer utilityDrawer" aria-label="Saved views" hidden><div class="drawerHeading"><strong>Saved views</strong><button data-close-drawer aria-label="Close saved views">×</button></div><p class="drawerIntro">Return to perspectives that matter while comparing this home.</p><div id="moreSlot"></div></section>`;
 body.append(shell);
 const move=(id,slot)=>$(slot).append($(id));
 move('#arriveWaterford','#roadSlot');move('#pause','#playSlot');move('.exploreTabs','#categorySlot');move('#placePanel','#placeSlot');move('.analysisTools','#layersSlot');move('.sideTools','#cameraSlot');move('.headActions','#moreSlot');
 move('#focus','#displaySlot');
 // Camera modes retain the same action identities; only their presentation changes.
 $('#orbit').innerHTML='Orbit';$('#top').innerHTML='From above';$('#road').innerHTML='Street View';$('#reset').innerHTML='Reset view';$('#focus').textContent='Hide controls';$('#save').textContent='＋ Save current view';
 $('#boundaries').textContent='Society boundaries';$('#neighborhoodMovie').textContent='▶ Tour the neighborhood';
 let panel=null;
 function setPanel(next){panel=next;for(const name of ['nearby','layers','more']){$('#'+name+'Drawer').hidden=panel!==name;$('#'+name+'Toggle').setAttribute('aria-expanded',String(panel===name));}body.dataset.panel=panel||'';}
 for(const name of ['nearby','layers','more'])$('#'+name+'Toggle').onclick=()=>setPanel(panel===name?null:name);
 shell.querySelectorAll('[data-close-drawer]').forEach(b=>b.onclick=()=>setPanel(null));
 $('#societyView').onclick=()=>{setPanel(null);$('#reset').click();};
 $('#arriveWaterford').addEventListener('click',()=>setPanel(null));
 $('#endVisit').onclick=()=>{setPanel(null);$('#reset').click();};
 $('#pause').addEventListener('click',()=>setPanel(null));$('#neighborhoodMovie').addEventListener('click',()=>setPanel(null));$('#tourCategory').addEventListener('click',()=>setPanel(null));
 const refresh=()=>{$('#nearbyEmpty').hidden=!$('#placePanel').hidden;if(!$('#placePanel').hidden&&!body.classList.contains('neighborhoodFilm')&&!body.classList.contains('arrivalActive'))setPanel('nearby');};
 new MutationObserver(refresh).observe($('#placePanel'),{attributes:true,attributeFilter:['hidden']});
 new MutationObserver(()=>{$('#endVisit').hidden=!$('#pause').textContent.includes('Pause')&&!$('#pause').textContent.includes('Resume');}).observe($('#pause'),{childList:true});
 document.addEventListener('keydown',e=>{if(e.key==='Escape')setPanel(null);});
 document.addEventListener('pointerdown',e=>{if(panel&&!shell.contains(e.target))setPanel(null);});
}
