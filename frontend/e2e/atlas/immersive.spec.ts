import { expect, test } from '@playwright/test';
import { projectCameraPointToScreen } from '../../src/lib/atlas/screenFit.ts';

// Archived property facts, real Google Maps camera and terrain. No renderer mock.
test('desktop: lake illumination, continuous reveal, interrupted flight and road light', async ({page}) => {
  test.setTimeout(120_000);
  await page.setViewportSize({width: 1600, height: 1000});
  const errors: string[] = [];
  page.on('pageerror', error => errors.push(error.message));
  await page.emulateMedia({reducedMotion: 'reduce'});
  await page.goto('/property/discovered-prestige-waterford-3bhk', {waitUntil: 'domcontentloaded'});
  const map = page.locator('gmp-map-3d');
  await expect(map).toHaveAttribute('data-google-initialized', 'true', {timeout: 45_000});
  await expect(page.locator('[data-map-renderer="google-3d"]')).toHaveAttribute('aria-busy', 'false', {timeout: 30_000});
  const assertHomeFramed = async () => {
    await expect(async () => {
    await expect(map).toHaveAttribute('data-atlas-safe-frame', /width/);
    await expect.poll(() => map.evaluate(element => Math.abs(
      JSON.parse((element as HTMLElement).dataset.atlasSafeFrame!).width - element.getBoundingClientRect().width))).toBeLessThan(1);
    const view = await map.evaluate(element => {
      const camera = element as HTMLElement & {center: {lat: number; lng: number; altitude: number}; range: number; tilt: number; heading: number; fov: number};
      const outline = element.querySelector('[data-atlas-polygon-id]') as HTMLElement & {path: {lat: number; lng: number}[]};
      // Google coordinates expose prototype getters; copy numeric fields before
      // crossing Playwright's serialization boundary.
      return {camera: {center: {lat: camera.center.lat, lng: camera.center.lng, altitude: camera.center.altitude},
        range: camera.range, tilt: camera.tilt, heading: camera.heading},
        fov: camera.fov, frame: JSON.parse(camera.dataset.atlasSafeFrame!),
        boundary: outline.path.map(point => ({lat: point.lat, lng: point.lng}))};
    });
    const points = view.boundary.map(point => projectCameraPointToScreen(view.camera, point, view.frame, view.fov));
    for (const point of points) {
      expect(point.x).toBeGreaterThanOrEqual(view.frame.left + 50);
      expect(point.x).toBeLessThanOrEqual(view.frame.width - view.frame.right - 50);
      expect(point.y).toBeGreaterThanOrEqual(view.frame.top + 50);
      expect(point.y).toBeLessThanOrEqual(view.frame.height - view.frame.bottom - 50);
    }
    }).toPass({timeout: 5000});
  };
  await assertHomeFramed();
  await page.setViewportSize({width: 1366, height: 900});
  await assertHomeFramed();
  await page.setViewportSize({width: 1600, height: 1000});
  await assertHomeFramed();
  await page.getByRole('button', {name: 'Photos', exact: true}).click();
  await assertHomeFramed();
  await page.locator('.property-atlas__drawer > header > button').click();
  await assertHomeFramed();
  // Quiet surroundings and the sourced home outline are on before any interaction.
  const veil = map.locator('[data-atlas-spotlight-kind="veil"]');
  await expect(veil).toHaveCount(1);
  await expect(page.getByRole('button', {name: 'Site outline', exact: true})).toHaveAttribute('aria-pressed', 'true');
  await expect.poll(() => veil.evaluate(element =>
    (element as HTMLElement & {innerPaths?: unknown[]}).innerPaths?.length ?? 0)).toBeGreaterThan(0);
  const quietHome = await map.evaluate(element => {
    const mask = element.querySelector('[data-atlas-spotlight-kind="veil"]') as HTMLElement & {fillColor: string; innerPaths: {lat: number; lng: number}[][]};
    const outline = element.querySelector('[data-atlas-polygon-id]') as HTMLElement & {path: {lat: number; lng: number}[]};
    const copyPoint = (point: {lat: number; lng: number}) => ({lat: point.lat, lng: point.lng});
    return {fill: mask.fillColor.toLowerCase(), openings: mask.innerPaths.map(path => path.map(copyPoint)), boundary: outline.path.map(copyPoint)};
  });
  expect(quietHome.fill).toBe('#061b28b0');
  expect(quietHome.openings).toEqual([[...quietHome.boundary].reverse()]);
  await page.emulateMedia({reducedMotion: 'no-preference'});
  await page.getByRole('button', {name: 'Lakes', exact: true}).click();
  await expect(map).toHaveAttribute('data-atlas-flight-stage', 'settled', {timeout: 15_000});
  const places = page.locator('.property-atlas__place-list > div > button');
  const count = await places.count();
  expect(count).toBeGreaterThan(1);
  await expect(map.locator('[data-atlas-place-id]')).toHaveCount(Math.min(3, count));
  await expect(veil).toHaveCount(1);
  expect(await veil.evaluate(element => (element as HTMLElement & {fillColor: string}).fillColor.toLowerCase())).toBe('#061b2866');
  await expect(map.locator('[data-atlas-spotlight-kind="halo"], [data-atlas-spotlight-kind="pulse"]')).toHaveCount(0);
  const lakePolygons = map.locator('[data-atlas-polygon-kind="lake"]');
  expect(await lakePolygons.count()).toBeGreaterThan(0);
  const originalPolygons = await lakePolygons.evaluateAll(elements => elements.map(element => ({
    id: (element as HTMLElement).dataset.atlasPolygonId,
    path: (element as HTMLElement & {path: {lat: number; lng: number}[]}).path.map(point => ({lat: point.lat, lng: point.lng})),
  })));
  const home = map.locator('[data-atlas-home]');
  const assertHomePresence = async () => {
    const presence = await home.evaluate(element => {
      const marker = element as HTMLElement & {sizePreserved: boolean; drawsWhenOccluded: boolean};
      // Maps3D rasterizes the pin into WebGL; its source DOM has no layout box.
      const pin = marker.querySelector('gmp-pin') as HTMLElement & {scale: number};
      return {sizePreserved: marker.sizePreserved, drawsWhenOccluded: marker.drawsWhenOccluded,
        scale: pin.scale};
    });
    expect(presence.sizePreserved && presence.drawsWhenOccluded).toBe(true);
    expect(presence.scale).toBeGreaterThanOrEqual(1.5);
    const placeScales = await map.locator('[data-atlas-place-id] gmp-pin').evaluateAll(pins =>
      pins.map(pin => (pin as HTMLElement & {scale: number}).scale));
    expect(presence.scale).toBeGreaterThan(Math.max(...placeScales));
  };
  await assertHomePresence();
  expect(await map.locator('[data-atlas-place-id]').evaluateAll(markers => markers.every(m =>
    (m as HTMLElement & {sizePreserved: boolean; drawsWhenOccluded: boolean}).sizePreserved
      && (m as HTMLElement & {drawsWhenOccluded: boolean}).drawsWhenOccluded))).toBe(true);

  for (const index of [0, count - 1]) {
    await places.nth(index).click();
    await expect(map).toHaveAttribute('data-atlas-flight-stage', 'reveal', {timeout: 10_000});
    await expect(map).toHaveAttribute('data-atlas-flight-stage', 'settled', {timeout: 10_000});
    await expect(map.locator('[data-atlas-place-id]')).toHaveCount(Math.min(3, count) + (index >= 3 ? 1 : 0));
    await expect(map.locator('[data-atlas-selected="true"]')).toHaveCount(1);
    await expect(veil).toHaveCount(1);
    expect(await veil.evaluate(element => (element as HTMLElement & {fillColor: string}).fillColor.toLowerCase())).toBe('#061b287a');
    await expect(map.locator('[data-atlas-relationship]')).toHaveCount(1);
    const tilt = await map.evaluate(m => (m as HTMLElement & {tilt: number}).tilt);
    expect(tilt).toBeGreaterThan(50);
    await assertHomePresence();
    await assertHomeFramed();
    await expect(map.locator('[data-atlas-spotlight-kind="halo"], [data-atlas-spotlight-kind="pulse"]')).toHaveCount(0);
    expect(await lakePolygons.evaluateAll(elements => elements.map(element => ({
      id: (element as HTMLElement).dataset.atlasPolygonId,
      path: (element as HTMLElement & {path: {lat: number; lng: number}[]}).path.map(point => ({lat: point.lat, lng: point.lng})),
    })))).toEqual(expect.arrayContaining(originalPolygons));
  }
  await expect(page.getByRole('button', {name: 'Look closer', exact: true})).toHaveCount(0);
  await expect(page.getByRole('button', {name: 'With home', exact: true})).toHaveCount(0);
  await expect(page.getByRole('button', {name: 'Replay view', exact: true})).toHaveCount(0);
  await places.first().click();
  await expect(map).toHaveAttribute('data-atlas-flight-stage', 'context');
  // A manual gesture must stop the pending reveal, not just the current flight.
  await page.mouse.move(700, 400);
  await page.mouse.wheel(0, -100);
  await page.waitForTimeout(900);
  const stopped = await map.evaluate(m => {
    const c = m as HTMLElement & {range: number; heading: number; tilt: number};
    return [c.range, c.heading, c.tilt];
  });
  await page.waitForTimeout(3500);
  const later = await map.evaluate(m => {
    const c = m as HTMLElement & {range: number; heading: number; tilt: number};
    return [c.range, c.heading, c.tilt];
  });
  later.forEach((value, i) => expect(value).toBeCloseTo(stopped[i], 1));

  await page.getByRole('button', {name: 'Tour lakes', exact: true}).click();
  await expect(map).toHaveAttribute('data-atlas-scene', /tour:.*overview/);
  await expect(map).toHaveAttribute('data-atlas-depth', 'pair', {timeout: 12_000});
  await page.getByRole('button', {name: 'Pause tour', exact: true}).click();
  await expect(page.getByRole('button', {name: 'Resume tour', exact: true})).toBeVisible();
  await page.getByRole('button', {name: 'End tour', exact: true}).click();
  // Rapid mode switches cannot hand the camera back to an old selection.
  await places.first().click();
  await page.getByRole('button', {name: 'Approach road', exact: true}).click();
  await expect(map).toHaveAttribute('data-atlas-camera-owner', 'road');
  await page.waitForTimeout(4000);
  await expect(map).toHaveAttribute('data-atlas-camera-owner', 'road');
  await expect(veil).toHaveCount(1);
  await page.getByRole('button', {name: 'Home', exact: true}).click();
  await page.emulateMedia({reducedMotion: 'reduce'});
  await page.getByRole('button', {name: 'Lakes', exact: true}).click();
  await places.first().click();
  await expect(map).toHaveAttribute('data-atlas-flight-stage', 'settled');
  expect(errors).toEqual([]);
});


test('nearby starts with three by distance and follows settled list scrolling', async ({page}) => {
  test.setTimeout(90_000);
  await page.setViewportSize({width: 1600, height: 1000});
  await page.emulateMedia({reducedMotion: 'reduce'});
  await page.goto('/property/discovered-prestige-waterford-3bhk');
  const map = page.locator('gmp-map-3d');
  await expect(map).toHaveAttribute('data-google-initialized', 'true', {timeout: 45_000});
  await page.getByRole('button', {name: 'Schools', exact: true}).click();
  const list = page.getByRole('region', {name: 'Places ordered by distance'});
  const rows = list.locator(':scope > div');
  const markers = map.locator('[data-atlas-place-id]');
  const count = await rows.count();
  expect(count).toBeGreaterThan(3);
  await expect(markers).toHaveCount(3);
  const distances = await list.locator('.property-atlas__place-distance').allTextContents();
  const metres = distances.map(text => parseFloat(text) * (text.includes('km') ? 1000 : 1));
  expect(metres).toEqual([...metres].sort((a, b) => a - b));
  const initial = await markers.evaluateAll(elements => elements.map(el => (el as HTMLElement).dataset.atlasPlaceId));
  const layout = await list.evaluate(element => {
    const rect = element.getBoundingClientRect();
    const rows = Array.from(element.children).map(child => child.getBoundingClientRect());
    return {thirdBottom: rows[2].bottom, fourthTop: rows[3].top, bottom: rect.bottom,
      overflow: element.scrollHeight > element.clientHeight};
  });
  expect(layout.overflow).toBe(true);
  expect(layout.thirdBottom).toBeLessThanOrEqual(layout.bottom);
  expect(layout.fourthTop).toBeLessThan(layout.bottom);
  const heading = await map.evaluate(el => (el as HTMLElement & {heading: number}).heading);
  await list.evaluate(element => { element.scrollTop = element.scrollHeight; });
  await expect.poll(() => markers.evaluateAll(elements => elements.map(el => (el as HTMLElement).dataset.atlasPlaceId)))
    .not.toEqual(initial);
  await expect(markers).toHaveCount(3);
  await expect(map).toHaveAttribute('data-atlas-flight-stage', 'settled');
  expect(await map.evaluate(el => (el as HTMLElement & {heading: number}).heading)).toBeCloseTo(heading, 3);
  await page.getByRole('button', {name: 'Home', exact: true}).click();
  await page.getByRole('button', {name: 'Schools', exact: true}).click();
  await expect.poll(() => markers.evaluateAll(elements => elements.map(el => (el as HTMLElement).dataset.atlasPlaceId)))
    .toEqual(initial);
  // Manual map controls retain ownership even if the list subsequently scrolls.
  await page.getByRole('button', {name: 'Zoom in', exact: true}).click();
  await list.evaluate(element => { element.scrollTop = element.scrollHeight; });
  await page.waitForTimeout(600);
  expect(await markers.evaluateAll(elements => elements.map(el => (el as HTMLElement).dataset.atlasPlaceId))).toEqual(initial);
  // A queued scroll cannot resurrect a category after the user leaves it.
  await page.getByRole('button', {name: 'Show together', exact: true}).click();
  await list.evaluate(element => { element.scrollTop = 0; });
  await page.getByRole('button', {name: 'Home', exact: true}).click();
  await page.waitForTimeout(600);
  await expect(map).toHaveAttribute('data-atlas-camera-owner', 'society');
});


test('near and far selected places both illuminate their area and Home', async ({page}) => {
  test.setTimeout(90_000);
  await page.setViewportSize({width: 1600, height: 1000});
  await page.emulateMedia({reducedMotion: 'reduce'});
  await page.goto('/property/discovered-prestige-waterford-3bhk');
  const map = page.locator('gmp-map-3d');
  await expect(map).toHaveAttribute('data-google-initialized', 'true', {timeout: 45_000});
  await page.getByRole('button', {name: 'Schools', exact: true}).click();
  const rows = page.locator('.property-atlas__place-list > div > button');
  for (const index of [0, (await rows.count()) - 1, 1]) {
    await rows.nth(index).click();
    await expect(map).toHaveAttribute('data-atlas-flight-stage', 'settled');
    await expect.poll(() => map.evaluate(element => {
      type Point = {lat: number; lng: number};
      const veil = element.querySelector('[data-atlas-spotlight-kind="veil"]') as HTMLElement & {fillColor: string; innerPaths: Point[][] | null};
      const contains = (point: Point, path: Point[]) => path.reduce((inside, current, index) => {
        const previous = path[(index + path.length - 1) % path.length];
        const crosses = (current.lat > point.lat) !== (previous.lat > point.lat)
          && point.lng < (previous.lng - current.lng) * (point.lat - current.lat) / (previous.lat - current.lat) + current.lng;
        return crosses ? !inside : inside;
      }, false);
      const markers = ['[data-atlas-home]', '[data-atlas-selected="true"]'].map(selector =>
        element.querySelector(selector) as HTMLElement & {position: Point});
      return Boolean(veil?.fillColor.toLowerCase() === '#061b287a' && markers.every(marker =>
        marker && veil.innerPaths?.some(path => contains(marker.position, path))));
    })).toBe(true);
    await expect(map.locator('[data-atlas-spotlight-kind="halo"]')).toHaveCount(1);
    await expect(map.locator('[data-atlas-spotlight-kind="pulse"]')).toHaveCount(0);
  }
});


test('Metro combines repeated scene stations into one row and marker per place', async ({page}) => {
  test.setTimeout(60_000);
  await page.setViewportSize({width: 1600, height: 1000});
  await page.emulateMedia({reducedMotion: 'reduce'});
  await page.goto('/property/discovered-prestige-waterford-3bhk');
  const map = page.locator('gmp-map-3d');
  await expect(map).toHaveAttribute('data-google-initialized', 'true', {timeout: 45_000});
  await page.getByRole('button', {name: 'Metro', exact: true}).click();
  const rows = page.locator('.property-atlas__place-list > div > button');
  await expect(rows).toHaveCount(2);
  const names = await rows.locator('strong').allTextContents();
  expect(new Set(names).size).toBe(names.length);
  await expect(map.locator('[data-atlas-place-id]')).toHaveCount(2);
  await rows.last().click();
  await expect(map.locator('[data-atlas-selected="true"]')).toHaveCount(1);
  await expect(rows).toHaveCount(2);
});

test('Top view owns the camera after a pending list scroll', async ({page}) => {
  await page.setViewportSize({width: 1600, height: 1000});
  await page.emulateMedia({reducedMotion: 'reduce'});
  await page.goto('/property/discovered-prestige-waterford-3bhk');
  const map = page.locator('gmp-map-3d');
  await expect(map).toHaveAttribute('data-google-initialized', 'true', {timeout: 45_000});
  await page.getByRole('button', {name: 'Schools', exact: true}).click();
  const list = page.getByRole('region', {name: 'Places ordered by distance'});
  const ids = () => map.locator('[data-atlas-place-id]').evaluateAll(elements => elements.map(el => (el as HTMLElement).dataset.atlasPlaceId));
  await expect(map.locator('[data-atlas-place-id]')).toHaveCount(3);
  const initial = await ids();
  await list.evaluate(element => { element.scrollTop = element.scrollHeight; });
  await page.getByRole('button', {name: 'Top view', exact: true}).click();
  await page.waitForTimeout(650);
  expect(await ids()).toEqual(initial);
  await expect.poll(() => map.evaluate(el => (el as HTMLElement & {tilt: number}).tilt)).toBeCloseTo(12, 1);
});
