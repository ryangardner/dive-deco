use crate::common::Depth;
#[cfg(feature = "alloc")]
use alloc::vec::Vec;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// High-level gas planning helpers for dive computers and applications.
/// This module provides pure mathematical calculations for gas requirements
/// without managing physical gear state like tank sizes or sidemount pooling.

/// Returns the volume of gas consumed for a single segment (L).
pub fn segment_usage(depth: crate::common::Depth, surface_pressure_mbar: u16, rmv: f32, duration_mins: f32) -> f32 {
    let pressure_abs_bar = (depth.as_meters() / 10.0) + (surface_pressure_mbar as f32 / 1000.0);
    rmv * pressure_abs_bar * duration_mins
}

/// A breakdown of gas volume required for a specific breathing source.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct GasRequirement {
    pub source: crate::common::BreathingSource,
    pub volume_liters: f32,
}

/// Helper for gas inventory and sufficiency checks.
#[cfg(feature = "alloc")]
pub struct GasInventory {
    pub sources: Vec<(crate::common::BreathingSource, f32)>,
}

#[cfg(feature = "alloc")]
impl GasInventory {
    pub fn new() -> Self {
        Self { sources: Vec::new() }
    }

    pub fn add_gas(&mut self, source: crate::common::BreathingSource, volume: f32) {
        if let Some(entry) = self.sources.iter_mut().find(|(s, _)| s == &source) {
            entry.1 += volume;
        } else {
            self.sources.push((source, volume));
        }
    }

    /// Verifies if the inventory can fulfill the given requirements.
    pub fn can_fulfill(&self, requirements: &[(crate::common::BreathingSource, f32)]) -> Result<(), crate::common::BreathingSource> {
        for (source, needed) in requirements {
            let available = self.sources.iter()
                .find(|(s, _)| s == source)
                .map(|(_, v)| *v)
                .unwrap_or(0.0);
            if available < *needed {
                return Err(*source);
            }
        }
        Ok(())
    }
}

/// Pure math for Gas Time Remaining (GTR) calculations.
pub struct DecoGasPlanner {
    pub rmv_l_min: f32,
    pub surface_pressure_mbar: u16,
}

impl DecoGasPlanner {
    pub fn new(rmv_l_min: f32, surface_pressure_mbar: u16) -> Self {
        Self { rmv_l_min, surface_pressure_mbar }
    }

    /// Calculates GTR (Gas Time Remaining) for the current bottom segment.
    /// This estimates how many more minutes can be spent at `current_depth` on `current_source`
    /// before the `available_volume_liters` matches the `needed_to_surface_liters`.
    pub fn calculate_gtr(
        &self, 
        current_depth: Depth, 
        available_volume_liters: f32, 
        needed_to_surface_liters: f32
    ) -> f32 {
        let pressure_abs_bar = (current_depth.as_meters() / 10.0) + (self.surface_pressure_mbar as f32 / 1000.0);
        let usage_rate = self.rmv_l_min * pressure_abs_bar;
        
        if usage_rate <= 0.0 { return f32::INFINITY; }
        
        let margin = (available_volume_liters - needed_to_surface_liters).max(0.0);
        margin / usage_rate
    }
}

/// Static Rule: Rule of Thirds Turn Pressure calculation (pure math).
pub fn rule_of_thirds_turn(start_pressure: f32) -> f32 {
    start_pressure - (start_pressure / 3.0)
}

/// Static Rule: Rule of Half Turn Pressure calculation (pure math).
pub fn rule_of_half_turn(start_pressure: f32) -> f32 {
    start_pressure / 2.0
}

/// Pure math for CCR Bailout safety checks.
pub struct BailoutMath;

impl BailoutMath {
    /// Checks if a set of available gas volumes meets the requirements for a given bailout profile.
    pub fn is_sufficient(
        inventory: &GasInventory,
        oc_requirements: &[(crate::common::BreathingSource, f32)]
    ) -> bool {
        inventory.can_fulfill(oc_requirements).is_ok()
    }

    /// Calculates the total volume margin (Available - Needed).
    pub fn volume_margin(
        inventory: &GasInventory,
        oc_requirements: &[(crate::common::BreathingSource, f32)]
    ) -> f32 {
        let total_available: f32 = inventory.sources.iter().map(|(_, v)| v).sum();
        let total_needed: f32 = oc_requirements.iter().map(|(_, v)| v).sum();
        total_available - total_needed
    }
}

/// Calculates the Maximum Operating Depth (MOD) for a gas based on configuration limits.
pub fn calculate_mod(
    gas: &crate::common::GasMix,
    config: &impl crate::common::DecoModelConfig,
    is_deco: bool
) -> crate::common::Depth {
    let limit = if is_deco {
        config.max_pp_o2_deco()
    } else {
        config.max_pp_o2_normal()
    };
    
    gas.max_operating_depth_at(
        limit,
        config.surface_pressure(),
        config.water_density()
    )
}
