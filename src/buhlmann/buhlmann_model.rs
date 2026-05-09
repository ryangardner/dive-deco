use crate::buhlmann::buhlmann_config::BuhlmannConfig;
use crate::buhlmann::compartment::{Compartment, Supersaturation};
use crate::buhlmann::zhl_values::{ZHLParams, ZHL_16C_N2_16A_HE_VALUES};
use crate::common::BreathingSource;
use crate::common::GasMix;
use crate::common::{abs, ceil, floor, ln};
use crate::common::{
    AscentRatePerMinute, ConfigValidationErr, Deco, DecoModel, DecoModelConfig, Depth, DiveState,
    GradientFactor, Otu, OxTox, RecordData,
};
use crate::{CeilingType, DecoCalculationError, DecoRuntime, GradientFactors, Sim, Time};

use core::cmp::Ordering;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

const NDL_CUT_OFF_MINS: u8 = 99;

#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct BuhlmannModel {
    config: BuhlmannConfig,
    compartments: [Compartment; 16],
    state: BuhlmannState,
    sim: bool,
}
pub type BuehlmannModel = BuhlmannModel;

#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct BuhlmannState {
    depth: Depth,
    time: Time,
    gas: BreathingSource,
    gf_low_depth: Option<Depth>,
    ox_tox: OxTox,
}

impl Default for BuhlmannState {
    fn default() -> Self {
        Self {
            depth: Depth::zero(),
            time: Time::zero(),
            gas: BreathingSource::OpenCircuit(GasMix::air()),
            gf_low_depth: None,
            ox_tox: OxTox::default(),
        }
    }
}

impl DecoModel for BuhlmannModel {
    type ConfigType = BuhlmannConfig;

    // initialize with the default config
    fn default() -> Self {
        Self::new(BuhlmannConfig::default())
    }

    /// initialize new Buhlmann (ZH-L16C) model with gradient factors
    fn new(config: BuhlmannConfig) -> Self {
        // validate config
        if let Err(e) = config.validate() {
            panic!("Config error [{}]: {}", e.field, e.reason);
        }
        // air as a default init gas
        let initial_model_state = BuhlmannState::default();
        let compartments = Self::create_compartments(ZHL_16C_N2_16A_HE_VALUES, config);

        Self {
            config,
            compartments,
            state: initial_model_state,
            sim: false,
        }
    }

    fn config(&self) -> BuhlmannConfig {
        self.config
    }

    fn dive_state(&self) -> DiveState {
        let BuhlmannState {
            depth,
            time,
            gas,
            ox_tox,
            ..
        } = self.state;
        DiveState {
            depth,
            time,
            gas,
            ox_tox,
        }
    }

    /// record data: depth (meters), time (seconds), gas
    fn record(&mut self, depth: Depth, time: Time, gas: &BreathingSource) {
        self.validate_depth(depth);
        self.state.depth = depth;
        self.state.gas = *gas;
        self.state.time += time;
        let record = RecordData { depth, time, gas };
        self.recalculate(record);
    }

    /// model travel between depths in 1s intervals
    fn record_travel(&mut self, target_depth: Depth, time: Time, gas: &BreathingSource) {
        self.validate_depth(target_depth);
        let start_depth = self.state.depth;

        // 1) Zero-time travel: still update state + derived values at target depth.
        if time.as_seconds() <= 0.0 {
            self.state.depth = target_depth;
            self.state.gas = *gas;
            let record = RecordData {
                depth: target_depth,
                time: Time::zero(),
                gas,
            };
            self.recalculate_compartments(&record);
            return;
        }

        // 2) No movement -> treat as constant depth segment (preserves any "record" side-effects).
        // Using crate::common::math_utils::abs instead of directly calling abs() on f32 to match imports
        use crate::common::math_utils::abs;

        if abs((target_depth - start_depth).as_meters()) < 1e-9 {
            self.record(target_depth, time, gas);
            return;
        }

        // 3) If CCR setpoint clamp boundary is crossed, split into two Schreiner segments.
        // Clamp boundary: (P_amb - P_wv) == setpoint  => P_amb == setpoint + P_wv
        let split_depth_opt = gas
            .ccr_clamp_transition_pressure()
            .map(|p_split| {
                crate::common::physics::pressure_to_depth(
                    p_split,
                    self.config.surface_pressure,
                    self.config.water_density,
                )
            })
            .filter(|d_split| {
                // keep only if split depth lies strictly between start and end
                let between = (*d_split >= start_depth && *d_split <= target_depth)
                    || (*d_split >= target_depth && *d_split <= start_depth);
                between
                    && abs((*d_split - start_depth).as_meters()) > 1e-6
                    && abs((target_depth - *d_split).as_meters()) > 1e-6
            });

        if let Some(split_depth) = split_depth_opt {
            let total_dist = abs((target_depth - start_depth).as_meters());
            let dist_1 = abs((split_depth - start_depth).as_meters());
            let t1_sec = time.as_seconds() * (dist_1 / total_dist);
            let t2_sec = time.as_seconds() - t1_sec;

            let t1 = Time::from_seconds(t1_sec);
            let t2 = Time::from_seconds(t2_sec);

            self.record_travel_schreiner_segment(start_depth, split_depth, t1, gas);
            self.record_travel_schreiner_segment(split_depth, target_depth, t2, gas);
        } else {
            self.record_travel_schreiner_segment(start_depth, target_depth, time, gas);
        }
    }

    fn record_travel_with_rate(
        &mut self,
        target_depth: Depth,
        rate: AscentRatePerMinute,
        gas: &BreathingSource,
    ) {
        self.validate_depth(target_depth);

        let travel_distance = abs((target_depth - self.state.depth).as_meters());

        self.record_travel(
            target_depth,
            Time::from_seconds(travel_distance / rate * 60.),
            gas,
        );
    }

    fn ndl(&self) -> Time {
        // Early return if already in deco
        if self.in_deco() {
            return Time::zero();
        }
        // Binary search for NDL between 0 and NDL_CUT_OFF_MINS using minute intervals
        let mut low: u8 = 0;
        let mut high: u8 = NDL_CUT_OFF_MINS;
        // Binary search until we narrow down to adjacent minutes
        while high - low > 1 {
            let mid = (low + high) / 2;
            // Check if staying for 'mid' minutes keeps us within NDL
            if self.check_ndl_for(Time::from_minutes(mid)) {
                // Still within NDL at mid-point, so NDL is at least this high
                low = mid;
            } else {
                // In deco at mid-point, so NDL is lower
                high = mid;
            }
        }
        // Check if we can stay for the full cut-off time
        if self.check_ndl_for(Time::from_minutes(high)) {
            Time::from_minutes(high)
        }
        // At this point, low is safe and high is in deco (or high == low + 1)
        // Verify that 'low' minutes keeps us within NDL
        else if self.check_ndl_for(Time::from_minutes(low as f32)) {
            Time::from_minutes(low as f32)
        } else {
            // Edge case: even 'low' puts us in deco
            Time::from_minutes(0.0)
        }
    }

    fn ceiling(&self) -> Depth {
        let BuhlmannConfig {
            deco_ascent_rate,
            mut ceiling_type,
            ..
        } = self.config();
        if self.sim {
            ceiling_type = CeilingType::Actual;
        }

        let leading_comp: &Compartment = self.leading_comp();
        let mut ceiling = match ceiling_type {
            CeilingType::Actual => leading_comp.ceiling(&self.config),
            CeilingType::Adaptive => {
                let mut sim_model = self.fork();
                let sim_gas = sim_model.dive_state().gas;
                let mut calculated_ceiling = sim_model.ceiling();
                loop {
                    let sim_depth = sim_model.dive_state().depth;
                    let sim_depth_cmp = sim_depth.partial_cmp(&Depth::zero());
                    let sim_depth_at_surface = match sim_depth_cmp {
                        Some(Ordering::Equal | Ordering::Less) => true,
                        Some(Ordering::Greater) => false,
                        None => panic!("Simulation depth incomparable to surface"),
                    };
                    if sim_depth_at_surface || sim_depth <= calculated_ceiling {
                        break;
                    }
                    sim_model.record_travel_with_rate(
                        calculated_ceiling,
                        deco_ascent_rate,
                        &sim_gas,
                    );
                    calculated_ceiling = sim_model.ceiling();
                }
                calculated_ceiling
            }
        };

        if self.config().round_ceiling() {
            ceiling = Depth::from_meters(ceil(ceiling.as_meters()));
        }

        ceiling
    }

    fn surface_gf(&self) -> f32 {
        let surface_p = self.config.surface_pressure();
        let current_p = crate::common::physics::depth_to_pressure(
            self.state.depth,
            surface_p,
            self.config.water_density,
        );
        
        let mut max_gf: f32 = 0.0;
        let surface_p_bar = surface_p as f32 / 1000.0;
        for c in &self.compartments {
            let ss = c.supersaturation(current_p, surface_p_bar);
            if ss.gf_surf > max_gf {
                max_gf = ss.gf_surf;
            }
        }
        max_gf
    }

    fn deco(&self, gas_mixes: &[BreathingSource], _include_safety_stop: bool) -> Result<DecoRuntime, DecoCalculationError> {
        let mut deco = Deco::default();
        deco.calc(self.fork(), gas_mixes)
    }
}

impl Sim for BuhlmannModel {
    fn fork(&self) -> Self {
        Self {
            sim: true,
            ..self.clone()
        }
    }
    fn is_sim(&self) -> bool {
        self.sim
    }
}

impl BuhlmannModel {
    /// Calculate time to desaturate to 105% of surface pressure (No-Dive Time)
    pub fn desaturation_time(&self) -> Time {
        let surface_pressure = self.config.surface_pressure;
        let air = BreathingSource::OpenCircuit(GasMix::air());
        use crate::common::physics::depth_to_pressure;
        let surface_p_amb =
            depth_to_pressure(Depth::zero(), surface_pressure, self.config.water_density);
        let surface_pp = air.inspired_partial_pressures(surface_p_amb);

        // Target is 1.05 * surface partial pressure (standard safety margin)
        let target_n2 = surface_pp.n2 * 1.05;
        let target_he = surface_pp.he * 1.05; // Usually 0 for air but good for completeness

        let mut max_minutes = 0.0;

        for comp in &self.compartments {
            // Time for N2 to decay to target
            // t = -1/k * ln( (P_target - P_inspired) / (P_current - P_inspired) )
            // If P_current <= P_target, time is 0.

            // N2
            if comp.n2_ip > target_n2 {
                let numerator = target_n2 - surface_pp.n2;
                let denominator = comp.n2_ip - surface_pp.n2;
                if denominator != 0.0 && numerator > 0.0 {
                    let ratio = numerator / denominator;
                    if ratio > 0.0 {
                        let t = -(ln(ratio)) / comp.n2_k;
                        if t > max_minutes {
                            max_minutes = t;
                        }
                    }
                }
            }

            // He
            if comp.he_ip > target_he {
                let numerator = target_he - surface_pp.he;
                let denominator = comp.he_ip - surface_pp.he;
                if denominator != 0.0 && numerator > 0.0 {
                    let ratio = numerator / denominator;
                    if ratio > 0.0 {
                        let t = -(ln(ratio)) / comp.he_k;
                        if t > max_minutes {
                            max_minutes = t;
                        }
                    }
                }
            }
        }

        Time::from_minutes(max_minutes)
    }

    /// set of current gradient factors (GF now, GF surface)
    pub fn supersaturation(&self) -> Supersaturation {
        let mut acc_gf_99 = 0.;
        let mut acc_gf_surf = 0.;
        use crate::common::physics::depth_to_pressure;
        let p_amb = depth_to_pressure(
            self.state.depth,
            self.config.surface_pressure,
            self.config.water_density,
        );
        let p_surf = self.config.surface_pressure as f32 / 1000.0f32;

        for comp in self.compartments.iter() {
            let Supersaturation { gf_99, gf_surf } = comp.supersaturation(p_amb, p_surf);
            if gf_99 > acc_gf_99 {
                acc_gf_99 = gf_99;
            }
            if gf_surf > acc_gf_surf {
                acc_gf_surf = gf_surf;
            }
        }

        Supersaturation {
            gf_99: acc_gf_99,
            gf_surf: acc_gf_surf,
        }
    }

    pub fn tissues(&self) -> [Compartment; 16] {
        self.compartments
    }

    pub fn update_config(&mut self, new_config: BuhlmannConfig) -> Result<(), ConfigValidationErr> {
        new_config.validate()?;
        self.config = new_config;
        Ok(())
    }

    fn check_ndl_for(&self, time: Time) -> bool {
        let mut sim_model = self.fork();
        sim_model.record(self.state.depth, time, &self.state.gas);
        !sim_model.in_deco()
    }

    fn leading_comp(&self) -> &Compartment {
        let mut leading_comp: &Compartment = &self.compartments[0];
        for compartment in &self.compartments[1..] {
            if compartment.min_tolerable_amb_pressure > leading_comp.min_tolerable_amb_pressure {
                leading_comp = compartment;
            }
        }

        leading_comp
    }

    fn leading_comp_mut(&mut self) -> &mut Compartment {
        let comps = &mut self.compartments;
        let mut leading_comp_index = 0;
        for (i, compartment) in comps.iter().enumerate().skip(1) {
            if compartment.min_tolerable_amb_pressure
                > comps[leading_comp_index].min_tolerable_amb_pressure
            {
                leading_comp_index = i;
            }
        }

        &mut comps[leading_comp_index]
    }

    fn create_compartments(zhl_values: [ZHLParams; 16], config: BuhlmannConfig) -> [Compartment; 16] {
        core::array::from_fn(|i| Compartment::new((i + 1) as u8, zhl_values[i], config))
    }

    fn recalculate(&mut self, record: RecordData) {
        self.recalculate_compartments(&record);
        if !self.is_sim() {
            self.recalculate_ox_tox(&record);
        }
    }

    fn recalculate_compartments(&mut self, record: &RecordData) {
        use crate::common::physics::depth_to_pressure;
        let p_amb = depth_to_pressure(
            record.depth,
            self.config.surface_pressure,
            self.config.water_density,
        );
        let inspired_pp = record.gas.inspired_partial_pressures(p_amb);
        let (gf_low, gf_high) = self.config.gf;
        for compartment in self.compartments.iter_mut() {
            compartment.recalculate(p_amb, inspired_pp, record.time, gf_high);
        }

        // recalc
        if gf_high != gf_low {
            let max_gf = self.calc_max_sloped_gf(self.config.gf, record.depth);

            let should_recalc_all_tissues =
                !self.is_sim() && self.config.recalc_all_tissues_m_values;
            match should_recalc_all_tissues {
                true => self.recalculate_all_tissues_with_gf(record, max_gf),
                false => self.recalculate_leading_compartment_with_gf(record, max_gf),
            }
        }
    }

    fn recalculate_all_tissues_with_gf(&mut self, record: &RecordData, max_gf: GradientFactor) {
        let recalc_record = RecordData {
            depth: record.depth,
            time: Time::zero(),
            gas: record.gas,
        };
        use crate::common::physics::depth_to_pressure;
        let p_amb = depth_to_pressure(
            record.depth,
            self.config.surface_pressure,
            self.config.water_density,
        );
        let inspired_pp = record.gas.inspired_partial_pressures(p_amb);
        for compartment in self.compartments.iter_mut() {
            compartment.recalculate(p_amb, inspired_pp, recalc_record.time, max_gf);
        }
    }

    fn recalculate_leading_compartment_with_gf(
        &mut self,
        record: &RecordData,
        max_gf: GradientFactor,
    ) {
        use crate::common::physics::depth_to_pressure;
        let p_amb = depth_to_pressure(
            record.depth,
            self.config.surface_pressure,
            self.config.water_density,
        );
        let inspired_pp = record.gas.inspired_partial_pressures(p_amb);
        let leading = self.leading_comp_mut();

        // recalculate leading tissue with max gf
        let leading_tissue_recalc_record = RecordData {
            depth: record.depth,
            time: Time::zero(),
            gas: record.gas,
        };
        leading.recalculate(
            p_amb,
            inspired_pp,
            leading_tissue_recalc_record.time,
            max_gf,
        );
    }

    fn recalculate_ox_tox(&mut self, record: &RecordData) {
        self.state.ox_tox.recalculate(
            record,
            self.config().surface_pressure,
            self.config().water_density,
        );
    }

    /// Calculate the maximum gradient factor (GF) for a given depth and gradient factors.
    /// This is the maximum supersaturation on a slope between GF_low and GF_high for a given depth.
    /// Side effect: updates self.state.gf_low_depth
    fn calc_max_sloped_gf(&mut self, gf: GradientFactors, depth: Depth) -> GradientFactor {
        let (gf_low, gf_high) = gf;
        let in_deco = self.ceiling() > Depth::zero();
        if !in_deco {
            return gf_high;
        }

        let gf_low_depth = match self.state.gf_low_depth {
            Some(gf_low_depth) => gf_low_depth,
            None => {
                // Direct calculation for gf_low_depth
                let surface_pressure_bar = self.config.surface_pressure as f32 / 1000.0;
                let gf_low_fraction = gf.0 as f32 / 100.0; // gf.0 is gf_low

                let mut max_calculated_depth_m = 0.0f32;

                for comp in self.compartments.iter() {
                    let total_ip = comp.total_ip;
                    let (_, a_weighted, b_weighted) =
                        comp.weighted_zhl_params(comp.he_ip, comp.n2_ip);

                    // General case: P_amb = (P_ip - G*a) / (1 - G + G/b)
                    let max_amb_p = (total_ip - gf_low_fraction * a_weighted)
                        / (1.0 - gf_low_fraction + gf_low_fraction / b_weighted);

                    let max_depth = (10.0 * (max_amb_p - surface_pressure_bar)).max(0.0);
                    max_calculated_depth_m = max_calculated_depth_m.max(max_depth);
                }

                let calculated_gf_low_depth = Depth::from_meters(max_calculated_depth_m);
                self.state.gf_low_depth = Some(calculated_gf_low_depth);
                calculated_gf_low_depth
            }
        };

        if depth > gf_low_depth {
            return gf_low;
        }

        self.gf_slope_point(gf, gf_low_depth, depth)
    }

    fn gf_slope_point(
        &self,
        gf: GradientFactors,
        gf_low_depth: Depth,
        depth: Depth,
    ) -> GradientFactor {
        let (gf_low, gf_high) = gf;
        let slope_point: f32 = gf_high as f32
            - (((gf_high - gf_low) as f32) / gf_low_depth.as_meters()) * depth.as_meters();

        slope_point as u8
    }

    fn validate_depth(&self, depth: Depth) {
        if depth < Depth::zero() {
            panic!("Invalid depth [{depth}]");
        }
    }

    pub fn calculate_otu(&self) -> Otu {
        self.otu()
    }

    fn record_travel_schreiner_segment(
        &mut self,
        start_depth: Depth,
        end_depth: Depth,
        time: Time,
        gas: &BreathingSource,
    ) {
        use crate::common::physics::depth_to_pressure;

        let travel_time_mins = time.as_minutes();
        if travel_time_mins <= 0.0 {
            // Still update final depth derived values
            self.state.depth = end_depth;
            self.state.gas = *gas;
            let record = RecordData {
                depth: end_depth,
                time: Time::zero(),
                gas,
            };
            self.recalculate_compartments(&record);
            return;
        }

        // Schreiner endpoints
        let p_start = depth_to_pressure(
            start_depth,
            self.config.surface_pressure,
            self.config.water_density,
        );
        let p_end = depth_to_pressure(
            end_depth,
            self.config.surface_pressure,
            self.config.water_density,
        );

        let pp_start = gas.inspired_partial_pressures(p_start);
        let pp_end = gas.inspired_partial_pressures(p_end);

        for compartment in self.compartments.iter_mut() {
            compartment.recalculate_schreiner(
                pp_start.n2,
                pp_end.n2,
                pp_start.he,
                pp_end.he,
                travel_time_mins,
            );
        }

        // Integrate ox-tox for the segment (1s steps + fractional remainder)
        if !self.is_sim() {
            let total_sec = time.as_seconds();
            let whole_steps = floor(total_sec) as usize;
            let rem = total_sec - (whole_steps as f32);

            if whole_steps > 0 {
                let delta =
                    (end_depth.as_meters() - start_depth.as_meters()) / (whole_steps as f32);
                let mut d_m = start_depth.as_meters();
                for _ in 0..whole_steps {
                    d_m += delta;
                    let step = RecordData {
                        depth: Depth::from_meters(d_m),
                        time: Time::from_seconds(1.0),
                        gas,
                    };
                    self.recalculate_ox_tox(&step);
                }
            }

            if rem > 1e-9 {
                let step = RecordData {
                    depth: end_depth,
                    time: Time::from_seconds(rem),
                    gas,
                };
                self.recalculate_ox_tox(&step);
            }
        }

        // Update state + derived values at segment end
        self.state.time += time;
        self.state.depth = end_depth;
        self.state.gas = *gas;

        let final_record = RecordData {
            depth: end_depth,
            time: Time::zero(),
            gas,
        };
        self.recalculate_compartments(&final_record);
    }

    /// Extract minimal persistable tissue state.
    ///
    /// Returns a fixed-size, `Copy`, no-alloc snapshot containing only the
    /// mutable state that changes during a dive: 16 N2 pressures, 16 He
    /// pressures, CNS fraction, and OTU. All static ZHL-16C table constants,
    /// decay constants, and derived M-values are omitted because they can be
    /// reconstructed from the config + ZHL tables.
    ///
    /// **136 bytes** vs ~1KB for the full serialized model.
    pub fn tissue_snapshot(&self) -> TissueSnapshot {
        let mut n2_pressures = [0.0f32; 16];
        let mut he_pressures = [0.0f32; 16];
        for (i, comp) in self.compartments.iter().enumerate() {
            n2_pressures[i] = comp.n2_ip;
            he_pressures[i] = comp.he_ip;
        }
        TissueSnapshot {
            n2_pressures,
            he_pressures,
            cns_fraction: self.state.ox_tox.cns(),
            otu: self.state.ox_tox.otu(),
        }
    }

    /// Reconstruct a full model from persisted tissue pressures + config.
    ///
    /// Re-derives all static ZHL params, decay constants, and M-values from
    /// the ZHL-16C lookup table. The snapshot's tissue pressures are injected
    /// into the freshly-created compartments, then all derived values
    /// (`total_ip`, `min_tolerable_amb_pressure`, M-values) are recomputed.
    ///
    /// # Arguments
    /// - `snapshot` — minimal tissue state (136 bytes)
    /// - `config` — full deco config (GFs, surface pressure, ceiling type, etc.)
    pub fn from_tissue_snapshot(snapshot: &TissueSnapshot, config: BuhlmannConfig) -> Self {
        use crate::common::physics::depth_to_pressure;
        use crate::common::OxTox;

        // 1. Create a fresh model with default compartments from ZHL tables
        let mut model = Self::new(config);

        // 2. Overwrite tissue pressures from the snapshot
        for (i, comp) in model.compartments.iter_mut().enumerate() {
            comp.n2_ip = snapshot.n2_pressures[i];
            comp.he_ip = snapshot.he_pressures[i];
            comp.total_ip = comp.n2_ip + comp.he_ip;
        }

        // 3. Recompute all derived values (M-values, min_tolerable_amb_pressure)
        //    by running a zero-time recalculation at the surface
        let p_amb = depth_to_pressure(
            Depth::zero(),
            config.surface_pressure,
            config.water_density,
        );
        let air = BreathingSource::OpenCircuit(GasMix::air());
        let inspired_pp = air.inspired_partial_pressures(p_amb);
        let (_, gf_high) = config.gf;
        for comp in model.compartments.iter_mut() {
            // Zero-time recalculate: updates M-values and ceiling from current tissue state
            comp.recalculate(p_amb, inspired_pp, Time::zero(), gf_high);
        }

        // 4. Restore CNS/OTU
        model.state.ox_tox = OxTox::from_values(snapshot.cns_fraction, snapshot.otu);

        model
    }
}

/// Minimal tissue state for persistence. Fixed-size, no-alloc, no-std safe.
///
/// Contains only the mutable state that changes during a dive:
/// - 16 N2 tissue inert pressures
/// - 16 He tissue inert pressures
/// - CNS fraction (oxygen toxicity clock)
/// - OTU (oxygen toxicity units)
///
/// **136 bytes** vs ~1KB for the full `BuehlmannModel`.
///
/// All static ZHL-16C constants, decay constants (`n2_k`, `he_k`), and derived
/// values (`total_ip`, `min_tolerable_amb_pressure`, `m_value_raw`, `m_value_calc`)
/// are reconstructable from the ZHL tables + a `BuehlmannConfig`.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct TissueSnapshot {
    pub n2_pressures: [f32; 16],
    pub he_pressures: [f32; 16],
    pub cns_fraction: f32,
    pub otu: f32,
}
