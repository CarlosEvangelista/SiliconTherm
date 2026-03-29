// SPDX-License-Identifier: GPL-3.0-only
// Copyright (c) 2026 CarlosEvangelista

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SensorSection {
    Cpu = 0,
    Gpu = 1,
    Battery = 2,
}

pub const WATCHTEMP_SENSOR_MIN_VALID_C: f64 = 5.0;
pub const WATCHTEMP_SENSOR_MAX_VALID_C: f64 = 130.0;
pub const WATCHTEMP_SENSOR_MAX_RETRIES: i32 = 15;
pub const WATCHTEMP_SENSOR_RETRY_DELAY_US: u64 = 20_000;

impl Default for SensorSection {
    fn default() -> Self {
        Self::Cpu
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SensorEntry {
    pub key: String,
    pub name: String,
    pub type_label: String,
    pub section: SensorSection,
    pub active: bool,
    pub last_temp_c: f64,
    pub data_type: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SensorCollection {
    pub items: Vec<SensorEntry>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiSortMode {
    SectionKey = 0,
    TempDesc = 1,
    Name = 2,
    Key = 3,
    Type = 4,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiHeatLevel {
    Cool = 0,
    Warm = 1,
    Hot = 2,
}

#[derive(Clone, Debug, PartialEq)]
pub struct UiTypeTile {
    pub type_label: String,
    pub count: usize,
    pub avg_temp_c: f64,
    pub max_temp_c: f64,
}

impl UiTypeTile {
    pub fn new(type_label: &str) -> Self {
        Self {
            type_label: type_label.to_string(),
            count: 0,
            avg_temp_c: 0.0,
            max_temp_c: 0.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BatteryStats {
    pub available: bool,
    pub current_capacity_mah: i32,
    pub max_capacity_mah: i32,
    pub design_capacity_mah: i32,
    pub charge_percent: f64,
    pub voltage_mv: i32,
    pub amperage_ma: i32,
    pub cycle_count: i32,
    pub is_charging: bool,
    pub has_accumulated_energy: bool,
    pub accumulated_energy_mwh: i32,
    pub power_w: f64,
    pub session_discharged_mwh: f64,
}

impl Default for BatteryStats {
    fn default() -> Self {
        Self {
            available: false,
            current_capacity_mah: -1,
            max_capacity_mah: -1,
            design_capacity_mah: -1,
            charge_percent: -1.0,
            voltage_mv: -1,
            amperage_ma: 0,
            cycle_count: -1,
            is_charging: false,
            has_accumulated_energy: false,
            accumulated_energy_mwh: -1,
            power_w: f64::NAN,
            session_discharged_mwh: 0.0,
        }
    }
}

#[cfg(test)]
mod tests;
