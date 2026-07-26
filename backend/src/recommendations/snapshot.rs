use crate::routes::properties::EvidenceSection;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EvidenceSnapshot {
    pub fact_count: usize,
    pub gap_count: usize,
    pub confidence_pct: u8,
    pub section_count: usize,
}

pub fn summarize_evidence_sections(sections: &[EvidenceSection]) -> EvidenceSnapshot {
    let visible: Vec<&EvidenceSection> = sections
        .iter()
        .filter(|section| !section.items.is_empty() || !section.missing.is_empty())
        .collect();

    if visible.is_empty() {
        return EvidenceSnapshot {
            fact_count: 0,
            gap_count: 0,
            confidence_pct: 0,
            section_count: 0,
        };
    }

    let fact_count = visible.iter().map(|section| section.items.len()).sum();
    let gap_count = visible.iter().map(|section| section.missing.len()).sum();
    let confidence_pct = (visible
        .iter()
        .map(|section| u32::from(section.confidence_pct))
        .sum::<u32>()
        / visible.len() as u32)
        .min(100) as u8;

    EvidenceSnapshot {
        fact_count,
        gap_count,
        confidence_pct,
        section_count: visible.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routes::properties::EvidenceSection;

    fn section(kind: &str, facts: usize, gaps: usize) -> EvidenceSection {
        EvidenceSection {
            kind: kind.to_string(),
            title: kind.to_string(),
            summary: String::new(),
            subtitle: String::new(),
            scope: "society".to_string(),
            relationship: None,
            priority: 10,
            constellation: "trust".to_string(),
            header_meta: "1 facts · Google".to_string(),
            confidence_pct: 80,
            source_types: vec!["Google".to_string()],
            entity_ids: vec!["society:sample".to_string()],
            presentation: crate::routes::properties::EvidencePresentation {
                variant: "fact_list".to_string(),
                density: "standard".to_string(),
                max_preview_items: 4,
            },
            items: (0..facts)
                .map(|idx| crate::routes::properties::SourceItem {
                    entity_id: "society:sample".to_string(),
                    key: format!("fact_{idx}"),
                    label: format!("Fact {idx}"),
                    value: "ok".to_string(),
                    scope: "society".to_string(),
                    relationship: None,
                    values: Vec::new(),
                    source_url: None,
                    attributions: Vec::new(),
                    source_type: "Google".to_string(),
                    confidence_pct: 80,
                    learned_at: String::new(),
                })
                .collect(),
            missing: (0..gaps).map(|idx| format!("gap {idx}")).collect(),
            media: Vec::new(),
            community_pulse: None,
        }
    }

    #[test]
    fn summarize_counts_visible_sections_only() {
        let snapshot =
            summarize_evidence_sections(&[section("rera", 4, 1), section("market", 2, 0)]);
        assert_eq!(snapshot.fact_count, 6);
        assert_eq!(snapshot.gap_count, 1);
        assert_eq!(snapshot.section_count, 2);
        assert_eq!(snapshot.confidence_pct, 80);
    }
}
