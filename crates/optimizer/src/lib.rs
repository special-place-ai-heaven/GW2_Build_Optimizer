pub mod balance;
pub mod benchmark;
pub mod combat;
pub mod context;
pub mod data;
pub mod engine;
pub mod gamedb;
pub mod gemini;
pub mod gemini_tools;
pub mod llm;
pub mod prompts;
pub mod referee;
pub mod rotation;
pub mod scenario;
pub mod scoring;
pub mod scraper;
pub mod search;
pub mod search_v2;
pub mod stats;
pub mod synergy;
pub mod synergy_pipeline;
pub mod validation;

// Re-export viability types for downstream consumers (S07 Trust UI).
pub use referee::{GateResult, ViabilityGate, ViabilityReport};
// Re-export scenario types for addon UI (S03).
pub use scenario::{CombatTier, RoleObjective, ScenarioSpec};
