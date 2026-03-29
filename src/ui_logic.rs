// SPDX-License-Identifier: GPL-3.0-only
// Copyright (c) 2026 CarlosEvangelista

use std::cmp::Ordering;

use crate::models::{SensorEntry, SensorSection, UiHeatLevel, UiSortMode, UiTypeTile};

fn contains_case_insensitive(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }

    haystack
        .to_ascii_lowercase()
        .contains(&needle.to_ascii_lowercase())
}

fn cmp_case_insensitive(left: &str, right: &str) -> Ordering {
    left.to_ascii_lowercase().cmp(&right.to_ascii_lowercase())
}

pub fn ui_clamp(value: f64, min_value: f64, max_value: f64) -> f64 {
    if value < min_value {
        return min_value;
    }
    if value > max_value {
        return max_value;
    }
    value
}

pub fn ui_temp_to_ratio(temp_c: f64, min_temp_c: f64, max_temp_c: f64) -> f64 {
    if max_temp_c <= min_temp_c {
        return 0.0;
    }

    let clamped = ui_clamp(temp_c, min_temp_c, max_temp_c);
    (clamped - min_temp_c) / (max_temp_c - min_temp_c)
}

pub fn ui_meter_filled_columns(ratio: f64, width: i32) -> i32 {
    if width <= 0 {
        return 0;
    }

    let clamped = ui_clamp(ratio, 0.0, 1.0);
    let columns = (clamped * f64::from(width) + 0.5) as i32;

    if columns < 0 {
        return 0;
    }
    if columns > width {
        return width;
    }

    columns
}

pub fn ui_temp_to_heat_level(
    temp_c: f64,
    warm_threshold_c: f64,
    hot_threshold_c: f64,
) -> UiHeatLevel {
    if temp_c >= hot_threshold_c {
        return UiHeatLevel::Hot;
    }
    if temp_c >= warm_threshold_c {
        return UiHeatLevel::Warm;
    }
    UiHeatLevel::Cool
}

pub fn ui_meter_heat_level_for_column(column: i32, inner_width: i32) -> UiHeatLevel {
    if inner_width <= 0 {
        return UiHeatLevel::Cool;
    }

    let mut clamped_column = column;
    if clamped_column < 0 {
        clamped_column = 0;
    }
    if clamped_column >= inner_width {
        clamped_column = inner_width - 1;
    }

    let frac = f64::from(clamped_column + 1) / f64::from(inner_width);
    if frac >= 0.88 {
        return UiHeatLevel::Hot;
    }
    if frac >= 0.68 {
        return UiHeatLevel::Warm;
    }
    UiHeatLevel::Cool
}

pub fn ui_meter_loading_column_highlighted(
    column: i32,
    inner_width: i32,
    frame: u32,
    pulse_radius: i32,
    speed: u32,
) -> bool {
    if column < 0 || inner_width <= 0 || column >= inner_width {
        return false;
    }

    let mut clamped_radius = pulse_radius;
    if clamped_radius < 0 {
        clamped_radius = 0;
    }
    if clamped_radius > inner_width {
        clamped_radius = inner_width;
    }

    let effective_speed = if speed == 0 { 1 } else { speed };
    let head = ((frame * effective_speed) % (inner_width as u32)) as i32;

    let mut distance = (column - head).abs();
    let wrap_distance = inner_width - distance;
    if wrap_distance < distance {
        distance = wrap_distance;
    }

    distance <= clamped_radius
}

pub fn ui_sensor_matches_query(entry: Option<&SensorEntry>, query: Option<&str>) -> bool {
    let q = query.unwrap_or("");
    if entry.is_none() || q.is_empty() {
        return true;
    }

    let sensor = entry.expect("checked above");
    contains_case_insensitive(&sensor.key, q)
        || contains_case_insensitive(&sensor.name, q)
        || contains_case_insensitive(&sensor.type_label, q)
        || contains_case_insensitive(&sensor.data_type, q)
}

pub fn ui_build_visible_indices(
    entries: &[SensorEntry],
    query: Option<&str>,
    filter_enabled: bool,
) -> Vec<usize> {
    ui_build_visible_indices_by_section(entries, query, filter_enabled, false, SensorSection::Cpu)
}

pub fn ui_build_visible_indices_by_section(
    entries: &[SensorEntry],
    query: Option<&str>,
    filter_enabled: bool,
    section_enabled: bool,
    section: SensorSection,
) -> Vec<usize> {
    let mut visible = Vec::new();

    for (index, entry) in entries.iter().enumerate() {
        if !entry.active {
            continue;
        }
        if section_enabled && entry.section != section {
            continue;
        }

        let matches = ui_sensor_matches_query(Some(entry), query);
        if filter_enabled && !matches {
            continue;
        }

        visible.push(index);
    }

    visible
}

fn sensor_compare(left: &SensorEntry, right: &SensorEntry, mode: UiSortMode) -> Ordering {
    match mode {
        UiSortMode::TempDesc => {
            if left.last_temp_c > right.last_temp_c {
                Ordering::Less
            } else if left.last_temp_c < right.last_temp_c {
                Ordering::Greater
            } else {
                left.key.cmp(&right.key)
            }
        }
        UiSortMode::Name => {
            let name_cmp = cmp_case_insensitive(&left.name, &right.name);
            if name_cmp != Ordering::Equal {
                name_cmp
            } else {
                left.key.cmp(&right.key)
            }
        }
        UiSortMode::Key => left.key.cmp(&right.key),
        UiSortMode::Type => {
            let type_cmp = cmp_case_insensitive(&left.type_label, &right.type_label);
            if type_cmp != Ordering::Equal {
                return type_cmp;
            }

            let name_cmp = cmp_case_insensitive(&left.name, &right.name);
            if name_cmp != Ordering::Equal {
                return name_cmp;
            }

            left.key.cmp(&right.key)
        }
        UiSortMode::SectionKey => {
            let left_section = left.section as i32;
            let right_section = right.section as i32;
            if left_section != right_section {
                left_section.cmp(&right_section)
            } else {
                left.key.cmp(&right.key)
            }
        }
    }
}

pub fn ui_sort_visible_indices(entries: &[SensorEntry], indices: &mut [usize], mode: UiSortMode) {
    if entries.is_empty() || indices.len() < 2 {
        return;
    }

    for i in 1..indices.len() {
        let current = indices[i];
        let mut j = i;

        while j > 0 {
            let left_index = indices[j - 1];
            let (Some(left), Some(right)) = (entries.get(left_index), entries.get(current)) else {
                break;
            };

            if sensor_compare(left, right, mode) != Ordering::Greater {
                break;
            }

            indices[j] = indices[j - 1];
            j -= 1;
        }

        indices[j] = current;
    }
}

pub fn ui_find_next_match(
    entries: &[SensorEntry],
    visible_indices: &[usize],
    current_visible_index: usize,
    query: Option<&str>,
    forward: bool,
) -> usize {
    let visible_count = visible_indices.len();
    if entries.is_empty() || visible_count == 0 {
        return 0;
    }

    let q = query.unwrap_or("");
    let mut current = current_visible_index;

    if q.is_empty() {
        if current >= visible_count {
            return 0;
        }
        return current;
    }

    if current >= visible_count {
        current = 0;
    }

    for offset in 1..=visible_count {
        let candidate = if forward {
            (current + offset) % visible_count
        } else {
            (current + visible_count - (offset % visible_count)) % visible_count
        };

        let Some(entry) = entries.get(visible_indices[candidate]) else {
            continue;
        };

        if ui_sensor_matches_query(Some(entry), Some(q)) {
            return candidate;
        }
    }

    current
}

pub fn ui_build_type_tiles(
    entries: &[SensorEntry],
    section: SensorSection,
    max_tiles: usize,
) -> Vec<UiTypeTile> {
    if max_tiles == 0 {
        return Vec::new();
    }

    let mut tiles: Vec<UiTypeTile> = Vec::new();

    for entry in entries {
        if !entry.active || entry.section != section {
            continue;
        }

        let label = if entry.type_label.is_empty() {
            "Other"
        } else {
            &entry.type_label
        };

        let tile_index = tiles
            .iter()
            .position(|tile| tile.type_label.eq_ignore_ascii_case(label));

        let index = match tile_index {
            Some(found) => found,
            None => {
                if tiles.len() >= max_tiles {
                    continue;
                }
                tiles.push(UiTypeTile::new(label));
                tiles.len() - 1
            }
        };

        let tile = &mut tiles[index];
        tile.avg_temp_c =
            ((tile.avg_temp_c * tile.count as f64) + entry.last_temp_c) / (tile.count + 1) as f64;
        if tile.count == 0 || entry.last_temp_c > tile.max_temp_c {
            tile.max_temp_c = entry.last_temp_c;
        }
        tile.count += 1;
    }

    for i in 1..tiles.len() {
        let current = tiles[i].clone();
        let mut j = i;

        while j > 0 && tiles[j - 1].avg_temp_c < current.avg_temp_c {
            tiles[j] = tiles[j - 1].clone();
            j -= 1;
        }

        tiles[j] = current;
    }

    tiles
}

pub fn ui_sort_mode_label(mode: UiSortMode) -> &'static str {
    match mode {
        UiSortMode::TempDesc => "Temp",
        UiSortMode::Name => "Name",
        UiSortMode::Key => "Key",
        UiSortMode::Type => "Type",
        UiSortMode::SectionKey => "Section",
    }
}

#[cfg(test)]
mod tests {
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

        let visible = ui_build_visible_indices_by_section(
            &entries,
            Some(""),
            false,
            true,
            SensorSection::Gpu,
        );

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
}
