import assert from "node:assert/strict";
import test from "node:test";
import { activeWorkspaceView, workspaceNavItems } from "../src/lib/workspaceNav.ts";

test("workspace view detection includes RERA property reports", () => {
  assert.equal(activeWorkspaceView("/"), "browse");
  assert.equal(activeWorkspaceView("/workspace"), "notebook");
  assert.equal(activeWorkspaceView("/workspace/compare"), "compare");
  assert.equal(activeWorkspaceView("/property/discovered-prestige-waterford-1bhk"), "home");
  assert.equal(activeWorkspaceView("/property/discovered-prestige-waterford-1bhk/rera"), "rera");
  assert.equal(activeWorkspaceView("/property/discovered-prestige-waterford-1bhk/plan"), "plan");
});

test("workspace nav follows the focused home across property views", () => {
  const items = workspaceNavItems("home one/with slash", "rera");
  const byView = new Map(items.map((item) => [item.view, item]));

  assert.equal(byView.get("home")?.to, "/property/home%20one%2Fwith%20slash");
  assert.equal(byView.get("rera")?.to, "/property/home%20one%2Fwith%20slash/rera");
  assert.equal(byView.get("plan")?.to, "/property/home%20one%2Fwith%20slash/plan");
  assert.equal(byView.get("rera")?.label, "RERA");
  assert.equal(byView.get("rera")?.active, true);
  assert.equal(byView.get("home")?.active, false);
});

test("property-specific workspace links fall back to discovery without focus", () => {
  const items = workspaceNavItems("", "browse");
  const byView = new Map(items.map((item) => [item.view, item]));

  assert.equal(byView.get("home")?.to, "/");
  assert.equal(byView.get("rera")?.to, "/");
  assert.equal(byView.get("plan")?.to, "/");
});
