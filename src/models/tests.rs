// SPDX-License-Identifier: GPL-3.0-only
// Copyright (c) 2026 CarlosEvangelista

use super::*;

#[test]
fn sensor_section_default_is_cpu() {
    assert_eq!(SensorSection::Cpu, SensorSection::default());
}

#[test]
fn battery_stats_default_uses_expected_sentinels() {
    let stats = BatteryStats::default();
    assert!(!stats.available);
    assert_eq!(-1, stats.current_capacity_mah);
    assert_eq!(-1, stats.max_capacity_mah);
    assert_eq!(-1, stats.design_capacity_mah);
    assert_eq!(-1.0, stats.charge_percent);
    assert_eq!(-1, stats.voltage_mv);
    assert_eq!(0, stats.amperage_ma);
    assert_eq!(-1, stats.cycle_count);
    assert!(!stats.is_charging);
    assert!(!stats.has_accumulated_energy);
    assert_eq!(-1, stats.accumulated_energy_mwh);
    assert!(stats.power_w.is_nan());
    assert_eq!(0.0, stats.session_discharged_mwh);
}

#[test]
fn ui_type_tile_new_starts_empty() {
    let tile = UiTypeTile::new("P-Core");
    assert_eq!("P-Core", tile.type_label);
    assert_eq!(0, tile.count);
    assert_eq!(0.0, tile.avg_temp_c);
    assert_eq!(0.0, tile.max_temp_c);
}
