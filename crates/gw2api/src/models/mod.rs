//! GW2 API v2 response models.
//! Each submodule contains serde structs for one API endpoint family.

pub mod characters;
pub mod facts;
pub mod items;
pub mod itemstats;
pub mod legends;
pub mod pets;
pub mod professions;
pub mod pvp;
pub mod skills;
pub mod specs;
pub mod traits;

// Re-export top-level types for convenience.
pub use characters::*;
pub use facts::{deserialize_facts, deserialize_traited_facts, Fact, TraitedFact};
pub use items::*;
pub use itemstats::*;
pub use legends::*;
pub use pets::*;
pub use professions::*;
pub use pvp::*;
pub use skills::*;
pub use specs::*;
pub use traits::*;
