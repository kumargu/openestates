//! Public attributes deliberately do not embed runtime storage records.
//! Measurement/evidence meaning is shared; internal scores and ingestion state are private.
use serde::Serialize;

#[derive(schemars::JsonSchema, Serialize)]
pub struct PropertyAttributes {
    pub id: String,
    pub title: String,
    pub area: String,
    pub area_id: String,
    pub city: String,
    pub society_id: String,
    pub builder_name: String,
    pub property_type: String,
    pub listing_type: String,
    #[serde(skip_serializing_if = "crate::models::property::is_zero")]
    pub bhk: u32,
    #[serde(skip_serializing_if = "crate::models::property::is_zero")]
    pub price: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "u64")]
    pub price_min: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "u64")]
    pub price_max: Option<u64>,
    #[serde(skip_serializing_if = "crate::models::property::is_zero")]
    pub price_per_sqft: u64,
    #[serde(skip_serializing_if = "crate::models::property::is_zero")]
    pub carpet_area_sqft: u32,
    #[serde(skip_serializing_if = "crate::models::property::is_zero")]
    pub super_builtup_sqft: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "crate::models::Measurement")]
    pub area_measurement: Option<crate::models::Measurement>,
    pub possession_status: String,
    pub images: Vec<String>,
    pub hero_image: String,
    pub description_summary: String,
}

impl From<&crate::models::Property> for PropertyAttributes {
    fn from(property: &crate::models::Property) -> Self {
        Self {
            id: property.id.clone(),
            title: property.title.clone(),
            area: property.area.clone(),
            area_id: property.area_id.clone(),
            city: property.city.clone(),
            society_id: property.society_id.clone(),
            builder_name: property.builder_name.clone(),
            property_type: property.property_type.clone(),
            listing_type: property.listing_type.clone(),
            bhk: property.bhk,
            price: property.price,
            price_min: property.price_min,
            price_max: property.price_max,
            price_per_sqft: property.price_per_sqft,
            carpet_area_sqft: property.carpet_area_sqft,
            super_builtup_sqft: property.super_builtup_sqft,
            area_measurement: property.area_measurement.clone(),
            possession_status: property.possession_status.clone(),
            images: property.images.clone(),
            hero_image: property.hero_image.clone(),
            description_summary: property.description_summary.clone(),
        }
    }
}

#[derive(schemars::JsonSchema, Serialize)]
pub struct SocietySummary {
    pub id: String,
    pub name: String,
    pub area: String,
    pub city: String,
    pub builder_name: String,
}

impl From<&crate::models::Society> for SocietySummary {
    fn from(society: &crate::models::Society) -> Self {
        Self {
            id: society.id.clone(),
            name: society.name.clone(),
            area: society.area.clone(),
            city: society.city.clone(),
            builder_name: society.builder_name.clone(),
        }
    }
}
