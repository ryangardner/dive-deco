use dive_deco::{
    BreathingSource, BuhlmannConfig, BuhlmannModel, Deco, DecoModel, Depth, GasMix, Time,
};

#[test]
fn test_hypoxic_ascent_limit() {
    // Scenario: Diver is on 10/50 Trimix (10% O2).
    // MinOD (approx): 10m (PO2 0.2) or 6m (PO2 0.16).
    // Surface (0m) has PO2 0.10 -> Hypoxia -> Death.
    // The algorithm SHOULD NOT plan an ascent to surface on this gas.

    let mut deco = Deco::default();
    let config = BuhlmannConfig::default();
    let mut model = BuhlmannModel::new(config);

    let hypoxic_gas = BreathingSource::OpenCircuit(GasMix::new(0.10, 0.50));

    // Dive to 50m
    model.record_travel_with_rate(Depth::from_meters(50.), 10., &hypoxic_gas);
    model.record(
        Depth::from_meters(50.),
        Time::from_minutes(20.),
        &hypoxic_gas,
    );

    // Calculate deco with ONLY this gas available
    let runtime = deco.calc(model, &[hypoxic_gas]);

    assert_eq!(
        runtime,
        Err(dive_deco::DecoCalculationError::NoBreathableGasToSurface)
    );
}
