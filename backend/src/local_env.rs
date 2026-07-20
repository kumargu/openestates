use std::path::Path;

/// Load key=value pairs from `.env.local` at the project root when vars are unset.
pub fn load_project_env(project_root: &Path) {
    let path = project_root.join(".env.local");
    let Ok(contents) = std::fs::read_to_string(path) else {
        return;
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
}
