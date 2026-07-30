//! Community evidence summarization.
//!
//! The durable contract is source-neutral: Google reviews, Reddit threads, and
//! future community sources all become evidence records before they become KG
//! facts. Local ML can improve theme ranking or prose later, but structured
//! evidence remains the source of truth.

use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeMap, BTreeSet};
use std::hash::{Hash, Hasher};
use std::sync::OnceLock;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::dag_config::{community_themes_config, CommunityThemeDefinition};

/// Target length for buyer-facing community pulse paragraphs.
pub const COMMUNITY_PARAGRAPH_MAX_WORDS: usize = 85;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommunityPulseQuote {
    pub text: String,
    pub source_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    pub polarity: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommunityPulse {
    pub source_label: String,
    pub sentiment_band: String,
    pub paragraph: String,
    pub positives: Vec<String>,
    pub concerns: Vec<String>,
    pub quotes: Vec<CommunityPulseQuote>,
    pub source_urls: Vec<String>,
    #[serde(skip_serializing)]
    pub confidence_pct: u8,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CommunityEvidenceRecord {
    pub entity_id: String,
    pub source_type: String,
    pub source_url: Option<String>,
    pub fact_key: String,
    pub text: Option<String>,
    pub numeric_value: Option<f64>,
    pub tags: Vec<String>,
    pub confidence: f32,
    pub learned_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CommunityEntitySummary {
    pub entity_id: String,
    pub summary: String,
    pub sentiment_score: Option<f64>,
    pub positive_themes: Vec<String>,
    pub concern_themes: Vec<String>,
    pub review_highlights: Vec<String>,
    pub source_urls: Vec<String>,
    pub source_types: Vec<String>,
    pub evidence_count: usize,
    pub confidence: f32,
    pub learned_at: DateTime<Utc>,
    pub model: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommunityThemePolarity {
    Positive,
    Concern,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommunityThemeHit {
    pub key: String,
    pub label: String,
    pub polarity: CommunityThemePolarity,
}

pub trait CommunityThemeRanker {
    fn rank_themes(&self, documents: &[CommunityEvidenceRecord]) -> Vec<CommunityThemeHit>;
}

pub trait CommunitySummaryWriter {
    fn write_summary(&self, input: &CommunitySummaryInput<'_>) -> Option<String>;
    fn model_name(&self) -> Option<&'static str> {
        None
    }
}

#[derive(Debug)]
pub struct CommunitySummaryInput<'a> {
    pub entity_id: &'a str,
    pub source_types: &'a BTreeSet<String>,
    pub rating: Option<f64>,
    pub review_count: Option<u64>,
    pub positive_themes: &'a [String],
    pub concern_themes: &'a [String],
    pub evidence_texts: &'a [String],
    pub text_evidence_count: usize,
}

pub trait CommunityEvidenceSummaryEngine {
    fn summarize(&self, records: &[CommunityEvidenceRecord]) -> Vec<CommunityEntitySummary>;
}

#[derive(Debug, Default)]
pub struct DeterministicCommunityThemeRanker;

impl CommunityThemeRanker for DeterministicCommunityThemeRanker {
    fn rank_themes(&self, documents: &[CommunityEvidenceRecord]) -> Vec<CommunityThemeHit> {
        let mut hits = BTreeMap::<String, CommunityThemeHit>::new();
        for document in documents {
            if !has_textual_evidence(document) {
                continue;
            }
            let haystack = document_haystack(document);
            if haystack.trim().is_empty() {
                continue;
            }
            for theme in community_theme_candidates() {
                if theme
                    .terms
                    .iter()
                    .any(|term| contains_theme_term(&haystack, term))
                {
                    hits.entry(theme.key.to_string())
                        .or_insert_with(|| CommunityThemeHit {
                            key: theme.key.to_string(),
                            label: theme.label.to_string(),
                            polarity: theme.polarity.clone(),
                        });
                }
            }
        }
        hits.into_values().collect()
    }
}

#[derive(Debug, Clone)]
pub struct LocalEmbeddingCommunityThemeRanker {
    dimensions: usize,
    minimum_similarity: f64,
}

impl Default for LocalEmbeddingCommunityThemeRanker {
    fn default() -> Self {
        Self {
            dimensions: 256,
            minimum_similarity: 0.18,
        }
    }
}

impl LocalEmbeddingCommunityThemeRanker {
    pub fn new(dimensions: usize, minimum_similarity: f64) -> Self {
        Self {
            dimensions: dimensions.max(32),
            minimum_similarity,
        }
    }
}

impl CommunityThemeRanker for LocalEmbeddingCommunityThemeRanker {
    fn rank_themes(&self, documents: &[CommunityEvidenceRecord]) -> Vec<CommunityThemeHit> {
        let mut hits = BTreeMap::<String, CommunityThemeHit>::new();
        let theme_vectors = community_theme_candidates()
            .iter()
            .map(|theme| {
                let query_text = theme.evidence_queries.join(" ");
                (theme, local_text_embedding(&query_text, self.dimensions))
            })
            .collect::<Vec<_>>();

        for document in documents {
            if !has_textual_evidence(document) {
                continue;
            }
            let evidence_text = document_haystack(document);
            if evidence_text.trim().is_empty() {
                continue;
            }
            let evidence_vector = local_text_embedding(&evidence_text, self.dimensions);
            let evidence_tokens = local_embedding_tokens(&evidence_text)
                .into_iter()
                .collect::<BTreeSet<_>>();
            for (theme, theme_vector) in &theme_vectors {
                let exact_match = theme
                    .terms
                    .iter()
                    .any(|term| contains_theme_term(&evidence_text, term));
                let semantic_match = cosine_similarity(&evidence_vector, theme_vector)
                    >= self.minimum_similarity
                    && has_theme_anchor(&evidence_tokens, theme);
                if exact_match || semantic_match {
                    hits.entry(theme.key.to_string())
                        .or_insert_with(|| CommunityThemeHit {
                            key: theme.key.to_string(),
                            label: theme.label.to_string(),
                            polarity: theme.polarity.clone(),
                        });
                }
            }
        }
        hits.into_values().collect()
    }
}

fn contains_theme_term(haystack: &str, term: &str) -> bool {
    let normalized = term.to_ascii_lowercase();
    if normalized.contains(' ') {
        return haystack.contains(&normalized);
    }
    haystack
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .any(|token| token == normalized)
}

fn has_theme_anchor(evidence_tokens: &BTreeSet<String>, theme: &CommunityThemeCandidate) -> bool {
    theme_anchor_tokens(theme)
        .iter()
        .any(|token| evidence_tokens.contains(token))
}

fn theme_anchor_tokens(theme: &CommunityThemeCandidate) -> BTreeSet<String> {
    let mut tokens = BTreeSet::new();
    tokens.extend(local_embedding_tokens(&theme.key));
    tokens.extend(local_embedding_tokens(&theme.label));
    for term in &theme.terms {
        tokens.extend(local_embedding_tokens(term));
    }
    tokens
}

#[derive(Debug, Default)]
pub struct DeterministicCommunitySummaryWriter;

impl CommunitySummaryWriter for DeterministicCommunitySummaryWriter {
    fn write_summary(&self, input: &CommunitySummaryInput<'_>) -> Option<String> {
        Some(deterministic_summary(input))
    }
}

#[derive(Debug, Default)]
pub struct CommunityEvidenceSummarizer<R, W> {
    theme_ranker: R,
    summary_writer: W,
}

impl<R, W> CommunityEvidenceSummarizer<R, W>
where
    R: CommunityThemeRanker,
    W: CommunitySummaryWriter,
{
    pub fn new(theme_ranker: R, summary_writer: W) -> Self {
        Self {
            theme_ranker,
            summary_writer,
        }
    }

    pub fn summarize(&self, records: &[CommunityEvidenceRecord]) -> Vec<CommunityEntitySummary> {
        let mut by_entity = BTreeMap::<String, Vec<CommunityEvidenceRecord>>::new();
        for record in records {
            by_entity
                .entry(record.entity_id.clone())
                .or_default()
                .push(record.clone());
        }

        by_entity
            .into_iter()
            .filter_map(|(entity_id, records)| self.summarize_entity(entity_id, records))
            .collect()
    }

    fn summarize_entity(
        &self,
        entity_id: String,
        records: Vec<CommunityEvidenceRecord>,
    ) -> Option<CommunityEntitySummary> {
        if records.is_empty() {
            return None;
        }

        let source_types = records
            .iter()
            .map(|record| record.source_type.clone())
            .collect::<BTreeSet<_>>();
        let source_urls = source_urls(&records);
        let rating = latest_numeric(&records, "google_rating");
        let review_count =
            latest_numeric(&records, "google_review_count").map(|value| value as u64);
        let text_evidence_count = records
            .iter()
            .filter(|record| {
                record
                    .text
                    .as_deref()
                    .is_some_and(|text| text.split_whitespace().count() >= 3)
                    || !record.tags.is_empty()
            })
            .count();
        let evidence_texts = community_evidence_texts(&records);
        let theme_hits = self.theme_ranker.rank_themes(&records);
        let positive_themes = theme_labels(&theme_hits, CommunityThemePolarity::Positive);
        let concern_themes = theme_labels(&theme_hits, CommunityThemePolarity::Concern);
        let review_highlights = review_highlights(&evidence_texts);
        let input = CommunitySummaryInput {
            entity_id: &entity_id,
            source_types: &source_types,
            rating,
            review_count,
            positive_themes: &positive_themes,
            concern_themes: &concern_themes,
            evidence_texts: &evidence_texts,
            text_evidence_count,
        };
        let summary = self.summary_writer.write_summary(&input)?;
        let confidence = summary_confidence(&records, text_evidence_count);
        let learned_at = records
            .iter()
            .map(|record| record.learned_at)
            .max()
            .unwrap_or_else(Utc::now);

        Some(CommunityEntitySummary {
            entity_id,
            summary,
            sentiment_score: rating.map(|value| (value / 5.0 * 100.0).clamp(0.0, 100.0)),
            positive_themes,
            concern_themes,
            review_highlights,
            source_urls,
            source_types: source_types.into_iter().collect(),
            evidence_count: records.len(),
            confidence,
            learned_at,
            model: self.summary_writer.model_name().map(str::to_string),
        })
    }
}

impl<R, W> CommunityEvidenceSummaryEngine for CommunityEvidenceSummarizer<R, W>
where
    R: CommunityThemeRanker,
    W: CommunitySummaryWriter,
{
    fn summarize(&self, records: &[CommunityEvidenceRecord]) -> Vec<CommunityEntitySummary> {
        CommunityEvidenceSummarizer::summarize(self, records)
    }
}

impl<T> CommunityThemeRanker for Box<T>
where
    T: CommunityThemeRanker + ?Sized,
{
    fn rank_themes(&self, documents: &[CommunityEvidenceRecord]) -> Vec<CommunityThemeHit> {
        (**self).rank_themes(documents)
    }
}

impl<T> CommunitySummaryWriter for Box<T>
where
    T: CommunitySummaryWriter + ?Sized,
{
    fn write_summary(&self, input: &CommunitySummaryInput<'_>) -> Option<String> {
        (**self).write_summary(input)
    }

    fn model_name(&self) -> Option<&'static str> {
        (**self).model_name()
    }
}

pub type DeterministicCommunityEvidenceSummarizer = CommunityEvidenceSummarizer<
    LocalEmbeddingCommunityThemeRanker,
    DeterministicCommunitySummaryWriter,
>;

pub fn deterministic_community_summarizer() -> DeterministicCommunityEvidenceSummarizer {
    CommunityEvidenceSummarizer::new(
        LocalEmbeddingCommunityThemeRanker::default(),
        DeterministicCommunitySummaryWriter,
    )
}

fn latest_numeric(records: &[CommunityEvidenceRecord], fact_key: &str) -> Option<f64> {
    records
        .iter()
        .filter(|record| record.fact_key == fact_key)
        .max_by_key(|record| record.learned_at)
        .and_then(|record| record.numeric_value)
}

fn source_urls(records: &[CommunityEvidenceRecord]) -> Vec<String> {
    let mut urls = records
        .iter()
        .filter_map(|record| record.source_url.clone())
        .filter(|url| url.starts_with("http://") || url.starts_with("https://"))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    urls.truncate(5);
    urls
}

fn community_evidence_texts(records: &[CommunityEvidenceRecord]) -> Vec<String> {
    let mut texts = records
        .iter()
        .flat_map(|record| {
            let mut values = Vec::new();
            if let Some(text) = record.text.as_deref() {
                values.push(text.to_string());
            }
            if record.fact_key == "google_review_snippets" {
                values.extend(record.tags.iter().cloned());
            }
            values
        })
        .map(|text| text.trim().to_string())
        .filter(|text| text.split_whitespace().count() >= 3)
        .collect::<Vec<_>>();
    texts.sort();
    texts.dedup();
    texts.truncate(12);
    texts
}

fn review_highlights(evidence_texts: &[String]) -> Vec<String> {
    let mut highlights = evidence_texts
        .iter()
        .filter(|text| !looks_like_generated_gap(text))
        .take(5)
        .cloned()
        .collect::<Vec<_>>();
    highlights.sort();
    highlights.dedup();
    highlights
}

fn looks_like_generated_gap(text: &str) -> bool {
    let normalized = text.to_ascii_lowercase();
    normalized.contains("not ingested yet")
        || normalized.contains("not captured")
        || normalized.contains("not available")
}

fn theme_labels(hits: &[CommunityThemeHit], polarity: CommunityThemePolarity) -> Vec<String> {
    hits.iter()
        .filter(|hit| hit.polarity == polarity)
        .map(|hit| hit.label.clone())
        .collect()
}

fn summary_confidence(records: &[CommunityEvidenceRecord], text_evidence_count: usize) -> f32 {
    let average =
        records.iter().map(|record| record.confidence).sum::<f32>() / records.len().max(1) as f32;
    if text_evidence_count == 0 {
        average.min(0.55)
    } else {
        average.min(0.85)
    }
}

pub fn community_pulse_from_summary(summary: &CommunityEntitySummary) -> CommunityPulse {
    let source_label = source_label_for_types(&summary.source_types);
    let sentiment_band = summary
        .sentiment_score
        .map(|score| sentiment_band_from_score(score).to_string())
        .unwrap_or_else(|| "Early signal".to_string());
    let quotes = bucket_review_quotes(
        &summary.review_highlights,
        &summary.positive_themes,
        &summary.concern_themes,
        &summary.source_urls,
        &summary.source_types,
    );
    CommunityPulse {
        source_label,
        sentiment_band,
        paragraph: summary.summary.clone(),
        positives: summary.positive_themes.clone(),
        concerns: summary.concern_themes.clone(),
        quotes,
        source_urls: summary.source_urls.clone(),
        confidence_pct: (summary.confidence.clamp(0.0, 1.0) * 100.0).round() as u8,
    }
}

pub fn community_evidence_from_fact_value(
    entity_id: &str,
    source_type: &str,
    source_url: Option<String>,
    fact_key: &str,
    value: &crate::knowledge::FactValue,
    confidence: f32,
    learned_at: DateTime<Utc>,
) -> Option<CommunityEvidenceRecord> {
    let mut source_url = source_url;
    let (text, numeric_value, tags) = match value {
        crate::knowledge::FactValue::Numeric(value) => (None, Some(*value), Vec::new()),
        crate::knowledge::FactValue::Text(value) => {
            if is_web_url(value) {
                source_url.get_or_insert_with(|| value.clone());
                (None, None, Vec::new())
            } else {
                (Some(value.clone()), None, Vec::new())
            }
        }
        crate::knowledge::FactValue::Bool(_) => (None, None, Vec::new()),
        crate::knowledge::FactValue::Tags(values) => (None, None, values.clone()),
        crate::knowledge::FactValue::Score { value, explanation } => {
            (Some(explanation.clone()), Some(*value), Vec::new())
        }
    };
    Some(CommunityEvidenceRecord {
        entity_id: entity_id.to_string(),
        source_type: source_type.to_string(),
        source_url,
        fact_key: fact_key.to_string(),
        text,
        numeric_value,
        tags,
        confidence,
        learned_at,
    })
}

pub(crate) fn deterministic_summary(input: &CommunitySummaryInput<'_>) -> String {
    compose_community_paragraph(input)
}

pub(crate) fn compose_community_paragraph(input: &CommunitySummaryInput<'_>) -> String {
    let source_label = source_label_for_type_set(input.source_types);
    let band = input.rating.map(sentiment_band_from_rating);

    if input.text_evidence_count == 0 && input.rating.is_none() {
        return clamp_paragraph_words(format!(
            "{source_label} feedback is still thin for this society."
        ));
    }

    let mut sentences = Vec::new();
    let positives = join_natural_list(input.positive_themes);
    let concerns = join_natural_list(input.concern_themes);

    if input.text_evidence_count > 0 {
        match (
            input.positive_themes.is_empty(),
            input.concern_themes.is_empty(),
            band,
        ) {
            (true, true, Some(band)) => sentences.push(format!(
                "{source_label} feedback reads {band}, though recurring themes are still being extracted."
            )),
            (true, true, None) => sentences.push(format!(
                "{source_label} feedback is available, though recurring themes are still being extracted."
            )),
            (false, true, Some(band)) => sentences.push(format!(
                "{source_label} feedback is {band} on life inside the society, with residents repeatedly mentioning {positives}."
            )),
            (false, true, None) => sentences.push(format!(
                "Residents repeatedly praise {positives} in {source_label} feedback."
            )),
            (true, false, Some(band)) => sentences.push(format!(
                "{source_label} feedback is {band}, with recurring concerns around {concerns}."
            )),
            (true, false, None) => sentences.push(format!(
                "{source_label} feedback flags recurring concerns around {concerns}."
            )),
            (false, false, Some(band)) => sentences.push(format!(
                "{source_label} feedback is {band} overall. Residents praise {positives}. The main cautions are {concerns}."
            )),
            (false, false, None) => sentences.push(format!(
                "{source_label} feedback leans on resident themes: praise for {positives}, with {concerns} as the main cautions."
            )),
        }
    } else if let Some(band) = band {
        sentences.push(format!(
            "{source_label} points to a {band} resident signal, but written review themes are still limited."
        ));
    }

    if input.text_evidence_count == 0 {
        sentences.push("Treat this as directional until more review text is ingested.".to_string());
    } else if input.rating.is_some() && input.review_count.unwrap_or(0) < 25 {
        sentences.push(
            "The written signal is still building, so weigh resident quotes over the headline read."
                .to_string(),
        );
    }

    clamp_paragraph_words(sentences.join(" "))
}

pub fn source_label_for_types(source_types: &[String]) -> String {
    let set = source_types.iter().cloned().collect::<BTreeSet<_>>();
    source_label_for_type_set(&set)
}

fn source_label_for_type_set(source_types: &BTreeSet<String>) -> String {
    if source_types.is_empty() {
        "Community".to_string()
    } else if source_types.len() == 1 && source_types.contains("Google") {
        "Google review".to_string()
    } else if source_types.len() == 1 && source_types.contains("Reddit") {
        "Reddit".to_string()
    } else if source_types.contains("Google") {
        "Google review".to_string()
    } else {
        "Community".to_string()
    }
}

pub fn sentiment_band_from_rating(rating: f64) -> &'static str {
    rating_band(rating)
}

fn sentiment_band_from_score(score: f64) -> &'static str {
    if score >= 84.0 {
        "Positive"
    } else if score >= 76.0 {
        "Mixed-positive"
    } else if score >= 68.0 {
        "Mixed"
    } else {
        "Cautious"
    }
}

fn rating_band(rating: f64) -> &'static str {
    if rating >= 4.2 {
        "broadly positive"
    } else if rating >= 3.8 {
        "mixed-positive"
    } else if rating >= 3.4 {
        "mixed"
    } else {
        "cautious"
    }
}

fn join_natural_list(values: &[String]) -> String {
    match values.len() {
        0 => String::new(),
        1 => values[0].clone(),
        2 => format!("{} and {}", values[0], values[1]),
        _ => format!(
            "{}, and {}",
            values[..values.len() - 1].join(", "),
            values[values.len() - 1]
        ),
    }
}

fn clamp_paragraph_words(mut text: String) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() <= COMMUNITY_PARAGRAPH_MAX_WORDS {
        return text;
    }
    text = words
        .into_iter()
        .take(COMMUNITY_PARAGRAPH_MAX_WORDS)
        .collect::<Vec<_>>()
        .join(" ");
    if !text.ends_with('.') {
        text.push('.');
    }
    text
}

fn bucket_review_quotes(
    highlights: &[String],
    positive_themes: &[String],
    concern_themes: &[String],
    source_urls: &[String],
    source_types: &[String],
) -> Vec<CommunityPulseQuote> {
    let source_url = source_urls.first().cloned();
    let source_type = source_types
        .first()
        .cloned()
        .unwrap_or_else(|| "Google".to_string());
    highlights
        .iter()
        .filter(|text| !looks_like_generated_gap(text))
        .map(|text| {
            let polarity = quote_polarity(text, positive_themes, concern_themes);
            CommunityPulseQuote {
                text: text.clone(),
                source_type: source_type.clone(),
                source_url: source_url.clone(),
                polarity,
            }
        })
        .take(5)
        .collect()
}

fn quote_polarity(text: &str, positive_themes: &[String], concern_themes: &[String]) -> String {
    let haystack = text.to_ascii_lowercase();
    let concern_hit = concern_themes
        .iter()
        .any(|theme| haystack.contains(&theme.to_ascii_lowercase()))
        || community_theme_candidates().iter().any(|theme| {
            theme.polarity == CommunityThemePolarity::Concern
                && theme
                    .terms
                    .iter()
                    .any(|term| contains_theme_term(&haystack, term))
        });
    let positive_hit = positive_themes
        .iter()
        .any(|theme| haystack.contains(&theme.to_ascii_lowercase()))
        || community_theme_candidates().iter().any(|theme| {
            theme.polarity == CommunityThemePolarity::Positive
                && theme
                    .terms
                    .iter()
                    .any(|term| contains_theme_term(&haystack, term))
        });
    if concern_hit && !positive_hit {
        "concern".to_string()
    } else if positive_hit {
        "positive".to_string()
    } else {
        "neutral".to_string()
    }
}

fn is_web_url(value: &str) -> bool {
    value.starts_with("http://") || value.starts_with("https://")
}

fn document_haystack(document: &CommunityEvidenceRecord) -> String {
    let mut parts = Vec::new();
    parts.push(document.fact_key.as_str());
    if let Some(text) = document.text.as_deref() {
        parts.push(text);
    }
    for tag in &document.tags {
        parts.push(tag);
    }
    parts.join(" ").to_ascii_lowercase()
}

fn has_textual_evidence(document: &CommunityEvidenceRecord) -> bool {
    document
        .text
        .as_deref()
        .is_some_and(|text| text.split_whitespace().count() >= 3)
        || !document.tags.is_empty()
}

fn local_text_embedding(text: &str, dimensions: usize) -> Vec<f32> {
    let mut vector = vec![0.0_f32; dimensions];
    for token in local_embedding_tokens(text) {
        let index = hash_index(&token, dimensions);
        vector[index] += token_weight(&token);
    }
    normalize(&mut vector);
    vector
}

fn local_embedding_tokens(text: &str) -> Vec<String> {
    let raw = text
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter_map(|token| {
            let token = token.trim().to_ascii_lowercase();
            (token.len() >= 2 && !is_embedding_stopword(&token)).then_some(token)
        })
        .collect::<Vec<_>>();
    let mut tokens = Vec::new();
    for token in &raw {
        tokens.push(token.clone());
        tokens.extend(local_embedding_expansions(token, ExpansionKind::Token));
    }
    for window in raw.windows(2) {
        let phrase = format!("{} {}", window[0], window[1]);
        tokens.push(phrase.replace(' ', "_"));
        tokens.extend(local_embedding_expansions(&phrase, ExpansionKind::Phrase));
    }
    tokens
}

#[derive(Debug, Clone, Copy)]
enum ExpansionKind {
    Token,
    Phrase,
}

fn local_embedding_expansions(input: &str, kind: ExpansionKind) -> Vec<String> {
    let input = input.trim().to_ascii_lowercase();
    let expansions = match kind {
        ExpansionKind::Token => &community_themes_config().embedding_expansions.token,
        ExpansionKind::Phrase => &community_themes_config().embedding_expansions.phrase,
    };
    expansions
        .iter()
        .find(|expansion| expansion.input.eq_ignore_ascii_case(&input))
        .map(|expansion| expansion.expanded_tokens.clone())
        .unwrap_or_default()
}

impl CommunityThemePolarity {
    fn from_config(value: &str) -> Self {
        match value {
            "concern" => Self::Concern,
            _ => Self::Positive,
        }
    }
}

fn token_weight(token: &str) -> f32 {
    if token.contains('_') {
        1.25
    } else {
        1.0
    }
}

fn hash_index(token: &str, dimensions: usize) -> usize {
    let mut hasher = DefaultHasher::new();
    token.hash(&mut hasher);
    (hasher.finish() as usize) % dimensions
}

fn normalize(vector: &mut [f32]) {
    let norm = vector
        .iter()
        .map(|value| (*value as f64) * (*value as f64))
        .sum::<f64>()
        .sqrt();
    if norm == 0.0 {
        return;
    }
    for value in vector {
        *value = (*value as f64 / norm) as f32;
    }
}

fn cosine_similarity(left: &[f32], right: &[f32]) -> f64 {
    if left.len() != right.len() || left.is_empty() {
        return 0.0;
    }
    left.iter()
        .zip(right.iter())
        .map(|(left, right)| (*left as f64) * (*right as f64))
        .sum()
}

fn is_embedding_stopword(token: &str) -> bool {
    matches!(
        token,
        "a" | "an"
            | "and"
            | "are"
            | "as"
            | "at"
            | "be"
            | "but"
            | "by"
            | "for"
            | "from"
            | "in"
            | "is"
            | "it"
            | "near"
            | "of"
            | "on"
            | "or"
            | "the"
            | "to"
            | "with"
    )
}

#[derive(Debug, Clone)]
pub struct CommunityThemeCandidate {
    pub key: String,
    pub label: String,
    pub polarity: CommunityThemePolarity,
    pub terms: Vec<String>,
    pub evidence_queries: Vec<String>,
}

pub fn community_theme_candidates() -> &'static [CommunityThemeCandidate] {
    static CANDIDATES: OnceLock<Vec<CommunityThemeCandidate>> = OnceLock::new();
    CANDIDATES.get_or_init(|| {
        community_themes_config()
            .themes
            .iter()
            .map(CommunityThemeCandidate::from)
            .collect()
    })
}

impl From<&CommunityThemeDefinition> for CommunityThemeCandidate {
    fn from(value: &CommunityThemeDefinition) -> Self {
        Self {
            key: value.key.clone(),
            label: value.label.clone(),
            polarity: CommunityThemePolarity::from_config(&value.polarity),
            terms: value.terms.clone(),
            evidence_queries: value.evidence_queries.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn google_rating_without_text_does_not_invent_themes() {
        let learned_at = Utc.with_ymd_and_hms(2026, 7, 14, 10, 0, 0).unwrap();
        let summarizer = deterministic_community_summarizer();
        let summaries = summarizer.summarize(&[
            CommunityEvidenceRecord {
                entity_id: "society:example".to_string(),
                source_type: "Google".to_string(),
                source_url: Some("https://maps.google.com/example".to_string()),
                fact_key: "google_rating".to_string(),
                text: None,
                numeric_value: Some(3.9),
                tags: Vec::new(),
                confidence: 0.85,
                learned_at,
            },
            CommunityEvidenceRecord {
                entity_id: "society:example".to_string(),
                source_type: "Google".to_string(),
                source_url: Some("https://maps.google.com/example".to_string()),
                fact_key: "google_review_count".to_string(),
                text: None,
                numeric_value: Some(392.0),
                tags: Vec::new(),
                confidence: 0.85,
                learned_at,
            },
        ]);

        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].sentiment_score, Some(78.0));
        assert!(summaries[0].positive_themes.is_empty());
        assert!(summaries[0].concern_themes.is_empty());
        assert!(summaries[0]
            .summary
            .contains("written review themes are still limited"));
        assert_eq!(summaries[0].confidence, 0.55);
        assert!(summaries[0].model.is_none());
    }

    #[test]
    fn text_evidence_produces_dynamic_themes() {
        let learned_at = Utc.with_ymd_and_hms(2026, 7, 14, 10, 0, 0).unwrap();
        let summarizer = deterministic_community_summarizer();
        let summaries = summarizer.summarize(&[CommunityEvidenceRecord {
            entity_id: "society:example".to_string(),
            source_type: "Reddit".to_string(),
            source_url: Some("https://reddit.com/example".to_string()),
            fact_key: "resident_discussion".to_string(),
            text: Some(
                "Calm layout with many trees, clubhouse, pool, but traffic is bad.".to_string(),
            ),
            numeric_value: None,
            tags: Vec::new(),
            confidence: 0.7,
            learned_at,
        }]);

        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].positive_themes, vec!["amenities", "greenery"]);
        assert_eq!(summaries[0].concern_themes, vec!["traffic"]);
        assert!(!summaries[0]
            .summary
            .contains("Review text is not ingested yet"));
        assert!(summaries[0].summary.contains("amenities"));
        assert!(summaries[0].summary.contains("greenery"));
        assert!(summaries[0].summary.contains("traffic"));
    }

    #[test]
    fn local_embeddings_do_not_create_unanchored_concern_themes() {
        let learned_at = Utc.with_ymd_and_hms(2026, 7, 14, 10, 0, 0).unwrap();
        let summarizer = deterministic_community_summarizer();
        let summaries = summarizer.summarize(&[CommunityEvidenceRecord {
            entity_id: "society:prestige-lavender-fields".to_string(),
            source_type: "Google".to_string(),
            source_url: Some("https://maps.google.com/example".to_string()),
            fact_key: "google_review_snippets".to_string(),
            text: None,
            numeric_value: None,
            tags: vec![
                "Good place with modern amenities, a clubhouse and swimming pool.".to_string(),
                "Open space, central area, greenery and nearby tech park access.".to_string(),
                "Traffic near the approach road can be slow right now.".to_string(),
            ],
            confidence: 0.85,
            learned_at,
        }]);

        assert_eq!(summaries.len(), 1);
        assert_eq!(
            summaries[0].positive_themes,
            vec!["amenities", "connectivity", "greenery"]
        );
        assert_eq!(summaries[0].concern_themes, vec!["traffic"]);
        assert!(!summaries[0].summary.contains("noise"));
    }

    #[test]
    fn paragraph_stays_within_word_limit() {
        let learned_at = Utc.with_ymd_and_hms(2026, 7, 14, 10, 0, 0).unwrap();
        let summarizer = deterministic_community_summarizer();
        let summaries = summarizer.summarize(&[CommunityEvidenceRecord {
            entity_id: "society:example".to_string(),
            source_type: "Google".to_string(),
            source_url: Some("https://maps.google.com/example".to_string()),
            fact_key: "google_review_snippets".to_string(),
            text: None,
            numeric_value: None,
            tags: (0..8)
                .map(|index| {
                    format!(
                        "Residents mention greenery, amenities, maintenance, connectivity, traffic, water, parking, and noise theme {index}."
                    )
                })
                .collect(),
            confidence: 0.85,
            learned_at,
        }]);

        assert_eq!(summaries.len(), 1);
        assert!(summaries[0].summary.split_whitespace().count() <= COMMUNITY_PARAGRAPH_MAX_WORDS);
        assert!(!summaries[0].summary.contains("/5"));
    }
}
