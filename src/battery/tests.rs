// SPDX-License-Identifier: GPL-3.0-only
// Copyright (c) 2026 CarlosEvangelista

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use super::*;

fn assert_close(expected: f64, actual: f64) {
    let delta = (expected - actual).abs();
    assert!(
        delta <= 0.0001,
        "expected {expected}, got {actual} (delta: {delta})"
    );
}

struct MockBatterySource<F>
where
    F: Fn(&mut BatteryStats),
{
    reader: F,
}

impl<F> BatteryDataSource for MockBatterySource<F>
where
    F: Fn(&mut BatteryStats),
{
    fn read_into(&self, stats: &mut BatteryStats) {
        (self.reader)(stats);
    }
}

#[test]
fn apply_derived_fields_computes_charge_power_and_availability() {
    let mut stats = BatteryStats {
        current_capacity_mah: 63,
        max_capacity_mah: 100,
        voltage_mv: 11_800,
        amperage_ma: -500,
        ..Default::default()
    };

    battery_apply_derived_fields(&mut stats);

    assert_close(63.0, stats.charge_percent);
    assert_close(-5.9, stats.power_w);
    assert!(stats.available);
}

#[test]
fn session_discharge_accumulates_only_while_discharging() {
    let start = Instant::now();
    let mut state = SessionState::default();
    let mut stats = BatteryStats {
        voltage_mv: 12_000,
        amperage_ma: -1_000,
        ..Default::default()
    };

    battery_update_session_discharge_with_state(&mut state, &mut stats, start);
    assert_close(0.0, stats.session_discharged_mwh);

    battery_update_session_discharge_with_state(
        &mut state,
        &mut stats,
        start + Duration::from_secs(1_800),
    );
    assert_close(6_000.0, stats.session_discharged_mwh);

    stats.amperage_ma = 500;
    battery_update_session_discharge_with_state(
        &mut state,
        &mut stats,
        start + Duration::from_secs(3_600),
    );
    assert_close(6_000.0, stats.session_discharged_mwh);
}

#[test]
fn read_stats_with_sources_resets_and_applies_sources_in_order() {
    let order = Rc::new(RefCell::new(Vec::new()));

    let order_registry = Rc::clone(&order);
    let source_registry = MockBatterySource {
        reader: move |stats: &mut BatteryStats| {
            order_registry.borrow_mut().push("registry");
            stats.current_capacity_mah = 50;
            stats.max_capacity_mah = 100;
            stats.voltage_mv = 11_800;
        },
    };

    let order_iops = Rc::clone(&order);
    let source_iops = MockBatterySource {
        reader: move |stats: &mut BatteryStats| {
            order_iops.borrow_mut().push("iops");
            stats.amperage_ma = -400;
            stats.cycle_count = 6;
        },
    };

    let sources: [&dyn BatteryDataSource; 2] = [&source_registry, &source_iops];

    let mut stats = BatteryStats {
        available: true,
        current_capacity_mah: 999,
        max_capacity_mah: 999,
        design_capacity_mah: 999,
        charge_percent: 999.0,
        voltage_mv: 999,
        amperage_ma: 999,
        cycle_count: 999,
        is_charging: true,
        has_accumulated_energy: true,
        accumulated_energy_mwh: 999,
        power_w: 999.0,
        session_discharged_mwh: 999.0,
    };

    let mut state = SessionState {
        discharged_mwh: 42.0,
        last_sample: None,
    };

    battery_read_stats_with_sources_and_state(&mut stats, &sources, Instant::now(), &mut state);

    assert_eq!(vec!["registry", "iops"], *order.borrow());
    assert_eq!(50, stats.current_capacity_mah);
    assert_eq!(100, stats.max_capacity_mah);
    assert_eq!(11_800, stats.voltage_mv);
    assert_eq!(-400, stats.amperage_ma);
    assert_eq!(6, stats.cycle_count);
    assert_eq!(-1, stats.design_capacity_mah);
    assert!(!stats.has_accumulated_energy);
    assert_close(50.0, stats.charge_percent);
    assert_close(-4.72, stats.power_w);
    assert!(stats.available);
    assert_close(42.0, stats.session_discharged_mwh);
}

#[test]
fn read_stats_with_sources_accumulates_session_between_calls() {
    let source = MockBatterySource {
        reader: |stats: &mut BatteryStats| {
            stats.current_capacity_mah = 50;
            stats.max_capacity_mah = 100;
            stats.voltage_mv = 12_000;
            stats.amperage_ma = -1_000;
        },
    };
    let sources: [&dyn BatteryDataSource; 1] = [&source];

    let mut state = SessionState::default();
    let start = Instant::now();
    let mut stats = BatteryStats {
        voltage_mv: 12_000,
        amperage_ma: -1_000,
        ..Default::default()
    };

    battery_read_stats_with_sources_and_state(&mut stats, &sources, start, &mut state);
    assert_close(0.0, stats.session_discharged_mwh);

    battery_read_stats_with_sources_and_state(
        &mut stats,
        &sources,
        start + Duration::from_secs(3_600),
        &mut state,
    );
    assert_close(12_000.0, stats.session_discharged_mwh);
}
