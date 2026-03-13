use std::path::Path;

/// Count non-empty lines in a JSONL file. Returns 0 if the file doesn't exist.
pub async fn count_lines(path: &Path) -> usize {
    match tokio::fs::read_to_string(path).await {
        Ok(contents) => contents.lines().filter(|l| !l.trim().is_empty()).count(),
        Err(_) => 0,
    }
}
