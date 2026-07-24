use url::Url;

#[derive(Debug, Clone)]
pub struct StreetViewFrameInput {
    pub pano_id: Option<String>,
    pub location: Option<StreetViewLocation>,
    pub location_query: Option<String>,
    pub radius_m: Option<u32>,
    pub heading: f64,
    pub pitch: f64,
    pub fov: f64,
}

#[derive(Debug, Clone)]
pub struct StreetViewLocation {
    pub latitude: f64,
    pub longitude: f64,
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
    let mut pairs = image_url.query_pairs_mut();
    pairs.append_pair("size", "640x420");
    if let Some(pano_id) = frame
        .pano_id
        .as_deref()
        .filter(|pano_id| !pano_id.is_empty())
    {
        pairs.append_pair("pano", pano_id);
    } else {
        if let Some(location) = frame.location.as_ref() {
            pairs.append_pair(
                "location",
                &format!("{:.7},{:.7}", location.latitude, location.longitude),
            );
        } else {
            pairs.append_pair("location", frame.location_query.as_deref()?.trim());
        }
        if let Some(radius_m) = frame.radius_m {
            pairs.append_pair("radius", &radius_m.to_string());
        }
    }
    pairs
        .append_pair("heading", &format!("{:.1}", frame.heading))
        .append_pair("pitch", &format!("{:.1}", frame.pitch))
        .append_pair("fov", &format!("{:.1}", frame.fov))
        .append_pair("source", "outdoor")
        .append_pair("key", api_key);
    drop(pairs);
    Some(image_url.to_string())
}

pub fn street_view_pano_url(frame: &StreetViewFrameInput) -> Option<String> {
    let mut source_url = Url::parse("https://www.google.com/maps/@").ok()?;
    let mut pairs = source_url.query_pairs_mut();
    pairs
        .append_pair("api", "1")
        .append_pair("map_action", "pano");
    if let Some(pano_id) = frame
        .pano_id
        .as_deref()
        .filter(|pano_id| !pano_id.is_empty())
    {
        pairs.append_pair("pano", pano_id);
    } else {
        if let Some(location) = frame.location.as_ref() {
            pairs.append_pair(
                "viewpoint",
                &format!("{:.7},{:.7}", location.latitude, location.longitude),
            );
        } else {
            drop(pairs);
            return street_view_search_url(frame.location_query.as_deref()?);
        }
    }
    pairs
        .append_pair("heading", &format!("{:.1}", frame.heading))
        .append_pair("pitch", &format!("{:.1}", frame.pitch))
        .append_pair("fov", &format!("{:.1}", frame.fov));
    drop(pairs);
    Some(source_url.to_string())
}

fn street_view_search_url(query: &str) -> Option<String> {
    let query = query.trim();
    if query.is_empty() {
        return None;
    }
    let mut source_url = Url::parse("https://www.google.com/maps/search/").ok()?;
    source_url
        .query_pairs_mut()
        .append_pair("api", "1")
        .append_pair("query", query);
    Some(source_url.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_url_can_render_coordinate_backed_frame() {
        let frame = StreetViewFrameInput {
            pano_id: None,
            location: Some(StreetViewLocation {
                latitude: 12.9923456,
                longitude: 77.7012345,
            }),
            location_query: None,
            radius_m: Some(250),
            heading: 90.0,
            pitch: 0.0,
            fov: 80.0,
        };

        let url = street_view_static_url(&frame, "test-key").expect("url should render");

        assert!(url.contains("location=12.9923456%2C77.7012345"));
        assert!(url.contains("radius=250"));
        assert!(url.contains("heading=90.0"));
        assert!(!url.contains("pano="));
    }

    #[test]
    fn static_url_can_render_query_backed_frame() {
        let frame = StreetViewFrameInput {
            pano_id: None,
            location: None,
            location_query: Some("CANDEUR SIGNATURE Bengaluru".to_string()),
            radius_m: Some(250),
            heading: 0.0,
            pitch: 0.0,
            fov: 80.0,
        };

        let url = street_view_static_url(&frame, "test-key").expect("url should render");
        let source_url = street_view_pano_url(&frame).expect("source url should render");

        assert!(url.contains("location=CANDEUR+SIGNATURE+Bengaluru"));
        assert!(url.contains("radius=250"));
        assert!(source_url.contains("query=CANDEUR+SIGNATURE+Bengaluru"));
    }
}
