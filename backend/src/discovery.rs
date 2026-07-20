use serde::{Deserialize, Serialize};

const DISCOVERY_CONFIG_JSON: &str = include_str!("../../app/config/product/discovery_home.json");

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DiscoveryConfig {
    pub product_promise: String,
    pub quotes: Vec<DiscoveryQuoteConfig>,
    pub shelves: Vec<DiscoveryShelfConfig>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DiscoveryQuoteConfig {
    pub text: String,
    pub tone: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DiscoveryShelfConfig {
    pub id: String,
    pub title: String,
    pub quote: String,
    pub description: String,
    pub search_query: String,
    pub receipt_copy: String,
}

pub fn load_discovery_config() -> DiscoveryConfig {
    serde_json::from_str(DISCOVERY_CONFIG_JSON)
        .expect("app/config/product/discovery_home.json must be valid JSON")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery_config_loads_from_app_config_embed() {
        let config = load_discovery_config();
        assert!(!config.product_promise.is_empty());
        assert!(!config.shelves.is_empty());
        assert!(
            config
                .shelves
                .iter()
                .all(|shelf| !shelf.receipt_copy.is_empty()),
            "every shelf needs receipt_copy"
        );
    }
}
