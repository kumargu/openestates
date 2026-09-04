import assert from "node:assert/strict";
import test from "node:test";
import {
  activeWorkspaceCompareIds,
  activeWorkspaceView,
  workspaceBuyVsRentHref,
  workspaceCompareHref,
  workspaceFocusedHomeId,
  workspaceNavItems,
  workspacePlanReplacementId,
  shouldShowWorkspaceSidebar,
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
    discoveryHref: "/?q=quiet+3bhk",
    discoveryResultCount: 18,
    hasDiscoveryContext: true,
  });
  const byView = new Map(items.map((item) => [item.view, item]));

  assert.equal(byView.get("browse")?.label, "Back to 18 results");
  assert.equal(byView.get("browse")?.icon, "back");
  assert.equal(byView.get("browse")?.to, "/?q=quiet+3bhk");
  assert.equal(byView.get("home")?.label, "This property");
  assert.equal(byView.get("notebook")?.label, "Workspace");
  assert.equal(byView.get("home")?.to, "/property/home%20one%2Fwith%20slash");
  assert.equal(byView.get("rera")?.to, "/property/home%20one%2Fwith%20slash/rera");
  assert.equal(byView.get("rera")?.label, "RERA");
  assert.equal(byView.get("plan")?.to, "/workspace/buy-vs-rent/home%20one%2Fwith%20slash");
  assert.equal(byView.get("plan")?.label, "EMI Plan");
  assert.equal(byView.get("rera")?.active, true);
  assert.equal(byView.get("home")?.active, false);
  assert.equal(byView.get("home")?.available, true);
  assert.equal(byView.get("rera")?.available, true);
  assert.equal(byView.get("plan")?.available, true);
});

test("sidebar never invents a current property", () => {
  const items = workspaceNavItems("", "browse");
  const byView = new Map(items.map((item) => [item.view, item]));

  assert.equal(byView.get("browse")?.available, true);
  assert.equal(byView.get("notebook")?.available, true);
  assert.equal(byView.get("home")?.available, false);
  assert.equal(byView.get("rera")?.available, false);
  assert.equal(byView.get("plan")?.available, false);
  assert.equal(byView.get("browse")?.label, "Explore");
  assert.equal(byView.get("browse")?.icon, "browse");
});

test("property context keeps a generic return when a legacy discovery has no count", () => {
  const returnItem = workspaceNavItems("home-1", "home", {
    discoveryHref: "/?q=near+metro",
    hasDiscoveryContext: true,
  }).find((item) => item.view === "browse");

  assert.equal(returnItem?.label, "Back to results");
  assert.equal(returnItem?.icon, "back");
});

test("property tools preserve a carried search context", () => {
  const items = workspaceNavItems("home one", "home", {
    propertySearchContext: {
      id: "context one",
      queryFingerprint: "qsearch1",
      selectedId: "home one",
    },
  });
  const byView = new Map(items.map((item) => [item.view, item]));

  assert.equal(
    byView.get("home")?.to,
    "/property/home%20one?context=context+one&qf=qsearch1&searchHome=home+one",
  );
  assert.equal(
    byView.get("rera")?.to,
    "/property/home%20one/rera?context=context+one&qf=qsearch1&searchHome=home+one",
  );
  assert.equal(
    byView.get("plan")?.to,
    "/workspace/buy-vs-rent/home%20one?context=context+one&qf=qsearch1&searchHome=home+one",
  );
  assert.equal(
    byView.get("notebook")?.to,
    "/workspace?context=context+one&qf=qsearch1&searchHome=home+one",
  );
});

test("workspace keeps the active search span", () => {
  const items = workspaceNavItems("home-1", "notebook", {
    propertySearchContext: {
      id: "span-1",
      queryFingerprint: "qspan1",
      selectedId: "home-1",
    },
  });
  const byView = new Map(items.map((item) => [item.view, item]));

  assert.equal(
    byView.get("notebook")?.to,
    "/workspace?context=span-1&qf=qspan1&searchHome=home-1",
  );
  assert.equal(byView.has("compare"), false);
});

test("workspace remains active for its notes and compare modes", () => {
  const compareItems = workspaceNavItems("home-1", "compare");
  assert.equal(compareItems.some((item) => item.view === "compare"), false);
  assert.equal(compareItems.find((item) => item.view === "notebook")?.active, true);

  const planItems = workspaceNavItems("home-1", "plan");
  assert.equal(planItems.find((item) => item.view === "plan")?.active, true);
  assert.equal(planItems.find((item) => item.view === "notebook")?.active, false);
});

test("sidebar rows stay stable inside the workspace", () => {
  const options = {
    discoveryHref: "/?q=near+metro",
    hasDiscoveryContext: true,
  };
  const items = workspaceNavItems("home-1", "notebook", options);
  assert.deepEqual(items.map((item) => item.label), ["Back to results", "This property", "RERA", "EMI Plan", "Workspace"]);
  for (const view of ["home", "compare", "rera", "plan"] as const) {
    assert.deepEqual(
      workspaceNavItems("home-1", view, options).map((item) => item.label),
      items.map((item) => item.label),
    );
  }
  assert.equal(items[0]?.to, "/?q=near+metro");
  assert.equal(items[2]?.to, "/property/home-1/rera");
  assert.equal(items[2]?.available, true);
  assert.equal(items[3]?.to, "/workspace/buy-vs-rent/home-1");
  assert.equal(items[3]?.available, true);
  assert.equal(items[4]?.to, "/workspace");
  assert.equal(items[4]?.active, true);
  assert.equal(items.some((item) => item.label === "Compare"), false);
  assert.equal(items.some((item) => item.label === "This home"), false);
  assert.equal(items.some((item) => item.label === "Buy vs Rent"), false);
});

test("workspace RERA is disabled until a home is selected", () => {
  const rera = workspaceNavItems("", "notebook")
    .find((item) => item.view === "rera");
  assert.equal(rera?.available, false);
});

test("landing shows workspace navigation only after a home is saved", () => {
  assert.equal(shouldShowWorkspaceSidebar("landing", 0), false);
  assert.equal(shouldShowWorkspaceSidebar("discovery", 0), false);
  assert.equal(shouldShowWorkspaceSidebar("landing", 1), true);
  assert.equal(shouldShowWorkspaceSidebar("discovery", 2), true);
  assert.equal(shouldShowWorkspaceSidebar("property-context", 0), true);
  assert.equal(shouldShowWorkspaceSidebar("workspace", 0), true);
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

test("one active comparison selection is shared by URL and workspace state", () => {
  assert.deepEqual(
    activeWorkspaceCompareIds(["linked-home", "second-home"], ["saved-home"]),
    ["linked-home", "second-home"],
  );
  assert.deepEqual(
    activeWorkspaceCompareIds([], ["saved-home", "second-home", "saved-home"]),
    ["saved-home", "second-home"],
  );
});

test("Buy vs Rent repairs missing and stale home routes", () => {
  assert.equal(workspacePlanReplacementId(undefined, ["home-one", "home-two"]), "home-one");
  assert.equal(workspacePlanReplacementId("stale-home", ["home-one", "home-two"]), "home-one");
  assert.equal(workspacePlanReplacementId("home-two", ["home-one", "home-two"]), null);
  assert.equal(workspacePlanReplacementId("stale-home", []), null);
});
