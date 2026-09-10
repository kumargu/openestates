import {geoDistance} from './atlas-core.js';

// Keep disconnected OSM segments separate. Never fly across an invented join.
export function createRoadPath(road,{direction='as-mapped'}={}){
 const paths=(road.segments||[]).map(segment=>segment.path).filter(path=>path.length>1);
 const length=path=>path.slice(1).reduce((sum,point,index)=>sum+geoDistance(path[index],point),0);
 const points=paths.sort((left,right)=>length(right)-length(left))[0]?.slice();
 if(!points)throw new Error('No continuous mapped road');
 if(direction==='reverse')points.reverse();
 if(direction!=='as-mapped'&&direction!=='reverse')throw new Error('Road direction must be as-mapped or reverse');

 const distances=[0];
 points.slice(1).forEach((point,index)=>distances.push(distances[index]+geoDistance(points[index],point)));
 const total=distances.at(-1);

 function at(distanceM){
  const metres=Math.max(0,Math.min(total,distanceM));
  let index=1;
  while(index<distances.length-1&&distances[index]<metres)index++;
  const progress=(metres-distances[index-1])/(distances[index]-distances[index-1]||1);
  return {
   lat:points[index-1].lat+(points[index].lat-points[index-1].lat)*progress,
   lng:points[index-1].lng+(points[index].lng-points[index-1].lng)*progress
  };
 }

 return {
  total,
  points,
  at,
  heading:distanceM=>bearing(at(Math.max(0,distanceM-25)),at(Math.min(total,distanceM+65))),
  nearest(point){
   let nearestM=0,distance=Infinity;
   for(let metres=0;metres<=total;metres+=3){
    const next=geoDistance(point,at(metres));
    if(next<distance){distance=next;nearestM=metres;}
   }
   return nearestM;
  }
 };
}

function bearing(from,to){
 const radians=Math.PI/180;
 const latitude1=from.lat*radians,latitude2=to.lat*radians;
 const deltaLongitude=(to.lng-from.lng)*radians;
 const y=Math.sin(deltaLongitude)*Math.cos(latitude2);
 const x=Math.cos(latitude1)*Math.sin(latitude2)-Math.sin(latitude1)*Math.cos(latitude2)*Math.cos(deltaLongitude);
 return (Math.atan2(y,x)/radians+360)%360;
}
