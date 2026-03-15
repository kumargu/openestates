/// Key generation matching the Python pipeline's scheme.
///
/// Keys follow the pattern: `{entity_type}/{city}/{area}/{id}/{filename}`
/// Example: `properties/bangalore/whitefield/prop_w_001/data.json`
#[allow(dead_code)]
pub struct StorageKey;

#[allow(dead_code)]
impl StorageKey {
    /// Key for listing all entities of a given type in a city.
    pub fn entity_prefix(entity_type: &str, city: &str) -> String {
        format!("{}/{}", entity_type, city)
    }

    /// Key for a specific entity's data file.
    pub fn entity_data(entity_type: &str, city: &str, area: &str, id: &str) -> String {
        format!("{}/{}/{}/{}/data.json", entity_type, city, area, id)
    }

    /// Key for a specific file within an entity's directory.
    pub fn entity_file(
        entity_type: &str,
        city: &str,
        area: &str,
        id: &str,
        filename: &str,
    ) -> String {
        format!("{}/{}/{}/{}/{}", entity_type, city, area, id, filename)
    }
}
