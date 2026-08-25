use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::security_tuning;

pub fn prune_rebuildable_serving_cache(project_root: &Path, active_cache_dir: &Path) {
    let root = project_root
        .join("data")
        .join("cache")
        .join("serving")
        .join("search_bundle");
    let active_materialization = active_cache_dir
        .ancestors()
        .find(|path| {
            path.parent() == Some(root.as_path())
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("materialization="))
        })
        .map(Path::to_path_buf);
    let mut entries = versioned_entries(&root, "materialization=");
    entries.sort_by_key(|(_, modified)| std::cmp::Reverse(*modified));

    let versions_to_keep = security_tuning().retention.serving_cache_versions;
    for (index, (path, _)) in entries.into_iter().enumerate() {
        if index < versions_to_keep || active_materialization.as_ref() == Some(&path) {
            continue;
        }
        if let Err(error) = fs::remove_dir_all(&path) {
            eprintln!("WARN: failed to prune rebuildable serving cache {path:?}: {error}");
        }
    }
}

pub fn prune_asset_run_logs(log_dir: &Path, active_log: Option<&Path>) {
    let mut entries = regular_log_entries(log_dir);
    entries.sort_by_key(|(_, modified)| std::cmp::Reverse(*modified));
    let files_to_keep = security_tuning().retention.asset_log_files;
    for (index, (path, _)) in entries.into_iter().enumerate() {
        if index < files_to_keep || active_log == Some(path.as_path()) {
            continue;
        }
        if let Err(error) = fs::remove_file(&path) {
            eprintln!("WARN: failed to prune asset-run log {path:?}: {error}");
        }
    }
}

fn versioned_entries(root: &Path, prefix: &str) -> Vec<(PathBuf, SystemTime)> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let file_type = entry.file_type().ok()?;
            let name = entry.file_name();
            if !file_type.is_dir() || file_type.is_symlink() || !name.to_str()?.starts_with(prefix)
            {
                return None;
            }
            let modified = entry
                .metadata()
                .and_then(|metadata| metadata.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            Some((entry.path(), modified))
        })
        .collect()
}

fn regular_log_entries(root: &Path) -> Vec<(PathBuf, SystemTime)> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let file_type = entry.file_type().ok()?;
            let path = entry.path();
            if !file_type.is_file() || file_type.is_symlink() || path.extension()? != "log" {
                return None;
            }
            let modified = entry
                .metadata()
                .and_then(|metadata| metadata.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            Some((path, modified))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serving_retention_keeps_active_version_even_when_it_is_old() {
        let root = tempfile::tempdir().unwrap();
        let cache_root = root.path().join("data/cache/serving/search_bundle");
        for index in 0..12 {
            fs::create_dir_all(
                cache_root
                    .join(format!("materialization={index}"))
                    .join("tantivy_index"),
            )
            .unwrap();
        }
        let active = cache_root.join("materialization=0/tantivy_index");
        prune_rebuildable_serving_cache(root.path(), &active);

        assert!(active.exists());
        assert!(
            fs::read_dir(cache_root).unwrap().count()
                <= security_tuning().retention.serving_cache_versions + 1
        );
    }
}
