use crate::dag_config::SearchResolutionConfig;

pub fn is_resolvable_entity_name(name: &str, resolution_config: &SearchResolutionConfig) -> bool {
    let meaningful_chars = name.chars().filter(|ch| ch.is_ascii_alphanumeric()).count();
    if meaningful_chars < resolution_config.min_resolvable_entity_name_chars {
        return false;
    }

    let normalized_name = name.trim().to_ascii_lowercase();
    !resolution_config
        .ignored_entity_names
        .iter()
        .any(|ignored| ignored.eq_ignore_ascii_case(&normalized_name))
}

pub fn query_contains_lower_text(query_lower: &str, text: &str) -> bool {
    let text = text.trim().to_ascii_lowercase();
    if text.is_empty() {
        return false;
    }

    let mut search_start = 0;
    while let Some(relative_pos) = query_lower[search_start..].find(&text) {
        let start = search_start + relative_pos;
        let end = start + text.len();
        let before_ok = query_lower[..start]
            .chars()
            .next_back()
            .is_none_or(|ch| !ch.is_ascii_alphanumeric());
        let after_ok = query_lower[end..]
            .chars()
            .next()
            .is_none_or(|ch| !ch.is_ascii_alphanumeric());

        if before_ok && after_ok {
            return true;
        }

        search_start = end;
        if search_start >= query_lower.len() {
            return false;
        }
    }

    false
}

pub fn slug(value: &str) -> String {
    value
        .trim()
        .to_ascii_lowercase()
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}
