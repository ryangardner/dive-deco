#![cfg_attr(feature = "no-std", no_std)]

#[cfg(feature = "alloc")]
extern crate alloc;

mod buhlmann;
mod common;

pub use buhlmann::{
    BuehlmannConfig, BuehlmannModel, BuhlmannConfig, BuhlmannModel, Compartment, Supersaturation,
};

pub use common::{
    BreathingSource, CeilingType, Deco, DecoCalculationError, DecoModel, DecoRuntime, DecoStage,
    DecoStageType, DecoStopFormatting, Depth, DepthType, DiveComputer, DiveMode, DiveState, Gas,
    GasMix, GradientFactors, NDLType, Pressure, RecordData, SetpointConfig, SetpointController,
    Sim, Time, Unit, Units,
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
