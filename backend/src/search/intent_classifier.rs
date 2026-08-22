use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use fasttext::args::{Args, ModelName};
use fasttext::FastText;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::dag_config::{
    intent_classifier_config, search_resolution_config, IntentClassifierConfig,
    IntentClassifierMode,
};

use super::intent::{Polarity, SourceSpan};
use super::query_plan::{self, ByteSpan, QueryPlan, QueryToken};
use super::schema::{self, PreferencePatternSpec};

const LABEL_PREFIX: &str = "__label__";

#[derive(Debug)]
pub struct FastTextIntentClassifier {
    model: FastText,
    labels: HashMap<String, ClassifierLabel>,
    minimum_probability: f32,
    minimum_margin: f32,
}

#[derive(Debug, Clone)]
struct ClassifierLabel {
    preference: String,
    polarity: Polarity,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntentClassifierTrace {
    pub mode: String,
    pub model_status: String,
    pub residual_clauses: Vec<ResidualClause>,
    pub decisions: Vec<IntentClassifierDecision>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResidualClause {
    pub text: String,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntentClassifierDecision {
    pub clause: ResidualClause,
    pub predictions: Vec<IntentClassifierPrediction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected: Option<IntentClassifierPrediction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub abstention: Option<IntentClassifierAbstention>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntentClassifierPrediction {
    pub label_id: String,
    pub preference: String,
    pub polarity: String,
    pub probability: f32,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IntentClassifierAbstention {
    BelowThreshold,
    AmbiguousMargin,
    ClauseTooLong,
    UnknownLabel,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentClassifierMetadata {
    pub format_version: u32,
    pub created_at: String,
    pub crate_version: String,
    pub model_sha256: String,
    pub config_digest: String,
    pub training_corpus_sha256: String,
    pub training_example_count: usize,
    pub label_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct IntentClassifierTrainingReport {
    pub model_path: String,
    pub model_size_bytes: u64,
    pub training_example_count: usize,
    pub label_count: usize,
    pub ambiguous_seed_count: usize,
    pub training_corpus_sha256: String,
    pub config_digest: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IntentClassifierEvaluationBank {
    pub version: u32,
    pub cases: Vec<IntentClassifierEvaluationCase>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IntentClassifierEvaluationCase {
    pub text: String,
    pub expected_preference: String,
    pub expected_polarity: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct IntentClassifierEvaluationReport {
    pub case_count: usize,
    pub selected_count: usize,
    pub correct_count: usize,
    pub abstained_count: usize,
    pub incorrect_selection_count: usize,
    pub selected_precision: f64,
    pub overall_accuracy: f64,
    pub prediction_p95_micros: u128,
    pub cases: Vec<IntentClassifierEvaluationResult>,
}

#[derive(Debug, Clone, Serialize)]
pub struct IntentClassifierEvaluationResult {
    pub text: String,
    pub expected_label_id: String,
    pub selected_label_id: Option<String>,
    pub top_probability: Option<f32>,
    pub top_margin: Option<f32>,
    pub duration_micros: u128,
    pub outcome: String,
}

impl FastTextIntentClassifier {
    pub fn load(project_root: &Path) -> Result<Option<Self>, String> {
        let config = intent_classifier_config();
        if config.mode == IntentClassifierMode::Disabled {
            return Ok(None);
        }
        let model_path = project_root.join(&config.artifact_path);
        if !model_path.exists() {
            return Ok(None);
        }
        validate_artifact_identity(&model_path, config)?;
        let model = FastText::load_model(&model_path)
            .map_err(|err| format!("failed to load {}: {err}", model_path.display()))?;
        let labels = configured_labels(config)?;
        for label in model.get_labels().0 {
            if !labels.contains_key(&label) {
                return Err(format!("model contains unknown classifier label {label}"));
            }
        }
        Ok(Some(Self {
            model,
            labels,
            minimum_probability: config.minimum_probability,
            minimum_margin: config.minimum_margin,
        }))
    }

    pub(crate) fn classify(
        &self,
        query: &str,
        plan: &QueryPlan,
        resolved_spans: &[SourceSpan],
    ) -> IntentClassifierTrace {
        let extraction = residual_clauses(query, plan, resolved_spans);
        let decisions = extraction
            .clauses
            .iter()
            .cloned()
            .map(|clause| self.classify_clause(clause))
            .collect();
        IntentClassifierTrace {
            mode: classifier_mode_name(&intent_classifier_config().mode).to_string(),
            model_status: "loaded".to_string(),
            residual_clauses: extraction.clauses,
            decisions,
            warning: extraction
                .truncated
                .then(|| "residual clause limit exceeded".to_string()),
        }
    }

    fn classify_clause(&self, clause: ResidualClause) -> IntentClassifierDecision {
        let config = intent_classifier_config();
        if clause.text.split_whitespace().count() > config.maximum_clause_tokens {
            return abstained_decision(
                clause,
                IntentClassifierAbstention::ClauseTooLong,
                Vec::new(),
            );
        }
        let predictions = self
            .model
            .predict(&clause.text, 2, 0.0)
            .into_iter()
            .filter_map(|prediction| {
                self.labels
                    .get(&prediction.label)
                    .map(|label| IntentClassifierPrediction {
                        label_id: prediction.label,
                        preference: label.preference.clone(),
                        polarity: polarity_name(&label.polarity).to_string(),
                        probability: prediction.prob,
                    })
            })
            .collect::<Vec<_>>();
        let Some(top) = predictions.first() else {
            return abstained_decision(
                clause,
                IntentClassifierAbstention::UnknownLabel,
                predictions,
            );
        };
        if top.probability < self.minimum_probability {
            return abstained_decision(
                clause,
                IntentClassifierAbstention::BelowThreshold,
                predictions,
            );
        }
        let margin = top.probability - predictions.get(1).map_or(0.0, |second| second.probability);
        if margin < self.minimum_margin {
            return abstained_decision(
                clause,
                IntentClassifierAbstention::AmbiguousMargin,
                predictions,
            );
        }
        IntentClassifierDecision {
            clause,
            selected: Some(top.clone()),
            predictions,
            abstention: None,
        }
    }
}

pub fn unavailable_classifier_trace(error: Option<String>) -> IntentClassifierTrace {
    IntentClassifierTrace {
        mode: classifier_mode_name(&intent_classifier_config().mode).to_string(),
        model_status: if error.is_some() {
            "invalid".to_string()
        } else {
            "unavailable".to_string()
        },
        residual_clauses: Vec::new(),
        decisions: Vec::new(),
        warning: error,
    }
}

pub fn train_classifier(project_root: &Path) -> Result<IntentClassifierTrainingReport, String> {
    let config = intent_classifier_config();
    let model_path = project_root.join(&config.artifact_path);
    let parent = model_path
        .parent()
        .ok_or_else(|| format!("invalid model path {}", model_path.display()))?;
    fs::create_dir_all(parent)
        .map_err(|err| format!("failed to create {}: {err}", parent.display()))?;

    let corpus = training_corpus(config)?;
    let training_path = parent.join("training.txt");
    fs::write(&training_path, corpus.lines.join("\n") + "\n")
        .map_err(|err| format!("failed to write {}: {err}", training_path.display()))?;

    let training = &config.training;
    let mut args = Args::default();
    args.apply_supervised_defaults();
    args.input = training_path;
    args.model = ModelName::Supervised;
    args.epoch = training.epoch;
    args.lr = training.learning_rate;
    args.dim = training.dimension;
    args.word_ngrams = training.word_ngrams;
    args.minn = training.min_character_ngrams;
    args.maxn = training.max_character_ngrams;
    args.bucket = training.bucket;
    args.thread = training.thread;
    args.seed = training.seed;

    let model = FastText::train(args).map_err(|err| format!("fastText training failed: {err}"))?;
    let temporary_model_path = model_path.with_extension("bin.tmp");
    model
        .save_model(&temporary_model_path)
        .map_err(|err| format!("failed to save model: {err}"))?;
    fs::rename(&temporary_model_path, &model_path)
        .map_err(|err| format!("failed to promote model: {err}"))?;

    let model_bytes =
        fs::read(&model_path).map_err(|err| format!("failed to hash model: {err}"))?;
    let model_sha256 = sha256_bytes(&model_bytes);
    let config_digest = classifier_config_digest(config)?;
    let metadata = IntentClassifierMetadata {
        format_version: 1,
        created_at: chrono::Utc::now().to_rfc3339(),
        crate_version: "fasttext-rs-0.8.0".to_string(),
        model_sha256,
        config_digest: config_digest.clone(),
        training_corpus_sha256: corpus.sha256.clone(),
        training_example_count: corpus.lines.len(),
        label_ids: corpus.label_ids.iter().cloned().collect(),
    };
    fs::write(
        metadata_path(&model_path),
        serde_json::to_vec_pretty(&metadata)
            .map_err(|err| format!("failed to encode model metadata: {err}"))?,
    )
    .map_err(|err| format!("failed to write model metadata: {err}"))?;

    Ok(IntentClassifierTrainingReport {
        model_path: model_path.display().to_string(),
        model_size_bytes: model_bytes.len() as u64,
        training_example_count: corpus.lines.len(),
        label_count: corpus.label_ids.len(),
        ambiguous_seed_count: corpus.ambiguous_seed_count,
        training_corpus_sha256: corpus.sha256,
        config_digest,
    })
}

pub fn evaluate_classifier(
    project_root: &Path,
) -> Result<IntentClassifierEvaluationReport, String> {
    let config = intent_classifier_config();
    let classifier = FastTextIntentClassifier::load(project_root)?
        .ok_or_else(|| "classifier model is unavailable; train it first".to_string())?;
    let bank_path = project_root.join(&config.held_out_path);
    let bank: IntentClassifierEvaluationBank = serde_json::from_slice(
        &fs::read(&bank_path)
            .map_err(|err| format!("failed to read {}: {err}", bank_path.display()))?,
    )
    .map_err(|err| format!("failed to parse {}: {err}", bank_path.display()))?;
    if bank.version != 1 {
        return Err(format!(
            "unsupported intent classifier evaluation version {}",
            bank.version
        ));
    }
    reject_evaluation_leakage(&bank, config)?;

    let mut results = Vec::with_capacity(bank.cases.len());
    for case in bank.cases {
        results.push(evaluate_case(&classifier, case));
    }
    let mut durations = results
        .iter()
        .map(|result| result.duration_micros)
        .collect::<Vec<_>>();
    durations.sort_unstable();
    let prediction_p95_micros = percentile_95(&durations);
    let selected_count = results
        .iter()
        .filter(|result| result.selected_label_id.is_some())
        .count();
    let correct_count = results
        .iter()
        .filter(|result| result.outcome == "correct")
        .count();
    let incorrect_selection_count = results
        .iter()
        .filter(|result| result.outcome == "incorrect")
        .count();
    let case_count = results.len();
    Ok(IntentClassifierEvaluationReport {
        case_count,
        selected_count,
        correct_count,
        abstained_count: case_count.saturating_sub(selected_count),
        incorrect_selection_count,
        selected_precision: ratio(correct_count, selected_count),
        overall_accuracy: ratio(correct_count, case_count),
        prediction_p95_micros,
        cases: results,
    })
}

struct ResidualExtraction {
    clauses: Vec<ResidualClause>,
    truncated: bool,
}

fn residual_clauses(
    query: &str,
    plan: &QueryPlan,
    resolved_spans: &[SourceSpan],
) -> ResidualExtraction {
    let consumed = consumed_spans(plan, resolved_spans);
    let deterministic_preferences = query_plan::deterministic_preference_spans(query, plan);
    let tokens = plan
        .tokens
        .iter()
        .filter(|token| !consumed.iter().any(|span| token_overlaps(token, *span)))
        .filter(|token| !is_structural_filler(&token.text))
        .collect::<Vec<_>>();
    let separators = configured_separators();
    let mut groups = Vec::<Vec<&QueryToken>>::new();
    let mut current = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        if let Some(length) = separator_length_at(&tokens, index, &separators) {
            push_non_empty(&mut groups, &mut current);
            index += length;
            continue;
        }
        if current.last().is_some_and(|previous: &&QueryToken| {
            query[previous.end..tokens[index].start]
                .chars()
                .any(|character| matches!(character, ',' | ';'))
        }) {
            push_non_empty(&mut groups, &mut current);
        }
        current.push(tokens[index]);
        index += 1;
    }
    push_non_empty(&mut groups, &mut current);

    let config = intent_classifier_config();
    let truncated = groups.len() > config.maximum_clauses;
    let clauses = groups
        .into_iter()
        .take(config.maximum_clauses)
        .filter(|tokens| {
            !tokens.iter().any(|token| {
                deterministic_preferences
                    .iter()
                    .any(|span| token_overlaps(token, *span))
            })
        })
        .filter_map(|tokens| residual_clause(query, &tokens))
        .collect();
    ResidualExtraction { clauses, truncated }
}

fn consumed_spans(plan: &QueryPlan, resolved_spans: &[SourceSpan]) -> Vec<ByteSpan> {
    let mut spans = plan
        .owned_spans
        .iter()
        .map(|owned| owned.span)
        .chain(plan.slots.budgets.iter().map(|budget| ByteSpan {
            start: budget.start,
            end: budget.end,
        }))
        .chain(plan.evidence.iter().map(|matched| ByteSpan {
            start: matched.start,
            end: matched.end,
        }))
        .chain(plan.clauses.iter().map(|clause| ByteSpan {
            start: clause.relation_span.start,
            end: clause.target_span.end,
        }))
        .chain(resolved_spans.iter().map(|span| ByteSpan {
            start: span.start,
            end: span.end,
        }))
        .collect::<Vec<_>>();
    spans.sort_by_key(|span| (span.start, span.end));
    spans
}

fn token_overlaps(token: &QueryToken, span: ByteSpan) -> bool {
    token.start < span.end && span.start < token.end
}

fn configured_separators() -> Vec<Vec<String>> {
    let mut separators = intent_classifier_config()
        .clause_separators
        .iter()
        .map(|separator| super::parser::query_tokens(separator))
        .filter(|tokens| !tokens.is_empty())
        .collect::<Vec<_>>();
    separators.sort_by_key(|tokens| std::cmp::Reverse(tokens.len()));
    separators
}

fn separator_length_at(
    tokens: &[&QueryToken],
    index: usize,
    separators: &[Vec<String>],
) -> Option<usize> {
    separators.iter().find_map(|separator| {
        (index + separator.len() <= tokens.len()
            && tokens[index..index + separator.len()]
                .iter()
                .zip(separator)
                .all(|(token, expected)| token.text.eq_ignore_ascii_case(expected)))
        .then_some(separator.len())
    })
}

fn is_structural_filler(token: &str) -> bool {
    schema::query_stopwords()
        .iter()
        .chain(search_resolution_config().ignored_entity_names.iter())
        .chain(search_resolution_config().generic_scope_nouns.iter())
        .any(|candidate| candidate.eq_ignore_ascii_case(token))
}

fn push_non_empty<'a>(groups: &mut Vec<Vec<&'a QueryToken>>, current: &mut Vec<&'a QueryToken>) {
    if !current.is_empty() {
        groups.push(std::mem::take(current));
    }
}

fn residual_clause(query: &str, tokens: &[&QueryToken]) -> Option<ResidualClause> {
    let first = tokens.first()?;
    let last = tokens.last()?;
    let text = tokens
        .iter()
        .map(|token| token.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    (!text.is_empty()).then(|| ResidualClause {
        text,
        span: SourceSpan {
            start: first.start,
            end: last.end,
            raw_text: query[first.start..last.end].to_string(),
        },
    })
}

struct TrainingCorpus {
    lines: Vec<String>,
    label_ids: BTreeSet<String>,
    ambiguous_seed_count: usize,
    sha256: String,
}

fn training_corpus(config: &IntentClassifierConfig) -> Result<TrainingCorpus, String> {
    let labels = configured_label_patterns(config)?;
    let mut seeds = BTreeMap::<String, BTreeSet<String>>::new();
    for (label_id, pattern) in &labels {
        for phrase in classifier_training_phrases(label_id, pattern) {
            for example in augmented_examples(&phrase) {
                seeds.entry(example).or_default().insert(label_id.clone());
            }
        }
    }
    let ambiguous_seed_count = seeds.values().filter(|labels| labels.len() != 1).count();
    let mut lines = Vec::new();
    let mut label_ids = BTreeSet::new();
    for (text, labels) in seeds {
        if labels.len() != 1 {
            continue;
        }
        let label_id = labels.into_iter().next().expect("one label");
        label_ids.insert(label_id.clone());
        lines.push(format!("{label_id} {text}"));
    }
    if label_ids.len() < 2 {
        return Err("classifier training requires at least two usable labels".to_string());
    }
    let sha256 = sha256_bytes((lines.join("\n") + "\n").as_bytes());
    Ok(TrainingCorpus {
        lines,
        label_ids,
        ambiguous_seed_count,
        sha256,
    })
}

fn augmented_examples(phrase: &str) -> Vec<String> {
    let phrase = normalize_training_text(phrase);
    [
        phrase.clone(),
        format!("want {phrase}"),
        format!("looking for {phrase}"),
        format!("{phrase} matters"),
    ]
    .into_iter()
    .collect()
}

fn configured_labels(
    config: &IntentClassifierConfig,
) -> Result<HashMap<String, ClassifierLabel>, String> {
    Ok(configured_label_patterns(config)?
        .into_iter()
        .map(|(label_id, pattern)| {
            let polarity = polarity_from_label_id(&label_id).expect("validated label id");
            (
                label_id,
                ClassifierLabel {
                    preference: pattern.label,
                    polarity,
                },
            )
        })
        .collect())
}

fn configured_label_patterns(
    config: &IntentClassifierConfig,
) -> Result<Vec<(String, PreferencePatternSpec)>, String> {
    let mut labels = Vec::new();
    for configured in &config.labels {
        let polarity = if configured.polarity.eq_ignore_ascii_case("positive") {
            Polarity::Positive
        } else if configured.polarity.eq_ignore_ascii_case("negative") {
            Polarity::Negative
        } else {
            return Err(format!(
                "unsupported classifier polarity {}",
                configured.polarity
            ));
        };
        let patterns = match polarity {
            Polarity::Positive => schema::positive_preference_patterns(),
            Polarity::Negative => schema::negative_preference_patterns(),
        };
        let Some(pattern) = patterns
            .iter()
            .find(|pattern| pattern.label.eq_ignore_ascii_case(&configured.preference))
        else {
            return Err(format!(
                "classifier preference is absent from fact registry: {} {}",
                configured.polarity, configured.preference
            ));
        };
        if pattern.patterns.len() < config.minimum_training_examples_per_label {
            continue;
        }
        labels.push((label_id(&pattern.label, &polarity), pattern.clone()));
    }
    Ok(labels)
}

fn reject_evaluation_leakage(
    bank: &IntentClassifierEvaluationBank,
    config: &IntentClassifierConfig,
) -> Result<(), String> {
    let training_phrases = configured_label_patterns(config)?
        .into_iter()
        .flat_map(|(label_id, pattern)| classifier_training_phrases(&label_id, &pattern))
        .flat_map(|phrase| augmented_examples(&phrase))
        .map(|phrase| normalize_training_text(&phrase))
        .collect::<BTreeSet<_>>();
    let leaked = bank
        .cases
        .iter()
        .filter(|case| training_phrases.contains(&normalize_training_text(&case.text)))
        .map(|case| case.text.clone())
        .collect::<Vec<_>>();
    if leaked.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "held-out cases duplicate training phrases: {}",
            leaked.join(", ")
        ))
    }
}

fn evaluate_case(
    classifier: &FastTextIntentClassifier,
    case: IntentClassifierEvaluationCase,
) -> IntentClassifierEvaluationResult {
    let started_at = Instant::now();
    let expected_polarity = match case.expected_polarity.as_str() {
        "positive" => Polarity::Positive,
        "negative" => Polarity::Negative,
        _ => {
            return IntentClassifierEvaluationResult {
                text: case.text,
                expected_label_id: "invalid".to_string(),
                selected_label_id: None,
                top_probability: None,
                top_margin: None,
                duration_micros: started_at.elapsed().as_micros(),
                outcome: "invalid_expected_polarity".to_string(),
            };
        }
    };
    let expected_label_id = label_id(&case.expected_preference, &expected_polarity);
    let raw = classifier.model.predict(&case.text, 2, 0.0);
    let top_probability = raw.first().map(|prediction| prediction.prob);
    let top_margin = raw
        .first()
        .map(|top| top.prob - raw.get(1).map_or(0.0, |second| second.prob));
    let selected_label_id = raw.first().and_then(|top| {
        let margin = top_margin.unwrap_or_default();
        (top.prob >= classifier.minimum_probability && margin >= classifier.minimum_margin)
            .then(|| top.label.clone())
    });
    let outcome = match selected_label_id.as_deref() {
        Some(selected) if selected == expected_label_id => "correct",
        Some(_) => "incorrect",
        None => "abstained",
    };
    IntentClassifierEvaluationResult {
        text: case.text,
        expected_label_id,
        selected_label_id,
        top_probability,
        top_margin,
        duration_micros: started_at.elapsed().as_micros(),
        outcome: outcome.to_string(),
    }
}

fn validate_artifact_identity(
    model_path: &Path,
    config: &IntentClassifierConfig,
) -> Result<(), String> {
    let metadata_file = metadata_path(model_path);
    let metadata: IntentClassifierMetadata = serde_json::from_slice(
        &fs::read(&metadata_file)
            .map_err(|err| format!("failed to read {}: {err}", metadata_file.display()))?,
    )
    .map_err(|err| format!("failed to parse {}: {err}", metadata_file.display()))?;
    let model_sha256 = sha256_bytes(
        &fs::read(model_path)
            .map_err(|err| format!("failed to read {}: {err}", model_path.display()))?,
    );
    if metadata.model_sha256 != model_sha256 {
        return Err("intent classifier model hash does not match metadata".to_string());
    }
    if metadata.config_digest != classifier_config_digest(config)? {
        return Err("intent classifier config changed; retrain the model".to_string());
    }
    Ok(())
}

fn classifier_config_digest(config: &IntentClassifierConfig) -> Result<String, String> {
    let patterns = configured_label_patterns(config)?;
    let mut identity = Vec::new();
    for (label_id, pattern) in patterns {
        identity.push(label_id.clone());
        identity.extend(classifier_training_phrases(&label_id, &pattern));
    }
    identity.extend([
        config.training.epoch.to_string(),
        config.training.learning_rate.to_string(),
        config.training.dimension.to_string(),
        config.training.word_ngrams.to_string(),
        config.training.min_character_ngrams.to_string(),
        config.training.max_character_ngrams.to_string(),
        config.training.bucket.to_string(),
        config.training.thread.to_string(),
        config.training.seed.to_string(),
    ]);
    Ok(sha256_bytes(identity.join("\n").as_bytes()))
}

fn metadata_path(model_path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.metadata.json", model_path.display()))
}

fn label_id(preference: &str, polarity: &Polarity) -> String {
    format!(
        "{LABEL_PREFIX}{}__{}",
        polarity_name(polarity),
        preference
            .chars()
            .map(|character| {
                if character.is_ascii_alphanumeric() {
                    character.to_ascii_lowercase()
                } else {
                    '_'
                }
            })
            .collect::<String>()
            .trim_matches('_')
    )
}

fn polarity_from_label_id(label_id: &str) -> Option<Polarity> {
    if label_id.starts_with("__label__positive__") {
        Some(Polarity::Positive)
    } else if label_id.starts_with("__label__negative__") {
        Some(Polarity::Negative)
    } else {
        None
    }
}

fn polarity_name(polarity: &Polarity) -> &'static str {
    match polarity {
        Polarity::Positive => "positive",
        Polarity::Negative => "negative",
    }
}

fn classifier_mode_name(mode: &IntentClassifierMode) -> &'static str {
    match mode {
        IntentClassifierMode::Disabled => "disabled",
        IntentClassifierMode::Shadow => "shadow",
    }
}

fn normalize_training_text(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn classifier_training_phrases(label_id: &str, pattern: &PreferencePatternSpec) -> Vec<String> {
    let polarity = polarity_from_label_id(label_id).expect("configured classifier label");
    pattern
        .patterns
        .iter()
        .cloned()
        .chain(pattern.expanded_keys.iter().flat_map(|fact_key| {
            schema::fact_answers_preferences(fact_key, &polarity)
                .iter()
                .cloned()
        }))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn sha256_bytes(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn ratio(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

fn percentile_95(sorted_values: &[u128]) -> u128 {
    if sorted_values.is_empty() {
        return 0;
    }
    let index = ((sorted_values.len() as f64 * 0.95).ceil() as usize)
        .saturating_sub(1)
        .min(sorted_values.len() - 1);
    sorted_values[index]
}

fn abstained_decision(
    clause: ResidualClause,
    abstention: IntentClassifierAbstention,
    predictions: Vec<IntentClassifierPrediction>,
) -> IntentClassifierDecision {
    IntentClassifierDecision {
        clause,
        predictions,
        selected: None,
        abstention: Some(abstention),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::query_plan::compile_query_plan;

    #[test]
    fn residual_text_excludes_deterministic_spans() {
        let query = "3BHK in East Bengaluru under 2Cr with rooms overlooking a courtyard";
        let plan = compile_query_plan(query);
        let extraction = residual_clauses(query, &plan, &[]);

        assert_eq!(extraction.clauses.len(), 1);
        assert_eq!(extraction.clauses[0].text, "rooms overlooking courtyard");
    }

    #[test]
    fn residual_text_excludes_resolved_named_entities() {
        let query = "3BHK in Whitefield with homes that allow pets";
        let plan = compile_query_plan(query);
        let start = query.find("Whitefield").expect("entity start");
        let spans = vec![SourceSpan {
            start,
            end: start + "Whitefield".len(),
            raw_text: "Whitefield".to_string(),
        }];
        let extraction = residual_clauses(query, &plan, &spans);

        assert_eq!(extraction.clauses.len(), 1);
        assert_eq!(extraction.clauses[0].text, "homes that allow pets");
    }

    #[test]
    fn residual_text_splits_configured_conjunctions() {
        let query = "sunrise views and space for pottery";
        let plan = compile_query_plan(query);
        let extraction = residual_clauses(query, &plan, &[]);

        assert_eq!(
            extraction
                .clauses
                .iter()
                .map(|clause| clause.text.as_str())
                .collect::<Vec<_>>(),
            vec!["sunrise views", "space pottery"]
        );
    }

    #[test]
    fn clauses_with_deterministic_preferences_are_not_reclassified() {
        let query = "dependable upkeep and homes with a reading nook";
        let plan = compile_query_plan(query);
        let extraction = residual_clauses(query, &plan, &[]);

        assert_eq!(extraction.clauses.len(), 1);
        assert_eq!(extraction.clauses[0].text, "homes reading nook");
    }

    #[test]
    fn fasttext_model_is_safe_to_share_between_search_threads() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<FastTextIntentClassifier>();
    }
}
