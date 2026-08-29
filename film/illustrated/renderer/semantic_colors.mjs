const MIN_CHANNEL = 64;
const CHANNEL_RANGE = 176;

function hashedColor(id, salt) {
  let hash = 2166136261;
  const value = salt === 0 ? id : `${id}#${salt}`;
  for (let index = 0; index < value.length; index += 1) {
    hash ^= value.charCodeAt(index);
    hash = Math.imul(hash, 16777619);
  }
  const r = MIN_CHANNEL + (hash & 0xff) % CHANNEL_RANGE;
  const g = MIN_CHANNEL + ((hash >>> 8) & 0xff) % CHANNEL_RANGE;
  const b = MIN_CHANNEL + ((hash >>> 16) & 0xff) % CHANNEL_RANGE;
  return { r, g, b, hex: (r << 16) | (g << 8) | b };
}

export function semanticPalette(objectIds) {
  const palette = {};
  const used = new Set();
  for (const id of [...new Set(objectIds)].sort()) {
    let salt = 0;
    let color = hashedColor(id, salt);
    while (used.has(color.hex)) {
      salt += 1;
      color = hashedColor(id, salt);
    }
    used.add(color.hex);
    palette[id] = color;
  }
  return palette;
}
