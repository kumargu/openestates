import { useEffect, useMemo, useState } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import { getProperties, getProperty } from "../lib/api.ts";
import { PUBLIC_BRAND_NAME } from "../lib/brand.ts";
import type { PropertyCard, PropertyDetailResponse } from "../lib/types.ts";
import { PageTitle } from "../components/PageTitle.tsx";
import { useSearchSpan } from "../components/workspace/SearchSpanContext.ts";
import { useNotebook } from "../hooks/useNotebook.ts";
import {
  workspaceBuyVsRentHref,
  workspacePlanReplacementId,
} from "../lib/workspaceNav.ts";
import {
  hrefWithSearchSpan,
  requestSearchSpanReturn,
  searchSpanReferenceForTarget,
} from "../lib/navigationContext.ts";
import {
  PlanAssumptionRail,
  RentAssumptionRail,
} from "../features/home-plan/PlanAssumptionRail.tsx";
import { PlanGraph } from "../features/home-plan/PlanGraph.tsx";
import { RepaymentDashboard } from "../features/home-plan/RepaymentDashboard.tsx";
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
import { calculateRepaymentDashboard } from "../features/home-plan/repaymentModel.ts";
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

export function HomePlanPage() {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const searchSpan = useSearchSpan();
  const { propertyIds } = useNotebook();
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
  const [planMode, setPlanMode] = useState<"repayment" | "rent-vs-buy">("repayment");
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
        setPlanMode("repayment");
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
    () => inputs
      ? calculateProjection(inputs, extraEmisPerYear, DEFAULT_PLAN_MODEL_CONFIG)
      : null,
    [inputs, extraEmisPerYear],
  );
  const repayment = useMemo(
    () => inputs
      ? calculateRepaymentDashboard(inputs, extraEmisPerYear)
      : null,
    [inputs, extraEmisPerYear],
  );

  const workspacePropertyIds = [...new Set([
    ...(id ? [id] : []),
    ...(searchSpan?.selectedId ? [searchSpan.selectedId] : []),
    ...propertyIds,
  ])];
  const availableWorkspacePropertyIds = workspacePropertyIds.filter((propertyId) => {
    if (status === "not_found" && propertyId === id) return false;
    return catalog.some((home) => home.id === propertyId)
      || propertyData?.property.id === propertyId;
  });
  const planReplacementId = catalogReady && (!id || status === "not_found")
    ? workspacePlanReplacementId(id, availableWorkspacePropertyIds)
    : null;
  const replacementPlanHref = planReplacementId
    ? hrefWithSearchSpan(
      workspaceBuyVsRentHref(planReplacementId),
      searchSpanReferenceForTarget(searchSpan, planReplacementId),
    )
    : null;
  const workspaceHref = hrefWithSearchSpan(
    "/workspace",
    searchSpanReferenceForTarget(searchSpan),
  );
  const exploreHref = searchSpan?.returnUrl ?? "/";

  useEffect(() => {
    if (!replacementPlanHref) return;
    navigate(replacementPlanHref, { replace: true });
  }, [navigate, replacementPlanHref]);

  const prepareSearchReturn = () => {
    if (searchSpan) requestSearchSpanReturn(searchSpan);
  };

  if (
    !id
    || status !== "ready"
    || propertyData?.property.id !== id
    || !inputs
    || !projection
    || !repayment
  ) {
    const propertyIsChanging = Boolean(id)
      && status === "ready"
      && propertyData?.property.id !== id;
    const content = planReplacementId || (!catalogReady && !id) ? (
      <LoadingPlan />
    ) : !id ? (
      <section className="home-plan-empty">
        <h1>Choose a home to plan.</h1>
        <p>EMI Plan uses the price of one home from your workspace to model its loan.</p>
        <Link to={exploreHref} onClick={prepareSearchReturn}>Explore</Link>
      </section>
    ) : status === "loading" || propertyIsChanging ? (
      <LoadingPlan />
    ) : status === "not_found" ? (
      <section className="home-plan-empty">
        <h1>This home is no longer available.</h1>
        <p>Add another home to your workspace to inspect its repayment plan.</p>
        <Link to={exploreHref} onClick={prepareSearchReturn}>Explore</Link>
      </section>
    ) : status === "no_price" ? (
      <section className="home-plan-empty">
        <h1>We don’t have a price for this home yet.</h1>
        <p>Loan planning starts from the asking price. Pick another home in your workspace.</p>
        <Link to={exploreHref} onClick={prepareSearchReturn}>Explore</Link>
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
          <Link to={workspaceHref}>Back to workspace</Link>
        </div>
      </section>
    );

    return (
      <div className="home-plan-shell home-plan-shell--workspace">
        <PageTitle title={`${BUY_VS_RENT.pageTitle} | ${PUBLIC_BRAND_NAME}`} />
        <meta name="robots" content="noindex" />
        {content}
      </div>
    );
  }

  const property = propertyData.property;
  const baseline = buildBaselinePlanInputs(property.price, constructionProfileFor(propertyData));
  // Drafts capture what the buyer changed, so they are written on edit only.
  const persistEdit = (
    nextInputs: PlanInputs,
    nextExtraEmisPerYear: number,
  ) => {
    if (!canPersistPlanDraft(id, propertyData.property.id, status)) return;
    writePlanDraft(id, nextInputs, nextExtraEmisPerYear);
  };

  const updateInput = (key: EditablePlanInput, value: number) => {
    const sipMultiple = [1, 2, 3].find((multiple) => (
      Math.abs(inputs.monthlySipThousands - inputs.monthlyEmiThousands * multiple) < 0.01
    ));
    const updated = updatePlanInput(inputs, key, value);
    const next = sipMultiple != null
      && (key === "monthlyEmiThousands" || key === "downPaymentPercent")
      ? {
        ...updated,
        monthlySipThousands: updated.monthlyEmiThousands * sipMultiple,
      }
      : updated;
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
      <PageTitle title={`${property.title} — EMI Plan | ${PUBLIC_BRAND_NAME}`} />
      <meta name="description" content={`Inspect repayment choices for ${property.title}.`} />

      <div className="home-plan-body">
        <div className="home-plan-main">
          <div className="home-plan-canvas">
            <header className="home-plan-property-context">
              <h1>
                {propertyData.society?.name?.trim() || property.title}
                <span> · {formatCurrency(property.price, true)} asking price</span>
              </h1>
              <p>Modelled loan {formatCurrency(projection.loanAmount, true)}</p>
            </header>

            <nav className="home-plan-mode-tabs" aria-label="Plan view">
              <button
                type="button"
                className={planMode === "repayment" ? "is-active" : undefined}
                aria-current={planMode === "repayment" ? "page" : undefined}
                onClick={() => setPlanMode("repayment")}
              >
                Repayment
              </button>
              <button
                type="button"
                className={planMode === "rent-vs-buy" ? "is-active" : undefined}
                aria-current={planMode === "rent-vs-buy" ? "page" : undefined}
                onClick={() => setPlanMode("rent-vs-buy")}
              >
                Rent vs Buy
              </button>
            </nav>

            <PlanAssumptionRail
              inputs={inputs}
              extraEmisPerYear={extraEmisPerYear}
              repaymentMode={planMode === "repayment"}
              onInputChange={updateInput}
              onExtraEmisChange={updateExtraEmisPerYear}
              onReset={resetInputs}
            />

            {planMode === "repayment" ? (
              <RepaymentDashboard
                inputs={inputs}
                model={repayment}
              />
            ) : (
              <section className="home-plan-rent-mode" aria-label="Rent versus buy scenario">
                <RentAssumptionRail
                  inputs={inputs}
                  onInputChange={updateInput}
                  onReset={resetInputs}
                />
                <PlanGraph
                  projection={projection}
                  activeYear={previewYear
                    ?? pinnedYear
                    ?? Math.min(inputs.holdingPeriodYears, projection.points.length - 1)}
                  onPreviewYearChange={setPreviewYear}
                  onPinYear={setPinnedYear}
                />
              </section>
            )}
          </div>
        </div>
      </div>

    </div>
  );
}
