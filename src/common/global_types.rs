#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

pub type Pressure = f32;
pub type DepthType = f32;
pub type GradientFactor = u8;
pub type GradientFactors = (u8, u8);
pub type MbarPressure = i32;
pub type AscentRatePerMinute = f32;
pub type Cns = f32;
pub type Otu = f32;

#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum NDLType {
    Actual,    // take into consideration off-gassing during ascent
    ByCeiling, // treat NDL as a point when ceiling > 0.
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum CeilingType {
    Actual,
    Adaptive,
}

/// Defines how decompression stops are rounded/calculated.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum DecoStopFormatting {
    /// Standard Metric: Stops at 3m, 6m, 9m...
    Metric,
    /// Standard Imperial: Stops at 10ft, 20ft, 30ft...
    Imperial,
    /// Continuous: Stops at exact ceiling depth (Continuous Decompression)
    Continuous,
}


