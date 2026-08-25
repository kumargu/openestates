//! Public API security policy.
//!
//! Keep authentication, client identity, request guards, and Tower middleware
//! policy in this directory so the complete launch perimeter can be audited
//! without reading route business logic.

mod admin_auth;
pub(crate) mod admin_run;
mod client_ip;
mod config;
mod execution;
mod guards;
pub(crate) mod interest_storage;
mod media_stream;
mod policy;
pub(crate) mod retention;

pub use admin_auth::require_admin;
pub use config::{security_tuning, SecurityTuning};
pub use execution::{CustomerComputeError, ExecutionLanes};
pub use media_stream::MediaStreamAdmission;
pub use policy::SecurityPolicy;
pub use retention::prune_rebuildable_serving_cache;
