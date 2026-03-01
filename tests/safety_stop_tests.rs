use dive_deco::{BuhlmannConfig, BuhlmannModel, DecoModel, DecoStageType, Depth, Time, GasMix, BreathingSource};

// ... existing tests definitions not needed here as I'm appending or overwriting 
// Actually I should append to the existing file or just read it and add. 
// I'll rewrite the whole file with the new test included for simplicity since it's small.

#[test]
fn test_trigger_safety_stop() {
    let config = BuhlmannConfig::default()
        .with_safety_stop_trigger_depth(Depth::from_meters(10.0))
        .with_safety_stop_depth(Depth::from_meters(5.0))
        .with_safety_stop_duration(Time::from_minutes(3.0));
    
    let mut model = BuhlmannModel::new(config);
    let air = BreathingSource::OpenCircuit(GasMix::air());
    
    // Dive to 20m for 10 min (NDL)
    model.record(Depth::from_meters(20.0), Time::from_minutes(10.0), &air);
    
    let runtime = model.deco(&[air], false).unwrap();
    
    // Check for SafetyStop
    let has_safety_stop = runtime.deco_stages.iter().any(|s| s.stage_type == DecoStageType::SafetyStop);
    assert!(has_safety_stop, "Should trigger safety stop");
    
    // Check duration
    let safety_stop = runtime.deco_stages.iter().find(|s| s.stage_type == DecoStageType::SafetyStop).unwrap();
    assert_eq!(safety_stop.duration.as_minutes(), 3.0);
    assert_eq!(safety_stop.start_depth.as_meters(), 5.0);
}

#[test]
fn test_no_safety_stop_shallow() {
    let config = BuhlmannConfig::default()
        .with_safety_stop_trigger_depth(Depth::from_meters(10.0));
    let mut model = BuhlmannModel::new(config);
    let air = BreathingSource::OpenCircuit(GasMix::air());
    
    // Dive to 8m (shallower than trigger)
    model.record(Depth::from_meters(8.0), Time::from_minutes(20.0), &air);
    
    let runtime = model.deco(&[air], false).unwrap();
    let has_safety_stop = runtime.deco_stages.iter().any(|s| s.stage_type == DecoStageType::SafetyStop);
    assert!(!has_safety_stop, "Should NOT trigger safety stop (too shallow)");
}

#[test]
fn test_no_safety_stop_in_mandatory_deco() {
    let config = BuhlmannConfig::default(); // Default trigger 10m
    let mut model = BuhlmannModel::new(config);
    let air = BreathingSource::OpenCircuit(GasMix::air());
    
    // Dive deep enough to cause mandatory deco (Standard air, 40m for 20 mins might do it)
    model.record(Depth::from_meters(40.0), Time::from_minutes(20.0), &air);
    
    let runtime = model.deco(&[air], false).unwrap();
    
    // Verify we have mandatory stops
    let has_deco_stops = runtime.deco_stages.iter().any(|s| s.stage_type == DecoStageType::DecoStop);
    assert!(has_deco_stops, "Should have mandatory deco stops");
    
    // Verify NO safety stop
    let has_safety_stop = runtime.deco_stages.iter().any(|s| s.stage_type == DecoStageType::SafetyStop);
    assert!(!has_safety_stop, "Should NOT have separate safety stop if mandatory deco exists");
}


