use dive_deco::{
    BuehlmannConfig, BuehlmannModel, BreathingSource, CeilingType, DecoModel,
    Depth, GasMix, Time, TissueSnapshot,
};

/// Helper: default config matching typical dive computer setup
fn default_config() -> BuehlmannConfig {
    BuehlmannConfig::default()
        .with_ceiling_type(CeilingType::Adaptive)
        .with_gradient_factors(30, 70)
}

#[test]
fn snapshot_round_trip_fresh_model() {
    let config = default_config();
    let model = BuehlmannModel::new(config);

    let snap = model.tissue_snapshot();
    let restored = BuehlmannModel::from_tissue_snapshot(&snap, config);

    // Tissue pressures should be identical
    let orig_tissues = model.tissues();
    let rest_tissues = restored.tissues();
    for i in 0..16 {
        assert_eq!(orig_tissues[i].n2_ip, rest_tissues[i].n2_ip,
            "N2 mismatch in compartment {}", i);
        assert_eq!(orig_tissues[i].he_ip, rest_tissues[i].he_ip,
            "He mismatch in compartment {}", i);
    }

    // CNS/OTU should be zero for fresh model
    assert_eq!(snap.cns_fraction, 0.0);
    assert_eq!(snap.otu, 0.0);
}

#[test]
fn snapshot_round_trip_loaded_tissues() {
    let config = default_config();
    let mut model = BuehlmannModel::new(config);
    let air = BreathingSource::OpenCircuit(GasMix::air());

    // Dive to 30m for 20 minutes
    model.record(Depth::from_meters(30.0), Time::from_minutes(20.0), &air);
    // Ascend to surface
    model.record(Depth::zero(), Time::from_minutes(1.0), &air);

    let snap = model.tissue_snapshot();
    let restored = BuehlmannModel::from_tissue_snapshot(&snap, config);

    // Tissue pressures must match exactly
    let orig = model.tissues();
    let rest = restored.tissues();
    for i in 0..16 {
        assert_eq!(orig[i].n2_ip, rest[i].n2_ip,
            "N2 mismatch in compartment {}", i);
        assert_eq!(orig[i].he_ip, rest[i].he_ip,
            "He mismatch in compartment {}", i);
    }

    // Derived values should be recomputed correctly
    for i in 0..16 {
        assert!(
            (orig[i].min_tolerable_amb_pressure - rest[i].min_tolerable_amb_pressure).abs() < 0.001,
            "min_tolerable_amb_pressure mismatch in compartment {}: {} vs {}",
            i, orig[i].min_tolerable_amb_pressure, rest[i].min_tolerable_amb_pressure
        );
    }
}

#[test]
fn snapshot_round_trip_with_cns_otu() {
    let config = BuehlmannConfig::default();
    let mut model = BuehlmannModel::new(config);
    let ean50 = BreathingSource::OpenCircuit(GasMix::try_new(0.50, 0.0).unwrap());

    // Dive to 22m on EAN50 — high PO2 to accumulate CNS/OTU
    model.record(Depth::from_meters(22.0), Time::from_minutes(30.0), &ean50);

    let snap = model.tissue_snapshot();
    assert!(snap.cns_fraction > 0.0, "CNS should have accumulated");
    assert!(snap.otu > 0.0, "OTU should have accumulated");

    let restored = BuehlmannModel::from_tissue_snapshot(&snap, config);
    let restored_state = restored.dive_state();

    assert_eq!(restored_state.ox_tox.cns(), snap.cns_fraction,
        "CNS not restored correctly");
    assert_eq!(restored_state.ox_tox.otu(), snap.otu,
        "OTU not restored correctly");
}

#[test]
fn snapshot_restored_model_can_continue_diving() {
    let config = default_config();
    let mut model = BuehlmannModel::new(config);
    let air = BreathingSource::OpenCircuit(GasMix::air());

    // Load tissues
    model.record(Depth::from_meters(30.0), Time::from_minutes(20.0), &air);

    let snap = model.tissue_snapshot();
    let mut restored = BuehlmannModel::from_tissue_snapshot(&snap, config);

    // Continue diving on the restored model — should not panic
    restored.record(Depth::from_meters(20.0), Time::from_minutes(10.0), &air);
    restored.record(Depth::zero(), Time::from_minutes(1.0), &air);

    // NDL should be calculable
    let ndl = restored.ndl();
    assert!(ndl.as_minutes() >= 0.0, "NDL should be non-negative");
}

#[test]
fn snapshot_with_different_config_rebuilds_correctly() {
    // Save snapshot with one config, restore with different GFs
    let config1 = BuehlmannConfig::default().with_gradient_factors(30, 70);
    let config2 = BuehlmannConfig::default().with_gradient_factors(50, 90);

    let mut model = BuehlmannModel::new(config1);
    let air = BreathingSource::OpenCircuit(GasMix::air());
    model.record(Depth::from_meters(30.0), Time::from_minutes(20.0), &air);

    let snap = model.tissue_snapshot();

    // Restore with different GFs — tissue pressures same, ceilings different
    let restored1 = BuehlmannModel::from_tissue_snapshot(&snap, config1);
    let restored2 = BuehlmannModel::from_tissue_snapshot(&snap, config2);

    // Tissue pressures must be identical
    for i in 0..16 {
        assert_eq!(restored1.tissues()[i].n2_ip, restored2.tissues()[i].n2_ip);
    }

    // But M-values should differ due to different GFs
    // (more conservative GFs = lower M-values = higher ceiling)
    // Just verify the restored models have valid configs
    assert_eq!(restored1.config().gf, (30, 70));
    assert_eq!(restored2.config().gf, (50, 90));
}

#[test]
fn snapshot_size_is_136_bytes() {
    // Verify the struct is the expected fixed size
    assert_eq!(
        core::mem::size_of::<TissueSnapshot>(),
        136,
        "TissueSnapshot should be exactly 136 bytes"
    );
}

#[test]
fn snapshot_is_copy() {
    // Verify TissueSnapshot implements Copy (compile-time check)
    let snap = TissueSnapshot {
        n2_pressures: [0.79; 16],
        he_pressures: [0.0; 16],
        cns_fraction: 0.0,
        otu: 0.0,
    };
    let snap2 = snap; // Copy
    let _snap3 = snap; // Still usable after copy
    assert_eq!(snap2, snap);
}

#[test]
fn snapshot_mid_dive_preserves_tissues_but_not_derived() {
    // Snapshot taken at depth — from_tissue_snapshot recomputes derived values
    // at the surface, so M-values will differ. Tissue pressures must still match.
    // This documents the contract: consumers must call record() after restore
    // to re-derive values at the actual ambient pressure.
    let config = default_config();
    let mut model = BuehlmannModel::new(config);
    let air = BreathingSource::OpenCircuit(GasMix::air());

    // Dive to 30m for 20 minutes — stay at depth, do NOT ascend
    model.record(Depth::from_meters(30.0), Time::from_minutes(20.0), &air);

    // Snapshot while still at 30m
    let snap = model.tissue_snapshot();
    let restored = BuehlmannModel::from_tissue_snapshot(&snap, config);

    let orig = model.tissues();
    let rest = restored.tissues();

    // Tissue pressures must be identical (snapshot preserves these exactly)
    for i in 0..16 {
        assert_eq!(orig[i].n2_ip, rest[i].n2_ip,
            "N2 must match in compartment {}", i);
        assert_eq!(orig[i].he_ip, rest[i].he_ip,
            "He must match in compartment {}", i);
    }

    // All derived values (m_value_raw, m_value_calc, min_tolerable_amb_pressure)
    // are p_amb-dependent — they WILL differ because the original was computed
    // at 30m ambient pressure while the restored model recomputes at surface.
    // m_value_raw = a + p_amb/b (directly p_amb dependent)
    // min_tolerable_amb_pressure depends on GF-adjusted params, which use the
    // GF slope — itself a function of depth when GFs != 100.
    let mut any_m_value_differs = false;
    for i in 0..16 {
        if (orig[i].m_value_raw - rest[i].m_value_raw).abs() > 0.01 {
            any_m_value_differs = true;
            break;
        }
    }
    assert!(any_m_value_differs,
        "m_value_raw should differ between depth-computed and surface-computed");

    // After one zero-time record() call at the original depth,
    // ALL derived values should re-converge
    let mut restored_at_depth = BuehlmannModel::from_tissue_snapshot(&snap, config);
    restored_at_depth.record(Depth::from_meters(30.0), Time::zero(), &air);

    for i in 0..16 {
        assert!(
            (orig[i].m_value_raw - restored_at_depth.tissues()[i].m_value_raw).abs() < 0.01,
            "m_value_raw should converge after record() at depth, compartment {}", i
        );
        assert!(
            (orig[i].min_tolerable_amb_pressure - restored_at_depth.tissues()[i].min_tolerable_amb_pressure).abs() < 0.01,
            "min_tolerable_amb_pressure should converge after record() at depth, compartment {}", i
        );
    }
}

#[test]
fn snapshot_cross_config_produces_different_ceilings() {
    // Same tissue loading, different GFs → different ceilings
    let config_conservative = BuehlmannConfig::default()
        .with_gradient_factors(30, 70)
        .with_ceiling_type(CeilingType::Actual);
    let config_liberal = BuehlmannConfig::default()
        .with_gradient_factors(80, 95)
        .with_ceiling_type(CeilingType::Actual);

    let mut model = BuehlmannModel::new(config_conservative);
    let air = BreathingSource::OpenCircuit(GasMix::air());

    // Deep dive to load tissues enough to create a deco obligation
    model.record(Depth::from_meters(40.0), Time::from_minutes(25.0), &air);
    model.record(Depth::zero(), Time::from_minutes(0.1), &air);

    let snap = model.tissue_snapshot();

    let restored_conservative = BuehlmannModel::from_tissue_snapshot(&snap, config_conservative);
    let restored_liberal = BuehlmannModel::from_tissue_snapshot(&snap, config_liberal);

    // Tissue pressures must be identical
    for i in 0..16 {
        assert_eq!(
            restored_conservative.tissues()[i].n2_ip,
            restored_liberal.tissues()[i].n2_ip,
            "Tissue pressures should be identical regardless of config"
        );
    }

    let ceiling_conservative = restored_conservative.ceiling();
    let ceiling_liberal = restored_liberal.ceiling();

    // Conservative GFs (30/70) should produce a higher (deeper) ceiling
    // than liberal GFs (80/95) for the same tissue loading
    assert!(
        ceiling_conservative > ceiling_liberal,
        "Conservative GFs should yield deeper ceiling: conservative={:.1}m vs liberal={:.1}m",
        ceiling_conservative.as_meters(), ceiling_liberal.as_meters()
    );
}
