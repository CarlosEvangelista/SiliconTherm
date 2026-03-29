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
mod tests;
