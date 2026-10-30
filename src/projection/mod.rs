//! Contracts for projecting approved private work onto the public map (ADR-002).
//!
//! Three formats, each with a fail-closed validator in [`schema`]:
//!
//! - `gitmaps.projection/v1`: the public, signed region file.
//! - `gitmaps.projection-allowlist/v1`: the private, founder-approved list of quests that
//!   may be projected. Its `source_issue` numbers never appear in a projection.
//! - `gitmaps.projection-contributors/v1`: opt-in public handles.
//!
//! The JSON Schemas in `docs/schemas/` document the same rules; the Rust checks are
//! authoritative. Nothing here reads GitHub, signs, or renders.

pub mod schema;

pub use schema::{validate_allowlist, validate_contributors, validate_projection};

pub const PROJECTION_SCHEMA: &str = "gitmaps.projection/v1";
pub const ALLOWLIST_SCHEMA: &str = "gitmaps.projection-allowlist/v1";
pub const CONTRIBUTORS_SCHEMA: &str = "gitmaps.projection-contributors/v1";
/// Same cap as the map itself (`MAX_CROSSINGS`).
pub const MAX_QUESTS: usize = 200;
pub const MAX_TITLE: usize = 80;
pub const MAX_SUMMARY: usize = 400;
pub const MAX_DISPLAY_NAME: usize = 60;
pub const MAX_TAGS: usize = 4;
pub const TAGS: &[&str] = &[
    "accessibility",
    "backend",
    "ci",
    "data",
    "docs",
    "frontend",
    "infrastructure",
    "security",
    "testing",
];
/// Projected states. INVALID is never published: the projector fails closed instead.
pub const STATES: &[&str] = &["DISCOVERED", "LOCKED", "CLAIMED", "PROVEN", "RETIRED"];
