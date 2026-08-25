use std::path::PathBuf;
use std::sync::OnceLock;

use serde::Deserialize;

const SECURITY_CONFIG_ENV: &str = "OPENESTATES_SECURITY_CONFIG";

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SecurityTuning {
    pub runtime: RuntimeTuning,
    pub search_cache: SearchCacheTuning,
    pub requests: RequestTuning,
    pub rate_limits: RateLimitTuning,
    pub media: MediaTuning,
    pub admin: AdminTuning,
    pub retention: RetentionTuning,
    pub interest_storage: InterestStorageTuning,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeTuning {
    pub http_worker_threads: usize,
    pub internal_worker_threads: usize,
    pub customer_compute_worker_threads: usize,
    pub customer_compute_limit: usize,
    pub customer_compute_queue_timeout_ms: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchCacheTuning {
    pub capacity: usize,
    pub max_bytes: usize,
    pub log_queue_capacity: usize,
    pub event_history: usize,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestTuning {
    pub public_timeout_ms: u64,
    pub global_concurrency: usize,
    pub search_concurrency: usize,
    pub read_concurrency: usize,
    pub catalog_concurrency: usize,
    pub max_request_target_bytes: usize,
    pub max_search_query_bytes: usize,
    pub batch_body_bytes: usize,
    pub interest_body_bytes: usize,
    pub admin_body_bytes: usize,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RateLimitTuning {
    pub read: RateLimitRule,
    pub search: RateLimitRule,
    pub batch: RateLimitRule,
    pub interest: RateLimitRule,
    pub admin: RateLimitRule,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RateLimitRule {
    pub period_ms: u64,
    pub burst: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaTuning {
    pub stream_concurrency: usize,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdminTuning {
    pub max_source_entities: usize,
    pub max_source_entity_bytes: usize,
    pub max_partition_parts: usize,
    pub max_field_bytes: usize,
    pub default_source_timeout_seconds: u64,
    pub max_source_timeout_seconds: u64,
    pub max_asset_run_log_bytes: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetentionTuning {
    pub serving_cache_versions: usize,
    pub asset_log_files: usize,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InterestStorageTuning {
    pub max_name_chars: usize,
    pub max_contact_chars: usize,
    pub max_record_bytes: usize,
    pub max_property_file_bytes: u64,
    pub max_total_bytes: u64,
}

pub fn security_tuning() -> &'static SecurityTuning {
    static TUNING: OnceLock<SecurityTuning> = OnceLock::new();
    TUNING.get_or_init(|| {
        let path = security_config_path();
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("failed to read security tuning {path:?}: {error}"));
        let tuning: SecurityTuning = toml::from_str(&text)
            .unwrap_or_else(|error| panic!("invalid security tuning {path:?}: {error}"));
        tuning
            .validate()
            .unwrap_or_else(|error| panic!("unsafe security tuning {path:?}: {error}"));
        tuning
    })
}

fn security_config_path() -> PathBuf {
    std::env::var_os(SECURITY_CONFIG_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .expect("backend must be inside the project root")
                .join("app/config/security/runtime.toml")
        })
}

impl SecurityTuning {
    fn validate(&self) -> Result<(), String> {
        non_zero(
            "runtime.http_worker_threads",
            self.runtime.http_worker_threads,
        )?;
        non_zero(
            "runtime.internal_worker_threads",
            self.runtime.internal_worker_threads,
        )?;
        non_zero(
            "runtime.customer_compute_worker_threads",
            self.runtime.customer_compute_worker_threads,
        )?;
        bounded(
            "runtime.customer_compute_limit",
            self.runtime.customer_compute_limit,
            64,
        )?;
        non_zero(
            "runtime.customer_compute_queue_timeout_ms",
            self.runtime.customer_compute_queue_timeout_ms,
        )?;
        bounded("search_cache.capacity", self.search_cache.capacity, 16_384)?;
        bounded(
            "search_cache.max_bytes",
            self.search_cache.max_bytes,
            512 * 1024 * 1024,
        )?;
        bounded(
            "search_cache.log_queue_capacity",
            self.search_cache.log_queue_capacity,
            65_536,
        )?;
        bounded(
            "search_cache.event_history",
            self.search_cache.event_history,
            10_000,
        )?;
        bounded(
            "requests.global_concurrency",
            self.requests.global_concurrency,
            4_096,
        )?;
        bounded(
            "requests.search_concurrency",
            self.requests.search_concurrency,
            1_024,
        )?;
        bounded(
            "requests.read_concurrency",
            self.requests.read_concurrency,
            1_024,
        )?;
        bounded(
            "requests.catalog_concurrency",
            self.requests.catalog_concurrency,
            128,
        )?;
        if self.requests.search_concurrency > self.requests.global_concurrency {
            return Err("requests.search_concurrency cannot exceed global_concurrency".to_string());
        }
        if self.requests.read_concurrency > self.requests.global_concurrency
            || self.requests.catalog_concurrency > self.requests.read_concurrency
        {
            return Err(
                "request class concurrency must satisfy catalog <= read <= global".to_string(),
            );
        }
        bounded(
            "requests.max_request_target_bytes",
            self.requests.max_request_target_bytes,
            64 * 1024,
        )?;
        bounded(
            "requests.max_search_query_bytes",
            self.requests.max_search_query_bytes,
            self.requests.max_request_target_bytes,
        )?;
        non_zero(
            "requests.public_timeout_ms",
            self.requests.public_timeout_ms,
        )?;
        for (name, value) in [
            ("requests.batch_body_bytes", self.requests.batch_body_bytes),
            (
                "requests.interest_body_bytes",
                self.requests.interest_body_bytes,
            ),
            ("requests.admin_body_bytes", self.requests.admin_body_bytes),
            ("media.stream_concurrency", self.media.stream_concurrency),
            ("admin.max_source_entities", self.admin.max_source_entities),
            (
                "admin.max_source_entity_bytes",
                self.admin.max_source_entity_bytes,
            ),
            ("admin.max_partition_parts", self.admin.max_partition_parts),
            ("admin.max_field_bytes", self.admin.max_field_bytes),
            (
                "retention.serving_cache_versions",
                self.retention.serving_cache_versions,
            ),
            ("retention.asset_log_files", self.retention.asset_log_files),
            (
                "interest_storage.max_name_chars",
                self.interest_storage.max_name_chars,
            ),
            (
                "interest_storage.max_contact_chars",
                self.interest_storage.max_contact_chars,
            ),
            (
                "interest_storage.max_record_bytes",
                self.interest_storage.max_record_bytes,
            ),
        ] {
            non_zero(name, value)?;
        }
        for (name, rule) in [
            ("rate_limits.read", &self.rate_limits.read),
            ("rate_limits.search", &self.rate_limits.search),
            ("rate_limits.batch", &self.rate_limits.batch),
            ("rate_limits.interest", &self.rate_limits.interest),
            ("rate_limits.admin", &self.rate_limits.admin),
        ] {
            non_zero(&format!("{name}.period_ms"), rule.period_ms)?;
            non_zero(&format!("{name}.burst"), rule.burst)?;
        }
        non_zero(
            "admin.default_source_timeout_seconds",
            self.admin.default_source_timeout_seconds,
        )?;
        non_zero(
            "admin.max_source_timeout_seconds",
            self.admin.max_source_timeout_seconds,
        )?;
        if self.admin.default_source_timeout_seconds > self.admin.max_source_timeout_seconds {
            return Err("admin default timeout cannot exceed its maximum".to_string());
        }
        non_zero(
            "admin.max_asset_run_log_bytes",
            self.admin.max_asset_run_log_bytes,
        )?;
        non_zero(
            "interest_storage.max_property_file_bytes",
            self.interest_storage.max_property_file_bytes,
        )?;
        non_zero(
            "interest_storage.max_total_bytes",
            self.interest_storage.max_total_bytes,
        )?;
        if self.interest_storage.max_property_file_bytes > self.interest_storage.max_total_bytes {
            return Err("interest per-property bytes cannot exceed total bytes".to_string());
        }
        Ok(())
    }
}

fn non_zero<T>(name: &str, value: T) -> Result<(), String>
where
    T: PartialEq + Default,
{
    if value == T::default() {
        Err(format!("{name} must be non-zero"))
    } else {
        Ok(())
    }
}

fn bounded(name: &str, value: usize, maximum: usize) -> Result<(), String> {
    non_zero(name, value)?;
    if value > maximum {
        Err(format!("{name} cannot exceed {maximum}"))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_in_security_tuning_is_valid() {
        let tuning = security_tuning();
        assert!(tuning.requests.search_concurrency <= tuning.requests.global_concurrency);
        assert!(tuning.search_cache.max_bytes >= 1024 * 1024);
    }
}
