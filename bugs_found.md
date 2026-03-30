# Comprehensive Bug Report - GW2 Build Optimizer

## Bug #1: Double Multiplication Error in Duration Bonus Calculations
**Location:** `crates/optimizer/src/combat.rs`, lines 65-66, 81-82  
**Severity:** ⚠️ **HIGH** - Causes 100x calculation error  
**Category:** Mathematical Formula Error

### Description
The functions `total_condi_duration_bonus()` and `total_boon_duration_bonus()` incorrectly multiply the sum by 100.0, when the values in `condi_duration_pct` and `boon_duration_pct` vectors are already stored as ratios (e.g., 0.07 for 7%).

### Code Evidence
```rust
// Line 65-66
pub fn total_condi_duration_bonus(&self) -> f64 {
    self.condi_duration_pct.iter().sum::<f64>() * 100.0  // BUG: Extra *100
}

// Line 81-82
pub fn total_boon_duration_bonus(&self) -> f64 {
    self.boon_duration_pct.iter().sum::<f64>() * 100.0  // BUG: Extra *100
}
```

### Impact
When these values are used in `calculate_combat_performance()` at lines 370 and 418:
```rust
let global_condi_ratio: f64 = modifiers.condi_duration_pct.iter().sum();  // Correct, no *100
let global_boon_ratio: f64 = modifiers.boon_duration_pct.iter().sum();    // Correct, no *100
```

The code correctly sums without multiplying by 100, but then if anyone calls `total_condi_duration_bonus()`, they get a value 100x too large. This is inconsistent and confusing.

### Root Cause
The `condi_duration_pct` and `boon_duration_pct` vectors store values as decimal ratios (e.g., 0.07 = 7%), but the `*_bonus()` methods multiply by 100.0 as if trying to convert to percentage points. This is inconsistent with how the data is actually used elsewhere.

### Expected Behavior
Either:
1. Remove the `* 100.0` from `total_condi_duration_bonus()` and `total_boon_duration_bonus()` to return ratios
2. OR, if the intention is to return percentage points, the name should be `total_condi_duration_bonus_percentage_points()` and all callers need to divide by 100 before using as ratio

### Fix
```rust
// Option 1: Return as ratio (Recommended for consistency)
pub fn total_condi_duration_bonus(&self) -> f64 {
    self.condi_duration_pct.iter().sum()  // No *100
}

// Option 2: Clear naming and proper conversion
pub fn total_condi_duration_bonus_percentage_points(&self) -> f64 {
    self.condi_duration_pct.iter().sum::<f64>() * 100.0
}
```

---

## Bug #2: Potential Division by Zero in Duration Calculations
**Location:** `crates/optimizer/src/combat.rs`, lines 447-449  
**Severity:** ⚠️ **CRITICAL** - Runtime panic/crash  
**Category:** Undefined Behavior

### Code Evidence
```rust
let strike_ehp = health * armor / f.tooltip_reference_armor / (1.0 - protection_dr);
let condition_ehp = health / (1.0 - resolution_dr);
```

### Issue
If `protection_dr` or `resolution_dr` equals 1.0 (100% damage reduction), the denominator becomes 0.0, causing division by zero which results in infinity (f64::INFINITY) or NaN.

According to the code at lines 437-446:
```rust
let protection_dr = if buffs.protection {
    1.0 - b.protection_multiplier()  // If protection_multiplier() returns 0.0, dr = 1.0
} else {
    0.0
};
```

If `b.protection_multiplier()` ever returns 0.0 (or a very small value), we get division by zero.

### Impact
- Division by zero produces `f64::INFINITY` or `f64::NaN`
- This breaks all downstream calculations
- May cause UI to display Infinity/NaN values
- May break sorting/comparisons

### Fix
```rust
let protection_dr_clamped = (1.0 - b.protection_multiplier()).max(0.01);  // Prevent 0
let resolution_dr_clamped = (1.0 - b.resolution_multiplier()).max(0.01);

let strike_ehp = health * armor / f.tooltip_reference_armor / protection_dr_clamped;
let condition_ehp = health / resolution_dr_clamped;
```

---

## Bug #3: Unused Parameter Warning / Dead Code
**Location:** `crates/optimizer/src/combat.rs`, line 325  
**Severity:** ℹ️ **LOW** - Code Quality  
**Category:** Dead Code / Unused Variable

### Code Evidence
```rust
pub fn calculate_combat_performance(
    stats: &StatBlock,
    _derived: &DerivedStats,  // PREPEND underscore - parameter not used!
    modifiers: &DamageModifiers,
    ...
) -> CombatPerformance {
```

The parameter `_derived` is prepended with underscore indicating it's intentionally unused, but this should be investigated - either it should be used or removed from the function signature.

### Impact
- API confusion for callers
- Potential breaking change if parameter is removed later
- Suggests incomplete refactoring

---

## Summary
Found **3 bugs** in `combat.rs` alone:
1. **HIGH**: Double multiplication causing 100x error in duration bonuses
2. **CRITICAL**: Division by zero potential in EHP calculations
3. **LOW**: Unused parameter suggesting incomplete refactoring

Continuing analysis of other files...
