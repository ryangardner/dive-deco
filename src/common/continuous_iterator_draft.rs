use crate::common::{BreathingSource, DecoModel, Depth, Time};
use crate::Deco;

/// Iterator that simulates the ascent curve by stepping the physics forward
/// internally at high resolution, but yielding points at user-defined intervals.
pub struct ContinuousCeilingIterator<'a, T: DecoModel + Clone> {
    deco: &'a Deco,
    model: T,
    gases: &'a [BreathingSource],
    current_time: Time,
}

impl<'a, T: DecoModel + Clone> ContinuousCeilingIterator<'a, T> {
    pub fn new(deco: &'a Deco, model: T, gases: &'a [BreathingSource]) -> Self {
        Self {
            deco,
            model,
            gases,
            current_time: Time::zero(),
        }
    }

    /// Advances the simulation by `step_size` and returns the new (Time, Depth).
    /// Internally performs 10s physics steps to ensure accuracy.
    pub fn next_step(&mut self, step_size: Time) -> Option<(Time, Depth)> {
        // If we've already surfaced, stop iterating
        if self.model.dive_state().depth <= Depth::zero() {
            return None;
        }

        let target_time = self.current_time + step_size;
        
        // Physics step size: 10 seconds for reasonable resolution vs performance
        // If the user asks for 5 minutes, we do ~30 physics updates.
        let physics_step = Time::from_seconds(10.0);

        while self.current_time < target_time {
            let state = self.model.dive_state();
            if state.depth <= Depth::zero() {
                // Reached surface
                break;
            }

            let mut ceiling = self.model.ceiling();
            // Just like in calc(), enforce MinOD if necessary
            let min_pp_o2 = self.model.config().min_pp_o2();
            let min_od = state.gas.min_operating_depth(min_pp_o2);
            if ceiling < min_od {
                ceiling = min_od;
            }
            if ceiling < Depth::zero() {
                ceiling = Depth::zero();
            }

            // Move depth to ceiling (Instant "Surfing" assumption for graph)
            // In reality, you'd ascend at a rate, but for a "Ceiling Graph" 
            // we want to plot the ceiling itself.
            // So we record a "Stop" at the ceiling for the physics_step.
            
            // Check for gas switch (Optimization)
            let surface_pressure = self.model.config().surface_pressure();
            let water_density = self.model.config().water_density();
            
            // Current gas check - if we can switch, do it.
            let best_gas = self.deco.next_switch_gas(
                state.depth, 
                &state.gas, 
                self.gases, 
                surface_pressure, 
                water_density
            );

            let gas_to_use = if let Some(switch_gas) = best_gas {
                // Check MOD of switch gas
                let mod_limit = switch_gas.max_operating_depth_at(1.6, surface_pressure, water_density);
                 // Only switch if we are safely above the MOD of the new gas
                if state.depth <= mod_limit {
                    switch_gas
                } else {
                    state.gas
                }
            } else {
                state.gas
            };

            // Advance physics
            // We use 'record' which simulates tissue loading at depth for time
            // To simulate "surfing", we effectively "stay" at the current ceiling
            // for the duration of the step.
            
            // NOTE: This assumes the diver is perfectly following the ceiling.
            self.model.record(ceiling, physics_step, &gas_to_use);
            
            self.current_time += physics_step;
        }

        Some((self.current_time, self.model.dive_state().depth))
    }
}
