import assert from "node:assert/strict";
import test from "node:test";

import {
  ArrivalPlaybackController,
  type ArrivalPlaybackClock,
} from "../src/lib/arrivalPlayback.ts";

class FakeClock implements ArrivalPlaybackClock {
  time = 0;
  nextId = 1;
  tasks = new Map<number, { at: number; callback: () => void }>();

  now = () => this.time;
  setTimeout = (callback: () => void, milliseconds: number) => {
    const id = this.nextId++;
    this.tasks.set(id, { at: this.time + milliseconds, callback });
    return id;
  };
  clearTimeout = (id: number) => {
    this.tasks.delete(id);
  };

  advance(milliseconds: number) {
    const target = this.time + milliseconds;
    while (true) {
      const due = [...this.tasks.entries()]
        .filter(([, task]) => task.at <= target)
        .sort((left, right) => left[1].at - right[1].at)[0];
      if (!due) break;
      this.time = due[1].at;
      this.tasks.delete(due[0]);
      due[1].callback();
    }
    this.time = target;
  }
}

test("cancelled arrival runs cannot fire stale timers or move renderers", async () => {
  const clock = new FakeClock();
  const controller = new ArrivalPlaybackController(clock);
  let mapStops = 0;
  let panoramaStops = 0;
  controller.registerStopper(() => { mapStops += 1; });
  controller.registerStopper(() => { panoramaStops += 1; });
  const run = controller.begin("revealing");
  run.activate();
  const wait = run.wait(7_000);

  clock.advance(2_000);
  controller.cancel("settled");
  clock.advance(10_000);

  assert.equal(await wait, false);
  assert.equal(run.isCurrent(), false);
  assert.equal(controller.snapshot(), "settled");
  assert.ok(mapStops >= 1);
  assert.ok(panoramaStops >= 1);
});

test("pause and resume preserve the remaining wait instead of restarting", async () => {
  const clock = new FakeClock();
  const controller = new ArrivalPlaybackController(clock);
  const run = controller.begin("playing");
  run.activate();
  const wait = run.wait(1_000);

  clock.advance(400);
  controller.pause();
  assert.equal(controller.remainingWaitMs(), 600);
  clock.advance(5_000);
  assert.equal(controller.snapshot(), "paused");
  controller.resume();
  clock.advance(599);
  assert.equal(controller.snapshot(), "playing");
  clock.advance(1);

  assert.equal(await wait, true);
  assert.equal(run.isCurrent(), true);
});

test("async preparation cannot override pause and resumes registered renderers", async () => {
  const clock = new FakeClock();
  const controller = new ArrivalPlaybackController(clock);
  let resumes = 0;
  controller.registerResumer(() => { resumes += 1; });
  const run = controller.begin("playing");

  controller.pause();
  assert.equal(run.activate(), true);
  assert.equal(controller.snapshot(), "paused");
  const wait = run.wait(100);
  clock.advance(500);

  controller.resume();
  assert.equal(resumes, 1);
  assert.equal(controller.snapshot(), "playing");
  clock.advance(100);
  assert.equal(await wait, true);
});
