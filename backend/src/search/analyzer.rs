use tantivy::tokenizer::{
    Language, LowerCaser, SimpleTokenizer, Stemmer, StopWordFilter, TextAnalyzer, TokenStream,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TokenSpan {
    pub text: String,
    pub start: usize,
    pub end: usize,
}

pub(crate) fn search_tokens(text: &str, domain_stopwords: &[String]) -> Vec<String> {
    let mut analyzer = analyzer_with_stopwords(domain_stopwords);
    collect_tokens(&mut analyzer, text)
}

pub(crate) fn surface_tokens(text: &str, domain_stopwords: &[String]) -> Vec<String> {
    let mut analyzer = surface_analyzer_with_stopwords(domain_stopwords);
    collect_tokens(&mut analyzer, text)
}

pub(crate) fn stemmed_tokens(text: &str) -> Vec<String> {
    let mut analyzer = stemmed_analyzer();
    collect_tokens(&mut analyzer, text)
}

pub(crate) fn stemmed_phrase_match_ranges(text: &str, phrase: &str) -> Vec<(usize, usize)> {
    let phrase_tokens = stemmed_tokens(phrase);
    if phrase_tokens.is_empty() {
        return Vec::new();
    }

    let text_spans = stemmed_token_spans(text);
    text_spans
        .windows(phrase_tokens.len())
        .filter_map(|window| {
            let matches = window
                .iter()
                .map(|span| span.text.as_str())
                .eq(phrase_tokens.iter().map(String::as_str));
            if matches {
                Some((window[0].start, window[window.len() - 1].end))
            } else {
                None
            }
        })
        .collect()
}

pub(crate) fn contains_stemmed_phrase(text: &str, phrase: &str) -> bool {
    !stemmed_phrase_match_ranges(text, phrase).is_empty()
}

fn analyzer_with_stopwords(domain_stopwords: &[String]) -> TextAnalyzer {
    TextAnalyzer::builder(SimpleTokenizer::default())
        .filter(LowerCaser)
        .filter(english_stopwords())
        .filter(Stemmer::new(Language::English))
        .filter(stemmed_domain_stopwords(domain_stopwords))
        .build()
}

fn surface_analyzer_with_stopwords(domain_stopwords: &[String]) -> TextAnalyzer {
    TextAnalyzer::builder(SimpleTokenizer::default())
        .filter(LowerCaser)
        .filter(english_stopwords())
        .filter(StopWordFilter::remove(domain_stopwords.iter().cloned()))
        .build()
}

fn stemmed_analyzer() -> TextAnalyzer {
    TextAnalyzer::builder(SimpleTokenizer::default())
        .filter(LowerCaser)
        .filter(Stemmer::new(Language::English))
        .build()
}

fn english_stopwords() -> StopWordFilter {
    StopWordFilter::new(Language::English).expect("tantivy is built with English stopword support")
}

fn stemmed_domain_stopwords(domain_stopwords: &[String]) -> StopWordFilter {
    StopWordFilter::remove(
        domain_stopwords
            .iter()
            .flat_map(|stopword| stemmed_tokens(stopword)),
    )
}

fn collect_tokens(analyzer: &mut TextAnalyzer, text: &str) -> Vec<String> {
    let mut stream = analyzer.token_stream(text);
    let mut tokens = Vec::new();
    while let Some(token) = stream.next() {
        if token.text.len() >= 2 {
            tokens.push(token.text.clone());
        }
    }
    tokens
}

fn stemmed_token_spans(text: &str) -> Vec<TokenSpan> {
    let mut analyzer = stemmed_analyzer();
    let mut stream = analyzer.token_stream(text);
    let mut tokens = Vec::new();
    while let Some(token) = stream.next() {
        if token.text.len() >= 2 {
            tokens.push(TokenSpan {
                text: token.text.clone(),
                start: token.offset_from,
                end: token.offset_to,
            });
        }
    }
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stems_plural_variants_without_config_duplicates() {
        assert_eq!(
            stemmed_tokens("green spaces"),
            stemmed_tokens("green space")
        );
        assert_eq!(
            stemmed_tokens("green covers"),
            stemmed_tokens("green cover")
        );
    }

    #[test]
    fn removes_english_and_domain_stopwords() {
        let stopwords = vec!["bhk".to_string(), "property".to_string()];
        assert_eq!(
            search_tokens("3 bhk property with green spaces", &stopwords),
            vec!["green", "space"]
        );
    }

    #[test]
    fn stems_domain_stopwords_before_removing_them() {
        let stopwords = vec![
            "acre".to_string(),
            "home".to_string(),
            "lakh".to_string(),
            "property".to_string(),
        ];
        assert!(search_tokens("acres homes lakhs properties", &stopwords).is_empty());
    }

    #[test]
    fn matches_stemmed_phrases() {
        assert!(contains_stemmed_phrase(
            "The campus has green covers and internal parks",
            "green cover"
        ));
        assert!(!contains_stemmed_phrase(
            "The campus has internal parks",
            "green cover"
        ));
    }

    #[test]
    fn returns_stemmed_phrase_offsets_for_negation_checks() {
        assert_eq!(
            stemmed_phrase_match_ranges("no green spaces, but has parks", "green space"),
            vec![(3, 15)]
        );
    }
}
