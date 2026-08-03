import assert from "node:assert/strict";
import test from "node:test";
import {
  activeWorkspaceView,
  workspaceBuyVsRentHref,
  workspaceCompareHref,
  workspaceFocusedHomeId,
  workspaceNavItems,
  workspacePlanReplacementId,
} from "../src/lib/workspaceNav.ts";

test("workspace view detection includes RERA property reports", () => {
  assert.equal(activeWorkspaceView("/"), "browse");
  assert.equal(activeWorkspaceView("/workspace"), "notebook");
  assert.equal(activeWorkspaceView("/workspace/compare"), "compare");
  assert.equal(activeWorkspaceView("/workspace/buy-vs-rent"), "plan");
  assert.equal(activeWorkspaceView("/workspace/buy-vs-rent/home-one"), "plan");
  assert.equal(activeWorkspaceView("/property/discovered-prestige-waterford-1bhk"), "home");
  assert.equal(activeWorkspaceView("/property/discovered-prestige-waterford-1bhk/rera"), "rera");
  assert.equal(activeWorkspaceView("/property/discovered-prestige-waterford-1bhk/plan"), "plan");
});

test("workspace nav follows the focused home across property views", () => {
  const items = workspaceNavItems("home one/with slash", "rera", {
    mode: "property-context",
    discoveryHref: "/?q=quiet+3bhk",
  });
  const byView = new Map(items.map((item) => [item.view, item]));

  assert.equal(byView.get("browse")?.label, "Explore");
  assert.equal(byView.get("browse")?.to, "/?q=quiet+3bhk");
  assert.equal(byView.get("home")?.label, "Property overview");
  assert.equal(byView.get("notebook")?.label, "Workspace");
  assert.equal(byView.get("home")?.to, "/property/home%20one%2Fwith%20slash");
  assert.equal(byView.get("rera")?.to, "/property/home%20one%2Fwith%20slash/rera");
  assert.equal(byView.get("rera")?.label, "RERA evidence");
  assert.equal(byView.get("rera")?.active, true);
  assert.equal(byView.get("home")?.active, false);
  assert.equal(byView.get("home")?.available, true);
  assert.equal(byView.get("rera")?.available, true);
  assert.equal(byView.has("plan"), false);
});

test("property context never invents a current home", () => {
  const items = workspaceNavItems("", "browse", { mode: "property-context" });
  const byView = new Map(items.map((item) => [item.view, item]));

  assert.equal(byView.get("browse")?.available, true);
  assert.equal(byView.get("notebook")?.available, true);
  assert.equal(byView.get("home")?.available, false);
  assert.equal(byView.get("rera")?.available, false);
});

test("workspace stays active for compare and Buy vs Rent views", () => {
  for (const view of ["compare", "plan"] as const) {
    const workspace = workspaceNavItems("home-1", view)
      .find((item) => item.view === "notebook");
    assert.equal(workspace?.active, true);
  }
});

test("workspace sidebar keeps RERA attached to the focused home", () => {
  const items = workspaceNavItems("home-1", "notebook", {
    mode: "workspace",
    discoveryHref: "/?q=near+metro",
  });
  assert.deepEqual(items.map((item) => item.label), ["Explore", "Workspace", "RERA evidence"]);
  assert.equal(items[0]?.to, "/?q=near+metro");
  assert.equal(items[2]?.to, "/property/home-1/rera");
  assert.equal(items[2]?.available, true);
  assert.equal(items.some((item) => item.label === "This home"), false);
  assert.equal(items.some((item) => item.label === "Buy vs Rent"), false);
});

test("workspace RERA is disabled until a home is selected", () => {
  const rera = workspaceNavItems("", "notebook", { mode: "workspace" })
    .find((item) => item.view === "rera");
  assert.equal(rera?.available, false);
});

test("workspace view links preserve explicit property context", () => {
  assert.equal(
    workspaceCompareHref(["home one", "home-two"], "home one"),
    "/workspace/compare?ids=home+one%2Chome-two&focus=home+one",
  );
  assert.equal(
    workspaceBuyVsRentHref("home one/with slash"),
    "/workspace/buy-vs-rent/home%20one%2Fwith%20slash",
  );
  assert.equal(workspaceBuyVsRentHref(), "/workspace/buy-vs-rent");
});

test("workspace focus stays aligned with the selected saved home", () => {
  const available = ["home-one", "home-two"];
  assert.equal(workspaceFocusedHomeId("home-two", "home-one", available), "home-two");
  assert.equal(workspaceFocusedHomeId(null, "home-two", available), "home-two");
  assert.equal(workspaceFocusedHomeId("stale", "home-one", available), "home-one");
  assert.equal(workspaceFocusedHomeId(null, null, available), "home-one");
  assert.equal(workspaceFocusedHomeId(null, null, []), "");
});

test("compare links discard focus outside the compared homes", () => {
  assert.equal(
    workspaceCompareHref(["home-one", "home-two"], "home-three"),
    "/workspace/compare?ids=home-one%2Chome-two",
  );
});

test("Buy vs Rent repairs missing and stale home routes", () => {
  assert.equal(workspacePlanReplacementId(undefined, ["home-one", "home-two"]), "home-one");
  assert.equal(workspacePlanReplacementId("stale-home", ["home-one", "home-two"]), "home-one");
  assert.equal(workspacePlanReplacementId("home-two", ["home-one", "home-two"]), null);
  assert.equal(workspacePlanReplacementId("stale-home", []), null);
});
