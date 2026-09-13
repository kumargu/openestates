import { expect, test, type Page, type TestInfo } from "@playwright/test";
import policy from "../../../app/config/ui/home-atlas.json" with {type: "json"};

type CameraSample = {
  atMs: number;
  scene?: string;
  owner?: string;
  visibility?: string;
  depth?: string;
  roadDistanceM?: number;
  roadLengthM?: number;
  center?: { lat: number; lng: number; altitude?: number };
  heading?: number;
  range?: number;
  tilt?: number;
};

async function startCameraTrace(page: Page) {
  await page.evaluate(() => {
    const state = window as unknown as {
      __atlasTrace?: CameraSample[];
      __atlasTraceTimer?: number;
    };
    state.__atlasTrace = [];
    state.__atlasTraceTimer = window.setInterval(() => {
      const map = document.querySelector("gmp-map-3d") as (HTMLElement & {
        center?: { lat: number; lng: number; altitude?: number };
        heading?: number;
        range?: number;
        tilt?: number;
      }) | null;
      if (!map) return;
      state.__atlasTrace?.push({
        atMs: performance.now(),
        scene: map.dataset.atlasScene,
        owner: map.dataset.atlasCameraOwner,
        visibility: map.dataset.atlasVisibility,
        depth: map.dataset.atlasDepth,
        roadDistanceM: Number(map.dataset.atlasRoadDistance) || undefined,
        roadLengthM: Number(map.dataset.atlasRoadLength) || undefined,
        center: map.center ? {...map.center} : undefined,
        heading: map.heading,
        range: map.range,
        tilt: map.tilt,
      });
    }, 100);
  });
}

async function finishCameraTrace(page: Page, testInfo: TestInfo): Promise<CameraSample[]> {
  const trace = await page.evaluate(() => {
    const state = window as unknown as {
      __atlasTrace?: CameraSample[];
      __atlasTraceTimer?: number;
    };
    if (state.__atlasTraceTimer !== undefined) window.clearInterval(state.__atlasTraceTimer);
    return state.__atlasTrace ?? [];
  });
  await testInfo.attach("camera-trace", {
    body: Buffer.from(JSON.stringify(trace, null, 2)),
    contentType: "application/json",
  });
  return trace;
}

async function captureScene(
  page: Page,
  testInfo: TestInfo,
  name: string,
  scene: string,
  timeout = 90_000,
) {
  const map = page.locator("gmp-map-3d");
  await expect(map).toHaveAttribute("data-atlas-scene", scene, {timeout});
  await page.locator(".property-arrival-map--atlas").screenshot({
    path: testInfo.outputPath(`${name}.png`),
  });
}

test("records the complete PR 126 spatial story in the PR 132 shell", async ({page}, testInfo) => {
  test.setTimeout(420_000);
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto("/property/fixture-prestige-waterford-3bhk");
  const atlas = page.locator(".property-arrival-map--atlas");
  const map = atlas.locator("gmp-map-3d");
  await expect(atlas.locator('[data-map-renderer="google-3d"]'))
    .toHaveAttribute("aria-busy", "false", {timeout: 30_000});
  await expect(map).toHaveAttribute("data-google-initialized", "true", {timeout: 30_000});
  await map.evaluate((element) => { element.dataset.atlasInstance = crypto.randomUUID(); });
  const instance = await map.getAttribute("data-atlas-instance");
  await startCameraTrace(page);

  await captureScene(page, testInfo, "01-society", "society:society");
  await atlas.getByRole('button', {name: 'Tour home', exact: true}).click();
  await captureScene(page, testInfo, "02-above", "society:above");
  await captureScene(page, testInfo, "03-neighborhood", "society:neighborhood");

  await atlas.getByRole("button", {name: "Road journey", exact: true}).click();
  await captureScene(page, testInfo, "04-road-context", "road:context", 30_000);
  await captureScene(page, testInfo, "05-road-flight", "road:flight", 30_000);
  await expect.poll(async () => {
    const distance = Number(await map.getAttribute("data-atlas-road-distance"));
    const length = Number(await map.getAttribute("data-atlas-road-length"));
    return length > 0 && distance >= length - 0.5;
  }, {timeout: 150_000}).toBe(true);
  await atlas.screenshot({path: testInfo.outputPath("06-road-arrival.png")});
  await expect(map).toHaveAttribute("data-atlas-instance", instance ?? "");

  for (const category of ["Schools", "Metro", "Lakes"]) {
    await atlas.getByRole("button", {name: category, exact: true}).first().click();
    await expect(map).toHaveAttribute("data-atlas-visibility", "category");
    await atlas.screenshot({path: testInfo.outputPath(`${category.toLowerCase()}-overview.png`)});
    const drawer = atlas.locator(".property-atlas__drawer");
    await drawer.getByRole("button", {name: new RegExp(`^Tour ${category.toLowerCase()}`)}).click();
    await expect(map).toHaveAttribute("data-atlas-depth", "pair", {timeout: 15_000});
    await expect(map).toHaveAttribute("data-atlas-visibility", "pair");
    await expect(map).toHaveAttribute("data-atlas-marker-count", "2");
    await expect(map.locator(":scope > gmp-marker-3d-interactive")).toHaveCount(2);
    await expect(map.locator(":scope > gmp-marker-3d-interactive[label]")).toHaveCount(0);
    await expect.poll(async () => Number(
      await map.getAttribute("data-atlas-camera-target-range"),
    )).toBeGreaterThan(0);
    await atlas.screenshot({path: testInfo.outputPath(`${category.toLowerCase()}-pair.png`)});
    await expect(map).toHaveAttribute("data-atlas-depth", "inspect", {timeout: 15_000});
    await atlas.screenshot({path: testInfo.outputPath(`${category.toLowerCase()}-inspect.png`)});
    await expect(map).toHaveAttribute("data-atlas-depth", "home", {timeout: 90_000});
    await expect(drawer.getByRole("button", {name: new RegExp(`^Tour ${category.toLowerCase()}`)}))
      .toBeVisible({timeout: policy.nearby.returnHomeMs + 2_000});
  }

  const trace = await finishCameraTrace(page, testInfo);
  const roadSamples = trace.filter((sample) => sample.roadDistanceM !== undefined);
  expect(roadSamples.length).toBeGreaterThan(10);
  for (let index = 1; index < roadSamples.length; index += 1) {
    expect(roadSamples[index].roadDistanceM!).toBeGreaterThanOrEqual(
      roadSamples[index - 1].roadDistanceM! - 0.5,
    );
  }
  const transitions = trace.filter((sample, index) =>
    sample.scene !== trace[index - 1]?.scene);
  const expectedDuration = (scene: string): number | undefined => {
    if (scene.endsWith(":overview")) return policy.nearby.overviewMs;
    if (scene.endsWith(":pair")) return policy.nearby.pairMs;
    if (scene.endsWith(":focus")) return policy.nearby.focusMs;
    if (scene.endsWith(":home")) return policy.nearby.returnHomeMs;
    return undefined;
  };
  for (let index = 0; index < transitions.length; index += 1) {
    const transition = transitions[index];
    if (!transition.scene?.startsWith("tour:")) continue;
    const expectedMs = expectedDuration(transition.scene);
    if (!expectedMs) continue;
    const endMs = transitions[index + 1]?.atMs ?? trace.at(-1)!.atMs;
    expect(Math.abs(endMs - transition.atMs - expectedMs) / expectedMs).toBeLessThanOrEqual(0.05);
  }
  expect(errors).toEqual([]);
});
