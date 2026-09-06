use serde::{Deserialize, Serialize};

use super::ast::{semantic_search_fingerprint, CompiledQuery, ConstraintExpr};
use super::intent::{SearchIntent, SourceSpan};

pub type BranchId = String;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", content = "value", rename_all = "camelCase")]
pub enum BoolExpr<T> {
    All(Vec<BoolExpr<T>>),
    Any(Vec<BoolExpr<T>>),
    Not(Box<BoolExpr<T>>),
    Leaf(T),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedEntityHandle {
    pub entity_id: String,
    pub entity_type: String,
    pub display_name: String,
}

#[derive(Debug, Clone)]
pub struct IntentBranch {
    pub branch_id: BranchId,
    pub source_spans: Vec<SourceSpan>,
    pub predicates: ConstraintExpr,
    pub resolved_entities: Vec<ResolvedEntityHandle>,
    pub constraints: SearchIntent,
    pub buyer_summary: String,
    pub compiled_query: CompiledQuery,
}

#[derive(Debug, Clone)]
pub struct CompiledSearchPlan {
    pub root: BoolExpr<BranchId>,
    pub branches: Vec<IntentBranch>,
    pub resolution_gaps: Vec<String>,
    pub semantic_fingerprint: String,
    pub snapshot_identity: String,
}

impl CompiledSearchPlan {
    pub fn single(compiled_query: CompiledQuery, snapshot_identity: impl Into<String>) -> Self {
        let branch_id = "branch-1".to_string();
        let source_span = SourceSpan {
            start: 0,
            end: compiled_query.raw.len(),
            raw_text: compiled_query.raw.clone(),
        };
        let predicates = compiled_query.constraints.clone();
        let constraints = compiled_query.intent.clone();
        let semantic_fingerprint = semantic_search_fingerprint(
            std::slice::from_ref(&predicates),
            std::slice::from_ref(&constraints),
        );
        Self {
            root: BoolExpr::Leaf(branch_id.clone()),
            branches: vec![IntentBranch {
                branch_id,
                source_spans: vec![source_span],
                buyer_summary: predicates.buyer_label(),
                predicates,
                resolved_entities: Vec::new(),
                constraints,
                compiled_query,
            }],
            resolution_gaps: Vec::new(),
            semantic_fingerprint,
            snapshot_identity: snapshot_identity.into(),
        }
    }

    pub fn combine(plans: Vec<Self>, snapshot_identity: impl Into<String>) -> Self {
        let mut branches = Vec::new();
        let mut resolution_gaps = Vec::new();
        for plan in plans {
            for mut branch in plan.branches {
                branch.branch_id = format!("branch-{}", branches.len() + 1);
                branches.push(branch);
            }
            for gap in plan.resolution_gaps {
                if !resolution_gaps.contains(&gap) {
                    resolution_gaps.push(gap);
                }
            }
        }
        let root = if branches.len() == 1 {
            BoolExpr::Leaf(branches[0].branch_id.clone())
        } else {
            BoolExpr::Any(
                branches
                    .iter()
                    .map(|branch| BoolExpr::Leaf(branch.branch_id.clone()))
                    .collect(),
            )
        };
        let predicates = branches
            .iter()
            .map(|branch| branch.predicates.clone())
            .collect::<Vec<_>>();
        let constraints = branches
            .iter()
            .map(|branch| branch.constraints.clone())
            .collect::<Vec<_>>();
        Self {
            root,
            semantic_fingerprint: semantic_search_fingerprint(&predicates, &constraints),
            branches,
            resolution_gaps,
            snapshot_identity: snapshot_identity.into(),
        }
    }
}
