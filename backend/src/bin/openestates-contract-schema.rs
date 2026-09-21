use schemars::{generate::SchemaSettings, JsonSchema};
use std::path::PathBuf;

fn export<T: JsonSchema>(root: &std::path::Path, name: &str) {
    let schema = SchemaSettings::draft07()
        .for_serialize()
        .into_generator()
        .into_root_schema_for::<T>();
    let mut bytes = serde_json::to_vec_pretty(&schema).unwrap();
    bytes.push(b'\n');
    std::fs::write(root.join(format!("{name}.json")), bytes).unwrap();
}
fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../frontend/src/generated/schema");
    std::fs::create_dir_all(&root).unwrap();
    export::<backend::search::journey::SearchJourneyEnvelope>(&root, "SearchJourneyEnvelope");
    export::<backend::search::proof::ProofResolutionFailure>(&root, "SearchProofFailure");
    export::<backend::search::proof::ProofResolution>(&root, "SearchProofResolution");
    export::<Vec<backend::models::PropertyCard>>(&root, "PropertyCatalog");
    export::<backend::routes::properties::PropertyEvidenceResponse>(&root, "PropertyEvidence");
    export::<backend::routes::properties::PropertyDetail>(&root, "PropertyDetail");
    export::<backend::routes::properties::PropertySummaries>(&root, "PropertySummaries");
    export::<backend::surfaces::SurfaceSceneResponse>(&root, "PropertyContext");
}
