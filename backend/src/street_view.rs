use url::Url;

#[derive(Debug, Clone)]
pub struct StreetViewFrameInput {
    pub pano_id: String,
    pub heading: f64,
    pub pitch: f64,
    pub fov: f64,
}

pub fn google_maps_api_key() -> Option<String> {
    std::env::var("GOOGLE_STREET_VIEW_API_KEY")
        .or_else(|_| std::env::var("GOOGLE_PLACES_API_KEY"))
        .or_else(|_| std::env::var("GOOGLE_MAPS_API_KEY"))
        .ok()
        .filter(|key| !key.trim().is_empty())
}

pub fn street_view_static_url(frame: &StreetViewFrameInput, api_key: &str) -> Option<String> {
    let mut image_url = Url::parse("https://maps.googleapis.com/maps/api/streetview").ok()?;
    image_url
        .query_pairs_mut()
        .append_pair("size", "640x420")
        .append_pair("pano", &frame.pano_id)
        .append_pair("heading", &format!("{:.1}", frame.heading))
        .append_pair("pitch", &format!("{:.1}", frame.pitch))
        .append_pair("fov", &format!("{:.1}", frame.fov))
        .append_pair("source", "outdoor")
        .append_pair("key", api_key);
    Some(image_url.to_string())
}

pub fn street_view_pano_url(frame: &StreetViewFrameInput) -> Option<String> {
    let mut source_url = Url::parse("https://www.google.com/maps/@").ok()?;
    source_url
        .query_pairs_mut()
        .append_pair("api", "1")
        .append_pair("map_action", "pano")
        .append_pair("pano", &frame.pano_id)
        .append_pair("heading", &format!("{:.1}", frame.heading))
        .append_pair("pitch", &format!("{:.1}", frame.pitch))
        .append_pair("fov", &format!("{:.1}", frame.fov));
    Some(source_url.to_string())
}
