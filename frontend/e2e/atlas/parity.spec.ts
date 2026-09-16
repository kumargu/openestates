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


test("records complete category tours without remounting the explorer", async ({page}, testInfo) => {
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

  await atlas.screenshot({path: testInfo.outputPath("01-home.png")});

  for (const category of ["Schools", "Metro", "Lakes"]) {
    await atlas.getByRole("button", {name: category, exact: true}).click();
    await expect(map).toHaveAttribute("data-atlas-visibility", "category");
    await atlas.screenshot({path: testInfo.outputPath(`${category.toLowerCase()}-overview.png`)});
    await atlas.getByRole("button", {name: "Start tour", exact: true}).click();
    await expect(map).toHaveAttribute("data-atlas-depth", "pair", {timeout: 15_000});
    await expect(map).toHaveAttribute("data-atlas-visibility", "pair");
    await expect(map).toHaveAttribute("data-atlas-marker-count", "2");
    await expect(map.locator(":scope > gmp-marker-3d-interactive")).toHaveCount(2);
    await expect(map.locator(":scope > gmp-marker-3d-interactive[label]")).toHaveCount(2);
    await expect.poll(async () => Number(
      await map.getAttribute("data-atlas-camera-target-range"),
    )).toBeGreaterThan(0);
    await atlas.screenshot({path: testInfo.outputPath(`${category.toLowerCase()}-pair.png`)});
    await expect(map).toHaveAttribute("data-atlas-depth", "inspect", {timeout: 15_000});
    await atlas.screenshot({path: testInfo.outputPath(`${category.toLowerCase()}-inspect.png`)});
    await expect(map).toHaveAttribute("data-atlas-depth", "home", {timeout: 90_000});
    await expect(atlas.getByRole("button", {name: "Start tour", exact: true}))
      .toBeVisible({timeout: policy.nearby.returnHomeMs + 2_000});
    await expect(map).toHaveAttribute("data-atlas-instance", instance ?? "");
  }

  const trace = await finishCameraTrace(page, testInfo);
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
    // Destination terrain may be fetched between shots; dwell must never be skipped.
    expect(endMs - transition.atMs).toBeGreaterThanOrEqual(expectedMs * 0.95);
    expect(endMs - transition.atMs).toBeLessThan(expectedMs + 5000);
  }
  expect(errors).toEqual([]);
});
