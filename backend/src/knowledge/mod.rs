//! Knowledge Graph — the brain that learns.
//!
//! Every search builds the graph. The graph makes every future search better.
//!
//! Core concepts:
//! - **SourcedFact**: Every fact has provenance (who said it, confidence, when learned)
//! - **Node**: An entity (property, society, area, builder, metro, school, signal)
//! - **Edge**: A typed relationship between nodes
//! - **KnowledgeGraph**: The container with indexes for fast traversal

pub mod edge;
pub mod embeddings;
pub mod fact;
pub mod graph;
pub mod node;
pub mod query;
pub mod search_event;
pub mod store;

pub use fact::{FactValue, SourcedFact};
pub use graph::{GraphStats, KnowledgeGraph};
pub use search_event::SearchEvent;
