//! Append enrichment requests to a JSONL file after each search.
//!
//! Rust writes. Python (`enrich_async.py`) reads and processes.
//! The file is the contract — no shared memory, no IPC.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

/// An enrichment request written after a search identifies an area.
#[derive(Serialize)]
pub struct EnrichmentRequest {
    pub area: String,
    pub entities: Vec<String>,
    pub preferences: Vec<String>,
    pub query: String,
    pub ts: String,
}

/// Path to the pending enrichment JSONL file.
fn pending_path(project_root: &Path) -> PathBuf {
    project_root.join("data/knowledge/enrichment_pending.jsonl")
}

/// Check if we already wrote a request for this area within the last `dedup_secs`.
/// Returns true if a duplicate exists (should skip).
fn is_duplicate(path: &Path, area: &str, dedup_secs: u64) -> bool {
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return false,
    };

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Read lines in reverse (most recent first) for efficiency
    for line in content.lines().rev() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // Quick JSON parse — only need area and ts fields
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(line) {
            let req_area = val.get("area").and_then(|v| v.as_str()).unwrap_or("");
            if !req_area.eq_ignore_ascii_case(area) {
                continue;
            }
            // Parse ts to check age
            if let Some(ts_str) = val.get("ts").and_then(|v| v.as_str()) {
                if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(ts_str) {
                    let req_secs = dt.timestamp() as u64;
                    if now.saturating_sub(req_secs) < dedup_secs {
                        return true; // Recent duplicate
                    }
                }
            }
        }
    }
    false
}

/// Append an enrichment request to the pending JSONL file.
/// Called via `tokio::spawn` — non-blocking to the search handler.
///
/// Deduplicates: skips if the same area was requested in the last 10 minutes.
pub fn append_enrichment_request(
    project_root: &Path,
    area: &str,
    entity_ids: Vec<String>,
    preferences: Vec<String>,
    query: &str,
) {
    let path = pending_path(project_root);

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    // Deduplicate: skip if same area requested in last 10 minutes
    if is_duplicate(&path, area, 600) {
        return;
    }

    let request = EnrichmentRequest {
        area: area.to_string(),
        entities: entity_ids,
        preferences,
        query: query.to_string(),
        ts: chrono::Utc::now().to_rfc3339(),
    };

    let json_line = match serde_json::to_string(&request) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("WARN: Failed to serialize enrichment request: {}", e);
            return;
        }
    };

    match OpenOptions::new().create(true).append(true).open(&path) {
        Ok(mut file) => {
            if let Err(e) = writeln!(file, "{}", json_line) {
                eprintln!("WARN: Failed to write enrichment request: {}", e);
            }
        }
        Err(e) => {
            eprintln!("WARN: Failed to open enrichment pending file: {}", e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn setup_test_dir() -> TempDir {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("data/knowledge")).unwrap();
        dir
    }

    #[test]
    fn test_append_creates_file_and_writes() {
        let dir = setup_test_dir();
        append_enrichment_request(
            dir.path(),
            "Whitefield",
            vec!["society:prestige-lakeside".into()],
            vec!["quiet".into(), "metro".into()],
            "quiet apartment whitefield",
        );

        let path = dir.path().join("data/knowledge/enrichment_pending.jsonl");
        assert!(path.exists());

        let content = fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 1);

        let val: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(val["area"], "Whitefield");
        assert_eq!(val["entities"][0], "society:prestige-lakeside");
        assert_eq!(val["preferences"][0], "quiet");
    }

    #[test]
    fn test_deduplication_within_10_minutes() {
        let dir = setup_test_dir();

        // First write
        append_enrichment_request(
            dir.path(),
            "Whitefield",
            vec!["society:a".into()],
            vec!["quiet".into()],
            "query 1",
        );

        // Second write for same area — should be deduplicated
        append_enrichment_request(
            dir.path(),
            "Whitefield",
            vec!["society:b".into()],
            vec!["metro".into()],
            "query 2",
        );

        let path = dir.path().join("data/knowledge/enrichment_pending.jsonl");
        let content = fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = content.lines().filter(|l| !l.is_empty()).collect();
        assert_eq!(lines.len(), 1, "Duplicate area should be skipped");
    }

    #[test]
    fn test_different_areas_not_deduplicated() {
        let dir = setup_test_dir();

        append_enrichment_request(
            dir.path(),
            "Whitefield",
            vec!["society:a".into()],
            vec![],
            "query 1",
        );

        append_enrichment_request(
            dir.path(),
            "Sarjapur",
            vec!["society:b".into()],
            vec![],
            "query 2",
        );

        let path = dir.path().join("data/knowledge/enrichment_pending.jsonl");
        let content = fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = content.lines().filter(|l| !l.is_empty()).collect();
        assert_eq!(lines.len(), 2, "Different areas should both be written");
    }
}
