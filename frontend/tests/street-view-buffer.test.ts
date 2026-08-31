import assert from "node:assert/strict";
import test from "node:test";

import { GoogleStreetViewAdapter } from "../src/hooks/useGuidedStreetViewTour.ts";
import type { StreetViewFrame } from "../src/lib/streetViewTour.ts";

class FakePanorama {
  panos: string[] = [];
  povs: { heading: number; pitch: number }[] = [];
  visibility: boolean[] = [];

  setPano(pano: string) {
    this.panos.push(pano);
  }

  setPov(pov: { heading: number; pitch: number }) {
    this.povs.push(pov);
  }

  setVisible(visible: boolean) {
    this.visibility.push(visible);
  }
}

function fakePane(active: boolean) {
  const classes = new Set(active ? ["is-active"] : []);
  const attributes = new Map<string, string>();
  let animations = 0;
  const pane = {
    animate: () => {
      animations += 1;
      return { cancel: () => undefined } as Animation;
    },
    classList: {
      add: (value: string) => classes.add(value),
      remove: (value: string) => classes.delete(value),
    },
    inert: !active,
    setAttribute: (name: string, value: string) => attributes.set(name, value),
  } as unknown as HTMLDivElement;
  return { attributes, classes, get animations() { return animations; }, pane };
}

function frame(pano: string, heading: number): StreetViewFrame {
  return {
    links: [],
    pano,
    panoramaPosition: { latitude: 12.98, longitude: 77.74 },
    waypoint: { latitude: 12.98, longitude: 77.74, heading, offsetM: 0 },
  };
}

test("Street View buffers the next panorama in memory before cross-fading", () => {
  const firstPane = fakePane(true);
  const secondPane = fakePane(false);
  const firstPanorama = new FakePanorama();
  const secondPanorama = new FakePanorama();
  const adapter = new GoogleStreetViewAdapter([
    {
      pane: firstPane.pane,
      pano: "pano-a",
      panorama: firstPanorama,
    },
    {
      pane: secondPane.pane,
      pano: "pano-b",
      panorama: secondPanorama,
    },
  ], 280);

  adapter.preload(frame("pano-c", 30), 30);
  assert.deepEqual(secondPanorama.panos, ["pano-c"]);
  assert.equal(firstPane.classes.has("is-active"), true);

  adapter.showPreloaded(frame("pano-c", 30), 15);
  assert.equal(firstPane.classes.has("is-active"), false);
  assert.equal(secondPane.classes.has("is-active"), true);
  assert.equal(firstPane.pane.inert, true);
  assert.equal(secondPane.pane.inert, false);
  assert.equal(firstPane.animations, 1);
  assert.equal(secondPane.animations, 1);

  adapter.preload(frame("pano-d", 45), 45);
  assert.deepEqual(firstPanorama.panos, ["pano-d"]);
  adapter.stop();
  adapter.showPreloaded(frame("pano-d", 45), 45);
  assert.equal(secondPane.classes.has("is-active"), true);

  adapter.resume();
  adapter.showPreloaded(frame("pano-d", 45), 45);
  assert.equal(firstPane.classes.has("is-active"), true);
  adapter.hide();
  assert.deepEqual(firstPanorama.visibility, [false]);
  assert.deepEqual(secondPanorama.visibility, [false]);
});
