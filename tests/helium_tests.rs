use dive_deco::{BreathingSource, DecoModel, Depth, GasMix, Supersaturation, Time};
pub mod fixtures;

#[test]
fn test_tmx_gf_surf() {
    let mut model = fixtures::model_gf((100, 100));

    let tmx = BreathingSource::OpenCircuit(GasMix::try_new(0.21, 0.35).unwrap());

    model.record(Depth::from_meters(30.), Time::from_minutes(300.), &tmx);

    let Supersaturation { gf_surf, .. } = model.supersaturation();

    assert_close_to_percent!(gf_surf, 335.77, 1.);
}

#[test]
fn test_tmx_ndl() {
    let mut model = fixtures::model_gf((30, 70));

    let tmx = BreathingSource::OpenCircuit(GasMix::try_new(0.21, 0.35).unwrap());

    model.record(Depth::from_meters(20.), Time::zero(), &tmx);

    assert_eq!(model.ndl(), Time::from_minutes(16.));
}

// heliox
#[test]
fn test_heliox_gf_surf() {
    let mut model = fixtures::model_gf((100, 100));
    let tmx = BreathingSource::OpenCircuit(GasMix::try_new(0.21, 0.79).unwrap());
    model.record(Depth::from_meters(30.), Time::from_minutes(40.), &tmx);

    let Supersaturation { gf_surf, .. } = model.supersaturation();

    assert_close_to_percent!(gf_surf, 197.93, 1.);
}

#[test]
fn test_heliox_he_desaturation_time_nonzero() {
    // Pure Heliox 21/79 has zero N2, so all N2 compartments actually off-gas
    // below surface equilibrium. Any non-zero desaturation_time() MUST come from He
    // decaying toward the 0.05 bar (50 mbar) residual He threshold.
    let mut model = fixtures::model_gf((30, 70));
    let heliox = BreathingSource::OpenCircuit(GasMix::try_new(0.21, 0.79).unwrap());
    model.record(Depth::from_meters(30.0_f32), Time::from_minutes(40.0_f32), &heliox);

    let desat_min = model.desaturation_time().as_minutes();
    assert!(
        desat_min > 300.0,
        "Heliox 21/79 dive (30m 40min) must produce substantial He desaturation time, got {:.2} min",
        desat_min
    );

    // Cross-check against analytical Haldane He decay to 0.05 bar across all 16 compartments
    let mut expected_max = 0.0_f32;
    for comp in model.tissues() {
        if comp.he_ip > 0.05 {
            let t = -(0.05_f32 / comp.he_ip).ln() / comp.he_k;
            if t > expected_max {
                expected_max = t;
            }
        }
    }
    assert!(
        (desat_min - expected_max).abs() < 0.5,
        "desaturation_time ({:.2} min) should match analytical He decay ({:.2} min)",
        desat_min,
        expected_max
    );
}

#[test]
fn test_trimix_he_desaturation_time_nonzero() {
    // Trimix 18/45 at 45m for 25 min loads both He and N2; verify He contributes
    // and desaturation_time() is strictly positive and decays after surface off-gassing.
    let mut model = fixtures::model_gf((30, 70));
    let tmx = BreathingSource::OpenCircuit(GasMix::try_new(0.18, 0.45).unwrap());
    model.record(Depth::from_meters(45.0_f32), Time::from_minutes(25.0_f32), &tmx);

    let initial_desat = model.desaturation_time().as_minutes();
    assert!(
        initial_desat > 200.0,
        "Trimix 18/45 45m/25min should produce >200 min desat, got {:.2}",
        initial_desat
    );

    let air = BreathingSource::OpenCircuit(GasMix::air());
    model.record(Depth::zero(), Time::from_minutes(60.0_f32), &air);
    let later_desat = model.desaturation_time().as_minutes();
    assert!(
        later_desat < initial_desat,
        "Desat should decrease after 60m on surface air: initial={:.2}, later={:.2}",
        initial_desat,
        later_desat
    );
}

#[test]
fn test_compartment_min_tolerable_amb_pressure_and_m_value_public() {
    let mut model = fixtures::model_gf((30, 70));
    let tmx = BreathingSource::OpenCircuit(GasMix::try_new(0.21, 0.35).unwrap());
    model.record(Depth::from_meters(30.0_f32), Time::from_minutes(20.0_f32), &tmx);

    let comp = model.tissues()[4]; // medium compartment
    let p_tol_70 = comp.min_tolerable_amb_pressure(70);
    let p_tol_100 = comp.min_tolerable_amb_pressure(100);
    assert!(
        p_tol_70 > p_tol_100,
        "Lower GF_high (70) must demand higher tolerable ambient pressure ({:.4}) than GF 100 ({:.4})",
        p_tol_70,
        p_tol_100
    );
    let m_surf_100 = comp.m_value(1.013, 100);
    let m_surf_70 = comp.m_value(1.013, 70);
    assert!(m_surf_100 > m_surf_70 && m_surf_70 > 1.013);
}
