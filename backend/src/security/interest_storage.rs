use std::io;
use std::path::Path;

use super::security_tuning;

pub(crate) fn validate_interest_fields(
    buyer_name: Option<&str>,
    buyer_contact: Option<&str>,
) -> Result<(), &'static str> {
    let tuning = &security_tuning().interest_storage;
    if buyer_name.is_some_and(|value| value.chars().count() > tuning.max_name_chars) {
        return Err("buyer_name is too long");
    }
    if buyer_contact.is_some_and(|value| value.chars().count() > tuning.max_contact_chars) {
        return Err("buyer_contact is too long");
    }
    Ok(())
}

pub(crate) fn interest_append_fits(
    file_bytes: u64,
    storage_bytes: u64,
    record_bytes: usize,
) -> bool {
    let tuning = &security_tuning().interest_storage;
    if record_bytes > tuning.max_record_bytes {
        return false;
    }
    let record_bytes = record_bytes as u64;
    file_bytes
        .checked_add(record_bytes)
        .is_some_and(|total| total <= tuning.max_property_file_bytes)
        && storage_bytes
            .checked_add(record_bytes)
            .is_some_and(|total| total <= tuning.max_total_bytes)
}

pub(crate) async fn interest_storage_bytes(directory: &Path) -> io::Result<u64> {
    let mut entries = match tokio::fs::read_dir(directory).await {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(error),
    };
    let mut total = 0_u64;
    while let Some(entry) = entries.next_entry().await? {
        let metadata = entry.metadata().await?;
        if metadata.is_file() {
            total = total.saturating_add(metadata.len());
        }
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interest_fields_have_small_explicit_limits() {
        assert!(validate_interest_fields(Some("Buyer"), Some("buyer@example.com")).is_ok());
        let tuning = &security_tuning().interest_storage;
        assert!(
            validate_interest_fields(Some(&"a".repeat(tuning.max_name_chars + 1)), None).is_err()
        );
        assert!(
            validate_interest_fields(None, Some(&"a".repeat(tuning.max_contact_chars + 1)))
                .is_err()
        );
    }

    #[test]
    fn interest_append_is_bounded_per_file_and_across_storage() {
        assert!(interest_append_fits(0, 0, 512));
        let tuning = &security_tuning().interest_storage;
        assert!(!interest_append_fits(0, 0, tuning.max_record_bytes + 1));
        assert!(!interest_append_fits(tuning.max_property_file_bytes, 0, 1));
        assert!(!interest_append_fits(0, tuning.max_total_bytes, 1));
    }
}
