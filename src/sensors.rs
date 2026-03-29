// SPDX-License-Identifier: GPL-3.0-only
// Copyright (c) 2026 CarlosEvangelista

use std::cmp::Ordering;
use std::ffi::{CString, c_char, c_int, c_void};
use std::mem::size_of;
use std::thread;
use std::time::Duration;

use crate::models::{
    SensorCollection, SensorEntry, SensorSection, WATCHTEMP_SENSOR_MAX_RETRIES,
    WATCHTEMP_SENSOR_MAX_VALID_C, WATCHTEMP_SENSOR_MIN_VALID_C, WATCHTEMP_SENSOR_RETRY_DELAY_US,
};
use crate::smc::{
    SmcBytes, SmcContext, SmcKeyDataKeyInfo, smc_decode_temperature, smc_fourcc_to_string,
    smc_is_valid_temperature, smc_read_key_at_index, smc_read_key_bytes, smc_read_key_count,
    smc_read_key_info,
};

struct KnownSensor {
    key: &'static str,
    name: &'static str,
    type_label: &'static str,
    section: SensorSection,
}

const APPLE_PROFILE: &[KnownSensor] = &[
    KnownSensor {
        key: "Tp09",
        name: "CPU Die",
        type_label: "Die",
        section: SensorSection::Cpu,
    },
    KnownSensor {
        key: "Tp0T",
        name: "CPU Cluster Average",
        type_label: "Cluster Avg",
        section: SensorSection::Cpu,
    },
    KnownSensor {
        key: "Tp1h",
        name: "SoC Hotspot",
        type_label: "Hotspot",
        section: SensorSection::Cpu,
    },
    KnownSensor {
        key: "pACC",
        name: "Performance Cluster Average",
        type_label: "P-Cluster Avg",
        section: SensorSection::Cpu,
    },
    KnownSensor {
        key: "eACC",
        name: "Efficiency Cluster Average",
        type_label: "E-Cluster Avg",
        section: SensorSection::Cpu,
    },
    KnownSensor {
        key: "Tp01",
        name: "Performance Core 1",
        type_label: "P-Core",
        section: SensorSection::Cpu,
    },
    KnownSensor {
        key: "Tp0D",
        name: "Performance Core 2",
        type_label: "P-Core",
        section: SensorSection::Cpu,
    },
    KnownSensor {
        key: "Tp0H",
        name: "Performance Core 3",
        type_label: "P-Core",
        section: SensorSection::Cpu,
    },
    KnownSensor {
        key: "Tp0L",
        name: "Performance Core 4",
        type_label: "P-Core",
        section: SensorSection::Cpu,
    },
    KnownSensor {
        key: "Tp0P",
        name: "Performance Core 5",
        type_label: "P-Core",
        section: SensorSection::Cpu,
    },
    KnownSensor {
        key: "Tp0X",
        name: "Performance Core 6",
        type_label: "P-Core",
        section: SensorSection::Cpu,
    },
    KnownSensor {
        key: "Tp0b",
        name: "Performance Core 7",
        type_label: "P-Core",
        section: SensorSection::Cpu,
    },
    KnownSensor {
        key: "Tp0f",
        name: "Performance Core 8",
        type_label: "P-Core",
        section: SensorSection::Cpu,
    },
    KnownSensor {
        key: "Tp05",
        name: "Efficiency Core 1",
        type_label: "E-Core",
        section: SensorSection::Cpu,
    },
    KnownSensor {
        key: "Tp0j",
        name: "Efficiency Core 2",
        type_label: "E-Core",
        section: SensorSection::Cpu,
    },
    KnownSensor {
        key: "Tp0n",
        name: "Efficiency Core 3",
        type_label: "E-Core",
        section: SensorSection::Cpu,
    },
    KnownSensor {
        key: "Tp0r",
        name: "Efficiency Core 4",
        type_label: "E-Core",
        section: SensorSection::Cpu,
    },
    KnownSensor {
        key: "Te05",
        name: "Efficiency Core 1",
        type_label: "E-Core",
        section: SensorSection::Cpu,
    },
    KnownSensor {
        key: "Te14",
        name: "Efficiency Core 2",
        type_label: "E-Core",
        section: SensorSection::Cpu,
    },
    KnownSensor {
        key: "Tf04",
        name: "Performance Core 1",
        type_label: "P-Core",
        section: SensorSection::Cpu,
    },
    KnownSensor {
        key: "Tf05",
        name: "Performance Core 2",
        type_label: "P-Core",
        section: SensorSection::Cpu,
    },
    KnownSensor {
        key: "Tf06",
        name: "Performance Core 3",
        type_label: "P-Core",
        section: SensorSection::Cpu,
    },
    KnownSensor {
        key: "Tf07",
        name: "Performance Core 4",
        type_label: "P-Core",
        section: SensorSection::Cpu,
    },
    KnownSensor {
        key: "Tf09",
        name: "Performance Core 5",
        type_label: "P-Core",
        section: SensorSection::Cpu,
    },
    KnownSensor {
        key: "Tf0A",
        name: "Performance Core 6",
        type_label: "P-Core",
        section: SensorSection::Cpu,
    },
    KnownSensor {
        key: "Tf0B",
        name: "Performance Core 7",
        type_label: "P-Core",
        section: SensorSection::Cpu,
    },
    KnownSensor {
        key: "Tf0C",
        name: "Performance Core 8",
        type_label: "P-Core",
        section: SensorSection::Cpu,
    },
    KnownSensor {
        key: "Tf0D",
        name: "Performance Core 9",
        type_label: "P-Core",
        section: SensorSection::Cpu,
    },
    KnownSensor {
        key: "Tf0E",
        name: "Performance Core 10",
        type_label: "P-Core",
        section: SensorSection::Cpu,
    },
    KnownSensor {
        key: "Tf0F",
        name: "Performance Core 11",
        type_label: "P-Core",
        section: SensorSection::Cpu,
    },
    KnownSensor {
        key: "Tf0G",
        name: "Performance Core 12",
        type_label: "P-Core",
        section: SensorSection::Cpu,
    },
    KnownSensor {
        key: "Tf16",
        name: "Neural Engine Zone",
        type_label: "NPU",
        section: SensorSection::Cpu,
    },
    KnownSensor {
        key: "Tf18",
        name: "SoC Logic Zone",
        type_label: "SoC",
        section: SensorSection::Cpu,
    },
    KnownSensor {
        key: "TN0D",
        name: "Memory Die",
        type_label: "Memory",
        section: SensorSection::Cpu,
    },
    KnownSensor {
        key: "TN0H",
        name: "Memory Hotspot",
        type_label: "Memory Hotspot",
        section: SensorSection::Cpu,
    },
    KnownSensor {
        key: "Tm0P",
        name: "Memory Proximity",
        type_label: "Memory",
        section: SensorSection::Cpu,
    },
    KnownSensor {
        key: "TG0D",
        name: "GPU Die",
        type_label: "Die",
        section: SensorSection::Gpu,
    },
    KnownSensor {
        key: "TG0P",
        name: "GPU Proximity",
        type_label: "Proximity",
        section: SensorSection::Gpu,
    },
    KnownSensor {
        key: "TG0T",
        name: "GPU Thermal Average",
        type_label: "Average",
        section: SensorSection::Gpu,
    },
    KnownSensor {
        key: "TG1D",
        name: "GPU Slice 2 Die",
        type_label: "Die",
        section: SensorSection::Gpu,
    },
    KnownSensor {
        key: "TG1P",
        name: "GPU Slice 2 Proximity",
        type_label: "Proximity",
        section: SensorSection::Gpu,
    },
    KnownSensor {
        key: "Tg05",
        name: "GPU Core Cluster 1",
        type_label: "Core",
        section: SensorSection::Gpu,
    },
    KnownSensor {
        key: "Tg0D",
        name: "GPU Core Cluster 2",
        type_label: "Core",
        section: SensorSection::Gpu,
    },
    KnownSensor {
        key: "Tg0f",
        name: "GPU Core Cluster 3",
        type_label: "Core",
        section: SensorSection::Gpu,
    },
    KnownSensor {
        key: "Tg0n",
        name: "GPU Core Cluster 4",
        type_label: "Core",
        section: SensorSection::Gpu,
    },
    KnownSensor {
        key: "Tf14",
        name: "GPU Tile 1",
        type_label: "Core",
        section: SensorSection::Gpu,
    },
    KnownSensor {
        key: "Tf15",
        name: "GPU Tile 2",
        type_label: "Core",
        section: SensorSection::Gpu,
    },
    KnownSensor {
        key: "TB0T",
        name: "Battery Pack",
        type_label: "Pack",
        section: SensorSection::Battery,
    },
    KnownSensor {
        key: "TB1T",
        name: "Battery Cell",
        type_label: "Cell",
        section: SensorSection::Battery,
    },
    KnownSensor {
        key: "TB2T",
        name: "Battery Cell 2",
        type_label: "Cell",
        section: SensorSection::Battery,
    },
    KnownSensor {
        key: "TW0P",
        name: "Battery Proximity",
        type_label: "Proximity",
        section: SensorSection::Battery,
    },
    KnownSensor {
        key: "Ta0P",
        name: "Chassis Ambient",
        type_label: "Ambient",
        section: SensorSection::Battery,
    },
];

const INTEL_PROFILE: &[KnownSensor] = &[
    KnownSensor {
        key: "TC0P",
        name: "CPU Proximity",
        type_label: "Proximity",
        section: SensorSection::Cpu,
    },
    KnownSensor {
        key: "TC0E",
        name: "CPU PECI",
        type_label: "PECI",
        section: SensorSection::Cpu,
    },
    KnownSensor {
        key: "TC0F",
        name: "CPU Die",
        type_label: "Die",
        section: SensorSection::Cpu,
    },
    KnownSensor {
        key: "TC1C",
        name: "CPU Core 1",
        type_label: "Core",
        section: SensorSection::Cpu,
    },
    KnownSensor {
        key: "TC2C",
        name: "CPU Core 2",
        type_label: "Core",
        section: SensorSection::Cpu,
    },
    KnownSensor {
        key: "TC3C",
        name: "CPU Core 3",
        type_label: "Core",
        section: SensorSection::Cpu,
    },
    KnownSensor {
        key: "TC4C",
        name: "CPU Core 4",
        type_label: "Core",
        section: SensorSection::Cpu,
    },
    KnownSensor {
        key: "TH0P",
        name: "CPU Heatsink",
        type_label: "Heatsink",
        section: SensorSection::Cpu,
    },
    KnownSensor {
        key: "TN0P",
        name: "Northbridge",
        type_label: "Chipset",
        section: SensorSection::Cpu,
    },
    KnownSensor {
        key: "TG0D",
        name: "GPU Die",
        type_label: "Die",
        section: SensorSection::Gpu,
    },
    KnownSensor {
        key: "TG0H",
        name: "GPU Heatsink",
        type_label: "Heatsink",
        section: SensorSection::Gpu,
    },
    KnownSensor {
        key: "TG0P",
        name: "GPU Proximity",
        type_label: "Proximity",
        section: SensorSection::Gpu,
    },
    KnownSensor {
        key: "TG1D",
        name: "GPU Die 2",
        type_label: "Die",
        section: SensorSection::Gpu,
    },
    KnownSensor {
        key: "TB0T",
        name: "Battery Pack",
        type_label: "Pack",
        section: SensorSection::Battery,
    },
    KnownSensor {
        key: "TB1T",
        name: "Battery Cell",
        type_label: "Cell",
        section: SensorSection::Battery,
    },
    KnownSensor {
        key: "TW0P",
        name: "Battery Proximity",
        type_label: "Proximity",
        section: SensorSection::Battery,
    },
    KnownSensor {
        key: "Ta0P",
        name: "Chassis Ambient",
        type_label: "Ambient",
        section: SensorSection::Battery,
    },
];

unsafe extern "C" {
    fn sysctlbyname(
        name: *const c_char,
        oldp: *mut c_void,
        oldlenp: *mut usize,
        newp: *mut c_void,
        newlen: usize,
    ) -> c_int;
}

pub fn sensor_section_label(section: SensorSection) -> &'static str {
    match section {
        SensorSection::Gpu => "GPU",
        SensorSection::Battery => "Battery",
        SensorSection::Cpu => "CPU",
    }
}

fn detect_apple_silicon() -> bool {
    let mut arm64: c_int = 0;
    let mut size = size_of::<c_int>();
    let name = CString::new("hw.optional.arm64").expect("static key has no NUL");

    // SAFETY: All pointers are valid for the duration of the call; this is a read-only sysctl.
    let result = unsafe {
        sysctlbyname(
            name.as_ptr(),
            &mut arm64 as *mut c_int as *mut c_void,
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    result == 0 && arm64 == 1
}

fn key4_bytes(key: &str) -> Option<[u8; 4]> {
    let raw = key.as_bytes();
    if raw.len() < 4 {
        return None;
    }
    Some([raw[0], raw[1], raw[2], raw[3]])
}

fn is_printable_key(key: &str) -> bool {
    let Some(raw) = key4_bytes(key) else {
        return false;
    };

    raw.into_iter().all(|b| b.is_ascii_graphic() || b == b' ')
}

fn is_battery_key(key: &str) -> bool {
    let Some([k0, k1, _, _]) = key4_bytes(key) else {
        return false;
    };
    if (k0 == b'T' || k0 == b't') && (k1 == b'B' || k1 == b'b' || k1 == b'W') {
        return true;
    }
    key.starts_with("Ta0P")
}

fn is_known_gpu_tile_key(key: &str) -> bool {
    key.starts_with("Tf14") || key.starts_with("Tf15")
}

fn is_gpu_key(key: &str) -> bool {
    let Some([k0, k1, _, _]) = key4_bytes(key) else {
        return false;
    };
    if (k0 == b'T' || k0 == b't') && (k1 == b'G' || k1 == b'g') {
        return true;
    }
    is_known_gpu_tile_key(key)
}

fn is_candidate_temp_key(key: &str) -> bool {
    if !is_printable_key(key) {
        return false;
    }

    if key.starts_with("pACC") || key.starts_with("eACC") {
        return true;
    }

    matches!(key.as_bytes().first(), Some(b'T' | b't'))
}

fn infer_section(key: &str) -> SensorSection {
    if is_battery_key(key) {
        return SensorSection::Battery;
    }
    if is_gpu_key(key) {
        return SensorSection::Gpu;
    }
    SensorSection::Cpu
}

fn find_known_sensor(key: &str, is_apple_silicon: bool) -> Option<&'static KnownSensor> {
    let (first_profile, fallback_profile) = if is_apple_silicon {
        (APPLE_PROFILE, INTEL_PROFILE)
    } else {
        (INTEL_PROFILE, APPLE_PROFILE)
    };

    first_profile
        .iter()
        .find(|entry| key.starts_with(entry.key))
        .or_else(|| {
            fallback_profile
                .iter()
                .find(|entry| key.starts_with(entry.key))
        })
}

fn decode_base36_char(value: u8) -> i32 {
    match value {
        b'0'..=b'9' => (value - b'0') as i32,
        b'A'..=b'Z' => 10 + (value - b'A') as i32,
        b'a'..=b'z' => 10 + (value - b'a') as i32,
        _ => -1,
    }
}

fn infer_perf_core_index_from_tf(key: &str) -> i32 {
    let Some([k0, k1, k2, k3]) = key4_bytes(key) else {
        return -1;
    };
    if k0 != b'T' || k1 != b'f' || k2 != b'0' {
        return -1;
    }

    let raw = decode_base36_char(k3);
    if raw < 4 {
        return -1;
    }

    raw - 3
}

fn infer_core_index_from_tc(key: &str) -> i32 {
    let Some([k0, k1, k2, k3]) = key4_bytes(key) else {
        return -1;
    };
    if k0 != b'T' || k1 != b'C' || k3 != b'C' {
        return -1;
    }

    decode_base36_char(k2)
}

fn infer_type_label(section: SensorSection, key: &str) -> String {
    if section == SensorSection::Battery {
        if key.starts_with("TB0T") {
            return "Pack".to_string();
        }
        if key.starts_with("TB1T") || key.starts_with("TB2T") {
            return "Cell".to_string();
        }
        if key.starts_with("TW0P") {
            return "Proximity".to_string();
        }
        if key.starts_with("Ta0P") {
            return "Ambient".to_string();
        }
        return "Battery".to_string();
    }

    if section == SensorSection::Gpu {
        let key3 = key.as_bytes().get(3).copied().unwrap_or_default();
        if key3 == b'D' {
            return "Die".to_string();
        }
        if key3 == b'P' {
            return "Proximity".to_string();
        }
        if key3 == b'T' {
            return "Average".to_string();
        }
        return "Core".to_string();
    }

    if key.starts_with("pACC") || key.starts_with("eACC") {
        return "Cluster Avg".to_string();
    }

    if key.starts_with("Te") {
        return "E-Core".to_string();
    }
    if key.starts_with("Tf") && !is_known_gpu_tile_key(key) {
        return "P-Core".to_string();
    }
    if key.starts_with("TN") {
        return "Memory".to_string();
    }
    if key.starts_with("Tm") {
        return "SoC".to_string();
    }

    let key3 = key.as_bytes().get(3).copied().unwrap_or_default();
    if key3 == b'D' {
        return "Die".to_string();
    }
    if key3 == b'P' {
        return "Proximity".to_string();
    }
    if key3 == b'H' || key3 == b'h' {
        return "Hotspot".to_string();
    }
    if key3 == b'T' {
        return "Average".to_string();
    }

    "Thermal".to_string()
}

fn infer_auto_name(section: SensorSection, key: &str, type_label: &str) -> String {
    if section == SensorSection::Battery {
        if type_label.starts_with("Pack") {
            return "Battery Pack".to_string();
        }
        if type_label.starts_with("Cell") {
            return "Battery Cell".to_string();
        }
        if type_label.starts_with("Ambient") {
            return "Chassis Ambient".to_string();
        }
        return format!("Battery Sensor ({key})");
    }

    if section == SensorSection::Gpu {
        if type_label.starts_with("Core") {
            return format!("GPU Core Zone ({key})");
        }
        return format!("GPU {type_label}");
    }

    let perf_index = infer_perf_core_index_from_tf(key);
    if perf_index > 0 {
        return format!("Performance Core {perf_index}");
    }

    let intel_core_index = infer_core_index_from_tc(key);
    if intel_core_index > 0 {
        return format!("CPU Core {intel_core_index}");
    }

    if type_label.starts_with("E-Core") {
        return format!("Efficiency Core Zone ({key})");
    }
    if type_label.starts_with("P-Core") {
        return format!("Performance Core Zone ({key})");
    }
    if type_label.starts_with("Memory") {
        return "Unified Memory Thermal".to_string();
    }
    if type_label.starts_with("SoC") {
        return format!("SoC Zone ({key})");
    }

    format!("CPU {type_label}")
}

fn fill_sensor_metadata(entry: &mut SensorEntry, is_apple_silicon: bool) {
    if let Some(known) = find_known_sensor(&entry.key, is_apple_silicon) {
        entry.section = known.section;
        entry.name = known.name.to_string();
        entry.type_label = known.type_label.to_string();
        return;
    }

    entry.section = infer_section(&entry.key);
    entry.type_label = infer_type_label(entry.section, &entry.key);
    entry.name = infer_auto_name(entry.section, &entry.key, &entry.type_label);
}

fn contains_key(collection: &SensorCollection, key: &str) -> bool {
    collection
        .items
        .iter()
        .any(|entry| entry.key.starts_with(key))
}

fn sort_collection(collection: &mut SensorCollection) {
    collection.items.sort_by(|left, right| {
        if left.section != right.section {
            return left.section.cmp(&right.section);
        }

        let type_cmp = left.type_label.cmp(&right.type_label);
        if type_cmp != Ordering::Equal {
            return type_cmp;
        }

        left.key.cmp(&right.key)
    });
}

fn sleep_retry(retry_delay_us: u64) {
    if retry_delay_us > 0 {
        thread::sleep(Duration::from_micros(retry_delay_us));
    }
}

pub fn sensor_read_temperature_retry(
    ctx: &SmcContext,
    entry: &mut SensorEntry,
    max_retries: i32,
    retry_delay_us: u64,
    min_valid: f64,
    max_valid: f64,
) -> bool {
    if max_retries <= 0 {
        return false;
    }

    for _ in 0..max_retries {
        let mut key_info = SmcKeyDataKeyInfo::default();
        let mut bytes: SmcBytes = [0; crate::smc::SMC_MAX_DATA_SIZE];
        let mut temperature = 0.0f64;

        if !smc_read_key_info(ctx, &entry.key, &mut key_info) {
            sleep_retry(retry_delay_us);
            continue;
        }
        if !smc_read_key_bytes(ctx, &entry.key, &key_info, &mut bytes) {
            sleep_retry(retry_delay_us);
            continue;
        }
        if !smc_decode_temperature(
            &bytes,
            key_info.data_type,
            key_info.data_size,
            min_valid,
            max_valid,
            &mut temperature,
        ) {
            sleep_retry(retry_delay_us);
            continue;
        }

        entry.data_type = smc_fourcc_to_string(key_info.data_type);
        if smc_is_valid_temperature(temperature, min_valid, max_valid) {
            entry.last_temp_c = temperature;
            return true;
        }

        sleep_retry(retry_delay_us);
    }

    false
}

fn try_register_sensor(
    ctx: &SmcContext,
    collection: &mut SensorCollection,
    key: &str,
    is_apple_silicon: bool,
    retries: i32,
    retry_delay_us: u64,
    min_valid: f64,
    max_valid: f64,
) -> bool {
    if !is_candidate_temp_key(key) || contains_key(collection, key) {
        return true;
    }

    let mut entry = SensorEntry {
        key: key.chars().take(4).collect(),
        name: String::new(),
        type_label: String::new(),
        section: SensorSection::Cpu,
        active: false,
        last_temp_c: 0.0,
        data_type: String::new(),
    };

    fill_sensor_metadata(&mut entry, is_apple_silicon);

    if !sensor_read_temperature_retry(
        ctx,
        &mut entry,
        retries,
        retry_delay_us,
        min_valid,
        max_valid,
    ) {
        return true;
    }

    entry.active = true;
    collection.items.push(entry);
    true
}

fn discover_profile_first(
    ctx: &SmcContext,
    collection: &mut SensorCollection,
    is_apple_silicon: bool,
    retries: i32,
    retry_delay_us: u64,
    min_valid: f64,
    max_valid: f64,
) -> bool {
    let profile = if is_apple_silicon {
        APPLE_PROFILE
    } else {
        INTEL_PROFILE
    };

    for known in profile {
        if !try_register_sensor(
            ctx,
            collection,
            known.key,
            is_apple_silicon,
            retries,
            retry_delay_us,
            min_valid,
            max_valid,
        ) {
            return false;
        }
    }

    true
}

fn discover_from_smc_keyspace(
    ctx: &SmcContext,
    collection: &mut SensorCollection,
    is_apple_silicon: bool,
    retries: i32,
    retry_delay_us: u64,
    min_valid: f64,
    max_valid: f64,
) -> bool {
    let mut key_count = 0u32;
    if !smc_read_key_count(ctx, &mut key_count) || key_count == 0 || key_count > 100_000 {
        return true;
    }

    for index in 0..key_count {
        let mut key_raw = [0u8; 5];
        if !smc_read_key_at_index(ctx, index, &mut key_raw) {
            continue;
        }

        let key = String::from_utf8_lossy(&key_raw[..4]).to_string();
        if !try_register_sensor(
            ctx,
            collection,
            &key,
            is_apple_silicon,
            retries,
            retry_delay_us,
            min_valid,
            max_valid,
        ) {
            return false;
        }
    }

    true
}

pub fn sensor_discover_profile(
    ctx: &SmcContext,
    collection: &mut SensorCollection,
    discovery_retries: i32,
    retry_delay_us: u64,
    min_valid: f64,
    max_valid: f64,
) -> bool {
    if discovery_retries <= 0 {
        return false;
    }

    let is_apple_silicon = detect_apple_silicon();
    if !discover_profile_first(
        ctx,
        collection,
        is_apple_silicon,
        discovery_retries,
        retry_delay_us,
        min_valid,
        max_valid,
    ) {
        return false;
    }

    sort_collection(collection);
    true
}

pub fn sensor_enrich_from_keyspace(
    ctx: &SmcContext,
    collection: &mut SensorCollection,
    discovery_retries: i32,
    retry_delay_us: u64,
    min_valid: f64,
    max_valid: f64,
) -> bool {
    if discovery_retries <= 0 {
        return false;
    }

    let is_apple_silicon = detect_apple_silicon();
    if !discover_from_smc_keyspace(
        ctx,
        collection,
        is_apple_silicon,
        discovery_retries,
        retry_delay_us,
        min_valid,
        max_valid,
    ) {
        return false;
    }

    sort_collection(collection);
    true
}

pub fn sensor_discover(
    ctx: &SmcContext,
    collection: &mut SensorCollection,
    discovery_retries: i32,
    retry_delay_us: u64,
    min_valid: f64,
    max_valid: f64,
) -> bool {
    if discovery_retries <= 0 {
        return false;
    }

    collection.items.clear();
    let keyspace_retries = discovery_retries.min(4);

    if !sensor_discover_profile(
        ctx,
        collection,
        discovery_retries,
        retry_delay_us,
        min_valid,
        max_valid,
    ) {
        sensor_collection_free(collection);
        return false;
    }

    if !sensor_enrich_from_keyspace(
        ctx,
        collection,
        keyspace_retries,
        retry_delay_us,
        min_valid,
        max_valid,
    ) {
        sensor_collection_free(collection);
        return false;
    }

    true
}

pub fn sensor_collection_free(collection: &mut SensorCollection) {
    collection.items.clear();
}

pub fn sensor_refresh_active(
    ctx: &SmcContext,
    collection: &mut SensorCollection,
    max_retries: i32,
    retry_delay_us: u64,
    min_valid: f64,
    max_valid: f64,
) {
    for entry in &mut collection.items {
        if !entry.active {
            continue;
        }

        let _ = sensor_read_temperature_retry(
            ctx,
            entry,
            max_retries,
            retry_delay_us,
            min_valid,
            max_valid,
        );
    }
}

pub fn sensor_count_active_by_section(entries: &[SensorEntry], section: SensorSection) -> i32 {
    entries
        .iter()
        .filter(|entry| entry.section == section && entry.active)
        .count() as i32
}

pub fn sensor_discover_default(ctx: &SmcContext, collection: &mut SensorCollection) -> bool {
    sensor_discover(
        ctx,
        collection,
        WATCHTEMP_SENSOR_MAX_RETRIES,
        WATCHTEMP_SENSOR_RETRY_DELAY_US,
        WATCHTEMP_SENSOR_MIN_VALID_C,
        WATCHTEMP_SENSOR_MAX_VALID_C,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn section_label_matches_expected_values() {
        assert_eq!("CPU", sensor_section_label(SensorSection::Cpu));
        assert_eq!("GPU", sensor_section_label(SensorSection::Gpu));
        assert_eq!("Battery", sensor_section_label(SensorSection::Battery));
    }

    #[test]
    fn count_active_by_section_counts_only_active_items() {
        let entries = vec![
            SensorEntry {
                key: "Tp09".to_string(),
                name: "CPU Die".to_string(),
                type_label: "Die".to_string(),
                section: SensorSection::Cpu,
                active: true,
                last_temp_c: 55.0,
                data_type: "sp78".to_string(),
            },
            SensorEntry {
                key: "TG0D".to_string(),
                name: "GPU Die".to_string(),
                type_label: "Die".to_string(),
                section: SensorSection::Gpu,
                active: true,
                last_temp_c: 65.0,
                data_type: "sp78".to_string(),
            },
            SensorEntry {
                key: "TB0T".to_string(),
                name: "Battery".to_string(),
                type_label: "Pack".to_string(),
                section: SensorSection::Battery,
                active: false,
                last_temp_c: 33.0,
                data_type: "sp78".to_string(),
            },
        ];

        assert_eq!(
            1,
            sensor_count_active_by_section(&entries, SensorSection::Cpu)
        );
        assert_eq!(
            1,
            sensor_count_active_by_section(&entries, SensorSection::Gpu)
        );
        assert_eq!(
            0,
            sensor_count_active_by_section(&entries, SensorSection::Battery)
        );
    }
}
