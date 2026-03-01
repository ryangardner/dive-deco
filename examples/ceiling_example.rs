use dive_deco::{BreathingSource, BuhlmannConfig, BuhlmannModel, DecoModel, Depth, GasMix, Time};

fn main() {
    let mut model = BuhlmannModel::new(BuhlmannConfig::default());

    let nitrox_32 = BreathingSource::OpenCircuit(GasMix::try_new(0.32, 0.).unwrap());

    // ceiling after 20 min at 20 meters using EAN32 - ceiling at 0m
    model.record(Depth::from_meters(20.), Time::from_minutes(20.), &nitrox_32);
    println!("Ceiling: {}m", model.ceiling()); // Ceiling: 0m

    // ceiling after another 42 min at 30 meters using EAN32 - ceiling at 3m
    model.record(Depth::from_meters(30.), Time::from_minutes(42.), &nitrox_32);
    println!("Ceiling: {},", model.ceiling()); // Ceiling: 3.004(..)m
}
