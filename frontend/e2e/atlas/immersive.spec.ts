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
  await expect(map.locator('[data-atlas-place-id]')).toHaveCount(count);
  await expect(veil).toHaveCount(0);
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
    await expect(map.locator('[data-atlas-place-id]')).toHaveCount(count);
    await expect(map.locator('[data-atlas-selected="true"]')).toHaveCount(1);
    await expect(map.locator('[data-atlas-relationship]')).toHaveCount(1);
    const tilt = await map.evaluate(m => (m as HTMLElement & {tilt: number}).tilt);
    expect(tilt).toBeGreaterThan(50);
    await assertHomePresence();
    await assertHomeFramed();
    await expect(map.locator('[data-atlas-spotlight-kind="halo"], [data-atlas-spotlight-kind="pulse"]')).toHaveCount(0);
    expect(await lakePolygons.evaluateAll(elements => elements.map(element => ({
      id: (element as HTMLElement).dataset.atlasPolygonId,
      path: (element as HTMLElement & {path: {lat: number; lng: number}[]}).path.map(point => ({lat: point.lat, lng: point.lng})),
    })))).toEqual(originalPolygons);
  }
  await expect(page.getByRole('button', {name: 'Look closer', exact: true})).toHaveCount(0);
  await expect(page.getByRole('button', {name: 'With home', exact: true})).toHaveCount(0);
  await page.getByRole('button', {name: 'Replay view', exact: true}).click();
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
