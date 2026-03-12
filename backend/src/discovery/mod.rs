//! Live Discovery — discover properties on-the-fly using Gemini + Google Search.
//!
//! When a search query matches no existing data (or scores poorly), the backend
//! calls Gemini Flash with Google Search grounding to discover real properties.
//! Results are ingested into the knowledge graph and seed data, so the next
//! search for that area is instant.

pub mod gemini;
pub mod cache;
pub mod ingest;

pub use cache::DiscoveryCache;
pub use gemini::GeminiClient;
