use super::ast::{ConstraintExpr, ConstraintTerm};
use super::compiled_plan::{expand_geo_cells, CompiledSearchPlan, GeoCellSearchPolicy, GeoScope};
use super::engine::{geography_match_for_property, SearchEngineOutput};
use super::{SearchEngine, TypedIntentAst};
use crate::discovery::{
    discovery_config, select_shelf_cards, BrowsePropertyCard, CollectionPlannerConfig,
    CollectionStrategyConfig,
};
use crate::state::SearchRuntimeSnapshot;
use serde::Serialize;
use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JourneyCollection {
    pub id: String,
    pub strategy: String,
    pub title: String,
    pub note: String,
    pub price_band: CollectionPriceBand,
    pub cards: Vec<BrowsePropertyCard>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionPriceBand {
    pub min: u64,
    pub max: u64,
    pub currency: String,
    pub label: String,
}

pub fn plan_collections(
    snapshot: &SearchRuntimeSnapshot,
    exact_plan: &CompiledSearchPlan,
    exact_output: &SearchEngineOutput,
) -> Vec<JourneyCollection> {
    let config = discovery_config();
    let planner = &config.collection_planner;
    let mut used = exact_output
        .results
        .iter()
        .filter_map(|r| snapshot.browse_cards.get(&r.card.id))
        .map(|card| card.society_id.clone())
        .collect::<HashSet<_>>();
    let engine = SearchEngine::new(snapshot);
    let geography = exact_geography_name(snapshot, exact_plan);
    // Strip only resolved positive location predicates. Negation and unresolved
    // required intent are immutable, including when topology is absent.
    let mut broad_ast = TypedIntentAst::from_plan(exact_plan);
    for branch in &mut broad_ast.branches {
        branch.predicates = without_positive_geography(&branch.predicates);
        branch.resolved_entities.retain(|e| {
            !matches!(
                e.entity_type.to_ascii_lowercase().as_str(),
                "area" | "society"
            )
        });
    }
    let Ok(mut broad_plan) = engine.compile_intent_ast(&broad_ast) else {
        return Vec::new();
    };
    broad_plan.refresh_semantic_fingerprint();
    let Some(broad) = engine.execute_collection_plan(broad_plan) else {
        return Vec::new();
    };
    let eligible = broad
        .results
        .iter()
        .map(|r| r.card.id.clone())
        .collect::<HashSet<_>>();
    // Calculate membership independently of result caps, using the same sourced
    // traversal and exact distance limits as the authoritative search compiler.
    let adjacent_scopes = exact_plan
        .branches
        .iter()
        .filter_map(|branch| {
            expanded_scope(snapshot, &branch.geo_scope, planner)
                .map(|scope| (branch.branch_id.clone(), scope))
        })
        .collect::<HashMap<_, _>>();
    let has_topology = !adjacent_scopes.is_empty();
    let in_scope = |scope: &GeoScope, society: &str| {
        geography_match_for_property(
            scope,
            society,
            &snapshot.bundle.graph_index,
            &snapshot.bundle.spatial_index,
            snapshot.bundle.manifest.proof_snapshot_identity(),
        )
        .is_some()
    };
    // Only evaluate geographic membership for eligible societies, once per request.
    let membership = broad
        .results
        .iter()
        .filter_map(|r| snapshot.browse_cards.get(&r.card.id))
        .map(|card| card.society_id.clone())
        .collect::<HashSet<_>>()
        .into_iter()
        .map(|society| {
            let exact = exact_plan
                .branches
                .iter()
                .filter(|b| !b.geo_scope.is_bundle_wide())
                .any(|b| in_scope(&b.geo_scope, &society));
            let nearby_branches = adjacent_scopes
                .iter()
                .filter(|(_, scope)| in_scope(scope, &society))
                .map(|(id, _)| id.clone())
                .collect::<HashSet<_>>();
            (society, (exact, nearby_branches))
        })
        .collect::<HashMap<_, _>>();
    let mut collections = Vec::new();
    for strategy in &planner.strategies {
        if collections.len() >= planner.max_collections {
            break;
        }
        let wider;
        let candidates = match strategy.strategy.as_str() {
            "same_geography_wider_budget"
                if has_topology
                    || exact_plan
                        .branches
                        .iter()
                        .all(|b| b.geo_scope.is_bundle_wide()) =>
            {
                let mut ast = TypedIntentAst::from_plan(exact_plan);
                let mut changed = false;
                for branch in &mut ast.branches {
                    changed |= expand_positive_budget(
                        &mut branch.predicates,
                        planner.budget_expansion_factor,
                    );
                }
                if !changed {
                    continue;
                }
                let Ok(plan) = engine.compile_intent_ast(&ast) else {
                    continue;
                };
                let Some(output) = engine.execute_collection_plan(plan) else {
                    continue;
                };
                wider = output;
                wider
                    .results
                    .iter()
                    .filter_map(|r| snapshot.browse_cards.get(&r.card.id))
                    .collect::<Vec<_>>()
            }
            "adjacent_areas" if has_topology => {
                // Branch-local eligibility matters: a 2BHK alternative cannot
                // borrow the geography of a different 3BHK branch.
                let ids = broad
                    .result_sets
                    .iter()
                    .flat_map(|set| {
                        set.results
                            .iter()
                            .filter(|r| {
                                snapshot
                                    .browse_cards
                                    .get(&r.card.id)
                                    .and_then(|c| membership.get(&c.society_id))
                                    .is_some_and(|(exact, branches)| {
                                        !exact && branches.contains(&set.branch_id)
                                    })
                            })
                            .map(|r| r.card.id.as_str())
                    })
                    .collect::<HashSet<_>>();
                broad
                    .results
                    .iter()
                    .filter(|r| ids.contains(r.card.id.as_str()))
                    .filter_map(|r| snapshot.browse_cards.get(&r.card.id))
                    .collect()
            }
            "other_areas" if has_topology => {
                let ids = broad
                    .result_sets
                    .iter()
                    .filter(|set| adjacent_scopes.contains_key(&set.branch_id))
                    .flat_map(|set| set.results.iter().map(|r| r.card.id.as_str()))
                    .collect::<HashSet<_>>();
                diversify_by_market(
                    snapshot,
                    broad
                        .results
                        .iter()
                        .filter(|r| ids.contains(r.card.id.as_str()))
                        .filter_map(|r| snapshot.browse_cards.get(&r.card.id))
                        .filter(|c| {
                            membership
                                .get(&c.society_id)
                                .is_some_and(|(exact, nearby)| !exact && nearby.is_empty())
                                && (!snapshot
                                    .bundle
                                    .graph_index
                                    .market_memberships(&c.society_id)
                                    .is_empty()
                                    || !snapshot
                                        .bundle
                                        .graph_index
                                        .occupied_cells(&c.society_id)
                                        .is_empty())
                        })
                        .collect(),
                )
            }
            _ => continue,
        };
        let mut seen = used.clone();
        let cards = candidates
            .into_iter()
            .filter(|c| seen.insert(c.society_id.clone()))
            .take(planner.card_limit)
            .cloned()
            .collect::<Vec<_>>();
        if cards.len() < planner.minimum_cards {
            continue;
        }
        used.extend(cards.iter().map(|c| c.society_id.clone()));
        collections.push(build_collection(
            strategy,
            geography.as_deref(),
            cards,
            planner,
        ));
    }
    let mut groups = HashMap::<usize, usize>::new();
    for ranked in &snapshot.discovery_shelves {
        if collections.len() >= planner.max_collections
            || collections.len() >= planner.fallback_count
        {
            break;
        }
        let shelf = &config.shelves[ranked.config_index];
        if groups.get(&ranked.config_index).copied().unwrap_or(0)
            >= shelf.group_by.as_ref().map_or(1, |g| g.limit)
        {
            continue;
        }
        let cards = select_shelf_cards(
            ranked,
            &snapshot.browse_cards,
            &used,
            Some(&eligible),
            planner.card_limit.min(shelf.card_limit),
        );
        if cards.len() < planner.minimum_cards.max(shelf.minimum_cards) {
            continue;
        }
        *groups.entry(ranked.config_index).or_default() += 1;
        used.extend(cards.iter().map(|c| c.society_id.clone()));
        collections.push(JourneyCollection {
            id: format!("fallback:{}", ranked.id),
            strategy: "configured_fallback".to_string(),
            title: ranked.title.clone(),
            note: String::new(),
            price_band: price_band(&cards, planner),
            cards,
        });
    }
    collections
}

fn expand_positive_budget(expression: &mut ConstraintExpr, factor: f64) -> bool {
    match expression {
        ConstraintExpr::And { clauses } | ConstraintExpr::AnyOf { clauses } => {
            let mut changed = false;
            for clause in clauses {
                changed |= expand_positive_budget(clause, factor);
            }
            changed
        }
        ConstraintExpr::Term {
            term: ConstraintTerm::Budget { max: Some(max), .. },
        } => {
            let expanded = ((max.value as f64) * factor).round() as u64;
            let changed = expanded > max.value;
            max.value = expanded;
            changed
        }
        // Exclusions are never broadening opportunities.
        _ => false,
    }
}

fn without_positive_geography(expression: &ConstraintExpr) -> ConstraintExpr {
    match expression {
        ConstraintExpr::And { clauses } => ConstraintExpr::And {
            clauses: clauses.iter().map(without_positive_geography).collect(),
        },
        ConstraintExpr::AnyOf { clauses } => ConstraintExpr::AnyOf {
            clauses: clauses.iter().map(without_positive_geography).collect(),
        },
        ConstraintExpr::Term {
            term:
                ConstraintTerm::Area {
                    entity_id: Some(_), ..
                }
                | ConstraintTerm::Society { .. },
        } => ConstraintExpr::And {
            clauses: Vec::new(),
        },
        _ => expression.clone(),
    }
}

fn expanded_scope(
    snapshot: &SearchRuntimeSnapshot,
    scope: &GeoScope,
    config: &CollectionPlannerConfig,
) -> Option<GeoScope> {
    let GeoScope::Scoped {
        anchors,
        market_locality_ids,
        seed_cells,
        supporting_evidence,
        ..
    } = scope
    else {
        return None;
    };
    if seed_cells.is_empty() {
        return None;
    }
    Some(GeoScope::Scoped {
        anchors: anchors.clone(),
        market_locality_ids: market_locality_ids.clone(),
        seed_cells: seed_cells.clone(),
        expanded_cell_paths: expand_geo_cells(
            anchors,
            seed_cells,
            &snapshot.bundle.graph_index,
            &snapshot.bundle.spatial_index,
            GeoCellSearchPolicy {
                max_hops: config.nearby_max_hops,
                max_distance_km: config.nearby_max_distance_km,
            },
            snapshot.bundle.manifest.proof_snapshot_identity(),
        ),
        supporting_evidence: supporting_evidence.clone(),
        max_distance_km: config.nearby_max_distance_km,
    })
}

fn diversify_by_market<'a>(
    snapshot: &SearchRuntimeSnapshot,
    candidates: Vec<&'a BrowsePropertyCard>,
) -> Vec<&'a BrowsePropertyCard> {
    let mut market_order = Vec::<Option<String>>::new();
    let mut by_market = HashMap::<Option<String>, VecDeque<&BrowsePropertyCard>>::new();
    for card in candidates {
        let market = snapshot
            .bundle
            .graph_index
            .market_memberships(&card.society_id)
            .first()
            .map(|membership| membership.target_entity_id.clone());
        if !by_market.contains_key(&market) {
            market_order.push(market.clone());
        }
        by_market.entry(market).or_default().push_back(card);
    }
    let mut diversified = Vec::new();
    loop {
        let mut added = false;
        for market in &market_order {
            if let Some(card) = by_market.get_mut(market).and_then(VecDeque::pop_front) {
                diversified.push(card);
                added = true;
            }
        }
        if !added {
            return diversified;
        }
    }
}

fn build_collection(
    strategy: &CollectionStrategyConfig,
    geography: Option<&str>,
    cards: Vec<BrowsePropertyCard>,
    config: &CollectionPlannerConfig,
) -> JourneyCollection {
    let price_band = price_band(&cards, config);
    let template = geography
        .map(|_| strategy.title_template.as_str())
        .unwrap_or(&strategy.generic_title_template);
    let title = template
        .replace("{geography}", geography.unwrap_or_default())
        .replace("{note}", &strategy.note)
        .replace("{price_band}", &price_band.label);
    JourneyCollection {
        id: strategy.id.clone(),
        strategy: strategy.strategy.clone(),
        title,
        note: strategy.note.clone(),
        price_band,
        cards,
    }
}

fn price_band(
    cards: &[BrowsePropertyCard],
    config: &CollectionPlannerConfig,
) -> CollectionPriceBand {
    let min = cards
        .iter()
        .map(|card| card.price_min.unwrap_or(card.price))
        .min()
        .unwrap_or(0);
    let max = cards
        .iter()
        .map(|card| card.price_max.unwrap_or(card.price))
        .max()
        .unwrap_or(0);
    let label = format!(
        "₹{}–{} {}",
        format_price(min, config.price_unit),
        format_price(max, config.price_unit),
        config.price_suffix,
    );
    CollectionPriceBand {
        min,
        max,
        currency: config.currency.clone(),
        label,
    }
}

fn format_price(value: u64, unit: u64) -> String {
    let scaled = value as f64 / unit as f64;
    if (scaled - scaled.round()).abs() < 0.05 {
        format!("{scaled:.0}")
    } else {
        format!("{scaled:.1}")
    }
}

fn exact_geography_name(
    snapshot: &SearchRuntimeSnapshot,
    plan: &CompiledSearchPlan,
) -> Option<String> {
    let names = plan
        .branches
        .iter()
        .flat_map(|branch| branch.geo_scope.market_locality_ids())
        .filter_map(|id| snapshot.entity_by_id.get(id))
        .filter_map(|index| snapshot.bundle.entities.get(*index))
        .map(|entity| entity.name.clone())
        .collect::<HashSet<_>>();
    (names.len() == 1)
        .then(|| names.into_iter().next())
        .flatten()
}

pub fn exact_result_set_label(
    snapshot: &SearchRuntimeSnapshot,
    plan: &CompiledSearchPlan,
    branch_id: &str,
    fallback: &str,
) -> String {
    let Some(branch) = plan
        .branches
        .iter()
        .find(|branch| branch.branch_id == branch_id)
    else {
        return fallback.to_string();
    };
    let names = branch
        .geo_scope
        .market_locality_ids()
        .iter()
        .filter_map(|id| snapshot.entity_by_id.get(id))
        .filter_map(|index| snapshot.bundle.entities.get(*index))
        .map(|entity| entity.name.clone())
        .collect::<HashSet<_>>();
    if names.len() == 1 {
        names
            .into_iter()
            .next()
            .unwrap_or_else(|| fallback.to_string())
    } else {
        fallback.to_string()
    }
}
