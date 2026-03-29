// SPDX-License-Identifier: GPL-3.0-only
// Copyright (c) 2026 CarlosEvangelista

use super::*;

fn make_entry(
    key: &str,
    name: &str,
    type_label: &str,
    section: SensorSection,
    data_type: &str,
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
        data_type: data_type.to_string(),
    }
}

fn assert_close(expected: f64, actual: f64) {
    let delta = (expected - actual).abs();
    assert!(
        delta <= 0.0001,
        "expected {expected}, got {actual} (delta: {delta})"
    );
}

#[test]
fn test_ui_clamp_clamps_to_bounds() {
    assert_close(0.0, ui_clamp(-2.0, 0.0, 10.0));
    assert_close(4.0, ui_clamp(4.0, 0.0, 10.0));
    assert_close(10.0, ui_clamp(15.0, 0.0, 10.0));
}

#[test]
fn test_ui_temp_to_ratio_clamps_and_handles_invalid_range() {
    assert_close(0.0, ui_temp_to_ratio(20.0, 30.0, 100.0));
    assert_close(0.5, ui_temp_to_ratio(65.0, 30.0, 100.0));
    assert_close(1.0, ui_temp_to_ratio(120.0, 30.0, 100.0));

    assert_close(0.0, ui_temp_to_ratio(50.0, 30.0, 30.0));
    assert_close(0.0, ui_temp_to_ratio(50.0, 50.0, 40.0));
}

#[test]
fn test_ui_meter_filled_columns_rounds_and_limits() {
    assert_eq!(0, ui_meter_filled_columns(0.0, 20));
    assert_eq!(10, ui_meter_filled_columns(0.5, 20));
    assert_eq!(20, ui_meter_filled_columns(1.0, 20));

    assert_eq!(0, ui_meter_filled_columns(-1.0, 20));
    assert_eq!(20, ui_meter_filled_columns(3.0, 20));

    assert_eq!(0, ui_meter_filled_columns(0.5, 0));
    assert_eq!(0, ui_meter_filled_columns(0.5, -3));
}

#[test]
fn test_ui_temp_to_heat_level_uses_thresholds() {
    assert_eq!(UiHeatLevel::Cool, ui_temp_to_heat_level(50.0, 70.0, 85.0));
    assert_eq!(UiHeatLevel::Warm, ui_temp_to_heat_level(70.0, 70.0, 85.0));
    assert_eq!(UiHeatLevel::Warm, ui_temp_to_heat_level(84.9, 70.0, 85.0));
    assert_eq!(UiHeatLevel::Hot, ui_temp_to_heat_level(85.0, 70.0, 85.0));
}

#[test]
fn test_ui_meter_heat_level_for_column_maps_bands() {
    let width = 50;

    assert_eq!(UiHeatLevel::Cool, ui_meter_heat_level_for_column(0, width));
    assert_eq!(UiHeatLevel::Cool, ui_meter_heat_level_for_column(32, width));
    assert_eq!(UiHeatLevel::Warm, ui_meter_heat_level_for_column(33, width));
    assert_eq!(UiHeatLevel::Warm, ui_meter_heat_level_for_column(42, width));
    assert_eq!(UiHeatLevel::Hot, ui_meter_heat_level_for_column(43, width));
    assert_eq!(UiHeatLevel::Hot, ui_meter_heat_level_for_column(49, width));

    assert_eq!(UiHeatLevel::Cool, ui_meter_heat_level_for_column(-5, width));
    assert_eq!(UiHeatLevel::Hot, ui_meter_heat_level_for_column(500, width));
    assert_eq!(UiHeatLevel::Cool, ui_meter_heat_level_for_column(2, 0));
}

#[test]
fn test_ui_meter_loading_column_highlighted_handles_wrap_and_guards() {
    assert!(ui_meter_loading_column_highlighted(0, 20, 0, 3, 2));
    assert!(ui_meter_loading_column_highlighted(2, 20, 0, 3, 2));
    assert!(!ui_meter_loading_column_highlighted(4, 20, 0, 3, 2));
    assert!(ui_meter_loading_column_highlighted(19, 20, 0, 3, 2));

    assert!(ui_meter_loading_column_highlighted(10, 20, 5, 3, 2));
    assert!(!ui_meter_loading_column_highlighted(14, 20, 5, 3, 2));

    assert!(!ui_meter_loading_column_highlighted(-1, 20, 0, 3, 2));
    assert!(!ui_meter_loading_column_highlighted(20, 20, 0, 3, 2));
    assert!(!ui_meter_loading_column_highlighted(0, 0, 0, 3, 2));

    assert!(ui_meter_loading_column_highlighted(2, 10, 1, -1, 2));
    assert!(!ui_meter_loading_column_highlighted(3, 10, 1, -1, 2));
    assert!(ui_meter_loading_column_highlighted(4, 10, 4, 30, 0));
}

#[test]
fn test_ui_sensor_matches_query_checks_all_fields() {
    let entry = make_entry(
        "TG0D",
        "GPU Die Main",
        "Die",
        SensorSection::Gpu,
        "sp78",
        62.0,
        true,
    );

    assert!(ui_sensor_matches_query(Some(&entry), Some("gpu")));
    assert!(ui_sensor_matches_query(Some(&entry), Some("tg0")));
    assert!(ui_sensor_matches_query(Some(&entry), Some("die")));
    assert!(ui_sensor_matches_query(Some(&entry), Some("SP78")));
    assert!(!ui_sensor_matches_query(Some(&entry), Some("battery")));
}

#[test]
fn test_ui_sensor_matches_query_empty_and_null_inputs() {
    let entry = make_entry(
        "Tp09",
        "CPU Die",
        "Die",
        SensorSection::Cpu,
        "sp78",
        50.0,
        true,
    );

    assert!(ui_sensor_matches_query(Some(&entry), Some("")));
    assert!(ui_sensor_matches_query(Some(&entry), None));
    assert!(ui_sensor_matches_query(None, Some("cpu")));
}

#[test]
fn test_ui_build_visible_indices_respects_active_and_filter_flag() {
    let entries = vec![
        make_entry(
            "Tp09",
            "CPU - Die",
            "Die",
            SensorSection::Cpu,
            "sp78",
            58.0,
            true,
        ),
        make_entry(
            "TG0D",
            "GPU - Die",
            "Die",
            SensorSection::Gpu,
            "sp78",
            72.0,
            true,
        ),
        make_entry(
            "TB0T",
            "Battery - Pack",
            "Pack",
            SensorSection::Battery,
            "sp78",
            34.0,
            true,
        ),
        make_entry(
            "TG1D",
            "GPU1 - Die",
            "Die",
            SensorSection::Gpu,
            "sp78",
            69.0,
            true,
        ),
        make_entry(
            "TXX1",
            "Inactive",
            "Thermal",
            SensorSection::Cpu,
            "sp78",
            80.0,
            false,
        ),
    ];

    let all_visible = ui_build_visible_indices(&entries, Some("gpu"), false);
    assert_eq!(4, all_visible.len());

    let filtered = ui_build_visible_indices(&entries, Some("gpu"), true);
    assert_eq!(2, filtered.len());
    assert_eq!("TG0D", entries[filtered[0]].key);
    assert_eq!("TG1D", entries[filtered[1]].key);
}

#[test]
fn test_ui_build_visible_indices_by_section_filters_section_when_enabled() {
    let entries = vec![
        make_entry(
            "Tp09",
            "CPU Die",
            "Die",
            SensorSection::Cpu,
            "sp78",
            58.0,
            true,
        ),
        make_entry(
            "TG0D",
            "GPU Die",
            "Die",
            SensorSection::Gpu,
            "sp78",
            66.0,
            true,
        ),
        make_entry(
            "TB0T",
            "Battery Pack",
            "Pack",
            SensorSection::Battery,
            "sp78",
            33.0,
            true,
        ),
        make_entry(
            "TG0P",
            "GPU Proximity",
            "Proximity",
            SensorSection::Gpu,
            "sp78",
            60.0,
            true,
        ),
    ];

    let visible =
        ui_build_visible_indices_by_section(&entries, Some(""), false, true, SensorSection::Gpu);

    assert_eq!(2, visible.len());
    assert_eq!(SensorSection::Gpu, entries[visible[0]].section);
    assert_eq!(SensorSection::Gpu, entries[visible[1]].section);
}

#[test]
fn test_ui_build_visible_indices_by_section_guard_clauses() {
    let entries: Vec<SensorEntry> = Vec::new();
    let visible =
        ui_build_visible_indices_by_section(&entries, None, false, false, SensorSection::Cpu);
    assert!(visible.is_empty());
}

#[test]
fn test_ui_sort_visible_indices_sorts_by_temp_desc_with_key_tiebreaker() {
    let entries = vec![
        make_entry(
            "TB0T",
            "Battery",
            "Pack",
            SensorSection::Battery,
            "sp78",
            33.0,
            true,
        ),
        make_entry(
            "TG1D",
            "GPU 1",
            "Die",
            SensorSection::Gpu,
            "sp78",
            60.0,
            true,
        ),
        make_entry(
            "TG0D",
            "GPU 0",
            "Die",
            SensorSection::Gpu,
            "sp78",
            60.0,
            true,
        ),
        make_entry("Tp09", "CPU", "Die", SensorSection::Cpu, "sp78", 55.0, true),
    ];

    let mut indices = vec![0, 1, 2, 3];
    ui_sort_visible_indices(&entries, &mut indices, UiSortMode::TempDesc);

    assert_eq!("TG0D", entries[indices[0]].key);
    assert_eq!("TG1D", entries[indices[1]].key);
    assert_eq!("Tp09", entries[indices[2]].key);
    assert_eq!("TB0T", entries[indices[3]].key);
}

#[test]
fn test_ui_sort_visible_indices_sorts_by_section_then_key() {
    let entries = vec![
        make_entry("TG0D", "GPU", "Die", SensorSection::Gpu, "sp78", 60.0, true),
        make_entry(
            "TB0T",
            "Battery",
            "Pack",
            SensorSection::Battery,
            "sp78",
            33.0,
            true,
        ),
        make_entry(
            "Tp0A",
            "CPU A",
            "Die",
            SensorSection::Cpu,
            "sp78",
            55.0,
            true,
        ),
        make_entry(
            "Tp09",
            "CPU B",
            "Die",
            SensorSection::Cpu,
            "sp78",
            54.0,
            true,
        ),
    ];

    let mut indices = vec![0, 1, 2, 3];
    ui_sort_visible_indices(&entries, &mut indices, UiSortMode::SectionKey);

    assert_eq!("Tp09", entries[indices[0]].key);
    assert_eq!("Tp0A", entries[indices[1]].key);
    assert_eq!("TG0D", entries[indices[2]].key);
    assert_eq!("TB0T", entries[indices[3]].key);
}

#[test]
fn test_ui_sort_visible_indices_sorts_by_name_type_and_key() {
    let entries = vec![
        make_entry(
            "T003",
            "Alpha",
            "Die",
            SensorSection::Cpu,
            "sp78",
            42.0,
            true,
        ),
        make_entry(
            "T002",
            "Beta",
            "P-Core",
            SensorSection::Cpu,
            "sp78",
            40.0,
            true,
        ),
        make_entry(
            "T001",
            "Gamma",
            "P-Core",
            SensorSection::Cpu,
            "sp78",
            41.0,
            true,
        ),
        make_entry(
            "T000",
            "Alpha",
            "Die",
            SensorSection::Cpu,
            "sp78",
            45.0,
            true,
        ),
    ];

    let mut indices_name = vec![0, 1, 2, 3];
    ui_sort_visible_indices(&entries, &mut indices_name, UiSortMode::Name);
    assert_eq!("T000", entries[indices_name[0]].key);
    assert_eq!("T003", entries[indices_name[1]].key);

    let mut indices_type = vec![0, 1, 2, 3];
    ui_sort_visible_indices(&entries, &mut indices_type, UiSortMode::Type);
    assert_eq!("T000", entries[indices_type[0]].key);
    assert_eq!("T003", entries[indices_type[1]].key);
    assert_eq!("T002", entries[indices_type[2]].key);
    assert_eq!("T001", entries[indices_type[3]].key);

    let mut indices_key = vec![0, 1, 2, 3];
    ui_sort_visible_indices(&entries, &mut indices_key, UiSortMode::Key);
    assert_eq!("T000", entries[indices_key[0]].key);
    assert_eq!("T001", entries[indices_key[1]].key);
    assert_eq!("T002", entries[indices_key[2]].key);
    assert_eq!("T003", entries[indices_key[3]].key);
}

#[test]
fn test_ui_sort_visible_indices_guard_clauses() {
    let entries = vec![make_entry(
        "Tp09",
        "CPU",
        "Die",
        SensorSection::Cpu,
        "sp78",
        55.0,
        true,
    )];

    let mut one_index = vec![0];
    ui_sort_visible_indices(&entries, &mut one_index, UiSortMode::Key);
    assert_eq!(0, one_index[0]);

    let mut empty_indices: Vec<usize> = Vec::new();
    ui_sort_visible_indices(&entries, &mut empty_indices, UiSortMode::Key);
    assert!(empty_indices.is_empty());
}

#[test]
fn test_ui_find_next_match_navigates_forward_and_backward() {
    let entries = vec![
        make_entry(
            "Tp09",
            "CPU Die",
            "Die",
            SensorSection::Cpu,
            "sp78",
            58.0,
            true,
        ),
        make_entry(
            "TG0D",
            "GPU Die",
            "Die",
            SensorSection::Gpu,
            "sp78",
            66.0,
            true,
        ),
        make_entry(
            "TB0T",
            "Battery Pack",
            "Pack",
            SensorSection::Battery,
            "sp78",
            33.0,
            true,
        ),
        make_entry(
            "TG0P",
            "GPU Proximity",
            "Proximity",
            SensorSection::Gpu,
            "sp78",
            60.0,
            true,
        ),
    ];
    let visible_indices = vec![0, 1, 2, 3];

    let next_gpu = ui_find_next_match(&entries, &visible_indices, 0, Some("gpu"), true);
    assert_eq!(1, next_gpu);

    let prev_gpu = ui_find_next_match(&entries, &visible_indices, 2, Some("gpu"), false);
    assert_eq!(1, prev_gpu);
}

#[test]
fn test_ui_find_next_match_handles_empty_query_no_match_and_bounds() {
    let entries = vec![
        make_entry(
            "Tp09",
            "CPU Die",
            "Die",
            SensorSection::Cpu,
            "sp78",
            58.0,
            true,
        ),
        make_entry(
            "TG0D",
            "GPU Die",
            "Die",
            SensorSection::Gpu,
            "sp78",
            66.0,
            true,
        ),
    ];
    let visible_indices = vec![0, 1];

    assert_eq!(
        1,
        ui_find_next_match(&entries, &visible_indices, 1, Some(""), true)
    );

    assert_eq!(
        0,
        ui_find_next_match(&entries, &visible_indices, 8, Some(""), true)
    );

    assert_eq!(
        0,
        ui_find_next_match(&entries, &visible_indices, 0, Some("nomatch"), true)
    );

    let empty_entries: Vec<SensorEntry> = Vec::new();
    assert_eq!(
        0,
        ui_find_next_match(&empty_entries, &visible_indices, 0, Some("gpu"), true)
    );

    let empty_visible: Vec<usize> = Vec::new();
    assert_eq!(
        0,
        ui_find_next_match(&entries, &empty_visible, 0, Some("gpu"), true)
    );
}

#[test]
fn test_ui_build_type_tiles_groups_orders_and_limits() {
    let entries = vec![
        make_entry(
            "Tp09",
            "CPU Die",
            "Die",
            SensorSection::Cpu,
            "sp78",
            55.0,
            true,
        ),
        make_entry(
            "Tf04",
            "Performance Core 1",
            "P-Core",
            SensorSection::Cpu,
            "sp78",
            63.0,
            true,
        ),
        make_entry(
            "Tf05",
            "Performance Core 2",
            "p-core",
            SensorSection::Cpu,
            "sp78",
            61.0,
            true,
        ),
        make_entry(
            "Te05",
            "Efficiency Core 1",
            "E-Core",
            SensorSection::Cpu,
            "sp78",
            49.0,
            true,
        ),
        make_entry(
            "Tz00",
            "Unnamed",
            "",
            SensorSection::Cpu,
            "sp78",
            57.0,
            true,
        ),
        make_entry(
            "TG0D",
            "GPU Die",
            "Die",
            SensorSection::Gpu,
            "sp78",
            58.0,
            true,
        ),
        make_entry(
            "Ti00",
            "Inactive",
            "P-Core",
            SensorSection::Cpu,
            "sp78",
            99.0,
            false,
        ),
    ];

    let tiles = ui_build_type_tiles(&entries, SensorSection::Cpu, 8);
    assert_eq!(4, tiles.len());

    assert_eq!("P-Core", tiles[0].type_label);
    assert_eq!(2, tiles[0].count);
    assert_close(62.0, tiles[0].avg_temp_c);
    assert_close(63.0, tiles[0].max_temp_c);

    assert_eq!("Other", tiles[1].type_label);
    assert_eq!(1, tiles[1].count);

    assert_eq!("Die", tiles[2].type_label);
    assert_eq!("E-Core", tiles[3].type_label);

    let limited_tiles = ui_build_type_tiles(&entries, SensorSection::Cpu, 2);
    assert_eq!(2, limited_tiles.len());
}

#[test]
fn test_ui_build_type_tiles_guard_clauses() {
    let entries: Vec<SensorEntry> = Vec::new();

    assert_eq!(
        0,
        ui_build_type_tiles(&entries, SensorSection::Cpu, 2).len()
    );
    assert_eq!(
        0,
        ui_build_type_tiles(&entries, SensorSection::Cpu, 0).len()
    );
}

#[test]
fn test_ui_sort_mode_label_returns_expected_names() {
    assert_eq!("Section", ui_sort_mode_label(UiSortMode::SectionKey));
    assert_eq!("Temp", ui_sort_mode_label(UiSortMode::TempDesc));
    assert_eq!("Name", ui_sort_mode_label(UiSortMode::Name));
    assert_eq!("Key", ui_sort_mode_label(UiSortMode::Key));
    assert_eq!("Type", ui_sort_mode_label(UiSortMode::Type));
}
