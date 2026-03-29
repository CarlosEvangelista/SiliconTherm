// SPDX-License-Identifier: GPL-3.0-only
// Copyright (c) 2026 CarlosEvangelista

use super::*;

fn make_entry(
    key: &str,
    name: &str,
    type_label: &str,
    section: SensorSection,
    temp_c: f64,
    active: bool,
) -> SensorEntry {
    SensorEntry {
        key: key.to_string(),
        name: name.to_string(),
        type_label: type_label.to_string(),
        section,
        active,
        last_temp_c: temp_c,
        data_type: "sp78".to_string(),
    }
}

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

#[test]
fn key_and_section_helpers_classify_expected_keys() {
    assert_eq!(Some(*b"Tp09"), key4_bytes("Tp09"));
    assert_eq!(Some(*b"Tp09"), key4_bytes("Tp09_extended"));
    assert_eq!(None, key4_bytes("Tp9"));

    assert!(is_printable_key("Tp09"));
    assert!(!is_printable_key("T\n09"));

    assert!(is_battery_key("TB0T"));
    assert!(is_battery_key("TW0P"));
    assert!(is_battery_key("Ta0P"));
    assert!(!is_battery_key("TG0D"));

    assert!(is_gpu_key("TG0D"));
    assert!(is_gpu_key("Tf14"));
    assert!(!is_gpu_key("Tp09"));

    assert_eq!(SensorSection::Battery, infer_section("TB0T"));
    assert_eq!(SensorSection::Gpu, infer_section("TG0D"));
    assert_eq!(SensorSection::Gpu, infer_section("Tf14"));
    assert_eq!(SensorSection::Cpu, infer_section("Tp09"));
}

#[test]
fn candidate_temp_key_accepts_expected_prefixes() {
    assert!(is_candidate_temp_key("Tp09"));
    assert!(is_candidate_temp_key("pACC"));
    assert!(is_candidate_temp_key("eACC"));
    assert!(!is_candidate_temp_key("Xp09"));
    assert!(!is_candidate_temp_key("T\n09"));
}

#[test]
fn known_sensor_lookup_uses_primary_and_fallback_profiles() {
    let intel_from_fallback =
        find_known_sensor("TC0P", true).expect("expected fallback lookup to find Intel key");
    assert_eq!("CPU Proximity", intel_from_fallback.name);

    let apple_from_fallback =
        find_known_sensor("Tp09", false).expect("expected fallback lookup to find Apple key");
    assert_eq!("CPU Die", apple_from_fallback.name);
}

#[test]
fn base36_and_core_index_helpers_cover_common_paths() {
    assert_eq!(0, decode_base36_char(b'0'));
    assert_eq!(10, decode_base36_char(b'A'));
    assert_eq!(35, decode_base36_char(b'z'));
    assert_eq!(-1, decode_base36_char(b'?'));

    assert_eq!(1, infer_perf_core_index_from_tf("Tf04"));
    assert_eq!(2, infer_perf_core_index_from_tf("Tf05"));
    assert_eq!(-1, infer_perf_core_index_from_tf("Tf03"));
    assert_eq!(-1, infer_perf_core_index_from_tf("TG04"));

    assert_eq!(1, infer_core_index_from_tc("TC1C"));
    assert_eq!(10, infer_core_index_from_tc("TCAC"));
    assert_eq!(-1, infer_core_index_from_tc("Tp09"));
}

#[test]
fn infer_type_label_maps_keys_by_section() {
    assert_eq!("Pack", infer_type_label(SensorSection::Battery, "TB0T"));
    assert_eq!("Cell", infer_type_label(SensorSection::Battery, "TB1T"));
    assert_eq!(
        "Proximity",
        infer_type_label(SensorSection::Battery, "TW0P")
    );
    assert_eq!("Ambient", infer_type_label(SensorSection::Battery, "Ta0P"));
    assert_eq!("Battery", infer_type_label(SensorSection::Battery, "TB9Z"));

    assert_eq!("Die", infer_type_label(SensorSection::Gpu, "TG0D"));
    assert_eq!("Proximity", infer_type_label(SensorSection::Gpu, "TG0P"));
    assert_eq!("Average", infer_type_label(SensorSection::Gpu, "TG0T"));
    assert_eq!("Core", infer_type_label(SensorSection::Gpu, "TG0X"));

    assert_eq!("Cluster Avg", infer_type_label(SensorSection::Cpu, "pACC"));
    assert_eq!("E-Core", infer_type_label(SensorSection::Cpu, "Te05"));
    assert_eq!("P-Core", infer_type_label(SensorSection::Cpu, "Tf04"));
    assert_eq!("Memory", infer_type_label(SensorSection::Cpu, "TN0P"));
    assert_eq!("SoC", infer_type_label(SensorSection::Cpu, "Tm01"));
    assert_eq!("Hotspot", infer_type_label(SensorSection::Cpu, "Tp0h"));
    assert_eq!("Thermal", infer_type_label(SensorSection::Cpu, "Txyz"));
}

#[test]
fn infer_auto_name_formats_expected_labels() {
    assert_eq!(
        "Battery Pack",
        infer_auto_name(SensorSection::Battery, "TB0T", "Pack")
    );
    assert_eq!(
        "Battery Sensor (TB9Z)",
        infer_auto_name(SensorSection::Battery, "TB9Z", "Battery")
    );

    assert_eq!(
        "GPU Core Zone (TG0X)",
        infer_auto_name(SensorSection::Gpu, "TG0X", "Core")
    );
    assert_eq!(
        "GPU Die",
        infer_auto_name(SensorSection::Gpu, "TG0D", "Die")
    );

    assert_eq!(
        "Performance Core 1",
        infer_auto_name(SensorSection::Cpu, "Tf04", "P-Core")
    );
    assert_eq!(
        "CPU Core 2",
        infer_auto_name(SensorSection::Cpu, "TC2C", "Core")
    );
    assert_eq!(
        "Efficiency Core Zone (Te05)",
        infer_auto_name(SensorSection::Cpu, "Te05", "E-Core")
    );
    assert_eq!(
        "Unified Memory Thermal",
        infer_auto_name(SensorSection::Cpu, "TN0P", "Memory")
    );
    assert_eq!(
        "SoC Zone (Tm01)",
        infer_auto_name(SensorSection::Cpu, "Tm01", "SoC")
    );
    assert_eq!(
        "CPU Thermal",
        infer_auto_name(SensorSection::Cpu, "Tp09", "Thermal")
    );
}

#[test]
fn fill_sensor_metadata_prefers_known_profile_and_infers_unknown() {
    let mut known = make_entry("TC0P", "", "", SensorSection::Cpu, 40.0, true);
    fill_sensor_metadata(&mut known, true);
    assert_eq!(SensorSection::Cpu, known.section);
    assert_eq!("CPU Proximity", known.name);
    assert_eq!("Proximity", known.type_label);

    let mut unknown = make_entry("Tm01", "", "", SensorSection::Cpu, 42.0, true);
    fill_sensor_metadata(&mut unknown, true);
    assert_eq!(SensorSection::Cpu, unknown.section);
    assert_eq!("SoC", unknown.type_label);
    assert_eq!("SoC Zone (Tm01)", unknown.name);
}

#[test]
fn collection_helpers_detect_prefix_and_sort_consistently() {
    let mut collection = SensorCollection {
        items: vec![
            make_entry(
                "TG0P",
                "GPU Proximity",
                "Proximity",
                SensorSection::Gpu,
                60.0,
                true,
            ),
            make_entry("Tp09", "CPU Die", "Die", SensorSection::Cpu, 50.0, true),
            make_entry(
                "TB0T",
                "Battery Pack",
                "Pack",
                SensorSection::Battery,
                30.0,
                true,
            ),
            make_entry("TC1C", "CPU Core 1", "Core", SensorSection::Cpu, 52.0, true),
        ],
    };

    assert!(contains_key(&collection, "TC1"));
    assert!(!contains_key(&collection, "TF0"));

    sort_collection(&mut collection);

    assert_eq!("TC1C", collection.items[0].key);
    assert_eq!("Tp09", collection.items[1].key);
    assert_eq!("TG0P", collection.items[2].key);
    assert_eq!("TB0T", collection.items[3].key);
}
