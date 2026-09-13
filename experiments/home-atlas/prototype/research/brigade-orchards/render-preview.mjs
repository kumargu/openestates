import { readFile, writeFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const directory = dirname(fileURLToPath(import.meta.url));
const inventory = JSON.parse(await readFile(
  resolve(directory, '../../web/brigade/inventory.json'),
  'utf8',
));
const output = resolve(directory, 'osm-preview.svg');
const width = 1000;
const height = 1320;
const margin = 84;
const { south, west, north, east } = inventory.site.bounds;
const latitudeScale = Math.cos(inventory.site.center.lat * Math.PI / 180);
const spanX = (east - west) * latitudeScale;
const spanY = north - south;
const scale = Math.min((width - margin * 2) / spanX, (height - margin * 2) / spanY);
const contentWidth = spanX * scale;
const contentHeight = spanY * scale;
const offsetX = (width - contentWidth) / 2;
const offsetY = (height - contentHeight) / 2;

const escape = value => String(value).replace(/[&<>"']/g, character => ({
  '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&apos;',
})[character]);

function project(point) {
  return {
    x: offsetX + (point.lng - west) * latitudeScale * scale,
    y: offsetY + (north - point.lat) * scale,
  };
}

function path(points, close = false) {
  if (!points?.length) return '';
  const projected = points.map(project);
  return `M ${projected.map(point => `${point.x.toFixed(1)} ${point.y.toFixed(1)}`).join(' L ')}${close ? ' Z' : ''}`;
}

const buildings = inventory.features.filter(feature => feature.category === 'building' && feature.geometry.length > 2);
const precincts = inventory.features.filter(feature => feature.category === 'precinct' && feature.id !== inventory.site.id && feature.geometry.length > 2);
const roads = inventory.features.filter(feature => feature.category === 'road' && feature.geometry.length > 1);
const amenities = inventory.features.filter(feature => feature.category === 'amenity' && feature.center);
const namedPrecincts = precincts.filter(feature => feature.name && feature.name !== inventory.site.name);
const labelledAmenities = new Set([
  'Brigade Orchard School',
  'Hanuman Temple',
  'Signature club Tennis court',
  'Tamarind restaurant',
  'The Arcade',
]);

const svg = `<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${width} ${height}" role="img" aria-labelledby="title description">
  <title id="title">Brigade Orchards OpenStreetMap research preview</title>
  <description id="description">Mapped society boundary, precincts, roads, building footprints, and selected amenities.</description>
  <rect width="100%" height="100%" fill="#0b1210"/>
  <g opacity="0.95">
    <path d="${path(inventory.site.boundary, true)}" fill="#17241d" stroke="#d9f6ad" stroke-width="4"/>
    ${precincts.map(feature => `<path d="${path(feature.geometry, true)}" fill="#5f806a" fill-opacity="0.12" stroke="#78a98a" stroke-opacity="0.5" stroke-width="1.5"/>`).join('\n    ')}
    ${buildings.map(feature => `<path d="${path(feature.geometry, true)}" fill="#b8c3bb" fill-opacity="0.33" stroke="#dbe4dd" stroke-opacity="0.28" stroke-width="0.8"/>`).join('\n    ')}
    ${roads.map(feature => `<path d="${path(feature.geometry)}" fill="none" stroke="${feature.name ? '#efb35b' : '#8d7659'}" stroke-opacity="${feature.name ? '0.95' : '0.62'}" stroke-width="${feature.name ? '3.5' : '2'}" stroke-linecap="round" stroke-linejoin="round"/>`).join('\n    ')}
  </g>
  <g font-family="Inter, ui-sans-serif, system-ui, sans-serif">
    ${namedPrecincts.map(feature => {
      const point = project(feature.center);
      return `<g transform="translate(${point.x.toFixed(1)} ${point.y.toFixed(1)})"><circle r="4" fill="#d9f6ad"/><text x="9" y="4" fill="#eaf3ec" font-size="13" font-weight="650" paint-order="stroke" stroke="#0b1210" stroke-width="4">${escape(feature.name.replace('Brigade Orchards-', ''))}</text></g>`;
    }).join('\n    ')}
    ${amenities.map(feature => {
      const point = project(feature.center);
      const label = labelledAmenities.has(feature.name)
        ? `<text x="9" y="4" fill="#9de9dc" font-size="11" paint-order="stroke" stroke="#0b1210" stroke-width="3">${escape(feature.name)}</text>`
        : '';
      return `<g transform="translate(${point.x.toFixed(1)} ${point.y.toFixed(1)})"><circle r="5" fill="#6dd7c3" stroke="#0b1210" stroke-width="2"/>${label}</g>`;
    }).join('\n    ')}
    <g transform="translate(44 46)">
      <text fill="#d9f6ad" font-size="28" font-weight="750">BRIGADE ORCHARDS</text>
      <text y="27" fill="#9eaaa3" font-size="13">OSM research preview · not a legal parcel map</text>
    </g>
    <g transform="translate(44 ${height - 42})" fill="#9eaaa3" font-size="12">
      <text>${inventory.site.areaAcres} mapped acres · ${inventory.site.bounds.heightM} m N–S · ${inventory.site.bounds.widthM} m E–W · ${inventory.summary.mappedBuildingFootprints} building footprints</text>
    </g>
    <g transform="translate(${width - 250} 42)" font-size="11">
      <circle cx="0" cy="0" r="5" fill="#d9f6ad"/><text x="11" y="4" fill="#d4ded7">Precinct</text>
      <circle cx="79" cy="0" r="5" fill="#6dd7c3"/><text x="90" y="4" fill="#d4ded7">Amenity</text>
      <line x1="158" y1="0" x2="181" y2="0" stroke="#efb35b" stroke-width="4"/><text x="189" y="4" fill="#d4ded7">Road</text>
    </g>
  </g>
</svg>
`;

await writeFile(output, svg);
console.log(output);
