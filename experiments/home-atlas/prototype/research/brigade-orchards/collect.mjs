import { mkdir, writeFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const BOUNDARY_WAY_ID = 843854047;
const OSM_API = 'https://api.openstreetmap.org/api/0.6';
const OVERPASS_APIS = [
  'https://overpass-api.de/api/interpreter',
  'https://overpass.kumi.systems/api/interpreter',
];
const OUTPUT = resolve(
  dirname(fileURLToPath(import.meta.url)),
  '../../web/brigade/inventory.json',
);
const EARTH_RADIUS_M = 6_371_008.8;

function buildQuery({ south, west, north, east }) {
  const bbox = `${south},${west},${north},${east}`;
  return `[out:json][timeout:45];
(
  nwr(${bbox})["name"];
  way(${bbox})["highway"];
  way(${bbox})["building"];
);
out center tags geom;`;
}

async function json(response) {
  if (!response.ok) throw new Error(`${response.status} ${response.statusText}: ${await response.text()}`);
  return response.json();
}

function radians(value) {
  return value * Math.PI / 180;
}

function distanceMetres(a, b) {
  const dLat = radians(b.lat - a.lat);
  const dLng = radians(b.lng - a.lng);
  const lat1 = radians(a.lat);
  const lat2 = radians(b.lat);
  const h = Math.sin(dLat / 2) ** 2 + Math.cos(lat1) * Math.cos(lat2) * Math.sin(dLng / 2) ** 2;
  return 2 * EARTH_RADIUS_M * Math.asin(Math.sqrt(h));
}

function bounds(points) {
  const latitudes = points.map(point => point.lat);
  const longitudes = points.map(point => point.lng);
  const south = Math.min(...latitudes);
  const north = Math.max(...latitudes);
  const west = Math.min(...longitudes);
  const east = Math.max(...longitudes);
  return {
    south,
    west,
    north,
    east,
    center: { lat: (south + north) / 2, lng: (west + east) / 2 },
    widthM: Math.round(distanceMetres({ lat: (south + north) / 2, lng: west }, { lat: (south + north) / 2, lng: east })),
    heightM: Math.round(distanceMetres({ lat: south, lng: (west + east) / 2 }, { lat: north, lng: (west + east) / 2 })),
  };
}

function polygonMetrics(points) {
  const closed = points[0]?.lat === points.at(-1)?.lat && points[0]?.lng === points.at(-1)?.lng
    ? points
    : [...points, points[0]];
  const latitude = radians(closed.reduce((sum, point) => sum + point.lat, 0) / closed.length);
  const scaleX = EARTH_RADIUS_M * Math.cos(latitude) * Math.PI / 180;
  const scaleY = EARTH_RADIUS_M * Math.PI / 180;
  let twiceArea = 0;
  let perimeterM = 0;
  for (let index = 1; index < closed.length; index += 1) {
    const a = { x: closed[index - 1].lng * scaleX, y: closed[index - 1].lat * scaleY };
    const b = { x: closed[index].lng * scaleX, y: closed[index].lat * scaleY };
    twiceArea += a.x * b.y - b.x * a.y;
    perimeterM += Math.hypot(b.x - a.x, b.y - a.y);
  }
  return { areaSqM: Math.round(Math.abs(twiceArea) / 2), perimeterM: Math.round(perimeterM) };
}

function contains(point, polygon) {
  let inside = false;
  for (let current = 0, previous = polygon.length - 1; current < polygon.length; previous = current++) {
    const a = polygon[current];
    const b = polygon[previous];
    const intersects = ((a.lat > point.lat) !== (b.lat > point.lat)) &&
      point.lng < (b.lng - a.lng) * (point.lat - a.lat) / (b.lat - a.lat) + a.lng;
    if (intersects) inside = !inside;
  }
  return inside;
}

async function queryOverpass(query) {
  const errors = [];
  for (const endpoint of OVERPASS_APIS) {
    try {
      const response = await fetch(endpoint, {
        method: 'POST',
        headers: {
          'content-type': 'application/x-www-form-urlencoded',
          'user-agent': 'OpenEstates spatial research',
        },
        body: new URLSearchParams({ data: query }),
      });
      return await json(response);
    } catch (error) {
      errors.push(`${endpoint}: ${error.message}`);
    }
  }
  throw new Error(`All Overpass endpoints failed:\n${errors.join('\n')}`);
}

function category(tags) {
  if (tags.highway) return 'road';
  if (tags.building) return 'building';
  if (tags.amenity || tags.leisure || tags.tourism || tags.shop) return 'amenity';
  if (tags.landuse === 'residential') return 'precinct';
  return 'named-context';
}

function selectedTags(tags = {}) {
  return Object.fromEntries(Object.entries(tags).filter(([key]) => [
    'access', 'amenity', 'building', 'building:levels', 'developer', 'highway', 'landuse',
    'leisure', 'name', 'oneway', 'residential', 'service', 'shop', 'sport', 'surface', 'tourism',
  ].includes(key)));
}

function normalize(element) {
  const geometry = element.geometry?.map(point => ({ lat: point.lat, lng: point.lon })) ??
    (Number.isFinite(element.lat) ? [{ lat: element.lat, lng: element.lon }] : []);
  const center = element.center
    ? { lat: element.center.lat, lng: element.center.lon }
    : geometry.length ? bounds(geometry).center : undefined;
  return {
    id: `${element.type}/${element.id}`,
    osmType: element.type,
    osmId: element.id,
    category: category(element.tags ?? {}),
    name: element.tags?.name ?? null,
    center,
    geometry,
    tags: selectedTags(element.tags),
  };
}

const boundaryResponse = await json(await fetch(`${OSM_API}/way/${BOUNDARY_WAY_ID}/full.json`, {
  headers: { 'user-agent': 'OpenEstates spatial research' },
}));
const boundaryWay = boundaryResponse.elements.find(element => element.type === 'way' && element.id === BOUNDARY_WAY_ID);
const nodes = new Map(boundaryResponse.elements.filter(element => element.type === 'node').map(node => [node.id, node]));
const boundary = boundaryWay.nodes.map(id => {
  const node = nodes.get(id);
  if (!node) throw new Error(`Boundary node ${id} is missing`);
  return { lat: node.lat, lng: node.lon };
});

const siteBounds = bounds(boundary);
const query = buildQuery(siteBounds);
const overpassResponse = await queryOverpass(query);

const features = [...new Map(overpassResponse.elements
  .filter(element => !(element.type === 'way' && element.id === BOUNDARY_WAY_ID))
  .map(normalize)
  .filter(feature => feature.center && contains(feature.center, boundary))
  .map(feature => [feature.id, feature])).values()]
  .sort((left, right) => left.category.localeCompare(right.category) || (left.name ?? '').localeCompare(right.name ?? '') || left.id.localeCompare(right.id));
const metrics = polygonMetrics(boundary);
const categoryCounts = Object.fromEntries([...new Set(features.map(feature => feature.category))]
  .sort().map(name => [name, features.filter(feature => feature.category === name).length]));

const inventory = {
  schemaVersion: '1.0',
  generatedAt: new Date().toISOString(),
  source: {
    provider: 'OpenStreetMap',
    license: 'ODbL 1.0',
    boundaryUrl: `https://www.openstreetmap.org/way/${BOUNDARY_WAY_ID}`,
    query,
    osmBaseTimestamp: overpassResponse.osm3s?.timestamp_osm_base ?? null,
  },
  site: {
    id: `way/${BOUNDARY_WAY_ID}`,
    name: boundaryWay.tags?.name ?? 'Brigade Orchards',
    center: siteBounds.center,
    bounds: siteBounds,
    boundary,
    areaSqM: metrics.areaSqM,
    areaAcres: Number((metrics.areaSqM / 4046.8564224).toFixed(1)),
    perimeterM: metrics.perimeterM,
    tags: selectedTags(boundaryWay.tags),
  },
  summary: {
    featureCount: features.length,
    categoryCounts,
    namedFeatureCount: features.filter(feature => feature.name).length,
    mappedBuildingFootprints: features.filter(feature => feature.category === 'building').length,
    namedRoads: [...new Set(features.filter(feature => feature.category === 'road' && feature.name).map(feature => feature.name))],
    namedPrecincts: [...new Set(features.filter(feature => feature.category === 'precinct' && feature.name).map(feature => feature.name))],
    namedAmenities: [...new Set(features.filter(feature => feature.category === 'amenity' && feature.name).map(feature => feature.name))],
  },
  features,
};

await mkdir(dirname(OUTPUT), { recursive: true });
await writeFile(OUTPUT, `${JSON.stringify(inventory, null, 2)}\n`);
console.log(JSON.stringify({ output: OUTPUT, site: inventory.site, summary: inventory.summary }, null, 2));
