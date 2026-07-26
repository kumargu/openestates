use std::fmt;

/// A logical object key inside the OpenEstates lake.
///
/// Keys are relative to the configured local or S3 store root. They must be
/// stable across backends so a local artifact can be copied to S3 without
/// rewriting manifests.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LakeKey(String);

/// A logical prefix used for object listing.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LakePrefix(String);

impl LakeKey {
    pub fn new(value: impl Into<String>) -> Result<Self, KeyError> {
        let value = normalize_key(value.into())?;
        Ok(Self(value))
    }

    pub fn join(parts: &[&str]) -> Result<Self, KeyError> {
        Self::new(parts.join("/"))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn parent_prefix(&self) -> LakePrefix {
        match self.0.rsplit_once('/') {
            Some((parent, _)) => LakePrefix(parent.to_string()),
            None => LakePrefix(String::new()),
        }
    }
}

impl LakePrefix {
    pub fn new(value: impl Into<String>) -> Result<Self, KeyError> {
        let value = normalize_prefix(value.into())?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for LakeKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Display for LakePrefix {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyError {
    Empty,
    Absolute,
    ParentTraversal,
    RepeatedSeparator,
}

impl fmt::Display for KeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("lake key cannot be empty"),
            Self::Absolute => f.write_str("lake key must be relative"),
            Self::ParentTraversal => f.write_str("lake key cannot contain parent traversal"),
            Self::RepeatedSeparator => f.write_str("lake key cannot contain repeated separators"),
        }
    }
}

impl std::error::Error for KeyError {}

fn normalize_key(value: String) -> Result<String, KeyError> {
    let value = value.trim().trim_matches('/').to_string();
    if value.is_empty() {
        return Err(KeyError::Empty);
    }
    validate_path(&value)?;
    Ok(value)
}

fn normalize_prefix(value: String) -> Result<String, KeyError> {
    let value = value.trim().trim_matches('/').to_string();
    if value.is_empty() {
        return Ok(value);
    }
    validate_path(&value)?;
    Ok(value)
}

fn validate_path(value: &str) -> Result<(), KeyError> {
    if value.starts_with('/') {
        return Err(KeyError::Absolute);
    }
    if value.contains("../") || value.ends_with("/..") || value == ".." {
        return Err(KeyError::ParentTraversal);
    }
    if value.contains("//") {
        return Err(KeyError::RepeatedSeparator);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_but_preserves_logical_key() {
        let key = LakeKey::new("/raw/source=rera/dt=2026-07/data.parquet/").unwrap();
        assert_eq!(key.as_str(), "raw/source=rera/dt=2026-07/data.parquet");
    }

    #[test]
    fn rejects_parent_traversal() {
        assert_eq!(
            LakeKey::new("../secret").unwrap_err(),
            KeyError::ParentTraversal
        );
    }
}
