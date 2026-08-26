use std::path::{Path, PathBuf};

pub const ENV_FILE_VAR: &str = "OPENESTATES_ENV_FILE";

/// Load key=value pairs from the configured environment file when vars are unset.
/// Local development falls back to `.env.local`; production services should set
/// `OPENESTATES_ENV_FILE` to an absolute, access-controlled path.
pub fn load_project_env(project_root: &Path) -> Result<Option<PathBuf>, String> {
    let explicit_path = std::env::var_os(ENV_FILE_VAR).map(PathBuf::from);
    if explicit_path
        .as_ref()
        .is_some_and(|path| !path.is_absolute())
    {
        return Err(format!("{ENV_FILE_VAR} must be an absolute path"));
    }
    let path = explicit_path
        .clone()
        .unwrap_or_else(|| project_root.join(".env.local"));
    let contents = match std::fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(error) if explicit_path.is_some() => {
            return Err(format!("failed to read {}: {error}", path.display()));
        }
        Err(_) => return Ok(None),
    };
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim().trim_matches('"').trim_matches('\'');
        if key.is_empty() || std::env::var(key).is_ok() {
            continue;
        }
        // SAFETY: called once on the main thread before Tokio workers start.
        unsafe { std::env::set_var(key, value) };
    }
    Ok(Some(path))
}
