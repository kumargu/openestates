import { useEffect, useMemo, useState } from "react";
import { Helmet } from "react-helmet-async";
import { Link, useNavigate, useParams } from "react-router-dom";
import { getProperties, getProperty } from "../lib/api.ts";
import { PUBLIC_BRAND_NAME } from "../lib/brand.ts";
import type { PropertyCard, PropertyDetailResponse } from "../lib/types.ts";
import { WorkspaceHeader } from "../components/workspace/WorkspaceHeader.tsx";
import { WorkspacePropertySwitcher } from "../components/workspace/WorkspacePropertySwitcher.tsx";
import { useNotebook } from "../hooks/useNotebook.ts";
import {
  workspaceBuyVsRentHref,
  workspaceCompareHref,
  workspacePlanReplacementId,
} from "../lib/workspaceNav.ts";
import { PlanAssumptionRail } from "../features/home-plan/PlanAssumptionRail.tsx";
import { PlanGraph } from "../features/home-plan/PlanGraph.tsx";
import { PlanWhisper } from "../features/home-plan/PlanWhisper.tsx";
import { VerdictBlock } from "../features/home-plan/VerdictBlock.tsx";
import {
  buildBaselinePlanInputs,
  calculateProjection,
  formatCurrency,
  hasPlannablePrice,
  type ConstructionProfile,
  updatePlanInput,
  type EditablePlanInput,
  type PlanInputs,
} from "../features/home-plan/model.ts";
import "../features/home-plan/home-plan.css";
import { BUY_VS_RENT } from "../features/home-plan/labels.ts";
import { DEFAULT_PLAN_MODEL_CONFIG } from "../features/home-plan/modelConfig.ts";
import {
  isExplicitlyReadyStatus,
  parsePlanDate,
} from "../features/home-plan/financeEngine.ts";
import { buildMonthlyPlanVerdict, defaultPlanFocusYear } from "../features/home-plan/monthlyPlanView.ts";
import {
  canPersistPlanDraft,
  clearPlanDraft,
  type PlanStatus,
  readPlanDraft,
  writePlanDraft,
} from "../features/home-plan/planDrafts.ts";

function constructionProfileFor(data: PropertyDetailResponse): ConstructionProfile {
  const asOfDate = new Date().toISOString().slice(0, 10);
  const stateText = [
    data.project_status,
    data.property.possession_status,
    data.rera?.status,
    data.home_state_display,
  ].filter(Boolean).join(" ").toLowerCase();
  const explicitlyReady = [
    data.project_status,
    data.property.possession_status,
    data.rera?.status,
    data.home_state_display,
  ].filter(Boolean).some((value) => (
    isExplicitlyReadyStatus(String(value))
  ));
  const completionDate = data.rera?.completion_date;
  const parsedCompletion = parsePlanDate(completionDate);
  const completionIsFuture = parsedCompletion ? parsedCompletion.getTime() > Date.now() : false;
  const underConstruction = !explicitlyReady && (
    completionIsFuture
    || ["under construction", "under_construction", "ongoing", "new launch"]
      .some((term) => stateText.includes(term))
  );

  return {
    state: underConstruction ? "under_construction" : "ready",
    asOfDate,
    startDate: data.rera?.start_date,
    completionDate,
    dateSource: completionDate ? "rera" : underConstruction ? "estimated" : "not_applicable",
  };
}

function LoadingPlan() {
  return (
    <div className="home-plan-loading" aria-label={BUY_VS_RENT.loading}>
      <div className="home-plan-loading-pane">
        <div />
        <div />
        <div />
      </div>
    </div>
  );
}

function propertyLabel(home: PropertyCard): string {
  return home.society_name?.trim() || home.title;
}

function propertyMeta(bhk: number, sqft: number, price: number): string {
  return [
    bhk > 0 ? `${bhk} BHK` : null,
    sqft > 0 ? `${sqft.toLocaleString("en-IN")} sqft` : null,
    price > 0 ? formatCurrency(price, true) : "Price unavailable",
  ].filter(Boolean).join(" · ");
}

export function HomePlanPage() {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const { compareIds, propertyIds } = useNotebook();
  const [catalog, setCatalog] = useState<PropertyCard[]>([]);
  const [catalogReady, setCatalogReady] = useState(false);
  const [propertyData, setPropertyData] = useState<PropertyDetailResponse | null>(null);
  const [status, setStatus] = useState<PlanStatus>("loading");
  const [inputs, setInputs] = useState<PlanInputs | null>(null);
  const [previewYear, setPreviewYear] = useState<number | null>(null);
  const [pinnedYear, setPinnedYear] = useState<number | null>(null);
  const [extraEmisPerYear, setExtraEmisPerYear] = useState(
    DEFAULT_PLAN_MODEL_CONFIG.defaults.extraEmisPerYear,
  );
  const [retryKey, setRetryKey] = useState(0);

  useEffect(() => {
    const controller = new AbortController();
    let active = true;
    getProperties({ signal: controller.signal })
      .then((homes) => {
        if (active) setCatalog(homes);
      })
      .catch((error: unknown) => {
        if (error instanceof DOMException && error.name === "AbortError") return;
        if (active) setCatalog([]);
      })
      .finally(() => {
        if (active) setCatalogReady(true);
      });
    return () => {
      active = false;
      controller.abort();
    };
  }, []);

  useEffect(() => {
    if (!id) return undefined;
    let active = true;
    queueMicrotask(() => {
      if (!active) return;
      setStatus("loading");
    });
    getProperty(id)
      .then((data) => {
        if (!active) return;
        setPropertyData(data);
        setPreviewYear(null);
        setPinnedYear(null);
        if (!hasPlannablePrice(data.property.price)) {
          setInputs(null);
          setExtraEmisPerYear(DEFAULT_PLAN_MODEL_CONFIG.defaults.extraEmisPerYear);
          setStatus("no_price");
          return;
        }
        const baseline = buildBaselinePlanInputs(data.property.price, constructionProfileFor(data));
        const draft = readPlanDraft(id);
        setInputs(draft ? {
          ...draft.inputs,
          propertyPriceLakh: baseline.propertyPriceLakh,
          construction: baseline.construction,
        } : baseline);
        setExtraEmisPerYear(
          draft?.extraEmisPerYear ?? DEFAULT_PLAN_MODEL_CONFIG.defaults.extraEmisPerYear,
        );
        setStatus("ready");
      })
      .catch((error: unknown) => {
        if (!active) return;
        const message = error instanceof Error ? error.message : "";
        setStatus(message.includes("404") ? "not_found" : "error");
      });
    return () => { active = false; };
  }, [id, retryKey]);

  const projection = useMemo(
    () => inputs ? calculateProjection(inputs, extraEmisPerYear) : null,
    [inputs, extraEmisPerYear],
  );

  const workspacePropertyIds = [...new Set([...(id ? [id] : []), ...propertyIds])];
  const homeOptions = workspacePropertyIds.flatMap((propertyId) => {
    if (status === "not_found" && propertyId === id) return [];
    const catalogHome = catalog.find((home) => home.id === propertyId);
    if (catalogHome) {
      return [{
        id: catalogHome.id,
        label: propertyLabel(catalogHome),
        meta: propertyMeta(catalogHome.bhk, catalogHome.sqft, catalogHome.price),
      }];
    }
    if (propertyData?.property.id === propertyId) {
      return [{
        id: propertyId,
        label: propertyData.society?.name?.trim() || propertyData.property.title,
        meta: propertyMeta(
          propertyData.property.bhk,
          propertyData.property.super_builtup_sqft,
          propertyData.property.price,
        ),
      }];
    }
    return [];
  });
  const compareHref = workspaceCompareHref(compareIds, id);
  const buyVsRentHref = workspaceBuyVsRentHref(id ?? propertyIds[0]);
  const planReplacementId = catalogReady && (!id || status === "not_found")
    ? workspacePlanReplacementId(id, homeOptions.map((home) => home.id))
    : null;

  useEffect(() => {
    if (!planReplacementId) return;
    navigate(workspaceBuyVsRentHref(planReplacementId), { replace: true });
  }, [navigate, planReplacementId]);

  const selectProperty = (propertyId: string) => {
    if (propertyId) navigate(workspaceBuyVsRentHref(propertyId));
  };

  if (
    !id
    || status !== "ready"
    || propertyData?.property.id !== id
    || !inputs
    || !projection
  ) {
    const propertyIsChanging = Boolean(id)
      && status === "ready"
      && propertyData?.property.id !== id;
    const content = planReplacementId || (!catalogReady && !id) ? (
      <LoadingPlan />
    ) : !id ? (
      <section className="home-plan-empty">
        <h1>Choose a home to plan.</h1>
        <p>Rent vs buy uses the price and status of one home from your workspace.</p>
        <Link to="/">Explore</Link>
      </section>
    ) : status === "loading" || propertyIsChanging ? (
      <LoadingPlan />
    ) : status === "not_found" ? (
      <section className="home-plan-empty">
        <h1>This home is no longer available.</h1>
        <p>Add another home to your workspace and its rent vs buy plan will be ready here.</p>
        <Link to="/">Explore</Link>
      </section>
    ) : status === "no_price" ? (
      <section className="home-plan-empty">
        <h1>We don’t have a price for this home yet.</h1>
        <p>Rent vs buy starts from the asking price. Pick another home in your workspace to plan.</p>
        <Link to="/">Explore</Link>
      </section>
    ) : (
      <section className="home-plan-empty">
        <h1>We couldn’t open this plan.</h1>
        <p>Live property data is temporarily unavailable.</p>
        <div className="home-plan-empty__actions">
          <button
            type="button"
            onClick={() => {
              setStatus("loading");
              setRetryKey((current) => current + 1);
            }}
          >
            Retry
          </button>
          <Link to="/workspace">Back to workspace</Link>
        </div>
      </section>
    );

    return (
      <div className="home-plan-shell home-plan-shell--workspace">
        <Helmet>
          <title>{BUY_VS_RENT.pageTitle} | {PUBLIC_BRAND_NAME}</title>
          <meta name="robots" content="noindex" />
        </Helmet>
        <WorkspaceHeader
          mode="buy-vs-rent"
          compareHref={compareHref}
          buyVsRentHref={buyVsRentHref}
          compareCount={compareIds.length}
          contextDisplay="mobile-only"
          context={homeOptions.length > 0 ? (
            <WorkspacePropertySwitcher
              selectedId={homeOptions.some((home) => home.id === id) ? id : undefined}
              homes={homeOptions}
              onSelect={selectProperty}
            />
          ) : undefined}
        />
        {content}
      </div>
    );
  }

  const property = propertyData.property;
  const baseline = buildBaselinePlanInputs(property.price, constructionProfileFor(propertyData));
  const defaultYear = defaultPlanFocusYear(projection, inputs.holdingPeriodYears);
  const activeYear = previewYear ?? pinnedYear ?? defaultYear;
  const verdict = buildMonthlyPlanVerdict(projection, activeYear);
  const perspectiveYear = pinnedYear ?? defaultYear;
  const perspectiveVerdict = buildMonthlyPlanVerdict(projection, perspectiveYear);
  const perspectiveTheme = projection.extraEmisPerYear > 0
    ? "prepay"
    : perspectiveVerdict.buyWins
      ? "buy"
      : "rent";
  const perspectiveSignature = [
    perspectiveTheme,
    perspectiveYear,
    projection.loanFreeYear ?? "open",
  ].join(":");
  // Drafts capture what the buyer changed, so they are written on edit only.
  const persistEdit = (nextInputs: PlanInputs, nextExtraEmisPerYear: number) => {
    if (!canPersistPlanDraft(id, propertyData.property.id, status)) return;
    writePlanDraft(id, nextInputs, nextExtraEmisPerYear);
  };

  const updateInput = (key: EditablePlanInput, value: number) => {
    const next = updatePlanInput(inputs, key, value);
    setPreviewYear(null);
    setInputs(next);
    persistEdit(next, extraEmisPerYear);
  };

  const updateExtraEmisPerYear = (count: number) => {
    setPreviewYear(null);
    setExtraEmisPerYear(count);
    persistEdit(inputs, count);
  };

  const resetInputs = () => {
    setInputs(baseline);
    setPreviewYear(null);
    setPinnedYear(null);
    setExtraEmisPerYear(DEFAULT_PLAN_MODEL_CONFIG.defaults.extraEmisPerYear);
    clearPlanDraft(id);
  };

  return (
    <div className="home-plan-shell home-plan-shell--workspace">
      <Helmet>
        <title>{property.title} — {BUY_VS_RENT.pageTitle} | {PUBLIC_BRAND_NAME}</title>
        <meta name="description" content={`Compare renting with buying ${property.title} over time.`} />
      </Helmet>

      <WorkspaceHeader
        mode="buy-vs-rent"
        compareHref={compareHref}
        buyVsRentHref={workspaceBuyVsRentHref(id)}
        compareCount={compareIds.length}
        contextDisplay="mobile-only"
        context={(
          <WorkspacePropertySwitcher
            selectedId={id}
            homes={homeOptions}
            onSelect={selectProperty}
          />
        )}
      />

      <div className="home-plan-body">
        <div className="home-plan-main">
          <div className="home-plan-canvas">
            <VerdictBlock verdict={verdict} />

            <PlanAssumptionRail
              inputs={inputs}
              extraEmisPerYear={extraEmisPerYear}
              loanFreeYear={projection.loanFreeYear}
              onInputChange={updateInput}
              onExtraEmisChange={updateExtraEmisPerYear}
              onReset={resetInputs}
            />

            <section className="home-plan-stage" aria-label="Projection over time">
              <PlanGraph
                projection={projection}
                activeYear={verdict.activeYear}
                onPreviewYearChange={setPreviewYear}
                onPinYear={setPinnedYear}
              />
              <PlanWhisper
                key={perspectiveSignature}
                theme={perspectiveTheme}
                activeYear={perspectiveYear}
                loanFreeYear={projection.loanFreeYear}
              />
            </section>
          </div>
        </div>
      </div>

    </div>
  );
}
