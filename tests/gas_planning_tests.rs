use dive_deco::{BuhlmannConfig, BuhlmannModel, DecoModel, Depth, Time, GasMix, BreathingSource};

#[test]
fn test_gas_consumption_calc() {
    let config = BuhlmannConfig::default();
    let mut model = BuhlmannModel::new(config);
    let air = BreathingSource::OpenCircuit(GasMix::air());
    
    // Simple dive: 20m for 10 min
    model.record(Depth::from_meters(20.0), Time::from_minutes(10.0), &air);
    
    let runtime = model.deco(&[air], false).unwrap();
    let gas_needed = runtime.calculate_gas_needs(1013, 20.0);
    
    // Total approx: 179 L (as calculated previously)
    assert!(gas_needed > 150.0);
    assert!(gas_needed < 200.0);
}

#[test]
fn test_gas_consumption_with_mandatory_deco() {
    let config = BuhlmannConfig::default();
    let mut model = BuhlmannModel::new(config);
    let air = BreathingSource::OpenCircuit(GasMix::air());
    
    // Deep dive: 40m for 20 min
    model.record(Depth::from_meters(40.0), Time::from_minutes(20.0), &air);
    
    let runtime = model.deco(&[air], false).unwrap();
    let gas_needed = runtime.calculate_gas_needs(1013, 20.0);
    
    // Should be significantly more gas than a recreational dive
    assert!(gas_needed > 400.0); 
}

#[test]
fn test_rule_helpers() {
    use dive_deco::{rule_of_thirds_turn, rule_of_half_turn};
    
    assert_eq!(rule_of_thirds_turn(210.0), 140.0);
    assert_eq!(rule_of_half_turn(210.0), 105.0);
}

#[test]
fn test_inventory_and_requirements() {
    use dive_deco::{GasInventory, GasMix, BreathingSource};
    
    let air = BreathingSource::OpenCircuit(GasMix::air());
    let nitrox = BreathingSource::OpenCircuit(GasMix::nitrox(0.5).unwrap());
    
    let mut inventory = GasInventory::new();
    inventory.add_gas(air, 1000.0);
    inventory.add_gas(nitrox, 500.0);
    
    let reqs = vec![(air, 800.0), (nitrox, 400.0)];
    assert!(inventory.can_fulfill(&reqs).is_ok());
    
    let reqs_fail = vec![(air, 1200.0)];
    assert!(inventory.can_fulfill(&reqs_fail).is_err());
}

#[test]
fn test_gtr_calculation() {
    use dive_deco::{DecoGasPlanner, Depth};
    
    let planner = DecoGasPlanner::new(20.0, 1013);
    
    // At 30m (4 ATA), 20L/min RMV = 80L/min consumption.
    // If we have 400L available and need 200L to surface, 
    // we have 200L margin.
    // 200L / 80L/min = 2.5 min GTR.
    let gtr = planner.calculate_gtr(
        Depth::from_meters(30.0), 
        400.0, 
        200.0
    );
    
    assert!((gtr - 2.5).abs() < 0.1);
}

#[test]
fn test_full_deco_planning_flow() {
    use dive_deco::{
        BuhlmannConfig, BuhlmannModel, DecoModel, Depth, Time, GasMix, 
        BreathingSource, GasInventory, DecoGasPlanner
    };
    
    let config = BuhlmannConfig::default();
    let mut model = BuhlmannModel::new(config);
    let air = BreathingSource::OpenCircuit(GasMix::air());
    let ean50 = BreathingSource::OpenCircuit(GasMix::nitrox(0.5).unwrap());
    
    // 40m for 15 min on air.
    model.record(Depth::from_meters(40.0), Time::from_minutes(15.0), &air);
    let runtime = model.deco(&[air, ean50], false).unwrap();
    
    // Check per-gas requirements
    let reqs = runtime.calculate_gas_needs_per_gas(1013, 20.0);
    
    // Setup Inventory
    let mut inventory = GasInventory::new();
    inventory.add_gas(air, 2000.0);
    inventory.add_gas(ean50, 1000.0);
    
    assert!(inventory.can_fulfill(&reqs).is_ok());
    
    // GTR Math
    let needed_to_surface = runtime.calculate_gas_needs(1013, 20.0);
    let planner = DecoGasPlanner::new(20.0, 1013);
    let gtr = planner.calculate_gtr(Depth::from_meters(40.0), 2000.0, needed_to_surface);
    
    assert!(gtr > 0.0);
}

#[test]
fn test_ccr_bailout_sufficiency() {
    use dive_deco::{
        BuhlmannConfig, BuhlmannModel, DecoModel, Depth, Time, GasMix, 
        BreathingSource, GasInventory, BailoutMath
    };
    
    let config = BuhlmannConfig::default();
    let mut model = BuhlmannModel::new(config);
    
    // CCR dive: 60m for 15 min at setpoint 1.3
    let diluent = GasMix::trimix(0.18, 0.45).unwrap(); // Trimix 18/45
    let ccr = BreathingSource::ClosedCircuit { setpoint: 1.3, diluent };
    
    model.record(Depth::from_meters(60.0), Time::from_minutes(15.0), &ccr);
    
    // Bailout scenario: what if we surface on OC?
    let bo_bottom = BreathingSource::OpenCircuit(diluent);
    let bo_deco = BreathingSource::OpenCircuit(GasMix::nitrox(0.5).unwrap());
    
    let bo_runtime = model.deco(&[ccr, bo_bottom, bo_deco], false).unwrap();
    
    // Setup Inventory
    let mut inventory = GasInventory::new();
    inventory.add_gas(bo_bottom, 500.0); // 500L bottom gas (insufficient)
    inventory.add_gas(bo_deco, 1000.0);
    
    let reqs = bo_runtime.calculate_gas_needs_per_gas(1013, 20.0);
    
    let sufficient = BailoutMath::is_sufficient(&inventory, &reqs);
    let margin = BailoutMath::volume_margin(&inventory, &reqs);
    
    // Should be insufficient for a 60m dive with only 500L bottom gas
    assert!(!sufficient || margin < 0.0);
}

#[test]
fn test_mod_calculation_with_config() {
    use dive_deco::{BuhlmannConfig, GasMix, calculate_mod};
    
    let config = BuhlmannConfig::default()
        .with_max_pp_o2_normal(1.4)
        .with_max_pp_o2_deco(1.6);
        
    let air = GasMix::air();
    
    let mod_normal = calculate_mod(&air, &config, false);
    // Calculated: 55.97m
    assert!(mod_normal.as_meters() > 55.5 && mod_normal.as_meters() < 56.5);
    
    let mod_deco = calculate_mod(&air, &config, true);
    // Calculated: 65.48m
    assert!(mod_deco.as_meters() > 65.0 && mod_deco.as_meters() < 66.0);
}
