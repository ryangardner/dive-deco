#![cfg_attr(feature = "no-std", no_std)]

#[cfg(feature = "alloc")]
extern crate alloc;

mod buhlmann;
mod common;

pub use buhlmann::{
    BuehlmannConfig, BuehlmannModel, BuhlmannConfig, BuhlmannModel, Compartment, Supersaturation,
    TissueSnapshot,
};

pub use common::{
    BreathingSource, CeilingType, Deco, DecoCalculationError, DecoModel, DecoRuntime, DecoStage,
    DecoStageType, DecoStopFormatting, Depth, DepthType, DiveComputer, DiveMode, DiveState, GasError,
    GasMix, GradientFactors, NDLType, Pressure, RecordData, SetpointConfig, SetpointController, Sim, Time,
    Unit, Units,
};

pub use common::planning::{
    calculate_mod, rule_of_half_turn, rule_of_thirds_turn, segment_usage, BailoutMath,
    DecoGasPlanner, GasInventory, GasRequirement,
};

// Re-export Vec and vec macro from alloc for convenience (only with alloc feature)
#[cfg(feature = "alloc")]
pub use alloc::vec;
#[cfg(feature = "alloc")]
pub use alloc::vec::Vec;

// Re-export buffer abstractions
pub use common::buffer::{DecoStageContainer, DefaultStageContainer};

// Re-export heapless for embedded users
#[cfg(feature = "heapless")]
pub use heapless;
