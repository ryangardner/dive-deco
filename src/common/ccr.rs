use crate::common::gas::{BreathingSource, GasMix};
use crate::{BuhlmannModel, Deco, DecoModel, DecoRuntime, Depth, Time};
use alloc::vec::Vec;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Configuration for automatic setpoint behavior.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct SetpointConfig {
    pub low_setpoint: f32,
    pub high_setpoint: f32,

    /// Depth to auto-switch Low -> High (Descent).
    /// If None, manual switching only.
    pub switch_depth_descent: Option<f32>,

    /// Depth to auto-switch High -> Low (Ascent).
    /// If None, the controller maintains High setpoint until surface/manual override.
    pub switch_depth_ascent: Option<f32>,
}

impl Default for SetpointConfig {
    fn default() -> Self {
        Self {
            low_setpoint: 0.7,
            high_setpoint: 1.3,
            switch_depth_descent: Some(20.0), // Standard descent switch
            switch_depth_ascent: Some(6.0),   // Shallow safety switch
        }
    }
}

/// The internal state of the controller.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum ControllerState {
    Low,
    High,
    /// User has manually overridden the auto logic.
    ManualOverride(f32),
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct SetpointController {
    config: SetpointConfig,
    state: ControllerState,
    /// The currently selected diluent gas.
    active_diluent: GasMix,
}

impl SetpointController {
    /// Creates a new controller state.
    pub fn new(config: SetpointConfig, diluent: GasMix) -> Self {
        Self {
            config,
            state: ControllerState::Low, // Default to Low on start
            active_diluent: diluent,
        }
    }

    /// Updates the state based on current depth and returns the active breathing source.
    /// This should be called every time step of the dive simulation.
    pub fn tick(&mut self, current_depth: f32) -> BreathingSource {
        self.handle_auto_switch(current_depth);

        let target_sp = match self.state {
            ControllerState::Low => self.config.low_setpoint,
            ControllerState::High => self.config.high_setpoint,
            ControllerState::ManualOverride(sp) => sp,
        };

        BreathingSource::ClosedCircuit {
            setpoint: target_sp,
            diluent: self.active_diluent,
        }
    }

    /// Return piecewise-constant BreathingSource segments for a monotonic linear travel.
    /// Splits once if an auto-switch threshold is crossed.
    ///
    /// Usage:
    ///   for (end_depth, seg_time, src) in controller.travel_segments(d0, d1, dt) {
    ///       model.record_travel(end_depth, seg_time, &src);
    ///   }
    pub fn travel_segments(
        &mut self,
        start_depth_m: f32,
        end_depth_m: f32,
        total_time: Time,
    ) -> Vec<(Depth, Time, BreathingSource)> {
        let mut out: Vec<(Depth, Time, BreathingSource)> = Vec::new();

        if total_time.as_seconds() <= 0.0 {
            let src = self.tick(end_depth_m);
            out.push((Depth::from_meters(end_depth_m), Time::zero(), src));
            return out;
        }

        // Sync state at start
        let start_src = self.tick(start_depth_m);

        // Manual override -> no auto switching
        if matches!(self.state, ControllerState::ManualOverride(_)) {
            out.push((Depth::from_meters(end_depth_m), total_time, start_src));
            let _ = self.tick(end_depth_m);
            return out;
        }

        let descending = end_depth_m > start_depth_m;
        let mut split_depth_opt: Option<f32> = None;

        if descending {
            if matches!(self.state, ControllerState::Low) {
                if let Some(d) = self.config.switch_depth_descent {
                    if d > start_depth_m && d < end_depth_m {
                        split_depth_opt = Some(d);
                    }
                }
            }
        } else if end_depth_m < start_depth_m {
            if matches!(self.state, ControllerState::High) {
                if let Some(d) = self.config.switch_depth_ascent {
                    if d < start_depth_m && d > end_depth_m {
                        split_depth_opt = Some(d);
                    }
                }
            }
        }

        if let Some(split_m) = split_depth_opt {
            let total_dist = (end_depth_m - start_depth_m).abs();
            let d1 = (split_m - start_depth_m).abs();
            let t1_sec = total_time.as_seconds() * (d1 / total_dist);
            let t2_sec = total_time.as_seconds() - t1_sec;

            let t1 = Time::from_seconds(t1_sec);
            let t2 = Time::from_seconds(t2_sec);

            // Segment 1: start -> split uses start_src
            out.push((Depth::from_meters(split_m), t1, start_src));

            // Segment 2: tick at split triggers state change, source updated
            let split_src = self.tick(split_m);
            out.push((Depth::from_meters(end_depth_m), t2, split_src));
            let _ = self.tick(end_depth_m);
        } else {
            out.push((Depth::from_meters(end_depth_m), total_time, start_src));
            let _ = self.tick(end_depth_m);
        }

        out
    }

    fn handle_auto_switch(&mut self, depth: f32) {
        match self.state {
            ControllerState::Low => {
                // Only switch if descent switching is enabled AND we are deep enough
                if let Some(trigger_depth) = self.config.switch_depth_descent {
                    if depth >= trigger_depth {
                        self.state = ControllerState::High;
                    }
                }
            }
            ControllerState::High => {
                // Only switch if ascent switching is explicitly enabled
                if let Some(trigger_depth) = self.config.switch_depth_ascent {
                    if depth <= trigger_depth {
                        self.state = ControllerState::Low;
                    }
                }
            }
            ControllerState::ManualOverride(_) => {
                // Manual overrides typically disable auto-switching until reset
            }
        }
    }

    /// Manual intervention: "Persisting diluent usage during setpoint switches"
    /// The user changes the setpoint, but the diluent remains the same.
    pub fn set_manual_setpoint(&mut self, sp: f32) {
        self.state = ControllerState::ManualOverride(sp);
    }

    /// Reset to auto mode
    pub fn reset_to_auto(&mut self) {
        // Logic to determine if we should be High or Low based on current state?
        // Simplest is to default to Low or check logic. For now, we'll reset to Low.
        // A smarter implementation might check current depth vs thresholds.
        self.state = ControllerState::Low;
    }

    /// Manual intervention: Changing the diluent gas.
    pub fn switch_diluent(&mut self, new_diluent: GasMix) {
        self.active_diluent = new_diluent;
    }
}

/// High-level structure to manage resources (Diluent, Bailout).
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum DiveMode {
    /// Normal operation: Breathing from the loop.
    ClosedCircuit,
    /// Emergency: Breathing Open Circuit gas.
    BailoutOC {
        /// Index into `bailout_gases` vector.
        gas_index: usize,
    },
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct DiveComputer {
    /// The finite resource of diluent gas (e.g., 3L tank). (Not actively tracked quantity here yet)
    pub diluent_supply: GasMix,

    /// The finite resource of bailout gases (e.g., AL80s).
    pub bailout_gases: Vec<GasMix>,

    /// The Logic Controller for the loop.
    pub ccr_controller: SetpointController,

    /// Current operational mode.
    pub mode: DiveMode,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct DivePlan {
    pub ccr_runtime: DecoRuntime,
    pub bailout_runtime: DecoRuntime,
}

impl DiveComputer {
    pub fn new(
        diluent: GasMix,
        bailout_gases: Vec<GasMix>,
        config: SetpointConfig,
        mode: DiveMode,
    ) -> Self {
        Self {
            diluent_supply: diluent,
            bailout_gases,
            ccr_controller: SetpointController::new(config, diluent),
            mode,
        }
    }

    pub fn step(&mut self, depth: f32) -> BreathingSource {
        // 1. Tick the CCR controller regardless of mode (it tracks depth)
        // This ensures that if we return to the loop, it's in the correct state (High/Low)
        // for the current depth.
        let ccr_source = self.ccr_controller.tick(depth);

        match self.mode {
            DiveMode::ClosedCircuit => ccr_source,
            DiveMode::BailoutOC { gas_index } => {
                // Fetch the specific bailout gas
                // If index is invalid, fallback to diluent OC? or panic?
                // Safe handling: clamp or standard gas
                if let Some(gas) = self.bailout_gases.get(gas_index) {
                    BreathingSource::OpenCircuit(*gas)
                } else {
                    // Fallback to diluent if bailout index invalid
                    BreathingSource::OpenCircuit(self.diluent_supply)
                }
            }
        }
    }

    pub fn switch_to_bailout(&mut self, gas_index: usize) {
        self.mode = DiveMode::BailoutOC { gas_index };
    }

    pub fn switch_to_ccr(&mut self) {
        self.mode = DiveMode::ClosedCircuit;
    }

    /// Calculates TTS and Decompression stops for both CCR (Primary) and OC (Bailout) scenarios.
    pub fn plan_dive(
        &self,
        current_model: &BuhlmannModel,
    ) -> Result<DivePlan, crate::DecoCalculationError> {
        // 1. Primary CCR Plan
        let mut ccr_deco = Deco::default();
        let current_source = current_model.dive_state().gas;
        // Include current source to satisfy Deco's requirement of current gas being in list
        let ccr_plan = ccr_deco.calc(current_model.clone(), vec![current_source])?;

        // 2. Bailout OC Plan
        let mut bailout_deco = Deco::default();
        let mut bailout_model = current_model.clone();
        let current_depth = bailout_model.dive_state().depth;

        // Simple Bailout Logic: Switch to the best available OC gas immediately
        // Filter available bailout gases to find the one with highest safe PO2
        let best_bailout = self
            .bailout_gases
            .iter()
            .filter(|g| {
                let p_amb = crate::common::physics::depth_to_pressure(
                    current_depth,
                    current_model.config().surface_pressure,
                    current_model.config().water_density,
                );
                // Hardcoded 1.6 limit for emergency bailout switch.
                // Using 1.61 to handle minor floats/density variations at exactly 6m on salt water.
                g.partial_pressures(p_amb).o2 <= 1.61
            })
            // Find the richest (highest O2) gas available at this depth
            .max_by(|a, b| a.fraction_o2.partial_cmp(&b.fraction_o2).unwrap())
            .unwrap_or(&self.diluent_supply);

        // Record the switch event in the simulation
        bailout_model.record(
            current_depth,
            Time::zero(),
            &BreathingSource::OpenCircuit(*best_bailout),
        );

        // Calculate the rest of the ascent using all available bailout gases
        let mut bailout_sources: Vec<BreathingSource> = self
            .bailout_gases
            .iter()
            .map(|g| BreathingSource::OpenCircuit(*g))
            .collect();

        // Ensure current bailout gas is in the list
        let current_bailout_gas = BreathingSource::OpenCircuit(*best_bailout);
        if !bailout_sources.contains(&current_bailout_gas) {
            bailout_sources.push(current_bailout_gas);
        }

        let bailout_plan = bailout_deco.calc(bailout_model, bailout_sources)?;

        Ok(DivePlan {
            ccr_runtime: ccr_plan,
            bailout_runtime: bailout_plan,
        })
    }
}
