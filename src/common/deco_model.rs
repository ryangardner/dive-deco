use crate::common::deco::{DecoCalculationError, DecoRuntime};
use crate::common::global_types::{CeilingType, DecoStopFormatting, MbarPressure, Pressure};
use crate::common::ox_tox::OxTox;
use crate::common::{AscentRatePerMinute, BreathingSource, Cns, Otu};
use crate::common::{Depth, Time};
#[cfg(feature = "alloc")]
use alloc::string::String;
#[cfg(all(feature = "heapless", not(feature = "alloc")))]
use heapless::String as HString;

#[cfg(feature = "alloc")]
pub type ValidationString = String;
#[cfg(all(feature = "heapless", not(feature = "alloc")))]
pub type ValidationString = HString<64>;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ConfigValidationErr {
    pub field: ValidationString,
    pub reason: ValidationString,
}

impl ConfigValidationErr {
    pub fn new(field: &str, reason: &str) -> Self {
        #[cfg(feature = "alloc")]
        let (f, r) = (String::from(field), String::from(reason));

        #[cfg(all(feature = "heapless", not(feature = "alloc")))]
        let (f, r) = (
            ValidationString::try_from(field).unwrap_or_default(),
            ValidationString::try_from(reason).unwrap_or_default()
        );

        Self {
            field: f,
            reason: r,
        }
    }
}

pub trait DecoModelConfig {
    fn validate(&self) -> Result<(), ConfigValidationErr>;
    fn surface_pressure(&self) -> MbarPressure;
    fn deco_ascent_rate(&self) -> AscentRatePerMinute;
    fn ceiling_type(&self) -> CeilingType;
    fn round_ceiling(&self) -> bool;
    fn water_density(&self) -> f32;
    fn stop_formatting(&self) -> DecoStopFormatting;
    fn last_stop_depth(&self) -> Depth;
    fn min_pp_o2(&self) -> Pressure;
    fn gas_switch_duration(&self) -> Time;
    fn switch_at_stop_only(&self) -> bool;
    fn max_end_depth(&self) -> Depth;
    fn deco_stop_increment(&self) -> Depth;
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct DiveState {
    pub depth: Depth,
    pub time: Time,
    pub gas: BreathingSource,
    pub ox_tox: OxTox,
}

pub trait DecoModel {
    type ConfigType: DecoModelConfig;

    // default
    fn default() -> Self;

    /// model init
    fn new(config: Self::ConfigType) -> Self;

    /// get model config
    fn config(&self) -> Self::ConfigType;

    /// get model dive state
    fn dive_state(&self) -> DiveState;

    /// record (depth: meters, time: seconds)
    fn record(&mut self, depth: Depth, time: Time, gas: &BreathingSource);

    /// record linear ascent / descent record given travel time
    fn record_travel(&mut self, target_depth: Depth, time: Time, gas: &BreathingSource);

    /// register linear ascent / descent record given rate
    fn record_travel_with_rate(
        &mut self,
        target_depth: Depth,
        rate: AscentRatePerMinute,
        gas: &BreathingSource,
    );

    /// current non decompression limit (NDL)
    fn ndl(&self) -> Time;

    /// current decompression ceiling in meters
    fn ceiling(&self) -> Depth;

    /// deco stages, TTL
    fn deco(&self, gas_mixes: &[BreathingSource]) -> Result<DecoRuntime, DecoCalculationError>;

    /// is in deco check
    fn in_deco(&self) -> bool {
        let ceiling_type = self.config().ceiling_type();
        match ceiling_type {
            CeilingType::Actual => self.ceiling() > Depth::zero(),
            CeilingType::Adaptive => {
                let current_gas = self.dive_state().gas;
                let runtime = self.deco(&[current_gas]).unwrap();
                let deco_stages = runtime.deco_stages;
                deco_stages.len() > 1
            }
        }
    }

    /// central nervous system oxygen toxicity
    fn cns(&self) -> Cns {
        self.dive_state().ox_tox.cns()
    }

    /// pulmonary oxygen toxicity
    fn otu(&self) -> Otu {
        self.dive_state().ox_tox.otu()
    }
}
