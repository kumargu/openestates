use crate::models::{AreaProfile, Property, Society};

pub struct AppState {
    pub properties: Vec<Property>,
    pub areas: Vec<AreaProfile>,
    pub societies: Vec<Society>,
}
