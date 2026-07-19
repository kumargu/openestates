use std::path::Path;
use std::sync::OnceLock;

use serde::Deserialize;

use super::loader::{dag_root, load_json, DagConfigError};

#[derive(Debug, Clone, Deserialize)]
pub struct SearchIntentFile {
    pub version: u32,
    #[serde(default)]
    pub area_aliases: AreaAliasConfig,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct AreaAliasConfig {
    #[serde(default)]
    pub city: String,
    #[serde(default)]
    pub entries: Vec<AreaAliasEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AreaAliasEntry {
    pub canonical: String,
    #[serde(default)]
    pub aliases: Vec<String>,
}

pub fn search_intent_path() -> std::path::PathBuf {
    dag_root().join("search_intent.json")
}

pub fn load_search_intent_from_path(path: &Path) -> Result<SearchIntentFile, DagConfigError> {
    load_json(path)
}

pub fn load_search_intent() -> Result<SearchIntentFile, DagConfigError> {
    load_search_intent_from_path(&search_intent_path())
}

pub fn area_alias_entries() -> &'static [AreaAliasEntry] {
    static ENTRIES: OnceLock<Vec<AreaAliasEntry>> = OnceLock::new();
    ENTRIES
        .get_or_init(|| match load_search_intent() {
            Ok(file) if !file.area_aliases.entries.is_empty() => file.area_aliases.entries,
            _ => embedded_area_aliases(),
        })
        .as_slice()
}

fn embedded_area_aliases() -> Vec<AreaAliasEntry> {
    vec![
        AreaAliasEntry {
            canonical: "Whitefield".to_string(),
            aliases: vec![
                "whitefield".into(),
                "wf".into(),
                "kadugodi".into(),
                "varthur".into(),
                "itpl".into(),
                "hope farm".into(),
                "kundalahalli".into(),
                "pattandur agrahara".into(),
                "brookefield".into(),
                "nallurhalli".into(),
                "hagadur".into(),
            ],
        },
        AreaAliasEntry {
            canonical: "Sarjapur Road".to_string(),
            aliases: vec![
                "sarjapur".into(),
                "sarjapur road".into(),
                "sjr".into(),
                "doddakannelli".into(),
                "carmelaram".into(),
            ],
        },
        AreaAliasEntry {
            canonical: "Bellandur".to_string(),
            aliases: vec![
                "bellandur".into(),
                "outer ring road".into(),
                "orr bellandur".into(),
            ],
        },
        AreaAliasEntry {
            canonical: "HSR Layout".to_string(),
            aliases: vec![
                "hsr".into(),
                "hsr layout".into(),
                "agara".into(),
                "sector 1 hsr".into(),
                "sector 2 hsr".into(),
            ],
        },
        AreaAliasEntry {
            canonical: "North Bengaluru".to_string(),
            aliases: vec![
                "north bangalore".into(),
                "north bengaluru".into(),
                "north blr".into(),
                "devanahalli".into(),
                "hebbal".into(),
                "yelahanka".into(),
                "thanisandra".into(),
                "jakkur".into(),
            ],
        },
        AreaAliasEntry {
            canonical: "Electronic City".to_string(),
            aliases: vec!["electronic city".into(), "ec".into(), "ecity".into()],
        },
        AreaAliasEntry {
            canonical: "Koramangala".to_string(),
            aliases: vec!["koramangala".into(), "koramangala 5th block".into()],
        },
        AreaAliasEntry {
            canonical: "Marathahalli".to_string(),
            aliases: vec!["marathahalli".into(), "marathon halli".into()],
        },
        AreaAliasEntry {
            canonical: "Indiranagar".to_string(),
            aliases: vec!["indiranagar".into(), "indira nagar".into()],
        },
        AreaAliasEntry {
            canonical: "Jayanagar".to_string(),
            aliases: vec!["jayanagar".into(), "jaya nagar".into()],
        },
        AreaAliasEntry {
            canonical: "Bannerghatta Road".to_string(),
            aliases: vec![
                "bannerghatta".into(),
                "bannerghatta road".into(),
                "btm".into(),
            ],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_area_aliases_from_search_intent_config() {
        let path = search_intent_path();
        if !path.exists() {
            return;
        }
        let file = load_search_intent().expect("search_intent.json should parse");
        assert!(!file.area_aliases.entries.is_empty());
        assert!(
            file.area_aliases
                .entries
                .iter()
                .any(|entry| entry.canonical == "Whitefield")
        );
    }
}
