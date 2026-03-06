use serde::Deserialize;

pub mod profession_profiles;
pub mod slot_budgets;
pub mod universal_formulas;

pub use profession_profiles::ProfessionProfiles;
pub use slot_budgets::SlotBudgets;
pub use universal_formulas::UniversalFormulas;

/// Evidence level for data entries. Shared across all data loaders.
/// - Factual: directly from wiki or game data with exact values.
/// - Derived: calculated from factual data using known formulas.
/// - Heuristic: empirically tuned or estimated values.
/// - Unknown: unverified or placeholder values.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub enum EvidenceLevel {
    Factual,
    Derived,
    Heuristic,
    Unknown,
}
