//! Discovery cache — prevents duplicate Gemini calls for the same area+intent.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use super::gemini::DiscoveredProperty;

/// Cache for live discovery results. Keyed by area + intent hash.
pub struct DiscoveryCache {
    entries: HashMap<String, CachedDiscovery>,
    ttl: Duration,
    /// Maximum discoveries allowed per hour for cost control.
    max_per_hour: u32,
    /// Timestamps of recent discoveries (for rate limiting).
    recent_discoveries: Vec<Instant>,
}

struct CachedDiscovery {
    properties: Vec<DiscoveredProperty>,
    discovered_at: Instant,
}

impl DiscoveryCache {
    pub fn new(ttl_hours: u64, max_per_hour: u32) -> Self {
        Self {
            entries: HashMap::new(),
            ttl: Duration::from_secs(ttl_hours * 3600),
            max_per_hour,
            recent_discoveries: Vec::new(),
        }
    }

    /// Build a cache key from area + intent.
    pub fn cache_key(area: &str, bhk: Option<u32>, budget_max: Option<u64>) -> String {
        let bhk_str = bhk.map_or("any".to_string(), |b| b.to_string());
        let budget_str = budget_max.map_or("any".to_string(), |b| {
            // Bucket budgets to reduce cache fragmentation
            // e.g., 1.2cr and 1.3cr both map to "1cr-2cr"
            let cr = b as f64 / 10_000_000.0;
            if cr <= 0.5 {
                "under-50l".to_string()
            } else if cr <= 1.0 {
                "50l-1cr".to_string()
            } else if cr <= 2.0 {
                "1cr-2cr".to_string()
            } else if cr <= 5.0 {
                "2cr-5cr".to_string()
            } else {
                "above-5cr".to_string()
            }
        });
        format!("{}:{}:{}", area.to_lowercase(), bhk_str, budget_str)
    }

    /// Check if we have a recent (non-expired) cache entry.
    pub fn get(&self, key: &str) -> Option<&[DiscoveredProperty]> {
        let entry = self.entries.get(key)?;
        if entry.discovered_at.elapsed() < self.ttl {
            Some(&entry.properties)
        } else {
            None
        }
    }

    /// Store discovery results in cache.
    pub fn put(&mut self, key: String, properties: Vec<DiscoveredProperty>) {
        self.entries.insert(
            key,
            CachedDiscovery {
                properties,
                discovered_at: Instant::now(),
            },
        );
        self.recent_discoveries.push(Instant::now());
    }

    /// Check if we're within the rate limit.
    pub fn can_discover(&mut self) -> bool {
        let one_hour_ago = Instant::now() - Duration::from_secs(3600);
        self.recent_discoveries.retain(|t| *t > one_hour_ago);
        (self.recent_discoveries.len() as u32) < self.max_per_hour
    }

    /// Evict expired entries.
    #[allow(dead_code)]
    pub fn evict_expired(&mut self) {
        self.entries
            .retain(|_, entry| entry.discovered_at.elapsed() < self.ttl);
    }
}
